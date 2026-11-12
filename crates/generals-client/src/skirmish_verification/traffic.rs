//! Installed traffic through ordinary movement, including a newly stopped ally.

use super::{Kind, Order, Skirmish, friendly_entity, require};
use std::error::Error;

pub(super) fn verify(game: &mut Skirmish, tank: usize) -> Result<(), Box<dyn Error>> {
    let owner = game.entities[tank].owner;
    let type_index = game.entities[tank].type_index;
    let other = game
        .entities
        .iter()
        .enumerate()
        .find(|(id, actor)| {
            *id != tank
                && actor.owner == owner
                && actor.type_index == type_index
                && actor.complete()
        })
        .map(|(id, _)| id)
        .ok_or("Traffic check needs a second normally produced tank")?;
    let infantry = friendly_entity(game, Kind::Infantry, owner)?;
    let home = game.entities[friendly_entity(game, Kind::CommandCenter, owner)?].position;
    let away = game.entities[friendly_entity(game, Kind::CommandCenter, 1 - owner)?].position;
    let direction = [(away[0] - home[0]) * 0.25, (away[1] - home[1]) * 0.25];
    let center = [home[0] + direction[0], home[1] + direction[1]];
    let lane = [
        [center[0] - 150.0, center[1]],
        [center[0], center[1] - 150.0],
        [center[0] + 150.0, center[1]],
        [center[0], center[1] + 150.0],
    ];
    let actors = [tank, other, infantry];
    park(
        game,
        infantry,
        [center[0] + 70.0, center[1] + 70.0],
        &actors,
    )?;
    park(game, tank, lane[0], &actors)?;
    park(game, other, lane[1], &actors)?;
    park(game, infantry, center, &actors)?;

    let goals = start_crossing(game, actors, center, [lane[2], lane[3]])?;
    let idle_position = game.entities[infantry].position;
    let required_yield = game.catalog[game.entities[infantry].type_index]
        .geometry
        .major;
    let mut furthest_yield = 0.0_f32;
    let mut settled_frames = 0;
    for _ in 0..30 * 120 {
        checked_step(game, &actors)?;
        let actor = &game.entities[infantry];
        require(
            matches!(actor.ai.order, Order::Idle),
            "Yield replaced the idle ally's parent order",
        )?;
        furthest_yield = furthest_yield.max(distance(actor.position, idle_position));
        let arrived = at_goal(game, tank, goals[0]) && at_goal(game, other, goals[1]);
        if arrived && furthest_yield > required_yield && !actor.moving() {
            settled_frames += 1;
        } else {
            settled_frames = 0;
        }
        if settled_frames == 30 {
            println!(
                "  {} traffic: both tanks arrived; {} yielded {furthest_yield:.3} source units and resumed idle; finite poses checked",
                game.catalog[type_index].name, game.catalog[actor.type_index].name
            );
            return Ok(());
        }
    }
    Err(format!("{} traffic timed out: tank {:?} toward {:?}; other {:?} toward {:?}; idle ally {:?}, displacement {furthest_yield:.3}, moving {}", game.catalog[type_index].name, game.entities[tank].position, goals[0], game.entities[other].position, goals[1], game.entities[infantry].position, game.entities[infantry].moving()).into())
}

fn start_crossing(
    game: &mut Skirmish,
    actors: [usize; 3],
    center: [f32; 2],
    destinations: [[f32; 2]; 2],
) -> Result<[[f32; 2]; 2], Box<dyn Error>> {
    let [tank, other, infantry] = actors;
    let origins = actors.map(|id| game.entities[id].position);
    game.move_to(infantry, [center[0], center[1] + 70.0])?;
    // Native paths are queued. Actual departure proves the infantry's new goal
    // was installed before tanks evaluate its transient occupancy, and proves
    // both tank routes were installed before Stop makes that ally fixed again.
    for _ in 0..90 {
        checked_step(game, &actors)?;
        if distance(game.entities[infantry].position, origins[2]) > 0.01 {
            break;
        }
    }
    require(
        distance(game.entities[infantry].position, origins[2]) > 0.01,
        "Traffic ally did not depart before tank path requests",
    )?;
    let goals = [
        command(game, tank, destinations[0])?,
        command(game, other, destinations[1])?,
    ];
    let departed = |game: &Skirmish| {
        distance(game.entities[tank].position, origins[0]) > 0.01
            && distance(game.entities[other].position, origins[1]) > 0.01
    };
    for _ in 0..90 {
        checked_step(game, &actors)?;
        if departed(game) {
            break;
        }
    }
    require(
        departed(game),
        "Traffic tanks did not obtain and start their queued paths",
    )?;
    game.stop(infantry)?;
    Ok(goals)
}

fn checked_step(game: &mut Skirmish, actors: &[usize]) -> Result<(), Box<dyn Error>> {
    game.step();
    for &id in actors {
        let actor = &game.entities[id];
        require(
            actor.alive()
                && actor.position.into_iter().all(f32::is_finite)
                && actor.facing.is_finite(),
            "Traffic produced a dead or nonfinite actor",
        )?;
    }
    Ok(())
}

fn command(game: &mut Skirmish, id: usize, point: [f32; 2]) -> Result<[f32; 2], Box<dyn Error>> {
    game.move_to(id, point)?;
    let Order::Move(goal) = game.entities[id].ai.order else {
        return Err("Traffic move did not retain its requested order".into());
    };
    require(
        distance(point, goal) <= 10.0 * 2.0_f32.sqrt(),
        "Traffic lane destination was clipped outside its requested path cell",
    )?;
    Ok(goal)
}

fn park(
    game: &mut Skirmish,
    id: usize,
    point: [f32; 2],
    actors: &[usize],
) -> Result<(), Box<dyn Error>> {
    let goal = command(game, id, point)?;
    for _ in 0..30 * 120 {
        checked_step(game, actors)?;
        if at_goal(game, id, goal) {
            return Ok(());
        }
    }
    Err(format!(
        "Traffic staging failed for {} at {:?}, requested {goal:?}",
        game.catalog[game.entities[id].type_index].name, game.entities[id].position
    )
    .into())
}

fn at_goal(game: &Skirmish, id: usize, goal: [f32; 2]) -> bool {
    let actor = &game.entities[id];
    let close = game.catalog[actor.type_index]
        .locomotor
        .ground
        .as_ref()
        .map_or(0.0, |rules| rules.close_enough);
    // Ordinary routes finish at a native ten-unit path-cell node. This bound
    // accounts for that cell plus the installed locomotor's arrival threshold.
    let cell_diagonal = 10.0 * 2.0_f32.sqrt();
    matches!(actor.ai.order, Order::Idle)
        && !actor.moving()
        && distance(actor.position, goal) < cell_diagonal + close
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
