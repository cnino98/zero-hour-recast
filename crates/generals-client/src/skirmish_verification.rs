//! Installed-content command-path checks for the bounded base-faction playtest.
pub mod crawler;
use crate::{playtest::MAP_NAME, skirmish_data, terrain::Terrain, world_data};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::{error::Error, path::Path};

const TICKS_PER_MINUTE: u32 = 30 * 60;

mod cancellation;
pub mod overlord;
mod roster;
pub mod technical;
mod traffic;
pub mod tunnel;

pub fn prepare_controls(game: &mut Skirmish) -> Result<Vec<usize>, Box<dyn Error>> {
    // Screenshot scenario uses the same construction and production APIs as play.
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    for (name, producer) in [
        ("AmericaInfantryMissileDefender", barracks),
        ("AmericaVehicleHumvee", factory),
    ] {
        let index = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .ok_or("control capture unit missing")?;
        produce_type(game, producer, index, 0)?;
    }
    let [x, y] = game.entities[factory].position;
    game.set_rally_point(
        factory,
        [
            x + 140.0,
            y - 100.0,
            game.ground_height([x + 140.0, y - 100.0]),
        ],
    )?;
    Ok(vec![factory])
}

fn verify_art(
    animations: &crate::model_animation::Animations,
    data: &skirmish_data::SkirmishData,
) -> Result<(), Box<dyn Error>> {
    animations.verify_salvage_art(&data.models, &data.types)?;
    animations.verify_weapon_research_art(&data.models, &data.types)?;
    animations.verify_weapon_visibility(&data.models, &data.types)?;
    animations.verify_upgrade_art(&data.types)?;
    animations.verify_hacker_art(&data.models, &data.types)?;
    animations.verify_alternate_weapon_art(&data.models, &data.types)?;
    animations.verify_turret_articulation(&data.models, &data.types)?;
    animations.verify_defense_art(&data.models, &data.types)?;
    animations.verify_rubble_art(&data.models, &data.types)?;
    Ok(())
}

fn verify_interfaces(
    assets: &VirtualAssets,
    catalog: &[generals_sim::skirmish::UnitType],
    options: &Path,
) -> Result<(), Box<dyn Error>> {
    drop(crate::control_bar::load(assets, catalog, options)?);
    drop(crate::source_audio::load(assets, catalog, options)?);
    drop(crate::input::load(assets, options, catalog)?);
    Ok(())
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    crate::source_audio::verify_result_scripts(&assets)?;
    let map = MapData::parse(&assets.read(MAP_NAME)?.bytes)?;
    let mut data = skirmish_data::load(&assets, &map.objects)?;
    verify_interfaces(&assets, &data.types, &root.join("UserData/Options.ini"))?;
    crate::income::load(&assets)?;
    crate::particles::verify(&assets, &data.types)?;
    let _lasers = crate::laser_beam::load(&assets, &data.types)?;
    crate::particles::orientation::load(&assets, &mut data.models)?;
    crate::attack_cursor::verify(&assets)?;
    let starting_cash = data.economy.starting_cash;
    let supply_value = data.economy.supply_value;
    let settings = world_data::camera_settings(&assets, root)?;
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )?;
    let (layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )?;
    let mut animations = crate::model_animation::load(&assets, &mut data.models, &data.types)?;
    animations.bind_logical_bones(&mut data.types);
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations)?;
    verify_art(&animations, &data)?;
    let movement_terrain = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement_terrain, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    crate::weapon_verification::verify(&mut game)?;
    animations.verify_defense_power_playback(&game)?;
    crate::maintenance_verification::verify(&mut game)?;
    crate::usa_research_verification::verify(&mut game)?;
    crate::income_verification::verify(&mut game)?;
    crate::transport_verification::verify(&mut game)?;
    overlord::verify(&mut game)?;
    tunnel::verify(&mut game)?;
    technical::verify(&mut game)?;
    crate::weapon_verification::research::verify(&mut game)?;
    crawler::verify(&mut game)?;
    verify_faction_loops(&mut game, starting_cash, supply_value)?;

    Ok(())
}
fn verify_faction_loops(
    game: &mut Skirmish,
    starting_cash: u32,
    supply_value: u32,
) -> Result<(), Box<dyn Error>> {
    for (name, faction, enemy) in [
        ("USA", Faction::Usa, Faction::China),
        ("China", Faction::China, Faction::Gla),
        ("GLA", Faction::Gla, Faction::Usa),
    ] {
        game.reset(faction, enemy)?;
        verify_reset(game, starting_cash)?;
        crate::projectile_verification::verify_missile(game)?;
        game.reset(faction, enemy)?;
        verify_reset(game, starting_cash)?;
        cancellation::verify(game)?;
        game.reset(faction, enemy)?;
        verify_reset(game, starting_cash)?;
        crate::progression_verification::initial(game)?;
        println!("{name}: checking construction, supply income and production");
        verify_faction(game, supply_value, 0)
            .map_err(|error| format!("{name} playtest at tick {}: {error}", game.tick))?;
        crate::progression_verification::earned(game, 0)?;
        println!(
            "{name}: victory at tick {}; checking restart and commanded opponent victory",
            game.tick
        );
        game.reset(faction, enemy)?;
        verify_reset(game, starting_cash)?;
        crate::progression_verification::initial(game)?;
        // Scenario orchestration is confined to this diagnostic, never the simulation.
        verify_faction(game, supply_value, 1)
            .map_err(|error| format!("{name} commanded opponent at tick {}: {error}", game.tick))?;
        crate::progression_verification::earned(game, 1)?;
        require(
            game.outcome == Some(1),
            "commanded opponent did not defeat player zero",
        )?;
        // This headless scenario has no global-script schedule. Installed SCB
        // timing is verified above; invoke its declared end action explicitly.
        game.start_end_game_timer();
        let deadline = game
            .end_game_at
            .ok_or("Defeat did not start its end timer")?;
        for _ in 0..121 {
            game.step();
        }
        require(
            game.tick == deadline,
            "finished match did not stop after the source result timer",
        )?;
        game.reset(faction, enemy)?;
        verify_reset(game, starting_cash)?;
        crate::progression_verification::exercise(game)?;
        game.reset(faction, enemy)?;
        verify_reset(game, starting_cash)?;
        println!(
            "{name}: construction, income, production, movement, combat, victory, defeat, promotion powers and restart passed"
        );
    }
    println!(
        "Installed base-faction command checks passed at the normal 30 Hz tick rate. Both sides were commanded by this diagnostic; autonomous source AI is checked separately by --verify-opponent. This does not establish complete engine parity."
    );
    Ok(())
}

fn verify_reset(game: &Skirmish, cash: u32) -> Result<(), Box<dyn Error>> {
    require(
        game.tick == 0
            && game.ended_at.is_none()
            && game.presentation.shots.is_empty()
            && game.projectiles.is_empty()
            && game
                .entities
                .iter()
                .filter(|entity| entity.owner != game.neutral_owner())
                .count()
                == 4
            && game.players.iter().enumerate().all(|(owner, player)| {
                player.cash
                    == if owner == game.neutral_owner() {
                        0
                    } else {
                        cash
                    }
            })
            && game
                .entities
                .iter()
                .all(|entity| entity.complete() && entity.queue.is_empty() && entity.cargo == 0),
        "restart did not restore a clean initial match",
    )
}

fn verify_faction(
    game: &mut Skirmish,
    supply_value: u32,
    owner: usize,
) -> Result<(), Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, owner)?;
    if game.players[owner].faction != Faction::Gla {
        construct(game, builder, Kind::PowerPlant, owner)?;
    }
    construct(game, builder, Kind::SupplyCenter, owner)?;
    advance_until(game, TICKS_PER_MINUTE, "source collector grant", |game| {
        supplied_collector(game, builder, owner).is_some()
    })?;
    let collector = supplied_collector(game, builder, owner)
        .ok_or("completed supply center did not grant its source collector")?;
    advance_until(
        game,
        TICKS_PER_MINUTE,
        "collector leaving its producer",
        |game| {
            game.entities[collector].complete()
                && !matches!(game.entities[collector].ai.order, Order::Exit(_))
        },
    )?;
    verify_income(game, collector, supply_value, owner)?;
    let barracks = construct(game, builder, Kind::Barracks, owner)?;
    let infantry = produce(game, barracks, Kind::Infantry, owner)?;
    let factory = construct(game, builder, Kind::Factory, owner)?;
    let tank = produce(game, factory, Kind::Tank, owner)?;
    let tank_type = game.entities[tank].type_index;
    crate::production_verification::verify(game, factory, tank_type)?;
    if owner == 0 {
        traffic::verify(game, tank)?;
    }
    let paid_types = [game.entities[infantry].type_index, tank_type];
    roster::verify(game, barracks, factory, owner, paid_types)?;
    verify_movement(game, tank)?;
    game.harvest(collector, home_supply(game, owner)?)?;
    verify_combat_and_victory(game, barracks, factory, owner).map(|_| ())
}

fn supplied_collector(game: &Skirmish, builder: usize, owner: usize) -> Option<usize> {
    game.entities
        .iter()
        .enumerate()
        .find(|(id, entity)| {
            *id != builder
                && entity.owner == owner
                && entity.complete()
                && game.catalog[entity.type_index].supply.capacity > 0
        })
        .map(|(id, _)| id)
}

pub fn friendly_entity(game: &Skirmish, kind: Kind, owner: usize) -> Result<usize, Box<dyn Error>> {
    game.entities
        .iter()
        .position(|entity| {
            entity.owner == owner
                && entity.complete()
                && game.catalog[entity.type_index].kind == kind
        })
        .ok_or_else(|| "required completed friendly object is missing".into())
}

pub fn construct(
    game: &mut Skirmish,
    builder: usize,
    kind: Kind,
    owner: usize,
) -> Result<usize, Box<dyn Error>> {
    let type_index = game
        .type_for(owner, kind)
        .ok_or("faction lacks building type")?;
    construct_type(game, builder, type_index, owner)
}

pub fn construct_type(
    game: &mut Skirmish,
    builder: usize,
    type_index: usize,
    owner: usize,
) -> Result<usize, Box<dyn Error>> {
    // Native Dozer tasks stamped at frame zero are not selected by
    // getMostRecentCommand. Start this verification's player action after
    // the first ordinary logic update, without rewriting the task clock.
    if game.tick == 0 {
        game.step();
    }
    let position = construction_position(game, builder, type_index, owner)?;
    construct_type_at(game, builder, type_index, owner, position)
}

pub fn construct_type_at(
    game: &mut Skirmish,
    builder: usize,
    type_index: usize,
    owner: usize,
    position: [f32; 2],
) -> Result<usize, Box<dyn Error>> {
    scout_construction_site(game, builder, type_index, position)?;
    let cash = game.players[owner].cash;
    let cost = game.catalog[type_index].cost;
    let building = game.build(builder, type_index, position).map_err(|error| {
        format!(
            "{error}; {} site {position:?}, builder {builder}: {}",
            game.catalog[type_index].name,
            generals_sim::skirmish::diagnostics::movement_details(game, builder)
        )
    })?;
    require(
        cash.checked_sub(cost) == Some(game.players[owner].cash),
        "construction did not deduct exactly the installed BuildCost",
    )?;
    require(
        !game.entities[building].complete(),
        "foundation completed instantly",
    )?;
    advance_until(
        game,
        10 * TICKS_PER_MINUTE,
        "construction completion",
        |game| game.entities[building].complete(),
    ).map_err(|error| {
        let foundation = &game.entities[building];
        let order = generals_sim::skirmish::diagnostics::movement_details(game, builder);
        format!(
            "{error}; {} foundation {building} owner {} at {position:?}, health {}, progress {}, radius {}; builder {builder} owner {} {order}",
            game.catalog[type_index].name,
            foundation.owner,
            foundation.health,
            foundation.build_progress,
            game.catalog[type_index].geometry.major,
            game.entities[builder].owner,
        )
    })?;
    println!(
        "  {} complete; cash {}",
        game.catalog[type_index].name, game.players[owner].cash
    );
    Ok(building)
}

/// Selects the same reachable paid-verification site without advancing simulation time.
/// # Errors
/// Rejects missing completed owners, supply sources or legal reachable construction sites.
pub fn construction_position(
    game: &Skirmish,
    builder: usize,
    type_index: usize,
    owner: usize,
) -> Result<[f32; 2], Box<dyn Error>> {
    let kind = game.catalog[type_index].kind;
    let center = friendly_entity(game, Kind::CommandCenter, owner)?;
    let base = game.entities[center].position;
    let wanted = if kind == Kind::SupplyCenter {
        [
            game.supply_position(home_supply(game, owner)?)[0] + 80.0,
            game.supply_position(home_supply(game, owner)?)[1],
        ]
    } else {
        [base[0] - 160.0, base[1] + 120.0]
    };
    construction_site(game, builder, type_index, wanted)
        .ok_or_else(|| "no legal construction site near the base".into())
}

pub fn scout_construction_site(
    game: &mut Skirmish,
    builder: usize,
    type_index: usize,
    position: [f32; 2],
) -> Result<(), Box<dyn Error>> {
    let angle = game.catalog[type_index].placement_angle;
    if game.can_build_facing(builder, type_index, position, angle) {
        return Ok(());
    }
    // Reveal the original site through a real movement order before paying.
    game.move_to(builder, position)?;
    advance_until(
        game,
        7200,
        "builder scouting the original construction site",
        |game| game.can_build_facing(builder, type_index, position, angle),
    )
}

fn construction_site(
    game: &Skirmish,
    builder: usize,
    type_index: usize,
    wanted: [f32; 2],
) -> Option<[f32; 2]> {
    let nearest = game.placement_near(type_index, wanted)?;
    if game.build_site_reachable(builder, nearest) {
        return Some(nearest);
    }
    // Footprint clearance alone can choose a destroyed building's rubble.
    // Use the ordinary placement preview's reachability before paying for a
    // different plot; keep the original terrain and destroyed objects intact.
    let extent = game
        .visibility_dimensions()
        .map(|cells| f32::from(cells) * game.visibility_cell_size());
    let axis = |limit| {
        std::iter::successors(Some(generals_sim::skirmish::CELL_SIZE * 0.5), |point| {
            Some(point + generals_sim::skirmish::CELL_SIZE)
        })
        .take_while(move |&point| point < limit)
    };
    let mut sites: Vec<_> = axis(extent[1])
        .flat_map(|y| axis(extent[0]).map(move |x| [x, y]))
        .collect();
    let distance = |point: [f32; 2]| (point[0] - wanted[0]).hypot(point[1] - wanted[1]);
    sites.sort_by(|&first, &second| distance(first).total_cmp(&distance(second)));
    sites
        .into_iter()
        .find(|&site| game.can_place(type_index, site) && game.build_site_reachable(builder, site))
}

pub fn produce(
    game: &mut Skirmish,
    producer: usize,
    kind: Kind,
    owner: usize,
) -> Result<usize, Box<dyn Error>> {
    let type_index = producer_unit(game, producer, kind)?;
    produce_type(game, producer, type_index, owner)
}

pub fn producer_unit(
    game: &Skirmish,
    producer: usize,
    kind: Kind,
) -> Result<usize, Box<dyn Error>> {
    let actor = game.entities.get(producer).ok_or("missing producer")?;
    let unit = &game.catalog[actor.type_index];
    unit.buildable_objects
        .iter()
        .find_map(|name| {
            let index = game
                .catalog
                .iter()
                .position(|candidate| candidate.kind == kind && &candidate.name == name)?;
            game.prerequisites_met(actor.owner, index).then_some(index)
        })
        .ok_or_else(|| {
            format!(
                "{} has no available production command for the requested unit kind",
                unit.name
            )
            .into()
        })
}

pub fn produce_type(
    game: &mut Skirmish,
    producer: usize,
    type_index: usize,
    owner: usize,
) -> Result<usize, Box<dyn Error>> {
    let first_new = game.entities.len();
    let cash = game.players[owner].cash;
    let cost = game.catalog[type_index].cost;
    game.train(producer, type_index)?;
    require(
        cash.checked_sub(cost) == Some(game.players[owner].cash),
        "production did not deduct exactly the installed BuildCost",
    )?;
    advance_until(game, 10 * TICKS_PER_MINUTE, "unit production", |game| {
        game.entities[first_new..].iter().any(|entity| {
            entity.owner == owner
                && game.catalog[entity.type_index].equivalent_to(&game.catalog[type_index])
                && entity.complete()
        })
    })?;
    let spawned = game.entities[first_new..]
        .iter()
        .position(|entity| {
            entity.owner == owner
                && game.catalog[entity.type_index].equivalent_to(&game.catalog[type_index])
                && entity.complete()
        })
        .map(|offset| first_new + offset)
        .ok_or("completed production did not spawn its unit")?;
    let creation_position = game.entities[spawned].position;
    advance_until(
        game,
        TICKS_PER_MINUTE,
        "unit leaving its producer",
        |game| {
            game.entities[spawned].complete()
                && !matches!(game.entities[spawned].ai.order, Order::Exit(_))
        },
    )
    .map_err(|error| {
        production_exit_error(game, producer, spawned, creation_position, error.as_ref())
    })?;
    Ok(spawned)
}

fn production_exit_error(
    game: &Skirmish,
    producer: usize,
    spawned: usize,
    created_at: [f32; 2],
    error: &dyn Error,
) -> Box<dyn Error> {
    let actor = &game.entities[spawned];
    let unit = &game.catalog[actor.type_index];
    let factory = &game.entities[producer];
    let goal = match actor.ai.order {
        Order::Exit(goal) => Some(goal),
        _ => None,
    };
    let nearby: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter(|(id, other)| {
            *id != spawned
                && other.alive()
                && squared_distance(other.position, actor.position) < 150.0 * 150.0
        })
        .map(|(id, other)| {
            format!(
                "{id}:{} owner={} at={:?} facing={} moving={} health={}",
                game.catalog[other.type_index].name,
                other.owner,
                other.position,
                other.facing,
                other.moving(),
                other.health
            )
        })
        .collect();
    let movement = generals_sim::skirmish::diagnostics::movement_details(game, spawned);
    format!("{error}; produced {spawned}:{} owner={} created={created_at:?} current={:?} exit_goal={goal:?} moving={} facing={} health={} complete={}; {movement}; producer {producer}:{} at={:?} facing={} rally={:?} native_path={:?}; nearby={nearby:?}", unit.name, actor.owner, actor.position, actor.moving(), actor.facing, actor.health, actor.complete(), game.catalog[factory.type_index].name, factory.position, factory.facing, factory.rally_point, game.producer_rally_path(producer)).into()
}

fn verify_income(
    game: &mut Skirmish,
    collector: usize,
    value: u32,
    owner: usize,
) -> Result<(), Box<dyn Error>> {
    let before_cash = game.players[owner].cash;
    let before_boxes = game.supplies[home_supply(game, owner)?].boxes;
    let capacity = game.catalog[game.entities[collector].type_index]
        .supply
        .capacity;
    game.harvest(collector, home_supply(game, owner)?)?;
    advance_until(
        game,
        10 * TICKS_PER_MINUTE,
        "collector picking up supplies",
        |game| game.entities[collector].cargo > 0,
    )?;
    require(
        game.players[owner].cash == before_cash,
        "cash appeared before supply dropoff",
    )?;
    advance_until(
        game,
        10 * TICKS_PER_MINUTE,
        "collector depositing supplies",
        |game| game.players[owner].cash > before_cash,
    )?;
    require(
        capacity
            .checked_mul(value)
            .and_then(|income| before_cash.checked_add(income))
            == Some(game.players[owner].cash)
            && game.entities[collector].cargo == 0
            && before_boxes.checked_sub(capacity)
                == Some(game.supplies[home_supply(game, owner)?].boxes),
        "delivered supply boxes, cargo and cash do not balance",
    )?;
    println!(
        "  supply deposit: {capacity} boxes, ${}",
        game.players[owner].cash - before_cash
    );
    Ok(())
}

fn verify_movement(game: &mut Skirmish, tank: usize) -> Result<(), Box<dyn Error>> {
    let start = game.entities[tank].position;
    let mut failures = Vec::new();
    // Rotated neighboring buildings can contain all four cardinal samples while
    // a diagonal destination remains reachable from the native production exit.
    // Try surrounding destinations through ordinary commands; never relocate units.
    for delta in [
        [60.0, 0.0],
        [0.0, 60.0],
        [-60.0, 0.0],
        [0.0, -60.0],
        [60.0, 60.0],
        [60.0, -60.0],
        [-60.0, 60.0],
        [-60.0, -60.0],
    ] {
        let destination = [start[0] + delta[0], start[1] + delta[1]];
        match game.move_to(tank, destination) {
            Ok(()) => {
                if let Order::Move(goal) = game.entities[tank].ai.order
                    && squared_distance(start, goal) <= 100.0
                {
                    failures.push(format!("{destination:?}: clipped to nearby {goal:?}"));
                    continue;
                }
                // Admission only queues a native path. Observe actual travel;
                // an idle/no-op path completion must try another direction.
                for _ in 0..2 * TICKS_PER_MINUTE {
                    game.step();
                    let actor = &game.entities[tank];
                    require(actor.alive(), "produced unit died during movement check")?;
                    if squared_distance(start, actor.position) > 100.0 {
                        return Ok(());
                    }
                    if matches!(actor.ai.order, Order::Idle) && !actor.moving() {
                        break;
                    }
                }
                failures.push(format!(
                    "{destination:?}: {}",
                    generals_sim::skirmish::diagnostics::movement_details(game, tank)
                ));
            }
            Err(error) => failures.push(format!("{destination:?}: {error}")),
        }
    }
    Err(format!(
        "Produced {} has no reachable nearby destination; started at {start:?}, now {:?}, health {}; attempted {failures:?}",
        game.catalog[game.entities[tank].type_index].name,
        game.entities[tank].position,
        game.entities[tank].health,
    ).into())
}

pub fn verify_combat_and_victory(
    game: &mut Skirmish,
    barracks: usize,
    factory: usize,
    owner: usize,
) -> Result<bool, Box<dyn Error>> {
    let tank_type = producer_unit(game, factory, Kind::Tank)?;
    let infantry_type = producer_unit(game, barracks, Kind::Infantry)?;
    let mut fired = false;
    let mut enemy_fired = false;
    let mut traveled = false;
    let mut damaged = false;
    let mut attack_started = false;
    for frame in 0..30 * TICKS_PER_MINUTE {
        if frame % 30 == 0 {
            if game.entities[factory].complete()
                && game.entities[factory].queue.len() < 2
                && game.players[owner].cash >= game.catalog[tank_type].cost
            {
                game.train(factory, tank_type)?;
            }
            if game.entities[barracks].complete()
                && game.entities[barracks].queue.len() < 2
                && game.players[owner].cash >= game.catalog[infantry_type].cost
            {
                game.train(barracks, infantry_type)?;
            }
            let tanks = game
                .entities
                .iter()
                .filter(|entity| {
                    entity.owner == owner && entity.complete() && entity.type_index == tank_type
                })
                .count();
            attack_started |= tanks >= 4;
            if attack_started {
                order_army(game, owner)?;
            }
        }
        game.step();
        fired |= game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.owner == owner);
        enemy_fired |= game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.owner != owner);
        traveled |= game
            .projectiles
            .iter()
            .any(|projectile| projectile.owner == owner);
        damaged |= game.entities.iter().any(|entity| {
            entity.owner != owner
                && entity.build_progress >= 1.0
                && entity.health < game.catalog[entity.type_index].initial_health
        });
        if let Some(winner) = game.outcome {
            require(winner == owner, "commanded player army lost the match")?;
            require(
                fired && traveled && damaged,
                "victory did not exercise firing, projectile travel and damage",
            )?;
            return Ok(enemy_fired);
        }
    }
    Err("player army did not finish the match within 30 simulated minutes".into())
}

fn order_army(game: &mut Skirmish, owner: usize) -> Result<(), Box<dyn Error>> {
    for actor in 0..game.entities.len() {
        let entity = &game.entities[actor];
        let kind = game.catalog[entity.type_index].kind;
        if entity.owner != owner
            || !entity.complete()
            || !entity.selectable(&game.catalog[entity.type_index])
            || entity.contained_by().is_some()
            || !matches!(kind, Kind::Tank | Kind::Infantry)
            || !game.weapon_available(actor, 0)
        {
            continue;
        }
        let target = game
            .entities
            .iter()
            .enumerate()
            .filter(|&(target_id, target)| {
                let target_kind = game.catalog[target.type_index].kind;
                game.enemies(owner, target.owner)
                    && game.player_attack_relationship(actor, target_id)
                    && target.alive()
                    && !target.masked()
                    && (target_kind.is_building() || target_kind == kind)
                    && game.weapon_target_reachable(actor, target_id)
            })
            .min_by(|(_, left), (_, right)| {
                squared_distance(entity.position, left.position)
                    .total_cmp(&squared_distance(entity.position, right.position))
            })
            .map(|(id, _)| id);
        if let Some(target) = target
            && !matches!(entity.ai.order, Order::Attack(current) if current == target)
        {
            game.attack(actor, target).map_err(|error| {
                format!(
                    "Army attack: actor {actor} {} owner {owner} at {:?}, target {target} {} owner {} at {:?}: {error}",
                    game.catalog[game.entities[actor].type_index].name,
                    game.entities[actor].position,
                    game.catalog[game.entities[target].type_index].name,
                    game.entities[target].owner,
                    game.entities[target].position,
                )
            })?;
        }
    }
    Ok(())
}

pub fn advance_until(
    game: &mut Skirmish,
    limit: u32,
    event: &str,
    mut complete: impl FnMut(&mut Skirmish) -> bool,
) -> Result<(), Box<dyn Error>> {
    for _ in 0..limit {
        if complete(game) {
            return Ok(());
        }
        if game.ended_at.is_some() {
            return Err(format!("match ended before {event}").into());
        }
        game.step();
    }
    if complete(game) {
        return Ok(());
    }
    Err(format!("timed out waiting for {event} at tick {}", game.tick).into())
}

fn squared_distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    let x = left[0] - right[0];
    let y = left[1] - right[1];
    x.mul_add(x, y * y)
}

pub fn require(condition: bool, message: &str) -> Result<(), Box<dyn Error>> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn home_supply(game: &Skirmish, owner: usize) -> Result<usize, Box<dyn Error>> {
    let center = friendly_entity(game, Kind::CommandCenter, owner)?;
    let base = game.entities[center].position;
    game.supplies
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let distance = |point: [f32; 2]| (point[0] - base[0]).hypot(point[1] - base[1]);
            distance(game.entities[a.object].position)
                .total_cmp(&distance(game.entities[b.object].position))
        })
        .map(|(index, _)| index)
        .ok_or_else(|| "scenario has no supplies".into())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_gla_opponent_keeps_consecutive_factory_rally_and_complete_command_loop() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let file =
            generals_assets::content::scan(assets.read("data/ini/gamedata.ini").unwrap()).unwrap();
        let global =
            crate::skirmish_data::definition(std::slice::from_ref(&file), "GameData", "").unwrap();
        let supply_value =
            crate::skirmish_data::unsigned(&global.lines, 0, "ValuePerSupplyBox").unwrap();
        let game = &mut battle.simulation;
        game.reset(super::Faction::China, super::Faction::Gla)
            .unwrap();
        super::verify_faction(game, supply_value, 1).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_base_factions_pass_the_complete_command_and_asset_scenarios() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
