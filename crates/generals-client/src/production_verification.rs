//! Consecutive installed-unit production through a retained factory rally.
use crate::skirmish_verification::{advance_until, require};
use generals_sim::skirmish::{Order, Skirmish};
use std::error::Error;

pub fn verify(
    game: &mut Skirmish,
    producer: usize,
    type_index: usize,
) -> Result<(), Box<dyn Error>> {
    let owner = game.entities[producer].owner;
    let scout = game
        .entities
        .iter()
        .position(|entity| {
            entity.owner == owner && entity.type_index == type_index && entity.complete()
        })
        .ok_or("Rally verification needs the already produced tank")?;
    let rally = rally_site(game, producer, scout)?;
    game.set_rally_point(producer, [rally[0], rally[1], game.ground_height(rally)])?;
    let cost = game.catalog[type_index]
        .cost
        .checked_mul(3)
        .ok_or("Rally verification production cost overflow")?;
    advance_until(
        game,
        18_000,
        "income for consecutive rally production",
        |game| game.players[owner].cash >= cost,
    )?;
    let first = game.entities.len();
    for _ in 0..3 {
        game.train(producer, type_index)?;
    }
    let mut exits = Vec::new();
    let mut advanced = Vec::new();
    let mut arrivals = Vec::new();
    for _ in 0..18_000 {
        game.step();
        let units: Vec<_> = game
            .entities
            .iter()
            .enumerate()
            .skip(first)
            .filter(|(_, unit)| unit.owner == owner && unit.type_index == type_index)
            .collect();
        for (index, (_, unit)) in units.iter().enumerate() {
            if exits.len() <= index {
                let Order::Exit(goal) = unit.ai.order else {
                    return Err("Produced tank did not enter its original factory exit".into());
                };
                exits.push(goal);
                advanced.push(false);
                arrivals.push(None);
            }
            if let Order::Exit(goal) = unit.ai.order {
                advanced[index] |= goal.map(f32::to_bits) != exits[index].map(f32::to_bits);
                exits[index] = goal;
            }
            if matches!(unit.ai.order, Order::Idle) && advanced[index] && arrivals[index].is_none()
            {
                // Capture completion before a later unit can ask this tank to yield.
                arrivals[index] = Some(unit.position);
            }
        }
        if units.len() == 3
            && units
                .iter()
                .all(|(_, unit)| matches!(unit.ai.order, Order::Idle))
        {
            require(
                advanced.iter().all(|advanced| *advanced),
                "A later tank stopped at the factory instead of following its rally",
            )?;
            let distances = verify_arrivals(&exits, &arrivals)?;
            require(
                game.entities[producer].rally_point
                    == Some([rally[0], rally[1], game.ground_height(rally)]),
                "Production consumed or changed the factory rally point",
            )?;
            println!(
                "  {}: three consecutively queued units completed distinct source rally paths; arrival positions={arrivals:?}, goals={exits:?}, distances={distances:?}",
                game.catalog[type_index].name,
            );
            return Ok(());
        }
    }
    Err(format!(
        "Consecutive factory rally production did not complete: owner {owner}, producer {producer} {} at {:?}, health {}, queued {}, rally {rally:?}, scout {scout}: {}; exits={exits:?}, advanced={advanced:?}, arrivals={arrivals:?}; produced={:?}",
        game.catalog[game.entities[producer].type_index].name,
        game.entities[producer].position,
        game.entities[producer].health,
        game.entities[producer].queue.len(),
        generals_sim::skirmish::diagnostics::movement_details(game, scout),
        game.entities.iter().enumerate().skip(first)
            .filter(|(_, unit)| unit.owner == owner && unit.type_index == type_index)
            .map(|(id, _)| (id, generals_sim::skirmish::diagnostics::movement_details(game, id)))
            .collect::<Vec<_>>(),
    ).into())
}

fn verify_arrivals(
    exits: &[[f32; 3]],
    arrivals: &[Option<[f32; 2]>],
) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut distances = Vec::new();
    for (index, goal) in exits.iter().enumerate() {
        let position = arrivals[index].ok_or("Produced unit did not finish its rally path")?;
        let dx = position[0] - goal[0];
        let dy = position[1] - goal[1];
        let squared = [dx * dx, dy * dy];
        let distance = (squared[0] + squared[1]).sqrt();
        // This diagnostic observes the completed second leg: production exits
        // skip it when within ten units. Its retained extra distance therefore
        // selects getTreatAsAircraftForLocoDistToGoal (>1), with the original
        // AIInternalMoveToState ground sanity limit, not Euclidean CloseEnoughDist.
        if !distance.is_finite() || distance > 40.0 {
            return Err(format!("Produced unit {index} ended outside native rally arrival bounds: position={position:?}, goal={goal:?}, distance={distance}").into());
        }
        require(
            exits[..index]
                .iter()
                .all(|previous| previous.map(f32::to_bits) != goal.map(f32::to_bits)),
            "Consecutive units received the same claimed rally destination",
        )?;
        distances.push(distance);
    }
    Ok(distances)
}

fn rally_site(
    game: &mut Skirmish,
    producer: usize,
    scout: usize,
) -> Result<[f32; 2], Box<dyn Error>> {
    let path = game
        .producer_rally_path(producer)
        .ok_or("Missing producer rally path")?;
    let origin = path[1];
    let start = game.entities[scout].position;
    let mut failures = Vec::new();
    for [x, y] in [[150.0, 0.0], [0.0, 150.0], [-150.0, 0.0], [0.0, -150.0]] {
        let destination = [origin[0] + x, origin[1] + y];
        if let Err(error) = game.move_to(scout, destination) {
            failures.push(format!("{destination:?}: {error}"));
            continue;
        }
        let Order::Move(goal) = game.entities[scout].ai.order else {
            return Err("Rally scout did not retain the ordinary move command".into());
        };
        // Acceptance only queues the native path. Idle can also mean that the
        // requested route failed, so observe both travel and actual arrival.
        advance_until(
            game,
            3600,
            "existing tank completing its queued rally approach",
            |game| {
                let actor = &game.entities[scout];
                matches!(actor.ai.order, Order::Idle) && !actor.moving()
            },
        )
        .map_err(|error| {
            format!(
                "{error}; requested {destination:?}, adjusted {goal:?}; {}",
                generals_sim::skirmish::diagnostics::movement_details(game, scout)
            )
        })?;
        let actor = &game.entities[scout];
        let position = actor.position;
        let distance = |point: [f32; 2]| (position[0] - point[0]).hypot(position[1] - point[1]);
        let close = game.catalog[actor.type_index]
            .locomotor
            .ground
            .as_ref()
            .map_or(0.0, |rules| rules.close_enough);
        // Ordinary routes end at ten-unit path-cell nodes, using the authored
        // locomotor arrival distance, as in the installed traffic diagnostic.
        if actor.complete()
            && distance(start) > 10.0
            && distance(goal) < 10.0_f32.mul_add(2.0_f32.sqrt(), close)
        {
            return Ok(position);
        }
        failures.push(format!(
            "{destination:?}: adjusted {goal:?}, finished at {position:?}; {}",
            generals_sim::skirmish::diagnostics::movement_details(game, scout)
        ));
    }
    Err(format!("No rally verification destination could be reached: {failures:?}").into())
}
