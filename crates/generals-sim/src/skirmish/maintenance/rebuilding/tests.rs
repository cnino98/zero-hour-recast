use crate::skirmish::{
    DeathType, Faction, HoleExposure, Kind, RebuildRules, Skirmish,
    death_callbacks::Conditions,
    tests::{fixture_type, game, register_behavior, register_update},
};

fn conditions() -> Conditions {
    Conditions {
        death_mask: u32::MAX,
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 0,
    }
}

fn fixture(delay: f32) -> (Skirmish, usize) {
    let mut game = game();
    game.catalog[1].name = "RebuildWorker".into();
    let mut hole = fixture_type(Faction::Gla, Kind::TechBuilding);
    hole.name = "SharedRebuildHole".into();
    hole.kind_of.push("REBUILD_HOLE".into());
    hole.initial_health = 80.0;
    hole.maintenance.rebuild.push(RebuildRules {
        worker: game.catalog[1].name.clone(),
        respawn_frames: delay,
        regeneration: 0.1,
    });
    register_update(&mut hole, "RebuildHoleBehavior");
    hole.behavior_modules.last_mut().unwrap().interfaces |= 2;
    let kind = game.catalog.len();
    game.catalog.push(hole);
    for index in [0, 3] {
        game.catalog[index]
            .maintenance
            .expose_hole
            .push(HoleExposure {
                conditions: conditions(),
                template: "SharedRebuildHole".into(),
                health: 500.0,
                transfer_attackers: true,
            });
        register_behavior(&mut game.catalog[index], "RebuildHoleExposeDie", 2);
    }
    (game, kind)
}

fn advance(game: &mut Skirmish) {
    game.tick += 1;
    game.run_object_updates();
    game.process_update_destruction();
}

#[test]
fn hole_exposure_requires_its_die_instance_without_an_update_registration() {
    let (mut game, _) = fixture(0.0);
    let unit = &mut game.catalog[0];
    super::super::super::module_updates::validate_registrations(unit).unwrap();
    unit.behavior_modules
        .retain(|module| module.family != "RebuildHoleExposeDie");
    assert!(
        super::super::super::module_updates::validate_registrations(unit)
            .unwrap_err()
            .contains("RebuildHoleExposeDie")
    );
}

#[test]
fn exposure_copies_each_live_geometry_and_name_without_mutating_the_shared_template() {
    let (mut game, kind) = fixture(0.0);
    let second = game.spawn(3, 0, [400.0, 350.0], true);
    game.set_dynamic_geometry(0, [70.0, 43.0, 21.0]);
    game.set_dynamic_geometry(second, [25.0, 12.0, 6.0]);
    game.map_objects.insert("OriginalFirst".into(), 0);
    game.map_objects.insert("OriginalSecond".into(), second);
    let original_major = game.catalog[kind].geometry.major;
    game.catalog[kind].geometry.is_small = true;
    game.kill_object(0, DeathType::Normal);
    let first_hole = game.map_rebuild_hole(0).unwrap();
    game.kill_object(second, DeathType::Normal);
    let second_hole = game.map_rebuild_hole(second).unwrap();
    assert_eq!(
        game.body_geometry(first_hole).major.to_bits(),
        43.0_f32.to_bits()
    );
    assert_eq!(
        game.body_geometry(second_hole).major.to_bits(),
        12.0_f32.to_bits()
    );
    assert_eq!(
        game.catalog[kind].geometry.major.to_bits(),
        original_major.to_bits()
    );
    assert!(!game.body_geometry(first_hole).is_small);
    assert!(game.catalog[kind].geometry.is_small);
    assert_eq!(game.map_objects["OriginalFirst"], first_hole);
    assert_eq!(game.map_objects["OriginalSecond"], second_hole);
    assert_eq!(
        game.entities[first_hole].health.to_bits(),
        80.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[first_hole].max_health.to_bits(),
        500.0_f32.to_bits()
    );
    assert_eq!(
        game.team_membership.team_of(first_hole),
        game.team_membership.team_of(0)
    );
}

#[test]
fn constructor_updates_unstarted_holes_and_zero_delay_never_spawns_a_worker() {
    let (mut game, kind) = fixture(0.0);
    let id = game.spawn(kind, 0, [350.0, 350.0], true);
    let count = game.entities.len();
    let health = game.entities[id].health;
    let amount = (0.1_f32 / 30.0) * game.entities[id].max_health;
    advance(&mut game);
    assert_eq!(
        game.entities[id].health.to_bits(),
        (health + amount).to_bits()
    );
    assert!(game.entities[id].maintenance.holes[0].worker.is_none());
    assert_eq!(game.entities.len(), count);
    game.kill_object(0, DeathType::Normal);
    let exposed = game.map_rebuild_hole(0).unwrap();
    for _ in 0..3 {
        advance(&mut game);
    }
    assert!(game.entities[exposed].maintenance.holes[0].worker.is_none());
    assert!(!game.entities[exposed].masked());
}

#[test]
fn real_delay_truncates_and_free_construct_preserves_native_initial_status_and_worker_pose() {
    let (mut game, _) = fixture(2.9);
    game.set_object_orientation(0, 0.75);
    let cash = game.players[0].cash;
    game.kill_object(0, DeathType::Normal);
    let hole = game.map_rebuild_hole(0).unwrap();
    advance(&mut game);
    assert!(game.entities[hole].maintenance.holes[0].worker.is_none());
    advance(&mut game);
    let state = &game.entities[hole].maintenance.holes[0];
    let worker = state.worker.unwrap();
    let foundation = state.foundation.unwrap();
    assert_eq!(game.players[0].cash, cash);
    assert_eq!(game.entities[worker].facing.to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        game.entities[foundation].facing.to_bits(),
        0.75_f32.to_bits()
    );
    assert_eq!(
        game.entities[foundation].health.to_bits(),
        1.0_f32.to_bits()
    );
    assert!(game.entities[foundation].reconstructing());
    assert!(game.entities[foundation].under_construction());
    assert_eq!(game.entities[foundation].producer, Some(hole));
    assert_eq!(game.entities[foundation].maintenance.builder, Some(worker));
    assert!(game.dozer_is_rebuilding(worker));
    assert!(game.entities[hole].masked());
}

#[test]
fn dead_existing_worker_does_not_restart_but_final_deletion_does() {
    let (mut game, _) = fixture(2.0);
    game.kill_object(0, DeathType::Normal);
    let hole = game.map_rebuild_hole(0).unwrap();
    advance(&mut game);
    advance(&mut game);
    let worker = game.entities[hole].maintenance.holes[0].worker.unwrap();
    game.kill_object(worker, DeathType::Normal);
    advance(&mut game);
    assert_eq!(
        game.entities[hole].maintenance.holes[0].worker,
        Some(worker)
    );
    assert_eq!(game.entities[hole].maintenance.holes[0].remaining, 0);
    game.destroy_maintenance_object(worker);
    game.process_update_destruction();
    advance(&mut game);
    assert!(game.entities[hole].maintenance.holes[0].worker.is_none());
    assert_eq!(game.entities[hole].maintenance.holes[0].remaining, 1);
    assert!(!game.entities[hole].masked());
}

#[test]
fn destruction_retires_generated_worker_and_authored_die_filters_skip_exposure() {
    let (mut game, _) = fixture(1.0);
    game.catalog[3].maintenance.expose_hole[0]
        .conditions
        .death_mask = 0;
    let filtered = game.spawn(3, 0, [400.0, 350.0], true);
    game.kill_object(filtered, DeathType::Normal);
    assert!(game.map_rebuild_hole(filtered).is_none());
    game.kill_object(0, DeathType::Normal);
    let hole = game.map_rebuild_hole(0).unwrap();
    advance(&mut game);
    let worker = game.entities[hole].maintenance.holes[0].worker.unwrap();
    game.destroy_maintenance_object(hole);
    game.process_update_destruction();
    assert!(!game.entities[hole].exists());
    assert!(!game.entities[worker].exists());
}

#[test]
fn masking_a_hole_retains_its_path_claim_until_explicit_removal() {
    let (mut game, kind) = fixture(0.0);
    let hole = game.spawn(kind, 0, [350.0, 350.0], true);
    game.restore_object_obstacle(hole);
    let cell = game.cell_at(game.entities[hole].position).unwrap();
    assert!(game.navigation.is_blocked(cell));
    game.mask_object(hole, true);
    game.rebuild_navigation();
    assert!(game.navigation.is_blocked(cell));
    game.unregister_object_obstacle(hole);
    game.rebuild_navigation();
    assert!(!game.navigation.is_blocked(cell));
}
