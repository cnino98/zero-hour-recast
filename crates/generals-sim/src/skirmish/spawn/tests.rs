use super::Rules;
use crate::skirmish::{Order, containment::tests::setup};

fn install_spawn(
    game: &mut crate::skirmish::Skirmish,
    old: usize,
    rules: Rules,
    points: Option<Vec<super::Point>>,
) -> usize {
    let kind = game.entities[old].type_index;
    let owner = game.entities[old].owner;
    let position = game.entities[old].position;
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    if game.catalog[kind].contain.is_none() {
        game.catalog[kind]
            .behavior_modules
            .retain(|module| module.family != "TransportContain");
        game.catalog[kind].update_modules.retain(|module| !matches!(module, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "TransportContain"));
    }
    if let Some(exit) = &game.catalog[kind].behavior.production_exit {
        let family = if exit.queue.is_some() {
            "QueueProductionExitUpdate"
        } else {
            "DefaultProductionExitUpdate"
        };
        if !game.catalog[kind]
            .behavior_modules
            .iter()
            .any(|module| module.family == family)
        {
            crate::skirmish::tests::register_update(&mut game.catalog[kind], family);
        }
    }
    if let Some(points) = points {
        game.catalog[kind].behavior.spawn_exits.push(points);
        crate::skirmish::tests::register_update(
            &mut game.catalog[kind],
            "SpawnPointProductionExitUpdate",
        );
    }
    game.catalog[kind].behavior.spawns.push(rules);
    crate::skirmish::tests::register_update(&mut game.catalog[kind], "SpawnBehavior");
    game.catalog[kind]
        .behavior_modules
        .last_mut()
        .unwrap()
        .interfaces |= 6;
    game.spawn(kind, owner, position, true)
}

#[test]
fn selling_stops_spawn_replacements_before_the_building_sinks() {
    let (mut game, host) = setup(0.0, 0.0);
    let spawn_rules = Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        templates: vec![game.catalog[5].name.clone()],
        count: 2,
        replace_frames: 0,
        flags: Rules::FREE_WILL,
        initial_burst: 0,
    };
    let host = install_spawn(&mut game, host, spawn_rules, None);
    let kind = game.entities[host].type_index;
    game.catalog[kind].kind = crate::skirmish::Kind::Defense;
    let owner = game.entities[host].owner;
    game.sell(owner, host).unwrap();
    for frame in 1..=30 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert!(!game.entities[host].sold_model());
    assert!(game.spawned_children(host).is_empty());
}

#[test]
fn construction_keeps_scan_cadence_then_grants_original_exits_once() {
    let (mut game, host) = setup(0.0, 0.0);
    game.catalog[5].name = "SpawnedDefender".into();
    let spawn_rules = Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        templates: vec![game.catalog[5].name.clone()],
        count: 2,
        replace_frames: 0,
        flags: Rules::ONE_SHOT | Rules::FREE_WILL,
        initial_burst: 0,
    };
    let host = install_spawn(&mut game, host, spawn_rules, None);
    game.entities[host].build_progress = 0.5;
    let before = game.entities.len();
    game.tick = 9;
    game.run_object_updates();
    game.entities[host].build_progress = 1.0;
    for frame in 10..24 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert_eq!(game.entities.len(), before);
    game.tick = 24;
    game.run_object_updates();
    assert_eq!(game.entities.len(), before + 2);
    for actor in &game.entities[before..] {
        assert_eq!(actor.type_index, 5);
        assert_eq!(actor.owner, game.entities[host].owner);
        assert!(matches!(actor.ai.order, Order::FollowPath));
        assert!(actor.contained_by().is_none());
        assert_eq!(actor.ai.waypoints.points.len(), 2);
    }
    game.tick = 39;
    game.run_object_updates();
    assert_eq!(game.entities.len(), before + 2);
}

#[test]
fn disabled_frames_pause_the_scan_countdown_without_delaying_ready_replacements() {
    let (mut game, host) = setup(0.0, 0.0);
    game.catalog[5].name = "SpawnedDefender".into();
    let spawn_rules = Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        templates: vec![game.catalog[5].name.clone()],
        count: 2,
        replace_frames: 0,
        flags: Rules::ONE_SHOT | Rules::FREE_WILL,
        initial_burst: 0,
    };
    let host = install_spawn(&mut game, host, spawn_rules, None);
    game.entities[host].build_progress = 0.5;
    let before = game.entities.len();
    for frame in 1..=6 {
        game.tick = frame;
        game.run_object_updates();
    }
    game.entities[host].build_progress = 1.0;
    game.set_hacked_until(host, crate::skirmish::ai_dispatch::native_frame(101));
    for frame in 7..=100 {
        game.tick = frame;
        game.run_object_updates();
    }
    game.clear_hacked_disable(host);
    for frame in 101..110 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert_eq!(game.entities.len(), before);
    game.tick = 110;
    game.run_object_updates();
    assert_eq!(game.entities.len(), before + 2);
}

#[test]
fn hole_reconstruction_permanently_suppresses_the_one_shot_grant() {
    let (mut game, host) = setup(0.0, 0.0);
    game.catalog[5].name = "SpawnedDefender".into();
    let spawn_rules = Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        templates: vec![game.catalog[5].name.clone()],
        count: 2,
        replace_frames: 0,
        flags: Rules::ONE_SHOT | Rules::FREE_WILL,
        initial_burst: 0,
    };
    let host = install_spawn(&mut game, host, spawn_rules, None);
    game.entities[host].object_status |= 1 << 21;
    let before = game.entities.len();
    game.tick = 1;
    game.run_object_updates();
    game.entities[host].object_status &= !(1 << 21);
    game.tick = 60;
    game.run_object_updates();
    assert_eq!(game.entities.len(), before);
}

fn stinger() -> (crate::skirmish::Skirmish, usize) {
    let (mut game, host) = setup(0.0, 0.0);
    game.catalog[5].name = "StationSoldier".into();
    let index = game.entities[host].type_index;
    game.catalog[index]
        .kind_of
        .push("SPAWNS_ARE_THE_WEAPONS".into());
    game.catalog[index].contain = None;
    let spawn_rules = Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        templates: vec!["StationSoldier".into()],
        count: 2,
        replace_frames: 30,
        flags: Rules::REQUIRE_SPAWNER,
        initial_burst: 0,
    };
    let points = vec![
        super::Point {
            position: [10.0, 0.0, 0.0],
            basis: crate::skirmish::physical_ocl::transform::yaw(0.0),
        },
        super::Point {
            position: [-10.0, 0.0, 0.0],
            basis: crate::skirmish::physical_ocl::transform::yaw(std::f32::consts::PI),
        },
    ];
    let host = install_spawn(&mut game, host, spawn_rules, Some(points));
    game.set_object_orientation(host, std::f32::consts::FRAC_PI_2);
    game.tick = 2;
    game.run_object_updates();
    (game, host)
}

#[test]
fn station_bones_rotate_and_hold_children_while_preserving_their_weapons() {
    let (mut game, host) = stinger();
    let children = game.spawned_children(host).to_vec();
    assert_eq!(children.len(), 2);
    let origin = game.entities[host].position;
    let first = children[0];
    assert!((game.entities[first].position[0] - origin[0]).abs() < 0.001);
    assert!((game.entities[first].position[1] - origin[1] - 10.0).abs() < 0.001);
    assert!(game.entities[first].disabled_held());
    assert_eq!(game.entities[first].producer, Some(host));
    assert!(game.move_to(first, [500.0, 500.0]).is_err());
    assert!(game.weapon_available(host, 0));
}

#[test]
fn recurring_replacement_uses_strict_due_frame_and_cannot_overfill_stations() {
    let (mut game, host) = stinger();
    let child = game.spawned_children(host)[0];
    let other = game.spawned_children(host)[1];
    game.apply_hit(child, child, game.entities[child].health);
    assert_eq!(game.spawned_children(host), [other]);
    // Death at 2 schedules frame 32. The source comparison is current > due.
    game.entities[host].behaviors.spawns[0].frames_to_wait = 0;
    game.tick = 32;
    game.run_object_updates();
    assert_eq!(game.spawned_children(host), [other]);
    assert!(
        game.entities[child].exists(),
        "corpse still owns its station"
    );
    game.destroy_maintenance_object(child);
    game.process_update_destruction();
    game.entities[host].behaviors.spawns[0].frames_to_wait = 0;
    game.tick = 33;
    game.run_object_updates();
    assert_eq!(game.spawned_children(host).len(), 2);
    let replacement = game.spawned_children(host)[1];
    assert_ne!(replacement, child);
    assert_eq!(
        game.entities[replacement].position.map(f32::to_bits),
        game.entities[child].position.map(f32::to_bits)
    );
    for frame in 34..100 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert_eq!(game.spawned_children(host), [other, replacement]);
}

#[test]
fn parent_death_detaches_before_killing_required_children_and_rebuilding_can_respawn() {
    let (mut game, host) = stinger();
    let children = game.spawned_children(host).to_vec();
    game.apply_hit(host, host, game.entities[host].health);
    for child in children {
        assert!(!game.entities[child].alive());
        assert!(game.entities[child].producer.is_none());
        assert!(game.entities[child].damage_state.last_attacker.is_none());
    }
    assert!(
        game.entities[host].behaviors.spawns[0]
            .replacements
            .is_empty()
    );
    let (mut game, host) = stinger();
    game.entities[host].object_status |= 1 << 21;
    let child = game.spawned_children(host)[0];
    game.apply_hit(child, child, game.entities[child].health);
    game.destroy_maintenance_object(child);
    game.process_update_destruction();
    game.tick = 100;
    game.entities[host].behaviors.spawns[0].frames_to_wait = 0;
    game.run_object_updates();
    assert_eq!(game.spawned_children(host).len(), 2);
}

#[test]
fn hive_damage_routes_to_nearest_spawn_and_swallows_after_the_last_casualty() {
    let (mut game, host) = stinger();
    let index = game.entities[host].type_index;
    game.catalog[index].hive_body = Some(crate::skirmish::hive_body::Rules {
        propagate: 1 << crate::skirmish::DamageType::Sniper as usize,
        swallow: 1 << crate::skirmish::DamageType::Sniper as usize,
    });
    let children = game.spawned_children(host).to_vec();
    let position = game.entities[children[1]].position;
    let shooter = game.spawn(6, 1, [position[0], position[1] - 5.0], true);
    assert_eq!(
        game.hive_damage_target(host, shooter, crate::skirmish::DamageType::Sniper),
        Some(children[1])
    );
    for child in children {
        game.apply_hit(child, child, game.entities[child].health);
    }
    assert_eq!(
        game.hive_damage_target(host, shooter, crate::skirmish::DamageType::Sniper),
        None
    );
}

fn mob() -> (crate::skirmish::Skirmish, usize) {
    mob_with_count(10)
}

fn mob_with_count(count: i32) -> (crate::skirmish::Skirmish, usize) {
    let (mut game, host) = setup(0.0, 0.0);
    let index = game.entities[host].type_index;
    game.catalog[5].name = "Pistol".into();
    game.catalog[6].name = "Rock".into();
    for kind in [5, 6] {
        game.catalog[kind].behavior.mobs = member_type().behavior.mobs;
        crate::skirmish::tests::register_update(&mut game.catalog[kind], "MobMemberSlavedUpdate");
    }

    game.catalog[index].contain = None;
    let spawn_rules = Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        templates: vec!["Pistol".into(), "Rock".into(), "Pistol".into()],
        count,
        replace_frames: 900,
        flags: Rules::EXIT_BY_BUDDING | Rules::AGGREGATE_HEALTH,
        initial_burst: 5,
    };
    game.catalog[index].behavior.production_exit = Some(crate::skirmish::ProductionExit {
        use_spawn_rally_point: false,
        create: [0.0; 3],
        rally: [0.0; 3],
        extend_rally: true,
        snap_rally: true,
        queue: Some(crate::skirmish::QueueExit {
            allow_airborne_creation: false,
            delay_frames: 150,
            initial_burst: 5,
        }),
    });
    let host = install_spawn(&mut game, host, spawn_rules, None);
    (game, host)
}

#[test]
fn budding_reserves_native_burst_then_waits_for_queue_exit_and_cycles_templates() {
    let (mut game, host) = mob();
    game.tick = 2;
    game.run_object_updates();
    assert_eq!(game.spawned_children(host).len(), 5);
    let types: Vec<_> = game
        .spawned_children(host)
        .iter()
        .map(|&id| game.entities[id].type_index)
        .collect();
    assert_eq!(types, [5, 6, 5, 5, 6]);
    assert!(
        game.spawned_children(host)
            .iter()
            .all(|&id| !game.entities[id].disabled_held())
    );
    for frame in 3..=151 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert_eq!(game.spawned_children(host).len(), 5);
    assert!(!game.entities[host].queue_exit.free());
    // Both callbacks share NORMAL. With the authored AI constructor wakes,
    // Queue decrements 1→0 before Spawn's ready scan at frame 152. Budding
    // consumes that opening and resets the exit delay in this same callback wave.
    game.tick = 152;
    game.run_object_updates();
    assert_eq!(game.spawned_children(host).len(), 6);
    assert!(!game.entities[host].queue_exit.free());
    for frame in 153..=167 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert_eq!(game.spawned_children(host).len(), 6);
    assert_eq!(game.entities[game.spawned_children(host)[5]].type_index, 5);
}

#[test]
fn aggregate_tracks_casualties_then_last_child_destroys_nexus_without_replacement() {
    let (mut game, host) = mob();
    game.tick = 2;
    game.run_object_updates();
    game.aggregate_spawn_health(host, 0);
    assert_eq!(
        game.entities[host].health.to_bits(),
        (game.entities[host].max_health * 0.5).to_bits()
    );
    assert!(game.entities[host].masked());
    let children = game.spawned_children(host).to_vec();
    for child in &children[..4] {
        game.apply_hit(*child, *child, game.entities[*child].health);
    }
    game.aggregate_spawn_health(host, 0);
    assert_eq!(
        game.entities[host].health.to_bits(),
        (game.entities[host].max_health * 0.1).to_bits()
    );
    let child = children[4];
    game.apply_hit(child, child, game.entities[child].health);
    game.process_update_destruction();
    assert!(!game.entities[host].alive());
    let count = game.entities.len();
    game.tick = 10_000;
    game.run_object_updates();
    assert!(!game.entities[host].alive());
    assert_eq!(game.entities.len(), count);
}

#[test]
fn member_creation_keeps_native_locomotor_wait_and_personal_color_draws() {
    let (mut game, _) = mob();
    let unit = member_type();
    let mut expected = game.random;
    // The declared AIUpdate constructs NORMAL before MobMemberSlavedUpdate.
    for _ in 0..3 {
        crate::skirmish::startup::random_value(&mut expected);
    }
    crate::skirmish::startup::random_between(&mut expected, 20);
    for _ in 0..3 {
        crate::skirmish::startup::random_real(&mut expected, 0.2, 0.4);
    }
    // Attack-move and both attack-follow states construct independent child Idles;
    // AI onObjectCreated then enters the outer Idle after all constructors.
    for _ in 0..3 {
        crate::skirmish::startup::random_between(&mut expected, 60);
    }
    let idle_offset = crate::skirmish::startup::random_between(&mut expected, 60);
    let index = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(index, 0, [300.0, 300.0], true);
    assert_eq!(game.random, expected);
    assert_eq!(game.entities[id].ai.idle.sleep_offset, idle_offset);
}

fn member_type() -> crate::skirmish::UnitType {
    let mut unit = crate::skirmish::tests::fixture_type(
        crate::skirmish::Faction::Gla,
        crate::skirmish::Kind::Infantry,
    );
    unit.behavior.mobs.push(super::MobRules {
        catch_up: 40,
        no_catch_up: 15,
        squirrelliness: 0.05,
        crisis_calls: 30,
        wander: crate::skirmish::tests::fixture_type(
            crate::skirmish::Faction::Gla,
            crate::skirmish::Kind::Infantry,
        )
        .locomotor,
        wander_speed: 10.0,
        panic: crate::skirmish::tests::fixture_type(
            crate::skirmish::Faction::Gla,
            crate::skirmish::Kind::Infantry,
        )
        .locomotor,
        panic_speed: 30.0,
    });
    crate::skirmish::tests::register_update(&mut unit, "MobMemberSlavedUpdate");
    unit
}

#[test]
fn orphan_and_catch_up_crisis_use_native_kill_without_attacker_credit() {
    let (mut game, host) = mob_with_count(1);
    let index = game.catalog.len();
    game.catalog.push(member_type());
    let orphan = game.spawn(index, 0, [300.0, 300.0], true);
    game.update_mob_member(orphan, 0);
    assert!(!game.entities[orphan].alive());
    assert!(game.entities[orphan].damage_state.last_attacker.is_none());
    game.tick = 2;
    game.update_spawn(host, 0);
    let member = game.spawned_children(host)[0];
    game.set_object_position(member, [700.0, 255.0, 0.0]);
    for _ in 0..512 {
        game.update_mob_member(member, 0);
        if !game.entities[member].alive() {
            break;
        }
    }
    assert!(!game.entities[member].alive());
    assert!(game.entities[member].damage_state.last_attacker.is_none());
    game.process_update_destruction();
    assert!(!game.entities[host].alive());
    assert!(game.entities[host].damage_state.last_attacker.is_none());
    assert_eq!(game.players[1].statistics.units_destroyed, 0);
    assert_eq!(game.players[0].statistics.units_destroyed, 0);
}

#[test]
fn armed_members_clear_primary_weapon_pose_and_preserve_ak47_secondary_pose() {
    let (mut game, host) = mob();
    let index = game.catalog.len();
    let mut unit = member_type();
    unit.secondary_weapon = Some(
        crate::skirmish::tests::fixture_type(
            crate::skirmish::Faction::Gla,
            crate::skirmish::Kind::Infantry,
        )
        .weapon,
    );
    game.catalog.push(unit);
    let member = game.spawn(index, 0, [300.0, 300.0], true);
    game.enslave_spawn(member, host);
    let target = game.spawn(6, 1, [330.0, 300.0], true);
    game.fire_weapon(member, target);
    game.update_weapon_status(member); // preceding frame's FINAL writes A.
    let normal = game.model_flags(&game.entities[member], [0.5, 0.1]);
    assert!(normal.contains("FIRING_A") && normal.contains("USING_WEAPON_A"));
    game.apply_weapon_set_upgrade(member);
    game.tick += 1;
    game.update_mob_member(member, 0); // next NORMAL clears only Drawable's A flags.
    let armed = game.model_flags(&game.entities[member], [0.5, 0.1]);
    assert!(armed.contains("WEAPONSET_PLAYER_UPGRADE"));
    assert!(!armed.contains("FIRING_A") && !armed.contains("USING_WEAPON_A"));
    game.switch_weapon(member, 1).unwrap();
    game.fire_weapon(member, target);
    game.update_weapon_status(member);
    game.tick += 1;
    game.update_mob_member(member, 0);
    let armed = game.model_flags(&game.entities[member], [0.5, 0.1]);
    assert!(armed.contains("FIRING_B") && armed.contains("USING_WEAPON_B"));
}

#[test]
fn authored_zero_build_time_is_admitted_for_spawn_members_only() {
    let (mut game, _) = mob();
    let mut member = member_type();
    member.build_seconds = 0.0;
    let index = game.catalog.len();
    game.catalog.push(member);
    assert!(crate::skirmish::validate_catalog(&game.catalog, &game.rules).is_ok());
    game.catalog[index].behavior.mobs.clear();
    assert!(crate::skirmish::validate_catalog(&game.catalog, &game.rules).is_err());
    game.catalog[index].behavior.mobs = member_type().behavior.mobs;
    for invalid in [-1.0, f32::NAN, f32::INFINITY] {
        game.catalog[index].build_seconds = invalid;
        assert!(crate::skirmish::validate_catalog(&game.catalog, &game.rules).is_err());
    }
}

#[test]
fn clearing_native_held_releases_a_station_child_without_changing_its_station_owner() {
    let (mut game, host) = stinger();
    let child = game.spawned_children(host)[0];
    assert!(game.entities[child].disabled_held());
    game.clear_disabled_held(child);
    assert!(!game.entities[child].disabled_held());
    assert_eq!(game.object_disabled_mask(child) & (1 << 3), 0);
    assert_eq!(game.entities[child].producer, Some(host));
    assert!(game.spawned_children(host).contains(&child));
    game.move_to(child, [500.0, 500.0]).unwrap();
}

#[cfg(test)]
mod hacked_disable {
    use crate::skirmish::{Order, Skirmish, spawn, tests};

    fn fixture() -> (Skirmish, usize) {
        let (mut game, old) = crate::skirmish::containment::tests::setup(0.0, 0.0);
        let kind = game.entities[old].type_index;
        // Native template names are unique. The common synthetic catalog uses
        // "Fixture" for every type, which would resolve to its first building.
        game.catalog[5].name = "HackedSpawnInfantry".into();
        let template = game.catalog[5].name.clone();
        game.destroy_maintenance_object(old);
        game.process_update_destruction();
        game.catalog[kind]
            .kind_of
            .push("SPAWNS_ARE_THE_WEAPONS".into());
        for count in [2, 1] {
            game.catalog[kind].behavior.spawns.push(spawn::Rules {
                conditions: tests::death_conditions(),
                templates: vec![template.clone()],
                count,
                replace_frames: 0,
                flags: spawn::Rules::ONE_SHOT | spawn::Rules::FREE_WILL,
                initial_burst: 0,
            });
            tests::register_update(&mut game.catalog[kind], "SpawnBehavior");
            game.catalog[kind]
                .behavior_modules
                .last_mut()
                .unwrap()
                .interfaces |= 6;
        }
        let parent = game.spawn(kind, 0, [300.0; 2], true);
        for frame in 1..60 {
            game.tick = frame;
            game.run_object_updates();
            // Each SpawnBehavior creates one child per eligible callback;
            // the first nonempty list is not the complete two-child fixture.
            if game.entities[parent]
                .behaviors
                .spawns
                .iter()
                .zip([2, 1])
                .all(|(state, count)| state.children.len() == count)
            {
                break;
            }
        }
        assert_eq!(game.entities[parent].behaviors.spawns[0].children.len(), 2);
        assert_eq!(game.entities[parent].behaviors.spawns[1].children.len(), 1);
        (game, parent)
    }

    #[test]
    fn hacked_fanout_idles_first_spawn_interface_only_and_equal_deadline_does_not_repeat_it() {
        let (mut game, parent) = fixture();
        let children = game.entities[parent].behaviors.spawns[0].children.clone();
        assert!(
            children
                .iter()
                .all(|&child| game.entities[child].type_index == 5)
        );
        let second = game.entities[parent].behaviors.spawns[1].children[0];
        assert_eq!(game.entities[second].type_index, 5);
        game.set_hacked_until(parent, 1000);
        for &child in &children {
            assert!(game.entities[child].hacked());
            assert!(game.entities[child].ai_idle());
        }
        assert!(!game.entities[second].hacked());
        let child = children[0];
        game.clear_hacked_disable(child);
        game.ai_idle_from_ai(child);
        game.construction_move(child, [450.0, 450.0, 0.0]);
        assert!(matches!(game.entities[child].ai.order, Order::Move(_)));
        game.set_hacked_until(parent, 1000);
        assert!(!game.entities[child].hacked());
        assert!(matches!(game.entities[child].ai.order, Order::Move(_)));
        game.clear_hacked_disable(parent);
        assert!(children.iter().all(|&child| !game.entities[child].hacked()));
        assert!(!game.entities[second].hacked());
    }
}
