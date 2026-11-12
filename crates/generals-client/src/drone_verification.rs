//! Paid installed drone upgrades, native flight, combat and repair acceptance.
use crate::{
    playtest::MAP_NAME,
    skirmish_data,
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::{research, template},
    terrain::Terrain,
    world_data,
};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, ProjectileFxStage, Skirmish, WeaponFxEvent};
use std::{collections::BTreeMap, error::Error, path::Path};

pub struct Setup {
    pub parent: usize,
    pub second: usize,
    pub battle: usize,
    pub hellfire: usize,
    pub enemy: usize,
    pub enemy_factory: usize,
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let map = MapData::parse(&assets.read(MAP_NAME)?.bytes)?;
    let mut data = skirmish_data::load(&assets, &map.objects)?;
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
    animations.verify_drone_art(&data.models, &data.types)?;
    let options = root.join("UserData/Options.ini");
    drop(crate::control_bar::load(&assets, &data.types, &options)?);
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    verify(&mut game)?;
    crate::sentry_verification::verify(&mut game)
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    verify_supply_flight(game)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let parent = produce_type(game, factory, template(game, "AmericaVehicleHumvee")?, 0)?;
    let second = produce_type(game, factory, template(game, "AmericaTankCrusader")?, 0)?;
    let battle = upgrade(
        game,
        parent,
        "Upgrade_AmericaBattleDrone",
        "AmericaVehicleBattleDrone",
    )?;
    let hellfire = upgrade(
        game,
        second,
        "Upgrade_AmericaHellfireDrone",
        "AmericaVehicleHellfireDrone",
    )?;
    require(
        game.train(factory, game.entities[battle].type_index)
            .is_err()
            && game
                .train(factory, game.entities[hellfire].type_index)
                .is_err(),
        "internal drones were exposed as factory products",
    )?;
    let before = [
        game.entities[battle].max_health,
        game.entities[hellfire].max_health,
    ];
    let center_type = template(game, "AmericaStrategyCenter")?;
    let cost = game.catalog[center_type].cost;
    advance_until(
        game,
        18000,
        "supplies funding original Strategy Center",
        |game| game.players[0].cash >= cost,
    )?;
    let center = construct_type(game, worker, center_type, 0)?;
    research(game, center, "Upgrade_AmericaDroneArmor")?;
    require(
        game.entities[battle].max_health.to_bits() == (before[0] + 50.0).to_bits()
            && game.entities[hellfire].max_health.to_bits() == (before[1] + 25.0).to_bits(),
        "Drone Armor lost its separate native health amounts",
    )?;
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, worker, Kind::PowerPlant, 1)?;
    construct(game, worker, Kind::SupplyCenter, 1)?;
    let enemy_factory = construct(game, worker, Kind::Factory, 1)?;
    let enemy = produce_type(
        game,
        enemy_factory,
        template(game, "ChinaTankBattleMaster")?,
        1,
    )?;
    let setup = Setup {
        parent,
        second,
        battle,
        hellfire,
        enemy,
        enemy_factory,
    };
    injure(game, &setup)?;
    Ok(setup)
}

fn verify_supply_flight(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    advance_until(
        game,
        1800,
        "completed supply center releases its Chinook",
        |game| friendly_entity(game, Kind::Harvester, 0).is_ok(),
    )?;
    let chinook = friendly_entity(game, Kind::Harvester, 0)?;
    advance_until(
        game,
        18000,
        "original Chinook flying to and loading supplies",
        |game| game.entities[chinook].cargo > 0,
    )?;
    let earned = game.players[0].statistics.money_earned;
    advance_until(
        game,
        18000,
        "original Chinook flying to and unloading at supply center",
        |game| {
            game.entities[chinook].cargo == 0 && game.players[0].statistics.money_earned > earned
        },
    )?;
    println!(
        "Affected flight check: original Chinook pickup, return, unloading and earned income passed"
    );
    Ok(())
}

fn upgrade(
    game: &mut Skirmish,
    parent: usize,
    name: &str,
    child: &str,
) -> Result<usize, Box<dyn Error>> {
    let unit = &game.catalog[game.entities[parent].type_index];
    let index = unit
        .maintenance
        .research
        .iter()
        .position(|entry| entry.name == name)
        .ok_or("original drone research missing")?;
    let cost = unit.maintenance.research[index].cost;
    let count = unit.maintenance.research.len();
    require(
        unit.maintenance.research[index].frames == 150,
        "drone upgrade time changed",
    )?;
    let cash = game.players[0].cash;
    game.queue_upgrade(parent, index)?;
    require(
        game.players[0].cash == cash - cost && game.produced_drone(parent).is_none(),
        "drone purchase cost or delayed creation differs",
    )?;
    for other in 0..count {
        if game.catalog[game.entities[parent].type_index]
            .maintenance
            .research[other]
            .name
            .contains("Drone")
        {
            require(
                !game.can_research(parent, other),
                "NOT_QUEUEABLE allowed a second pending drone purchase",
            )?;
        }
    }
    advance_until(game, 300, "paid drone upgrade completion", |game| {
        game.produced_drone(parent).is_some()
    })?;
    let drone = game.produced_drone(parent).ok_or("paid drone missing")?;
    let drone_type = &game.catalog[game.entities[drone].type_index];
    require(
        drone_type.name == child
            && game.entities[drone].drone_parent() == Some(parent)
            && game.entities[drone].owner == game.entities[parent].owner
            && drone_type.kind_of.iter().any(|flag| flag == "NO_SELECT")
            && !game.entities[drone].selectable(drone_type),
        "drone creation ownership/template/selectability differs",
    )?;
    Ok(drone)
}

fn injure(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let enemy = game.entities[setup.enemy].position;
    // Keep travelling toward the actual enemy production exit. The map's
    // arbitrary midpoint leaves IndustrialBuilding02 between these units:
    // native tank shells correctly hit that structure while Humvee direct fire
    // reaches its target. This repair trial requires real shell damage.
    game.move_to(setup.parent, enemy)?;
    attack(game, setup.enemy, setup.parent, "Battlemaster damage trial")?;
    let damage = damage_trial(game, setup);
    if let Err(error) = damage {
        for id in [
            setup.parent,
            setup.second,
            setup.battle,
            setup.hellfire,
            setup.enemy,
        ] {
            eprintln!(
                "Drone damage trial: {} id={id} {}",
                game.catalog[game.entities[id].type_index].name,
                generals_sim::skirmish::diagnostics::movement_details(game, id)
            );
        }
        return Err(error);
    }
    require(
        game.entities[setup.parent].alive(),
        "native drone parent died before repair trial",
    )?;
    game.move_to(setup.enemy, enemy)?;
    game.stop(setup.parent)?;
    let site = game.entities[setup.parent].position;
    game.move_to(setup.second, [site[0] + 50.0, site[1] + 30.0])?;
    Ok(())
}

fn damage_trial(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let mut shots = BTreeMap::new();
    let mut trace = Vec::new();
    for _ in 0..18_000 {
        let before = game.entities[setup.enemy].health;
        game.step();
        record_tank_projectiles(game, setup, &mut trace);
        for shot in &game.presentation.shots {
            *shots
                .entry((shot.source, shot.weapon_index, shot.target))
                .or_insert(0_u32) += 1;
        }
        let enemy = &game.entities[setup.enemy];
        if enemy.health < before
            && (before.to_bits() == enemy.max_health.to_bits() || !enemy.alive())
        {
            trace.push(format!(
                "Drone attacker damage: frame={} before={before} after={} parent_health={}",
                game.tick, enemy.health, game.entities[setup.parent].health
            ));
            for shot in &game.presentation.shots {
                if shot.target == Some(setup.enemy) || shot.source == setup.enemy {
                    let unit = &game.catalog[game.entities[shot.source].type_index];
                    let weapon = unit
                        .weapon_template(shot.weapon_index)
                        .expect("shot weapon");
                    trace.push(format!(
                        "Drone damage frame shot: source={} {} weapon={} target={:?} from={:?} to={:?}",
                        shot.source, unit.name, weapon.name, shot.target, shot.from, shot.to
                    ));
                }
            }
            for id in [setup.parent, setup.enemy] {
                trace.push(format!(
                    "Drone damage frame actor id={id}: {}",
                    generals_sim::skirmish::diagnostics::movement_details(game, id)
                ));
            }
        }
        if game.entities[setup.parent].health < game.entities[setup.parent].max_health {
            return Ok(());
        }
        if !enemy.alive() || game.ended_at.is_some() {
            break;
        }
    }
    for line in trace {
        eprintln!("{line}");
    }
    for ((source, weapon_index, target), count) in shots {
        let unit = &game.catalog[game.entities[source].type_index];
        let weapon = unit.weapon_template(weapon_index).expect("shot weapon");
        eprintln!(
            "Drone damage trial shots: source={source} {} weapon={} target={target:?} count={count}",
            unit.name, weapon.name
        );
    }
    Err(format!(
        "Battlemaster failed to damage Humvee at frame {}: attacker_alive={}",
        game.tick,
        game.entities[setup.enemy].alive()
    )
    .into())
}

fn record_tank_projectiles(game: &Skirmish, setup: &Setup, trace: &mut Vec<String>) {
    if trace.len() >= 16 {
        return;
    }
    for event in &game.presentation.weapon_fx {
        let WeaponFxEvent::Projectile(event) = event else {
            continue;
        };
        if event.source != setup.enemy {
            continue;
        }
        let stage = match event.stage {
            ProjectileFxStage::Created => "created",
            ProjectileFxStage::Detonation => "detonation",
            _ => continue,
        };
        let parent = game.object_position(setup.parent);
        let unit = &game.catalog[game.entities[setup.parent].type_index];
        let horizontal = (event.position[0] - parent[0]).hypot(event.position[1] - parent[1]);
        trace.push(format!(
            "Drone tank shell: frame={} projectile={} {stage} position={:?} ground={} parent={parent:?} horizontal={horizontal} target_radius={} target_height={}",
            game.tick,
            event.projectile,
            event.position,
            game.ground_height([event.position[0], event.position[1]]),
            unit.geometry_bounds.sphere_radius,
            unit.geometry_bounds.max_height
        ));
        if matches!(event.stage, ProjectileFxStage::Detonation) {
            for (id, actor) in game.entities.iter().enumerate() {
                let kind = &game.catalog[actor.type_index];
                if id != setup.parent
                    && actor.alive()
                    && kind.kind.is_building()
                    && (actor.position[0] - event.position[0])
                        .hypot(actor.position[1] - event.position[1])
                        < kind.geometry_bounds.sphere_radius + 10.0
                {
                    trace.push(format!(
                        "Drone shell nearby structure: id={id} {} position={:?}",
                        kind.name,
                        game.object_position(id)
                    ));
                }
            }
        }
    }
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let health = game.entities[setup.parent].health;
    advance_until(
        game,
        9000,
        "original Battle Drone welding and healing",
        |game| {
            game.presentation
                .slave_welds
                .iter()
                .any(|event| event.object == setup.battle)
                && game.entities[setup.parent].health > health
        },
    )?;
    require(
        game.entities[setup.parent].alive(),
        "repair healed a dead parent",
    )?;
    // Human Hellfire TurretAI has no ATTACK_BUILDINGS mask. SlavedUpdate moves
    // toward the parent's victim but never copies its attack command, so an
    // unarmed command center is not a valid independent acquisition fixture.
    let target = if game.entities[setup.enemy].alive() {
        setup.enemy
    } else {
        produce_type(
            game,
            setup.enemy_factory,
            template(game, "ChinaTankBattleMaster")?,
            1,
        )?
    };
    attack(game, setup.second, target, "Crusader/Hellfire attack trial")?;
    hellfire_trial(game, &setup, target)?;
    require(
        game.projectiles
            .iter()
            .any(|projectile| projectile.source == setup.hellfire),
        "Hellfire fired without the original missile projectile",
    )?;
    retreat_attack_pair(game, &setup)?;
    let gattling = produce_type(
        game,
        setup.enemy_factory,
        template(game, "ChinaTankGattling")?,
        1,
    )?;
    attack(
        game,
        gattling,
        setup.battle,
        "Gattling drone replacement trial",
    )?;
    destroy_drone(game, &setup, gattling)?;
    require(
        game.entities[setup.parent].alive()
            && !game.object_has_upgrade(setup.parent, "Upgrade_AmericaBattleDrone"),
        "UpgradeDie did not release its live producer",
    )?;
    let replacement = upgrade(
        game,
        setup.parent,
        "Upgrade_AmericaBattleDrone",
        "AmericaVehicleBattleDrone",
    )?;
    require(
        replacement != setup.battle
            && game.entities[replacement].max_health.to_bits()
                == game.entities[setup.battle].max_health.to_bits(),
        "replacement drone lost its player Drone Armor",
    )?;
    println!(
        "Installed drones: paid mutually exclusive object upgrades, native HOVER/WINGS follow, real combat repair, original Hellfire projectile, Drone Armor, combat death and paid replacement passed"
    );
    Ok(())
}

fn retreat_attack_pair(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let factory = friendly_entity(game, Kind::Factory, 0)?;
    let home = game.entities[factory].position;
    game.move_to(setup.second, home)?;
    // Let the original slave follow its parent all the way home before buying
    // the next attacker. Otherwise these two correctly kill the Gattling while
    // it is testing the Battle Drone's real damage/UpgradeDie lifecycle.
    advance_until(
        game,
        18_000,
        "Crusader and Hellfire returning to their paid factory",
        |game| {
            [setup.second, setup.hellfire].into_iter().all(|id| {
                let actor = &game.entities[id];
                actor.alive()
                    && (actor.position[0] - home[0]).hypot(actor.position[1] - home[1]) < 150.0
            })
        },
    )
}

fn attack(
    game: &mut Skirmish,
    actor: usize,
    target: usize,
    stage: &str,
) -> Result<(), Box<dyn Error>> {
    game.attack(actor, target).map_err(|error| {
        let source = &game.entities[actor];
        let unit = &game.catalog[source.type_index];
        let weapons: Vec<_> = (0..3)
            .filter_map(|slot| {
                unit.weapon_template(source.weapon_index(slot))
                    .map(|weapon| (slot, &weapon.name, weapon.anti_mask))
            })
            .collect();
        format!(
            "{stage}: {error}; frame={} source={actor} {} position={:?} weapons={weapons:?} target={target} {} position={:?} targetable={} source_state=[{}] target_state=[{}]",
            game.tick,
            unit.name,
            game.object_position(actor),
            game.catalog[game.entities[target].type_index].name,
            game.object_position(target),
            game.weapon_can_target(actor, target),
            generals_sim::skirmish::diagnostics::movement_details(game, actor),
            generals_sim::skirmish::diagnostics::movement_details(game, target),
        )
        .into()
    })
}

fn destroy_drone(
    game: &mut Skirmish,
    setup: &Setup,
    attacker: usize,
) -> Result<(), Box<dyn Error>> {
    let mut shots = BTreeMap::new();
    for _ in 0..18_000 {
        game.step();
        for shot in &game.presentation.shots {
            *shots
                .entry((shot.source, shot.weapon_index, shot.target))
                .or_insert(0_u32) += 1;
        }
        if !game.entities[setup.battle].alive() {
            return Ok(());
        }
        if !game.entities[attacker].alive() || game.ended_at.is_some() {
            break;
        }
    }
    for id in [
        attacker,
        setup.battle,
        setup.parent,
        setup.second,
        setup.hellfire,
        setup.enemy,
    ] {
        let actor = &game.entities[id];
        let unit = &game.catalog[actor.type_index];
        let weapon = actor.active_weapon(unit);
        eprintln!(
            "Drone replacement trial: id={id} {} weapon={} position3d={:?} target_reachable={} {}",
            unit.name,
            weapon.name,
            game.object_position(id),
            game.weapon_target_reachable(id, setup.battle),
            generals_sim::skirmish::diagnostics::movement_details(game, id)
        );
    }
    for ((source, weapon_index, target), count) in shots {
        let unit = &game.catalog[game.entities[source].type_index];
        let weapon = unit.weapon_template(weapon_index).expect("shot weapon");
        eprintln!(
            "Drone replacement shots: source={source} {} weapon={} target={target:?} count={count}",
            unit.name, weapon.name
        );
    }
    Err(format!(
        "Gattling failed to destroy Battle Drone at frame {}: attacker_alive={}",
        game.tick,
        game.entities[attacker].alive()
    )
    .into())
}

fn hellfire_trial(game: &mut Skirmish, setup: &Setup, target: usize) -> Result<(), Box<dyn Error>> {
    let mut shots = BTreeMap::new();
    for _ in 0..18_000 {
        game.step();
        for shot in &game.presentation.shots {
            let unit = &game.catalog[game.entities[shot.source].type_index];
            let weapon = unit
                .weapon_template(shot.weapon_index)
                .expect("shot weapon");
            if shot.source == setup.hellfire && weapon.name == "HellfireMissileWeapon" {
                return Ok(());
            }
            *shots
                .entry((shot.source, shot.weapon_index, shot.target))
                .or_insert(0_u32) += 1;
        }
        if !game.entities[setup.second].alive()
            || !game.entities[setup.hellfire].alive()
            || !game.entities[target].alive()
            || game.ended_at.is_some()
        {
            break;
        }
    }
    for ((source, weapon_index, target), count) in shots {
        let unit = &game.catalog[game.entities[source].type_index];
        let weapon = unit.weapon_template(weapon_index).expect("shot weapon");
        eprintln!(
            "Hellfire trial shots: source={source} {} weapon={} target={target:?} count={count}",
            unit.name, weapon.name
        );
    }
    for id in [
        setup.second,
        setup.hellfire,
        target,
        setup.parent,
        setup.enemy,
    ] {
        let actor = &game.entities[id];
        let unit = &game.catalog[actor.type_index];
        let weapon = actor.active_weapon(unit);
        eprintln!(
            "Hellfire trial actor: id={id} {} weapon={} rounds={} position3d={:?} drone_parent={:?} target_reachable={} {}",
            unit.name,
            weapon.name,
            actor.weapon_rounds_remaining(actor.weapon_slot()),
            game.object_position(id),
            actor.drone_parent(),
            game.weapon_target_reachable(id, target),
            generals_sim::skirmish::diagnostics::movement_details(game, id)
        );
    }
    Err(format!("Hellfire failed to fire at frame {}", game.tick).into())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_drones_repair_attack_and_replace_through_original_commands() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
