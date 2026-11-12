use super::{Cell, Grid, Kind, game};
use crate::collision_geometry::Shape;
use std::f32::consts::FRAC_PI_2;

#[test]
fn dragged_facing_can_make_an_illegal_default_site_legal_and_is_used_by_the_spawned_footprint() {
    let mut game = game();
    let worker = game
        .entities
        .iter()
        .position(|unit| unit.owner == 0 && game.catalog[unit.type_index].kind == Kind::Builder)
        .unwrap();
    let factory = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[factory].geometry.shape = Shape::Box;
    game.catalog[factory].geometry.major = 40.0;
    game.catalog[factory].geometry.minor = 62.0;
    game.catalog[factory].placement_angle = 0.0;
    let site = [385.0, 305.0];
    game.grid = Grid::new(80, 60, [Cell { x: 38, y: 36 }]).unwrap();
    game.rebuild_navigation();
    game.move_to(worker, [285.0, 305.0]).unwrap();
    for _ in 0..180 {
        game.step();
    }
    assert!(matches!(
        game.visibility_cell(0, site),
        crate::skirmish::visibility::Status::Clear
    ));
    assert!(!game.can_place(factory, site));
    assert!(game.can_place_facing(factory, site, FRAC_PI_2));
    assert!(matches!(
        game.build(worker, factory, site),
        Err("Building site is obstructed or outside the map")
    ));
    let cash = game.players[0].cash;
    let building = game.build_facing(worker, factory, site, FRAC_PI_2).unwrap();
    assert_eq!(
        game.entities[building].facing.to_bits(),
        FRAC_PI_2.to_bits()
    );
    assert_eq!(game.players[0].cash, cash - game.catalog[factory].cost);
    assert!(game.navigation.is_blocked(Cell { x: 44, y: 30 }));
    assert_eq!(
        game.catalog[factory].placement_angle.to_bits(),
        0.0_f32.to_bits()
    );
}

#[test]
fn invalid_drag_angle_is_rejected_without_charging_or_creating_a_foundation() {
    let mut game = game();
    let worker = game
        .entities
        .iter()
        .position(|unit| unit.owner == 0 && game.catalog[unit.type_index].kind == Kind::Builder)
        .unwrap();
    let factory = game.type_for(0, Kind::Factory).unwrap();
    let site = [385.0, 305.0];
    let cash = game.players[0].cash;
    let objects = game.entities.len();
    for angle in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!game.can_place_facing(factory, site, angle));
        assert!(matches!(
            game.build_facing(worker, factory, site, angle),
            Err("Building site is obstructed or outside the map")
        ));
        assert_eq!(game.players[0].cash, cash);
        assert_eq!(game.entities.len(), objects);
    }
}

#[test]
fn original_close_center_collision_rejects_crossed_building_footprints_without_charging() {
    let mut game = game();
    let worker = game
        .entities
        .iter()
        .position(|unit| game.catalog[unit.type_index].kind == Kind::Builder)
        .unwrap();
    let existing = game.type_for(0, Kind::Barracks).unwrap();
    let proposed = game.type_for(0, Kind::Factory).unwrap();
    // Installed AmericaPowerPlant and AmericaFireBase bounds: neither set of
    // corners lies inside the other at the same original placement angle.
    for (index, major, minor) in [(existing, 22.0, 30.0), (proposed, 26.0, 26.0)] {
        game.catalog[index].geometry.shape = Shape::Box;
        game.catalog[index].geometry.major = major;
        game.catalog[index].geometry.minor = minor;
        game.catalog[index].placement_angle = -std::f32::consts::FRAC_PI_4;
    }
    let site = [385.0, 305.0];
    game.spawn(existing, 0, site, true);
    let cash = game.players[0].cash;
    let count = game.entities.len();
    assert!(!game.can_place(proposed, site));
    assert!(matches!(
        game.build(worker, proposed, site),
        Err("Building site is obstructed or outside the map")
    ));
    assert_eq!(game.players[0].cash, cash);
    assert_eq!(game.entities.len(), count);
    assert_ne!(game.placement_near(proposed, site), Some(site));
}

#[test]
fn close_center_preflight_preserves_strict_boundary_and_native_corner_only_fallback() {
    let first = super::Footprint {
        position: [0.0, 0.0],
        major: 10.0,
        minor: 1.0,
        angle: 0.0,
        shape: crate::collision_geometry::Shape::Box,
    };
    let mut second = super::Footprint {
        position: [f32::from_bits(1.0_f32.to_bits() - 1), 0.0],
        major: 1.0,
        minor: 10.0,
        angle: 0.0,
        shape: crate::collision_geometry::Shape::Box,
    };
    assert!(crate::skirmish::movement::footprints_overlap(
        &first, &second
    ));
    second.position[0] = 1.0;
    assert!(!crate::skirmish::movement::footprints_overlap(
        &first, &second
    ));
    second.position[0] = f32::from_bits(1.0_f32.to_bits() + 1);
    assert!(!crate::skirmish::movement::footprints_overlap(
        &first, &second
    ));
}

#[test]
fn cylinder_box_preflight_retains_the_distinct_sphere_overlap_rule() {
    let rectangle = super::Footprint {
        position: [0.0; 2],
        major: 26.0,
        minor: 26.0,
        angle: 0.0,
        shape: Shape::Box,
    };
    let mut round = super::Footprint {
        position: [0.0; 2],
        major: 22.0,
        minor: 22.0,
        angle: std::f32::consts::FRAC_PI_4,
        shape: Shape::Cylinder,
    };
    // Both native rectangular approximations cross without containing a corner.
    // Only non-sphere pairs pass the original close-center preflight.
    assert!(crate::skirmish::movement::footprints_overlap(
        &rectangle, &round
    ));
    assert!(crate::skirmish::movement::footprints_overlap(
        &round, &rectangle
    ));
    round.shape = Shape::Sphere;
    assert!(!crate::skirmish::movement::footprints_overlap(
        &rectangle, &round
    ));
    assert!(!crate::skirmish::movement::footprints_overlap(
        &round, &rectangle
    ));
}

#[test]
fn cylinder_at_a_crossed_building_site_rejects_construction_without_spending() {
    let mut game = game();
    let worker = game
        .entities
        .iter()
        .position(|unit| game.catalog[unit.type_index].kind == Kind::Builder)
        .unwrap();
    let existing = game.type_for(0, Kind::Barracks).unwrap();
    let proposed = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[existing].geometry.shape = Shape::Cylinder;
    game.catalog[existing].geometry.major = 22.0;
    game.catalog[existing].geometry.minor = 22.0;
    game.catalog[proposed].geometry.shape = Shape::Box;
    game.catalog[proposed].geometry.major = 26.0;
    game.catalog[proposed].geometry.minor = 26.0;
    let site = [385.0, 305.0];
    let obstacle = game.spawn(existing, 0, site, true);
    game.entities[obstacle].facing = std::f32::consts::FRAC_PI_4;
    let cash = game.players[0].cash;
    let count = game.entities.len();
    assert!(!game.can_place_facing(proposed, site, 0.0));
    assert!(matches!(
        game.build_facing(worker, proposed, site, 0.0),
        Err("Building site is obstructed or outside the map")
    ));
    assert_eq!(game.players[0].cash, cash);
    assert_eq!(game.entities.len(), count);
}

#[test]
fn human_preview_and_commit_accept_idle_friendly_tank_and_builder_inside_footprint() {
    let mut game = game();
    let site = [385.0, 305.0];
    let worker_type = game.type_for(0, Kind::Builder).unwrap();
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let worker = game.spawn(worker_type, 0, site, true);
    let tank = game.spawn(tank_type, 0, [site[0] + 10.0, site[1]], true);
    game.update_ground_contacts();
    game.update_visibility();
    let angle = game.catalog[building_type].placement_angle;
    assert!(game.can_build_facing(worker, building_type, site, angle));
    let cash = game.players[0].cash;
    let start = game.entities[tank].position;
    let building = game.build(worker, building_type, site).unwrap();
    assert_eq!(
        game.players[0].cash,
        cash - game.catalog[building_type].cost
    );
    assert!(!game.entities[building].complete());
    assert_eq!(game.entities[worker].build_task_target(), Some(building));
    assert!(matches!(
        game.entities[tank].ai.order,
        super::Order::Move(_)
    ));
    assert_eq!(
        game.entities[tank].position.map(f32::to_bits),
        start.map(f32::to_bits)
    ); // command, never teleport
    assert!(!game.can_build_facing(worker, building_type, site, angle));
}

#[test]
fn human_preview_rejects_disabled_and_enemy_but_not_ordinary_friendly_orders() {
    let mut game = game();
    let site = [385.0, 305.0];
    let worker_type = game.type_for(0, Kind::Builder).unwrap();
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let worker = game.spawn(worker_type, 0, [325.0, 305.0], true);
    let tank = game.spawn(tank_type, 0, site, true);
    game.update_ground_contacts();
    game.update_visibility();
    let angle = game.catalog[building_type].placement_angle;
    game.entities[tank].ai.order = super::Order::Attack(0);
    assert!(game.can_build_facing(worker, building_type, site, angle));
    game.set_hacked_until(
        tank,
        crate::skirmish::ai_dispatch::native_frame(game.tick + 100),
    );
    assert!(!game.can_build_facing(worker, building_type, site, angle));
    game.clear_hacked_disable(tank);
    game.entities[tank].owner = 1;
    assert!(!game.can_build_facing(worker, building_type, site, angle));
    let cash = game.players[0].cash;
    assert!(game.build(worker, building_type, site).is_err());
    assert_eq!(game.players[0].cash, cash);
    game.entities[tank].owner = 0;
    assert!(game.can_build_facing(worker, building_type, site, angle));
}

#[test]
fn source_displacement_reports_unmovable_without_skipping_later_friendly_commands() {
    let mut game = game();
    let site = [385.0, 305.0, 0.0];
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let enemy = game.spawn(tank_type, 1, [385.0, 305.0], true);
    let friend = game.spawn(tank_type, 0, [395.0, 305.0], true);
    game.update_ground_contacts();
    assert!(!game.move_objects_for_construction(0, building_type, site, 0.0));
    assert!(matches!(game.entities[enemy].ai.order, super::Order::Idle));
    assert!(matches!(
        game.entities[friend].ai.order,
        super::Order::Move(_)
    ));
}

#[test]
fn human_retry_keeps_cash_when_an_allied_object_has_no_ai() {
    let mut game = game();
    let site = [385.0, 305.0];
    let builder_type = game.type_for(0, Kind::Builder).unwrap();
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[tank_type].ai.update = super::UnitAiUpdate::Absent;
    let builder = game.spawn(builder_type, 0, [325.0, 305.0], true);
    let unmovable = game.spawn(tank_type, 0, site, true);
    game.update_ground_contacts();
    game.update_visibility();
    let angle = game.catalog[building_type].placement_angle;
    assert!(game.can_build_facing(builder, building_type, site, angle));
    let before = game.players[0].cash;
    assert!(matches!(
        game.build(builder, building_type, site),
        Err("Construction area contains objects that cannot move")
    ));
    assert_eq!(game.players[0].cash, before);
    // Remove the obstruction through ordinary destruction before the retry.
    game.destroy_maintenance_object(unmovable);
    game.process_update_destruction();
    let building = game.build(builder, building_type, site).unwrap();
    assert!(game.entities[unmovable].maintenance.deleted);
    assert!(!game.entities[building].complete());
    assert_eq!(
        game.players[0].cash,
        before - game.build_cost(0, building_type)
    );
}

#[test]
fn ai_construction_bypasses_unmovable_location_and_withdraws_available_cash() {
    let mut game = game();
    let site = [385.0, 305.0];
    let builder_type = game.type_for(0, Kind::Builder).unwrap();
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let builder = game.spawn(builder_type, 0, [325.0, 305.0], true);
    let enemy = game.spawn(tank_type, 1, site, true);
    game.update_ground_contacts();
    game.players[0].cash = 1;
    let building = game
        .build_at_angle(builder, building_type, site, 0.0, true)
        .unwrap();
    assert_eq!(game.players[0].cash, 0);
    assert!(game.entities[enemy].alive());
    assert!(!game.entities[building].complete());
    assert_eq!(game.entities[builder].build_task_target(), Some(building));
}

#[test]
fn unavailable_friend_consumes_displacement_rng_before_private_move_rejects() {
    let mut game = game();
    let site = [385.0, 305.0, 0.0];
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let friend = game.spawn(tank_type, 0, [site[0], site[1]], true);
    let enemy = game.spawn(tank_type, 1, [site[0] + 10.0, site[1]], true);
    game.set_hacked_until(
        friend,
        crate::skirmish::ai_dispatch::native_frame(game.tick + 100),
    );
    game.update_ground_contacts();
    let mut expected = game.random;
    super::startup::random_real(&mut expected, 0.5, 1.5);
    super::startup::random_real(&mut expected, -std::f32::consts::PI, std::f32::consts::PI);
    assert!(!game.move_objects_for_construction(0, building_type, site, 0.0));
    assert_eq!(game.random, expected);
    assert!(matches!(game.entities[friend].ai.order, super::Order::Idle));
    assert!(matches!(game.entities[enemy].ai.order, super::Order::Idle));
}
