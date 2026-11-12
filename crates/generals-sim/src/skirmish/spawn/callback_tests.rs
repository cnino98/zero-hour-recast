use super::{ACTIVE, INITIALIZED, Rules};
use crate::skirmish::{DeathType, Faction, Kind, Skirmish, drones, tests};

fn rules(name: &str, flags: u8) -> Rules {
    Rules {
        templates: vec![name.into()],
        count: 1,
        replace_frames: 3,
        flags,
        initial_burst: 0,
        conditions: tests::death_conditions(),
    }
}

fn fixture() -> (Skirmish, usize) {
    let mut game = tests::game();
    let mut child = tests::fixture_type(Faction::Gla, Kind::Infantry);
    child.name = "ProneSpawn".into();
    child.behavior.slaves.push(drones::Rules {
        guard_range: 0.0,
        guard_wander: 0.0,
        attack_range: 0.0,
        attack_wander: 0.0,
        scout_range: 0.0,
        scout_wander: 0.0,
        spotting_range: 0.0,
        repair: None,
        panic: None,
        panic_speed: 0.0,
        stay_on_master_layer: false,
    });
    tests::register_update(&mut child, "SlavedUpdate");
    child.behavior.prone.push(1.5);
    tests::register_update(&mut child, "ProneUpdate");
    game.catalog.push(child);
    let mut parent = tests::fixture_type(Faction::Gla, Kind::Vehicle);
    parent.contain = Some(crate::skirmish::containment::tests::rules(0.0, 0.0));
    tests::register_update(&mut parent, "TransportContain");
    for flags in [Rules::ONE_SHOT | Rules::REQUIRE_SPAWNER, Rules::ONE_SHOT] {
        parent.behavior.spawns.push(rules("ProneSpawn", flags));
        tests::register_update(&mut parent, "SpawnBehavior");
        parent.behavior_modules.last_mut().unwrap().interfaces |= 6;
    }
    parent.behavior.spawns[0].conditions.death_mask = 1 << (DeathType::Burned as u8);
    let kind = game.catalog.len();
    game.catalog.push(parent);
    let parent = game.spawn(kind, 0, [355.0, 255.0], true);
    (game, parent)
}

#[test]
fn authored_spawn_instances_initialize_at_update_and_fan_out_actual_unclipped_damage_before_die() {
    let (mut game, parent) = fixture();
    assert_eq!(game.entities[parent].behaviors.spawns.len(), 2);
    assert!(
        game.entities[parent]
            .behaviors
            .spawns
            .iter()
            .all(|state| state.replacements.is_empty() && state.flags & INITIALIZED == 0)
    );
    game.tick = 1;
    game.run_object_updates();
    let first = game.entities[parent].behaviors.spawns[0].children[0];
    let second = game.entities[parent].behaviors.spawns[1].children[0];
    assert_ne!(first, second);
    assert!(
        game.entities[parent]
            .behaviors
            .spawns
            .iter()
            .all(|state| state.flags & ACTIVE == 0)
    );
    game.apply_native_damage(parent, None, 250.75, DeathType::Normal);
    assert_eq!(game.entities[first].behaviors.prone, [375]);
    assert_eq!(game.entities[second].behaviors.prone, [375]);
    assert_eq!(
        game.entities[first].producer,
        Some(parent),
        "first module's DieMux rejected NORMAL"
    );
    assert_eq!(game.entities[second].producer, None);
    assert_eq!(game.entities[first].drone_parent(), Some(parent));
    assert_eq!(game.entities[second].drone_parent(), None);
}

#[test]
fn direct_deletion_destroys_only_required_spawns_without_kill_or_damage_callbacks() {
    let (mut game, parent) = fixture();
    game.tick = 1;
    game.run_object_updates();
    let first = game.entities[parent].behaviors.spawns[0].children[0];
    let second = game.entities[parent].behaviors.spawns[1].children[0];
    let health = game.entities[first].health;
    game.destroy_maintenance_object(parent);
    game.process_update_destruction();
    assert!(!game.entities[first].exists());
    assert_eq!(game.entities[first].health.to_bits(), health.to_bits());
    assert_eq!(game.entities[first].behaviors.prone, [0]);
    assert!(game.entities[second].exists());
    assert_eq!(game.entities[second].producer, Some(parent));
}

#[test]
fn ordinary_nonempty_orphan_names_preserve_the_pinned_comparison_skip() {
    let (mut game, parent) = fixture();
    let child_kind = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ProneSpawn")
        .unwrap();
    let orphan = game.spawn(child_kind, 0, game.entities[parent].position, true);
    let random = game.random;
    assert_eq!(game.reclaim_orphan_spawn(parent, 0), None);
    assert_eq!(game.random, random);
    assert!(game.entities[orphan].producer.is_none());
    game.tick = 1;
    game.run_object_updates();
    assert_ne!(
        game.entities[parent].behaviors.spawns[0].children[0],
        orphan
    );
}

#[test]
fn signed_initial_burst_retains_native_unsigned_runtime_countdown() {
    let mut data = rules("ProneSpawn", Rules::ONE_SHOT);
    data.initial_burst = -1;
    let state = super::State::new(&data);
    assert_eq!(state.burst_left, u32::MAX);
    assert_eq!(state.remaining, 1);
    assert!(state.replacements.is_empty());
}

#[test]
fn initial_burst_uses_structure_kindof_and_first_real_spawn_point_exit() {
    let mut game = tests::game();
    game.catalog[5].name = "BurstChild".into();
    let mut barracks = tests::fixture_type(Faction::Gla, Kind::Vehicle);
    barracks.kind_of.push("STRUCTURE".into());
    barracks.behavior.spawn_exits.push(vec![super::Point {
        position: [12.0, 0.0, 0.0],
        basis: crate::skirmish::physical_ocl::transform::yaw(0.0),
    }]);
    tests::register_update(&mut barracks, "SpawnPointProductionExitUpdate");
    let barracks_type = game.catalog.len();
    game.catalog.push(barracks);
    let barracks = game.spawn(barracks_type, 0, [300.0, 300.0], true);
    let mut nexus = tests::fixture_type(Faction::Gla, Kind::Vehicle);
    nexus.behavior.production_exit = Some(crate::skirmish::ProductionExit {
        use_spawn_rally_point: false,
        create: [0.0; 3],
        rally: [0.0; 3],
        extend_rally: false,
        snap_rally: false,
        queue: Some(crate::skirmish::QueueExit {
            allow_airborne_creation: false,
            delay_frames: 3,
            initial_burst: 1,
        }),
    });
    tests::register_update(&mut nexus, "QueueProductionExitUpdate");
    let mut data = rules("BurstChild", Rules::ONE_SHOT | Rules::EXIT_BY_BUDDING);
    data.initial_burst = 1;
    nexus.behavior.spawns.push(data);
    tests::register_update(&mut nexus, "SpawnBehavior");
    nexus.behavior_modules.last_mut().unwrap().interfaces |= 6;
    let nexus_type = game.catalog.len();
    game.catalog.push(nexus);
    let nexus = game.spawn(nexus_type, 0, [350.0, 350.0], true);
    game.entities[nexus].producer = Some(barracks);
    game.tick = 2;
    game.update_spawn(nexus, 0);
    let child = game.spawned_children(nexus)[0];
    assert_eq!(game.entities[child].producer, Some(nexus));
    assert!(game.entities[child].disabled_held());
    let origin = game.entities[barracks].position;
    assert_eq!(
        game.entities[child].position.map(f32::to_bits),
        [origin[0] + 12.0, origin[1]].map(f32::to_bits)
    );
    assert_eq!(game.entities[nexus].behaviors.spawns[0].burst_left, 0);
}

#[test]
fn contained_delete_precedes_later_required_spawn_delete_in_authored_module_order() {
    let (mut game, parent) = fixture();
    game.tick = 1;
    game.run_object_updates();
    let required_spawn = game.entities[parent].behaviors.spawns[0].children[0];
    let rider = game.spawn(5, 0, game.entities[parent].position, true);
    assert!(game.native_container_admits(rider, parent, true));
    game.add_to_contain(rider, parent);
    for id in [parent, rider, required_spawn] {
        game.entities[id].drawable.present = true;
    }
    game.presentation.weapon_fx.clear();
    game.destroy_maintenance_object(parent);
    game.process_update_destruction();
    let deleted: Vec<_> = game
        .presentation
        .weapon_fx
        .iter()
        .filter_map(|event| match event {
            crate::skirmish::WeaponFxEvent::DrawableDestroyed(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(deleted, [parent, rider, required_spawn]);
}
