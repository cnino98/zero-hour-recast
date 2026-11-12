//! Original helicopter death reached by paid, powered enemy Patriot batteries.
use super::{Setup, advance_until, friendly_entity, require, template};
use crate::skirmish_verification::construct;
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

pub fn prepare(game: &mut Skirmish, target: usize) -> Result<[usize; 2], Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    let kind = template(game, "AmericaPatriotBattery")?;
    let center = game.entities[target].position;
    let mut defenders = [0; 2];
    for (index, id) in defenders.iter_mut().enumerate() {
        let point = game
            .placement_near(
                kind,
                [center[0] + 900.0, center[1] - 350.0 + [0.0, 140.0][index]],
            )
            .ok_or("no legal Patriot site outside Helix weapon trial")?;
        crate::skirmish_verification::scout_construction_site(game, builder, kind, point)?;
        let cash = game.players[1].cash;
        *id = game.build(builder, kind, point)?;
        require(
            game.players[1].cash.checked_add(game.catalog[kind].cost) == Some(cash),
            "Helix opposing Patriot did not deduct its original price",
        )?;
        advance_until(game, 7200, "paid Helix opposing Patriot", |game| {
            game.entities[*id].complete()
        })?;
    }
    Ok(defenders)
}

/// Fly the loaded Bunker into ordinary anti-air acquisition range.
pub fn approach(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let helicopter = setup.helicopters[0];
    let battery = &game.entities[setup.defenders[0]];
    let battery_type = &game.catalog[battery.type_index];
    let center = battery.position;
    let from = game.entities[helicopter].position;
    let bearing = (from[1] - center[1]).atan2(from[0] - center[0]);
    let range = battery_type.weapon.range * 0.75;
    // A falling vehicle that hits a STRUCTURE is immediately destroyed (PhysicsUpdate.cpp1399).
    // Enter the real AA envelope over open ground so its spiral can reach the ground callback.
    let goal = (0..32_u8)
        .find_map(|index| {
            let turn = f32::from(index.div_ceil(2)) * (std::f32::consts::TAU / 32.0);
            let angle = if index % 2 == 0 {
                bearing + turn
            } else {
                bearing - turn
            };
            let point = [
                center[0] + range * angle.cos(),
                center[1] + range * angle.sin(),
            ];
            if !game.can_place(battery.type_index, point) {
                return None;
            }
            let clearance = game
                .entities
                .iter()
                .filter(|actor| actor.exists() && !actor.masked())
                .filter_map(|actor| {
                    let unit = &game.catalog[actor.type_index];
                    unit.kind_of
                        .iter()
                        .any(|flag| flag == "STRUCTURE")
                        .then(|| {
                            (point[0] - actor.position[0]).hypot(point[1] - actor.position[1])
                                - unit.geometry_bounds.sphere_radius
                        })
                })
                .fold(f32::INFINITY, f32::min);
            (clearance >= 150.0).then_some(point)
        })
        .ok_or("no open ground inside the powered Patriot's range for the Helix crash")?;
    game.move_to(helicopter, goal)?;
    Ok(())
}

pub fn verify(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let id = setup.helicopters[0];
    let child = game.entities[id]
        .attachment_child()
        .ok_or("missing Helix death attachment")?;
    let first_entity = game.entities.len();
    approach(game, setup)?;
    let mut seen = [false; 4];
    let mut shots = 0;
    for _ in 0..9000 {
        game.step();
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| setup.defenders.contains(&shot.source))
            .count();
        seen[0] |= game.entities[id].helicopter_corpse()
            && game.entities[id]
                .last_attacker()
                .is_some_and(|source| setup.defenders.contains(&source));
        seen[1] |= game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.object == id && event.effect == "FX_HelicopterBladeExplosion");
        seen[2] |= game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.object == id && event.effect == "FX_HelicopterHitGround");
        seen[3] |= game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.object == id && event.effect == "FX_HelixHelicopterBlowUpBig");
        let hull = game.entities[first_entity..]
            .iter()
            .find(|actor| game.catalog[actor.type_index].name == "HelixRubbleHull");
        if let Some(hull) = hull.filter(|actor| actor.physical_present()) {
            require(
                seen.into_iter().all(|value| value) && shots > 0,
                "Helix hull bypassed real AA, spiral or original death effects",
            )?;
            require(
                game.entities[id].masked()
                    && !game.entities[child].alive()
                    && setup
                        .passengers
                        .iter()
                        .all(|&passenger| !game.entities[passenger].alive()),
                "Helix death left its portable attachment or airborne passengers alive",
            )?;
            require(
                hull.owner == 0 && hull.physical_basis().is_some(),
                "Helix rubble lost original ownership or physical transform",
            )?;
            println!(
                "  Helix actual AA death: {shots} Patriot shots, original spiral/blades/ground/final FX, destroyed Bunker and five passengers, physical HelixRubbleHull at {}",
                game.tick
            );
            return Ok(());
        }
    }
    Err(format!(
        "Helix AA death incomplete at tick {}: shots={shots}, effects={seen:?}, exists={}, dead={}, held={}, ground_hit={}, end={:?}, {}; terrain={}",
        game.tick,
        game.entities[id].exists(),
        game.entities[id].is_effectively_dead(),
        game.entities[id].disabled_held(),
        game.entities[id].helicopter_hit_ground(),
        game.ended_at,
        generals_sim::skirmish::diagnostics::movement_details(game, id),
        game.ground_height(game.entities[id].position)
    )
    .into())
}
