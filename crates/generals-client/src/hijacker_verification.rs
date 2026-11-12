//! Paid base GLA Hijacker: science/Palace, real contact and retained original driver.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
};
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
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    // The promotion fight prepared the victim's original powered economy.
    let enemy_barracks = construct(game, enemy, Kind::Barracks, 1)?;
    let factory = construct(game, enemy, Kind::Factory, 1)?;
    let host_kind = template(game, "AmericaVehicleHumvee")?;
    let host = produce_type(game, factory, host_kind, 1)?;
    let defender = template(game, "AmericaInfantryMissileDefender")?;
    let attacker = produce_type(game, enemy_barracks, defender, 1)?;
    // Keep the later destruction weapon at its own base while the unarmed
    // Hijacker loses stationary stealth during its actual enter approach.
    let enemy_command = friendly_entity(game, Kind::CommandCenter, 1)?;
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
    let health = game.entities[setup.host].health.to_bits();
    let losses = game.players[0].statistics.units_lost;
    game.hijack(setup.actor, setup.host)?;
    advance_until(game, 3000, "original Hijacker physical contact", |game| {
        game.entities[setup.host].hijacked()
    })?;
    require(
        game.entities[setup.host].owner == 0
            && game.entities[setup.host].health.to_bits() == health,
        "Hijacker capture changed host health or failed ownership",
    )?;
    require(
        game.entities[setup.actor].alive()
            && game.entities[setup.actor].masked()
            && game.entities[setup.actor].driving_vehicle() == Some(setup.host),
        "EjectPilotDie host consumed rather than stored living Hijacker",
    )?;
    require(
        game.players[0].statistics.units_lost == losses,
        "Hijacking granted a driver death loss",
    )?;
    game.attack(setup.attacker, setup.host)?;
    advance_until(
        game,
        18_000,
        "paid enemy weapon destroys hijacked host",
        |game| !game.entities[setup.host].alive(),
    )?;
    require(
        game.entities[setup.actor].masked(),
        "Hidden driver ejected before final host destruction",
    )?;
    advance_until(
        game,
        18_000,
        "original host destruction releases driver",
        |game| !game.entities[setup.actor].masked(),
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

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut game = load_game(root)?;
    verify(&mut game)
}

fn load_game(root: &Path) -> Result<Skirmish, Box<dyn Error>> {
    let mut game = crate::infiltrator_verification::load_game(root)?;
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let map = generals_assets::map::MapData::parse(&assets.read(crate::playtest::MAP_NAME)?.bytes)?;
    let mut data = crate::skirmish_data::load(&assets, &map.objects)?;
    let mut animations = crate::model_animation::load(&assets, &mut data.models, &game.catalog)?;
    animations.bind_logical_bones(&mut game.catalog);
    Ok(game)
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_hijacker_converts_and_returns_after_original_host_destruction() {
        let root = generals_assets::game_root().unwrap();
        super::run(&root).unwrap();
    }
}
