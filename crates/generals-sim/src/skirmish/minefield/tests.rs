use super::*;
use crate::skirmish::{DamageType, DeathType, Faction, Kind, tests};

fn mine_rules() -> MineRules {
    MineRules {
        weapon: None,
        detonated_by: 3,
        stop_regeneration: true,
        regenerates: true,
        workers_detonate: false,
        creator_check_frames: 30,
        scoot_frames: 0,
        virtual_mines: 4,
        repeat_move_threshold: 1.0,
        drain_percent: 0.5,
        creation: None,
    }
}

fn setup() -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::China, Kind::Effect);
    unit.name = "RegeneratingNativeMine".into();
    unit.kind_of = vec!["MINE".into()];
    unit.max_health = 100.0;
    unit.initial_health = 100.0;
    unit.behavior.minefields.push(mine_rules());
    tests::register_update(&mut unit, "MinefieldBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces = 0x17;
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let mine = game.spawn(kind, 0, [355.0, 255.0], true);
    let enemy = game.spawn(6, 1, [355.0, 255.0], true);
    (game, mine, enemy)
}

#[test]
fn real_disarm_preserves_regenerating_body_and_healing_reactivates_virtual_mines() {
    let (mut game, mine, _) = setup();
    assert!(game.entities[mine].object_status & (1 << 26) != 0);
    assert!(game.disarm_native_land_mine(mine));
    let native_damage = 100.0_f32 - 0.1;
    assert_eq!(
        game.entities[mine].health.to_bits(),
        (100.0_f32 - native_damage).to_bits()
    );
    assert!(game.entities[mine].masked());
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 0);
    assert!(
        game.model_flags(&game.entities[mine], [0.5, 0.25])
            .contains("RUBBLE")
    );
    game.apply_healing(mine, None, 50.0);
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 2);
    assert!(!game.entities[mine].masked());
    assert!(
        !game
            .model_flags(&game.entities[mine], [0.5, 0.25])
            .contains("RUBBLE")
    );
    assert!(game.entities[mine].exists());
}

#[test]
fn collision_tracks_three_dimensional_displacement_before_repeated_detonation() {
    let (mut game, mine, enemy) = setup();
    game.object_contact(mine, enemy, false);
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 3);
    game.object_contact(mine, enemy, false);
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 3);
    let mut point = game.object_position(enemy);
    point[2] += 1.0;
    game.set_object_position(enemy, point);
    game.object_contact(mine, enemy, false);
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 3);
    point[2] += 0.01;
    game.set_object_position(enemy, point);
    game.object_contact(mine, enemy, false);
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 2);
}

#[test]
fn creator_death_disables_first_auto_heal_and_drains_without_detonating() {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::China, Kind::Effect);
    unit.name = "DrainingNativeMine".into();
    unit.behavior.minefields.push(mine_rules());
    unit.maintenance
        .auto_heal
        .push(tests::self_heal(10.0, 1, 1, &[]));
    unit.maintenance.auto_heal[0].starts_active = true;
    tests::register_update(&mut unit, "MinefieldBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces = 0x17;
    tests::register_update(&mut unit, "AutoHealBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces = 0x85;
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let mine = game.spawn(kind, 0, [355.0, 255.0], true);
    game.entities[mine].producer = Some(0);
    game.apply_native_damage(0, None, game.entities[0].health, DeathType::Normal);
    game.tick = 1;
    game.run_object_updates();
    let state = &game.entities[mine].mines.mines[0];
    assert!(!state.regenerates);
    assert!(state.draining);
    let health = game.entities[mine].health;
    assert!(health < game.entities[mine].max_health);
    assert_eq!(game.entities[mine].damage_state.retained_source, Some(mine));
    game.tick = 2;
    game.run_object_updates();
    assert!(game.entities[mine].health < health);
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn damage_uses_native_fast_ceil_and_disarm_of_nonregenerating_mine_is_deferred() {
    let (mut game, mine, _) = setup();
    game.apply_damage(
        mine,
        None,
        50.0,
        DamageType::Unresistable,
        DeathType::Normal,
    );
    // fast_float_ceil(4 * 50 / 100) adds almost-one in binary32, yielding 3.
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 3);
    game.entities[mine].mines.mines[0].regenerates = false;
    assert!(game.disarm_native_land_mine(mine));
    assert!(game.entities[mine].exists());
    assert!(game.update_objects[game.entities[mine].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.entities[mine].exists());
}

#[test]
fn mine_clearer_immunity_survives_contact_and_expires_strictly_after_two_frames() {
    let (mut game, mine, enemy) = setup();
    game.entities[enemy].weapon_set.mine_clearing = true;
    game.entities[enemy].ai.command.goal_object = Some(mine);
    game.tick = 1;
    game.object_contact(mine, enemy, false);
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 4);
    game.entities[enemy].weapon_set.mine_clearing = false;
    game.tick = 3;
    game.update_land_mine(mine, 0);
    game.object_contact(mine, enemy, false);
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 4);
    game.tick = 5;
    game.update_land_mine(mine, 0);
    assert_eq!(
        game.entities[mine].mines.mines[0].immunes[0],
        (Some(enemy), 3)
    );
    game.tick = 6;
    game.update_land_mine(mine, 0);
    game.object_contact(mine, enemy, false);
    assert_eq!(game.entities[mine].mines.mines[0].remaining, 3);
}

fn generator_fixture(placement: Placement, circular: bool) -> (Skirmish, usize) {
    let (mut game, mine, _) = setup();
    let mine_kind = game.entities[mine].type_index;
    game.catalog[mine_kind].geometry = crate::collision_geometry::Geometry {
        shape: crate::collision_geometry::Shape::Cylinder,
        is_small: true,
        major: 2.0,
        minor: 2.0,
        height: 1.0,
    };
    game.catalog[mine_kind].geometry_bounds = crate::skirmish::GeometryBounds {
        center_height: 0.5,
        sphere_radius: (4.0_f32 + 0.25).sqrt(),
        max_height: 1.0,
    };
    let mut building = tests::fixture_type(Faction::China, Kind::PowerPlant);
    building.name = "MineGeneratorFixture".into();
    building.geometry = crate::collision_geometry::Geometry {
        shape: crate::collision_geometry::Shape::Box,
        is_small: false,
        major: 10.0,
        minor: 10.0,
        height: 20.0,
    };
    building.behavior.mine_generators.push(GeneratorRules {
        mine_name: "RegeneratingNativeMine".into(),
        upgraded_name: "RegeneratingNativeMine".into(),
        emp_upgrade: Some("Upgrade_ChinaEMPMines".into()),
        generation_fx: None,
        distance: 4.0,
        density: 0.01,
        placement,
        on_death: false,
        circular,
        upgradable: true,
        jitter: 0.0,
        skip_under_structure: 0.33,
    });
    tests::register_update(&mut building, "GenerateMinefieldBehavior");
    building.behavior_modules.last_mut().unwrap().interfaces = 0x83;
    let kind = game.catalog.len();
    game.catalog.push(building);
    let generator = game.spawn(kind, 0, [700.0, 700.0], true);
    (game, generator)
}

#[test]
fn generated_border_and_emp_upgrade_keep_original_mine_lifetime_and_object_scope() {
    let (mut game, generator) = generator_fixture(Placement::Border, false);
    game.give_self_upgrade_instance(generator, "GenerateMinefieldBehavior", 0);
    let first = game.entities[generator].mines.generators[0].mines.clone();
    // Each 28-unit edge uses fast_float_ceil(28/4)=8 intervals, skipping its start.
    assert_eq!(first.len(), 32);
    for &id in &first {
        let point = game.entities[id].position;
        assert!(
            (point[0] - 700.0).abs().to_bits() == 14.0_f32.to_bits()
                || (point[1] - 700.0).abs().to_bits() == 14.0_f32.to_bits()
        );
        assert_eq!(game.entities[id].producer, Some(generator));
    }
    game.players[0]
        .upgrades
        .push("Upgrade_ChinaEMPMines".into());
    game.update_mine_generator(generator, 0);
    assert!(!game.entities[generator].mines.generators[0].upgraded);
    game.grant_native_object_upgrade(generator, "Upgrade_ChinaEMPMines");
    game.update_mine_generator(generator, 0);
    assert!(game.entities[generator].mines.generators[0].upgraded);
    assert_eq!(game.entities[generator].mines.generators[0].mines.len(), 32);
    assert!(
        first
            .iter()
            .all(|&id| game.update_objects[game.entities[id].update_owner].destroy_requested)
    );
    game.process_update_destruction();
    assert!(first.iter().all(|&id| !game.entities[id].exists()));
}

#[test]
fn smart_circular_border_uses_bounding_circle_and_runs_at_least_once() {
    let (mut game, generator) = generator_fixture(
        Placement::Smart {
            skip_interior: true,
        },
        true,
    );
    game.give_self_upgrade_instance(generator, "GenerateMinefieldBehavior", 0);
    let mines = &game.entities[generator].mines.generators[0].mines;
    assert_eq!(mines.len(), 26);
    let squared = [10.0_f32 * 10.0, 10.0 * 10.0];
    let radius = (squared[0] + squared[1]).sqrt() + 2.0;
    assert_eq!(
        game.object_position(mines[0]).map(f32::to_bits),
        [700.0 + radius, 700.0, 0.0].map(f32::to_bits)
    );
    let count = game.entities.len();
    game.give_self_upgrade_instance(generator, "GenerateMinefieldBehavior", 0);
    assert_eq!(game.entities.len(), count);
}

#[test]
fn scoot_lands_above_bridge_and_repeated_launch_retains_vertical_velocity() {
    let (mut game, mine, _) = setup();
    let kind = game.entities[mine].type_index;
    game.catalog[kind].behavior.minefields[0].scoot_frames = 10;
    game.gravity = -1.0;
    let layer = game
        .movement_terrain
        .add_bridge(
            crate::terrain::BridgeGeometry {
                from: [100.0, 200.0, 30.0],
                to: [300.0, 200.0, 30.0],
                corners: [
                    [100.0, 220.0, 30.0],
                    [100.0, 180.0, 30.0],
                    [300.0, 220.0, 30.0],
                    [300.0, 180.0, 30.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    game.set_mine_scoot(mine, [200.0, 200.0, 40.0], [220.0, 200.0, 0.0]);
    for tick in 1..=10 {
        game.tick = tick;
        game.update_land_mine(mine, 0);
    }
    assert_eq!(game.entities[mine].layer, layer);
    assert_eq!(
        game.object_position(mine).map(f32::to_bits),
        [220.0_f32, 200.0, 31.0].map(f32::to_bits)
    );
    assert_eq!(game.entities[mine].mines.mines[0].scoot_frames, 0);
    game.set_mine_scoot(mine, [220.0, 200.0, 31.0], [240.0, 200.0, 0.0]);
    assert_eq!(
        game.entities[mine].mines.mines[0].velocity[2].to_bits(),
        (-10.0_f32).to_bits()
    );
    game.update_land_mine(mine, 0);
    assert_eq!(
        game.entities[mine].mines.mines[0].velocity[2].to_bits(),
        (-11.0_f32).to_bits()
    );
}

#[test]
fn mine_border_rejects_water_across_grid_boundary_before_orientation_rng() {
    let (mut game, generator) = generator_fixture(Placement::Border, true);
    game.set_object_position(generator, [0.0, 50.0, 0.0]);
    game.movement_terrain.set_water_polygons(vec![
        crate::terrain::WaterPolygon::new(vec![
            [-100, -100, 10],
            [100, -100, 10],
            [100, 150, 10],
            [-100, 150, 10],
        ])
        .unwrap(),
    ]);
    let mut expected_random = game.random;
    // Radius fourteen, mine diameter four: the source circle emits 22
    // candidate positions. Each uses two jitter draws, even at zero jitter;
    // underwater rejection precedes every orientation draw and constructor.
    for _ in 0..44 {
        crate::skirmish::startup::random_real(&mut expected_random, -0.0, 0.0);
    }
    let count = game.entities.len();
    game.give_self_upgrade_instance(generator, "GenerateMinefieldBehavior", 0);
    assert!(
        game.entities[generator].mines.generators[0]
            .mines
            .is_empty()
    );
    assert_eq!(game.entities.len(), count);
    assert_eq!(game.random, expected_random);
}
