//! Paid base GLA Hijacker: science/Palace, real contact and retained original driver.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
};
use bevy::math::Vec2;
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::{error::Error, path::Path};

pub struct Setup {
    pub actor: usize,
    pub host: usize,
    pub attacker: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let kind = template(game, "GLAInfantryHijacker")?;
    let cash = game.players[0].cash;
    require(
        game.train(barracks, kind).is_err() && game.players[0].cash == cash,
        "Hijacker bypassed Palace/science prerequisite",
    )?;
    let gla_factory = construct(game, worker, Kind::Factory, 0)?;
    let palace = template(game, "GLAPalace")?;
    construct_type(game, worker, palace, 0)?;
    let cash = game.players[0].cash;
    require(
        game.train(barracks, kind).is_err() && game.players[0].cash == cash,
        "Hijacker bypassed original SCIENCE_Hijacker",
    )?;
    require(
        !game.can_purchase_science(0, "SCIENCE_Hijacker"),
        "Hijacker science bypassed original rank-three prerequisite",
    )?;
    earn_rank_three(game, worker, gla_factory, barracks)?;
    let points = game.players[0].science_points;
    game.purchase_science(0, "SCIENCE_Hijacker")?;
    require(
        game.players[0].science_points + 1 == points,
        "Hijacker science did not pay its original promotion point",
    )?;
    require(
        game.catalog[kind].cost == 400
            && game.catalog[kind].build_seconds.to_bits() == 10.0_f32.to_bits(),
        "Hijacker production differs from original cost/time",
    )?;
    let actor = produce_type(game, barracks, kind, 0)?;
    let enemy_command = friendly_entity(game, Kind::CommandCenter, 1)
        .map_err(|error| format!("USA Command Center after promotion combat: {error}"))?;
    let enemy = crate::progression_verification::construction::builder(game)
        .map_err(|error| format!("USA Dozer after promotion combat: {error}"))?;
    // The promotion fight prepared the victim's original powered economy.
    let enemy_barracks = construct(game, enemy, Kind::Barracks, 1)?;
    let factory = construct(game, enemy, Kind::Factory, 1)?;
    let host_kind = template(game, "AmericaVehicleHumvee")?;
    let host = produce_type(game, factory, host_kind, 1)?;
    let defender = template(game, "AmericaInfantryMissileDefender")?;
    let attacker = produce_type(game, enemy_barracks, defender, 1)?;
    // Keep the later destruction weapon at its own base while the unarmed
    // Hijacker loses stationary stealth during its actual enter approach.
    let enemy_base = game.entities[enemy_command].position;
    approach(game, attacker, enemy_base, 150.0)?;
    let point = stage_driver(game, actor, gla_factory)?;
    // Approach the concealed driver without touching it. The armed Humvee can
    // legitimately kill an exposed driver crossing a longer firing interval.
    approach(game, host, point, 35.0)?;
    let attacker_position = game.entities[attacker].position;
    let clearance = game.catalog[defender].weapon.range
        + game.catalog[defender].geometry_bounds.sphere_radius
        + game.catalog[kind].geometry_bounds.sphere_radius;
    require(
        (attacker_position[0] - point[0]).hypot(attacker_position[1] - point[1]) > clearance,
        "Paid destruction attacker remained within the Hijacker approach scene",
    )?;
    advance_until(
        game,
        60,
        "Hijacker's original stationary concealment",
        |game| game.entities[actor].stealthed(),
    )?;
    require(
        game.entities[actor].alive() && game.can_hijack(actor, host),
        "Paid Hijacker lost its cloak or eligible empty enemy vehicle",
    )?;
    require(
        game.catalog[host_kind].has_eject_pilot() && game.entities[host].veterancy == 0,
        "Regular Humvee must retain driver by EjectPilotDie presence",
    )?;
    Ok(Setup {
        actor,
        host,
        attacker,
    })
}

fn stage_driver(
    game: &mut Skirmish,
    actor: usize,
    factory: usize,
) -> Result<[f32; 2], Box<dyn Error>> {
    let start = game.entities[actor].position;
    let footprint = game.entities[factory].type_index;
    let clear = |point: [f32; 2]| {
        game.entities
            .iter()
            .filter(|entity| !entity.masked())
            .all(|entity| {
                let unit = &game.catalog[entity.type_index];
                let clearance = if unit.kind.is_building() {
                    unit.geometry_bounds.sphere_radius + 100.0
                } else if unit.weapon.damage > 0.0 {
                    unit.weapon.range + 100.0
                } else {
                    0.0
                };
                (point[0] - entity.position[0]).hypot(point[1] - entity.position[1]) > clearance
            })
    };
    let point = [[300.0, 0.0], [-300.0, 0.0], [0.0, 300.0], [0.0, -300.0]]
        .into_iter()
        .filter_map(|offset| {
            game.placement_near(footprint, [start[0] + offset[0], start[1] + offset[1]])
        })
        .find(|&point| clear(point))
        .ok_or("no clear original terrain for the paid Hijacker contact scene")?;
    game.move_to(actor, point)?;
    advance_until(
        game,
        3600,
        "paid Hijacker leaves its barracks exit",
        |game| {
            let driver = &game.entities[actor];
            driver.alive()
                && !driver.moving()
                && matches!(driver.ai.order, generals_sim::skirmish::Order::Idle)
                && (driver.position[0] - point[0]).hypot(driver.position[1] - point[1]) < 20.0
        },
    )?;
    advance_until(game, 60, "staged Hijacker stationary concealment", |game| {
        game.entities[actor].stealthed()
    })?;
    Ok(game.entities[actor].position)
}

fn earn_rank_three(
    game: &mut Skirmish,
    worker: usize,
    factory: usize,
    barracks: usize,
) -> Result<(), Box<dyn Error>> {
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy, Kind::PowerPlant, 1)?;
    construct(game, enemy, Kind::SupplyCenter, 1)?;
    let supply = game
        .supplies
        .iter()
        .position(|supply| supply.boxes > 0)
        .ok_or("rank exercise has no original supplies")?;
    game.harvest(worker, supply)?;
    let market = template(game, "GLABlackMarket")?;
    advance_until(game, 18_000, "supplies funding the Black Market", |game| {
        game.players[0].cash >= game.catalog[market].cost
    })?;
    let market = construct_type(game, worker, market, 0)?;
    game.harvest(worker, supply)?;
    crate::strategy_verification::research(game, market, "Upgrade_GLAJunkRepair")?;
    let scorpion = template(game, "GLATankScorpion")?;
    let mut army = Vec::new();
    for _ in 0..4 {
        advance_until(
            game,
            18_000,
            "original worker pays promotion army",
            |game| game.players[0].cash >= game.catalog[scorpion].cost,
        )?;
        army.push(produce_type(game, factory, scorpion, 0)?);
    }
    for &actor in &army {
        game.stop(actor)?;
    }
    advance_until(game, 1800, "promotion army settles", |game| {
        army.iter().all(|&id| !game.entities[id].moving())
    })?;
    let retreat: Vec<_> = army.iter().map(|&id| game.entities[id].position).collect();
    for _ in 0..12 {
        if game.players[0].rank >= 3 {
            break;
        }
        advance_until(
            game,
            18_000,
            "Junk Repair restores the returning Scorpions",
            |game| {
                army.iter().all(|&id| {
                    let tank = &game.entities[id];
                    tank.alive() && tank.health >= tank.max_health
                })
            },
        )?;
        let target = crate::progression_verification::target_supply_center(game)?;
        let before_attack = game.entities.len();
        crate::progression_verification::destroy_target(game, &army, target)?;
        defeat_supply_defenders(game, &army, before_attack)?;
        crate::progression_verification::retreat_army(game, &army, &retreat, target)?;
    }
    require(
        game.players[0].rank >= 3 && game.players[0].has_science("SCIENCE_Rank3"),
        "ordinary paid combat did not earn Hijacker's rank-three science",
    )?;
    park_promotion_army(game, &army, barracks)?;
    Ok(())
}

fn defeat_supply_defenders(
    game: &mut Skirmish,
    army: &[usize],
    first: usize,
) -> Result<(), Box<dyn Error>> {
    // The American Supply Center releases three Rangers on destruction.
    // Fight the actual defenders before retreating and rebuilding the next target.
    let rangers: Vec<_> = (first..game.entities.len())
        .filter(|&id| {
            let actor = &game.entities[id];
            actor.owner == 1 && game.catalog[actor.type_index].name == "AmericaInfantryRanger"
        })
        .collect();
    for ranger in rangers {
        advance_until(
            game,
            300,
            "ejected Ranger entering ground weapon range",
            |game| {
                !game.entities[ranger].alive()
                    || army
                        .iter()
                        .all(|&id| game.player_attack_target_reachable(id, ranger))
            },
        )?;
        if !game.entities[ranger].alive() {
            continue;
        }
        for &id in army {
            game.attack(id, ranger)?;
        }
        advance_until(
            game,
            1800,
            "Scorpions defeating a defending Ranger",
            |game| !game.entities[ranger].alive(),
        )?;
    }
    Ok(())
}

fn park_promotion_army(
    game: &mut Skirmish,
    army: &[usize],
    barracks: usize,
) -> Result<(), Box<dyn Error>> {
    let command = friendly_entity(game, Kind::CommandCenter, 0)?;
    let center = game.entities[command].position;
    let scene = game.entities[barracks].position;
    // Keep the real promotion tanks from attacking the later vehicle target.
    // Select an actual reachable goal near their own base, then move normally.
    let mut goals: Vec<_> = (0_u8..8)
        .map(|index| {
            let angle = f32::from(index) * (std::f32::consts::TAU / 8.0);
            let offset = [450.0 * angle.cos(), 450.0 * angle.sin()];
            [center[0] + offset[0], center[1] + offset[1]]
        })
        .collect();
    goals.sort_by(|left, right| {
        (right[0] - scene[0])
            .hypot(right[1] - scene[1])
            .total_cmp(&(left[0] - scene[0]).hypot(left[1] - scene[1]))
    });
    for goal in goals {
        if game.group_move_to(army, goal, false).is_err() {
            continue;
        }
        advance_until(
            game,
            18_000,
            "promotion army leaves Hijacker scene",
            |game| {
                army.iter().all(|&id| {
                    let actor = &game.entities[id];
                    actor.alive()
                        && !actor.moving()
                        && matches!(actor.ai.order, generals_sim::skirmish::Order::Idle)
                        && (actor.position[0] - scene[0]).hypot(actor.position[1] - scene[1])
                            > 300.0
                })
            },
        )?;
        return Ok(());
    }
    Err("No original reachable promotion-army parking goal".into())
}

fn approach(
    game: &mut Skirmish,
    actor: usize,
    point: [f32; 2],
    gap: f32,
) -> Result<(), Box<dyn Error>> {
    game.move_to(actor, point)?;
    advance_until(game, 18_000, "paid Hijacker target approach", |game| {
        let current = game.entities[actor].position;
        (current[0] - point[0]).hypot(current[1] - point[1]) < gap
    })
    .map_err(|error| -> Box<dyn Error> {
        let unit = &game.entities[actor];
        let goal = match unit.ai.order {
            generals_sim::skirmish::Order::Move(position) => Some(position),
            _ => None,
        };
        format!(
            "{error}; object={actor} health={} position={:?} requested={point:?} gap={gap} move_goal={goal:?} moving={} last_attacker={:?}",
            unit.health,
            unit.position,
            unit.moving(),
            unit.last_attacker()
        ).into()
    })?;
    game.stop(actor)?;
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    capture_host(game, setup.actor, setup.host)?;
    verify_ground_release(game, &setup)
}

fn capture_host(game: &mut Skirmish, actor: usize, host: usize) -> Result<(), Box<dyn Error>> {
    let health = game.entities[host].health.to_bits();
    let losses = game.players[0].statistics.units_lost;
    game.hijack(actor, host)?;
    advance_until(game, 3000, "original Hijacker physical contact", |game| {
        game.entities[host].hijacked()
    })?;
    require(
        game.entities[host].owner == 0 && game.entities[host].health.to_bits() == health,
        "Hijacker capture changed host health or failed ownership",
    )?;
    require(
        game.entities[actor].alive()
            && game.entities[actor].masked()
            && game.entities[actor].driving_vehicle() == Some(host)
            && game.players[0].statistics.units_lost == losses,
        "EjectPilotDie host did not store the living Hijacker without death credit",
    )
}

fn verify_ground_release(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let losses = game.players[0].statistics.units_lost;
    game.attack(setup.attacker, setup.host)?;
    let mut host_smoke = false;
    let mut hull_smoke = false;
    advance_until(
        game,
        18_000,
        "paid enemy weapon destroys hijacked host",
        |game| {
            host_smoke |= emitted_damage_smoke(game, setup.host);
            hull_smoke |= emitted_hull_smoke(game);
            !game.entities[setup.host].alive()
        },
    )?;
    require(host_smoke, "original damaged Humvee smoke never started")?;
    require(
        game.entities[setup.actor].masked(),
        "Hidden driver ejected before final host destruction",
    )?;
    advance_until(
        game,
        18_000,
        "original host destruction releases driver",
        |game| {
            hull_smoke |= emitted_hull_smoke(game);
            !game.entities[setup.actor].masked()
        },
    )
    .map_err(|error| -> Box<dyn Error> {
        let host = &game.entities[setup.host];
        let driver = &game.entities[setup.actor];
        let delays = game.catalog[host.type_index].death_lifecycle.as_ref().map(|rules| {
            rules.slow.iter().map(|slow| slow.destruction_delay).collect::<Vec<_>>()
        });
        format!(
            "{error}; host health={} masked={} contained={:?} death_condition={:?} source_delays={delays:?}; driver health={} masked={} driving={:?}",
            host.health,
            host.masked(),
            host.contained_by(),
            host.death_condition(),
            driver.health,
            driver.masked(),
            driver.driving_vehicle()
        ).into()
    })?;
    require(hull_smoke, "original Humvee rubble smoke never started")?;
    require(
        game.entities[setup.actor].alive()
            && game.entities[setup.actor].driving_vehicle().is_none()
            && game.players[0].statistics.units_lost == losses + 1,
        "Host destruction lost the driver or credited more than the host death",
    )?;
    println!(
        "Installed Hijacker: paid Palace/science/cost/time, stationary cloak, explicit physical ownership conversion, regular-host stored driver and final-destruction release passed"
    );
    Ok(())
}

fn emitted_damage_smoke(game: &Skirmish, object: usize) -> bool {
    game.presentation
        .structure_events
        .iter()
        .any(|event| event.object == object && is_damage_smoke(&event.action))
}

fn emitted_hull_smoke(game: &Skirmish) -> bool {
    game.presentation.structure_events.iter().any(|event| {
        game.catalog[game.entities[event.object].type_index].name == "AmericaVehicleHumveeDeadHull"
            && is_damage_smoke(&event.action)
    })
}

fn is_damage_smoke(action: &generals_sim::skirmish::structure_death::VisualAction) -> bool {
    matches!(
        action,
        generals_sim::skirmish::structure_death::VisualAction::TransitionParticle { template, .. }
            if template == "SmokeSmallContinuous01"
    )
}

pub struct Airborne {
    pub driver: usize,
    pub chinook: usize,
    pub host: usize,
    pub defender: usize,
    pub losses: u32,
}

pub fn prepare_airborne(game: &mut Skirmish) -> Result<Airborne, Box<dyn Error>> {
    let setup = prepare(game)?;
    capture_host(game, setup.actor, setup.host)?;
    verify_ground_release(game, &setup)?;
    stage_airborne(game, &setup)
}

fn stage_airborne(game: &mut Skirmish, setup: &Setup) -> Result<Airborne, Box<dyn Error>> {
    let barracks = friendly_entity(game, Kind::Barracks, 0)?;
    let gla_factory = friendly_entity(game, Kind::Factory, 0)?;
    let enemy_barracks = friendly_entity(game, Kind::Barracks, 1)?;
    let enemy_factory = friendly_entity(game, Kind::Factory, 1)?;
    let enemy_base = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    if game.entities[setup.attacker].alive() {
        approach(game, setup.attacker, enemy_base, 150.0)?;
    }

    crate::strategy_verification::research(game, barracks, "Upgrade_InfantryCaptureBuilding")?;
    let rebel_kind = template(game, "GLAInfantryRebel")?;
    let rebel = produce_type(game, barracks, rebel_kind, 0)?;
    let supply = crate::progression_verification::target_supply_center(game)?;
    let supply_point = game.entities[supply].position;
    approach(game, rebel, supply_point, 90.0)?;
    game.capture(rebel, supply)?;
    advance_until(
        game,
        3600,
        "paid Rebel capturing original USA Supply Center",
        |game| game.entities[supply].owner == 0,
    )?;
    let host_kind = game.entities[setup.host].type_index;
    let host = produce_type(game, enemy_factory, host_kind, 1)?;
    let point = stage_driver(game, setup.actor, gla_factory)?;
    approach(game, host, point, 35.0)?;
    capture_host(game, setup.actor, host)?;

    let chinook_kind = template(game, "AmericaVehicleChinook")?;
    if game.entities[setup.attacker].alive() {
        park_ground_attacker(
            game,
            setup.attacker,
            enemy_base,
            [supply_point, point],
            enemy_barracks,
            chinook_kind,
        )?;
    }
    advance_until(game, 18_000, "normal income funding paid Chinook", |game| {
        game.players[0].cash >= game.catalog[chinook_kind].cost
    })?;
    let chinook = produce_type(game, supply, chinook_kind, 0)?;
    // The Chinook's route from the captured Supply Center crosses the USA base.
    // Purchase its assigned defender once the cargo is safely airborne.
    approach(game, chinook, point, 20.0)?;
    game.enter(host, chinook)?;
    advance_until(
        game,
        6000,
        "Chinook boarding the captured Humvee and taking off",
        |game| {
            game.entities[host].contained_by() == Some(chinook)
                && game.height_above_ground(host) > 80.0
        },
    )?;
    require(
        game.entities[setup.actor].driving_vehicle() == Some(host)
            && game.entities[setup.actor].masked(),
        "Airborne cargo transport lost the hidden Hijacker before combat",
    )?;
    require_pristine_cargo(game, chinook, "after boarding and takeoff")?;
    settle_airborne_cargo(game, chinook, host, point)?;
    let defender_kind = template(game, "AmericaInfantryMissileDefender")?;
    let defender = produce_type(game, enemy_barracks, defender_kind, 1)?;
    let transport = &game.entities[chinook];
    require(
        matches!(transport.ai.order, generals_sim::skirmish::Order::Idle)
            && !transport.moving()
            && Vec2::from_array(transport.position).distance(Vec2::from_array(point)) < 20.0,
        &format!(
            "loaded Chinook left the rendezvous during defender production; {} {}",
            transport.chinook_diagnostic(),
            generals_sim::skirmish::diagnostics::movement_details(game, chinook)
        ),
    )?;
    stage_air_defender(game, defender, chinook)?;
    require_pristine_cargo(game, chinook, "after the defender's approach")?;
    Ok(Airborne {
        driver: setup.actor,
        chinook,
        host,
        defender,
        losses: game.players[0].statistics.units_lost,
    })
}

fn settle_airborne_cargo(
    game: &mut Skirmish,
    chinook: usize,
    host: usize,
    point: [f32; 2],
) -> Result<(), Box<dyn Error>> {
    let rendezvous = Vec2::from_array(point);
    // Let the automatic supply regroup move begin. A real player move back
    // from there gives the supply machine a non-idle player order to observe.
    advance_until(
        game,
        1800,
        "loaded Chinook starting its supply regroup",
        |game| {
            let transport = &game.entities[chinook];
            matches!(transport.ai.order, generals_sim::skirmish::Order::Move(_))
                && Vec2::from_array(transport.position).distance(rendezvous) >= 40.0
        },
    )
    .map_err(|error| {
        format!(
            "{error}; {} {}",
            game.entities[chinook].chinook_diagnostic(),
            generals_sim::skirmish::diagnostics::movement_details(game, chinook)
        )
    })?;
    require_pristine_cargo(game, chinook, "after starting the supply regroup")?;
    game.move_to(chinook, point)?;
    let mut previous = Vec2::from_array(game.entities[chinook].position);
    let mut stationary_frames = 0;
    advance_until(
        game,
        1800,
        "loaded Chinook settling over rendezvous",
        |game| {
            let transport = &game.entities[chinook];
            let position = Vec2::from_array(transport.position);
            let settled = transport.alive()
                && matches!(transport.ai.order, generals_sim::skirmish::Order::Idle)
                && !transport.moving()
                && game.height_above_ground(chinook) > 80.0
                && game.entities[host].contained_by() == Some(chinook)
                && position.distance(rendezvous) < 20.0
                && position.distance_squared(previous) < 0.01;
            previous = position;
            stationary_frames = if settled { stationary_frames + 1 } else { 0 };
            stationary_frames >= 5
        },
    )
    .map_err(|error| {
        format!(
            "{error}; {} {}",
            game.entities[chinook].chinook_diagnostic(),
            generals_sim::skirmish::diagnostics::movement_details(game, chinook)
        )
    })?;
    require_pristine_cargo(game, chinook, "after settling over the rendezvous")
}

fn park_ground_attacker(
    game: &mut Skirmish,
    attacker: usize,
    enemy_base: [f32; 2],
    flight: [[f32; 2]; 2],
    barracks: usize,
    chinook_kind: usize,
) -> Result<(), Box<dyn Error>> {
    let [supply, rendezvous] = flight.map(Vec2::from_array);
    let route = rendezvous - supply;
    let direction = route.normalize_or_zero();
    let clearance = |point: [f32; 2]| {
        let offset = Vec2::from_array(point) - supply;
        let along_route = offset.dot(direction).clamp(0.0, route.length());
        (offset - direction * along_route).length()
    };
    let defender = &game.catalog[game.entities[attacker].type_index];
    let minimum_clearance = defender_weapon_range(defender)
        + defender.geometry_bounds.sphere_radius
        + game.catalog[chinook_kind].geometry_bounds.sphere_radius
        + 50.0;
    let barracks = game.entities[barracks].type_index;
    // Keep the ground trial's Defender clear of the airlift route.
    let parking = [[400.0, 0.0], [-400.0, 0.0], [0.0, 400.0], [0.0, -400.0]]
        .into_iter()
        .filter_map(|offset| {
            game.placement_near(
                barracks,
                [enemy_base[0] + offset[0], enemy_base[1] + offset[1]],
            )
        })
        .filter(|&point| game.build_site_reachable(attacker, point))
        .max_by(|&left, &right| clearance(left).total_cmp(&clearance(right)))
        .ok_or("no reachable parking position for the ground trial's Missile Defender")?;
    approach(game, attacker, parking, 20.0)?;
    require(
        clearance(game.entities[attacker].position) > minimum_clearance,
        "ground trial's Missile Defender remained near the airlift route",
    )
}

fn defender_weapon_range(unit: &generals_sim::skirmish::UnitType) -> f32 {
    unit.secondary_weapon
        .as_ref()
        .map_or(unit.weapon.range, |weapon| {
            weapon.range.max(unit.weapon.range)
        })
}

fn stage_air_defender(
    game: &mut Skirmish,
    defender: usize,
    chinook: usize,
) -> Result<(), Box<dyn Error>> {
    let missile_defender = &game.entities[defender];
    let unit = &game.catalog[missile_defender.type_index];
    // Include the authored laser-lock weapon's longer range. The combat phase
    // still issues the ordinary attack and fires the standard missiles.
    let range = defender_weapon_range(unit);
    let cargo_position = game.entities[chinook].position;
    approach(game, defender, cargo_position, range + 100.0)?;
    let transport = &game.entities[chinook];
    let missile_defender = &game.entities[defender];
    let distance = (transport.position[0] - missile_defender.position[0])
        .hypot(transport.position[1] - missile_defender.position[1]);
    require(
        missile_defender.alive() && (range + 50.0..=range + 150.0).contains(&distance),
        &format!(
            "airborne trial standoff failed: Chinook moved from {cargo_position:?} to {:?}, {}, {}; Missile Defender {:?} health={}, distance={distance}, longest weapon range={range}",
            transport.position,
            transport.chinook_diagnostic(),
            generals_sim::skirmish::diagnostics::movement_details(game, chinook),
            missile_defender.position,
            missile_defender.health,
        ),
    )
}

fn require_pristine_cargo(
    game: &Skirmish,
    chinook: usize,
    phase: &str,
) -> Result<(), Box<dyn Error>> {
    let transport = &game.entities[chinook];
    let last_attacker = transport.last_attacker().map(|id| {
        let actor = &game.entities[id];
        (
            id,
            game.catalog[actor.type_index].name.as_str(),
            actor.position,
        )
    });
    require(
        transport.alive() && transport.health >= transport.max_health,
        &format!(
            "airborne cargo damaged {phase}: Chinook {:?} health={}/{}, last attacker={last_attacker:?}",
            transport.position, transport.health, transport.max_health
        ),
    )
}

#[cfg(test)]
fn verify_airborne_release(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let airborne = stage_airborne(game, setup)?;
    let Airborne {
        chinook,
        host,
        defender,
        losses,
        ..
    } = airborne;
    game.attack(defender, chinook)?;
    advance_until(
        game,
        2_700,
        "paid Missile Defender destroying loaded Chinook",
        |game| !game.entities[chinook].alive(),
    )?;
    require(
        game.height_above_ground(chinook) > 60.0
            && game.entities[chinook].last_attacker() == Some(defender)
            && !game.entities[host].alive(),
        "Missile Defender did not destroy the loaded Chinook in flight",
    )?;
    advance_until(
        game,
        180,
        "host deletion placing Hijacker in original parachute",
        |game| game.entities[setup.actor].contained_by().is_some(),
    )?;
    let chute = game.entities[setup.actor]
        .contained_by()
        .ok_or("airborne Hijacker lost its parachute")?;
    require(
        !game.entities[host].exists()
            && game.catalog[game.entities[chute].type_index].name == "AmericaParachute"
            && game.entities[chute].owner == 0
            && game.height_above_ground(chute) > 60.0
            && game.entities[setup.actor].driving_vehicle().is_none(),
        "Airborne host deletion failed original player-owned parachute release",
    )?;
    advance_until(game, 600, "airborne Hijacker parachute opening", |game| {
        game.entities[chute].parachute_opened()
    })?;
    advance_until(game, 600, "paid Hijacker landing", |game| {
        game.entities[setup.actor].contained_by().is_none()
    })?;
    let driver = &game.entities[setup.actor];
    require(
        driver.alive()
            && !driver.masked()
            && driver.selectable(&game.catalog[driver.type_index])
            && game.height_above_ground(setup.actor).abs() < 0.1
            && game.players[0].statistics.units_lost == losses + 2,
        "Airborne return did not restore the original living driver or score both vehicle losses",
    )?;
    println!(
        "HIJACKER_AIRBORNE chinook={chinook} host={host} driver={} parachute={chute} opened=true landed=true",
        setup.actor
    );
    Ok(())
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut game = crate::infiltrator_verification::load_game(root)?;
    verify(&mut game)
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_hijacker_converts_and_returns_after_original_host_destruction() {
        let root = generals_assets::game_root().unwrap();
        super::run(&root).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_hijacker_returns_after_airborne_chinook_cargo_death() {
        let root = generals_assets::game_root().unwrap();
        let mut game = crate::infiltrator_verification::load_game(&root).unwrap();
        let setup = super::prepare(&mut game).expect("paid Hijacker combat preparation");
        super::capture_host(&mut game, setup.actor, setup.host).expect("initial Humvee capture");
        super::verify_ground_release(&mut game, &setup).expect("initial ground driver return");
        super::verify_airborne_release(&mut game, &setup).expect("airborne cargo driver return");
    }
}
