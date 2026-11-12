use crate::skirmish::{Kind, economy::DockTarget, tests::game};

#[test]
fn supply_choice_uses_terrain_and_aircraft_height_without_geometry_offsets() {
    let mut game = game();
    game.set_object_position(1, [100.0, 100.0, 0.0]);
    game.set_object_position(game.supplies[0].object, [110.0, 100.0, 0.0]);
    game.set_object_position(game.supplies[1].object, [100.0, 130.0, 0.0]);
    let mut heights = vec![0.0; 81 * 61];
    heights[10 * 81 + 11] = 80.0;
    game.set_projectile_terrain(81, 61, [0.0, 0.0], heights, 40.0)
        .unwrap();
    game.set_object_position(game.supplies[0].object, [110.0, 100.0, 80.0]);
    game.update_visibility();
    assert_eq!(game.best_supply(1), Some(1));
    game.set_object_position(1, [100.0, 100.0, 80.0]);
    game.update_visibility();
    assert_eq!(game.best_supply(1), Some(0));
    let unit = game.entities[1].type_index;
    game.catalog[unit].geometry_bounds.center_height = 10_000.0;
    game.catalog[unit].geometry_bounds.sphere_radius = 10_000.0;
    game.update_visibility();
    assert_eq!(game.best_supply(1), Some(0));
}

#[test]
fn warehouse_scan_is_strict_3d_but_a_valid_preference_bypasses_range() {
    let mut game = game();
    game.set_object_position(1, [100.0, 100.0, 0.0]);
    game.set_object_position(game.supplies[0].object, [103.0, 100.0, 4.0]);
    game.supplies[1].boxes = 0;
    let unit = game.entities[1].type_index;
    game.catalog[unit].supply.scan_distance = 5.0;
    game.update_visibility();
    assert_eq!(game.best_supply(1), None);
    game.entities[1].ai.supply.preferred_dock = Some(DockTarget::Warehouse(0));
    game.update_visibility();
    assert_eq!(game.best_supply(1), Some(0));
    game.supplies[0].boxes = 0;
    game.update_visibility();
    assert_eq!(game.best_supply(1), None);
}

#[test]
fn supply_centers_have_no_scan_cap_and_equal_costs_keep_registration_order() {
    let mut game = game();
    game.set_object_position(1, [100.0, 100.0, 0.0]);
    let collector = game.entities[1].type_index;
    game.catalog[collector].supply.scan_distance = 1.0;
    let center = game.type_for(0, Kind::SupplyCenter).unwrap();
    let elevated = game.spawn(center, 0, [110.0, 100.0], true);
    let ground = game.spawn(center, 0, [100.0, 130.0], true);
    game.native_build_complete(elevated);
    game.native_build_complete(ground);
    game.entities[1].cargo = 1;
    let mut heights = vec![0.0; 81 * 61];
    heights[10 * 81 + 11] = 80.0;
    game.set_projectile_terrain(81, 61, [0.0, 0.0], heights, 40.0)
        .unwrap();
    game.set_object_position(elevated, [110.0, 100.0, 80.0]);
    game.update_visibility();
    assert_eq!(game.closest_supply_center(1), Some(ground));
    game.entities[1].ai.supply.preferred_dock = Some(DockTarget::Center(elevated));
    game.update_visibility();
    assert_eq!(game.closest_supply_center(1), Some(elevated));
    game.entities[1].ai.supply.preferred_dock = None;
    game.set_object_position(elevated, [100.0, 70.0, 0.0]);
    game.update_visibility();
    assert_eq!(game.closest_supply_center(1), Some(elevated));
    game.set_object_position(game.supplies[0].object, [100.0, 70.0, 0.0]);
    game.set_object_position(game.supplies[1].object, [100.0, 130.0, 0.0]);
    game.catalog[collector].supply.scan_distance = 31.0;
    game.update_visibility();
    assert_eq!(game.best_supply(1), Some(0));
}

#[test]
fn center_completion_order_controls_ties_and_destroyed_registrations_are_pruned_lazily() {
    let mut game = game();
    game.set_object_position(1, [100.0, 100.0, 0.0]);
    game.entities[1].cargo = 1;
    let kind = game.type_for(0, Kind::SupplyCenter).unwrap();
    let first = game.spawn(kind, 0, [100.0, 70.0], true);
    let second = game.spawn(kind, 0, [100.0, 130.0], true);
    game.update_visibility();
    assert_eq!(game.closest_supply_center(1), None);
    game.native_build_complete(second);
    game.native_build_complete(first);
    game.native_build_complete(second);
    assert_eq!(game.players[0].supply_centers, [second, first]);
    game.update_visibility();
    assert_eq!(game.closest_supply_center(1), Some(second));
    game.destroy_maintenance_object(second);
    game.process_update_destruction();
    assert!(game.players[0].supply_centers.contains(&second));
    game.update_visibility();
    assert_eq!(game.closest_supply_center(1), Some(first));
    assert_eq!(game.players[0].supply_centers, [first]);
    assert!(game.players[1].supply_centers.contains(&second));
    game.entities[1].cargo = 0;
    game.update_visibility();
    assert_eq!(game.closest_supply_center(1), None);
}
