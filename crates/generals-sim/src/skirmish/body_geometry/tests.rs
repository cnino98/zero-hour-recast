use super::*;
use crate::collision_geometry::Body;
use crate::skirmish::{
    Faction, Kind,
    tests::{fixture_type, game},
};

#[test]
fn native_z_mutation_preserves_sphere_height_but_changes_its_radius() {
    let geometry = Geometry {
        shape: Shape::Sphere,
        is_small: true,
        major: 13.0,
        minor: 7.0,
        height: 24.0,
    };
    let rubble = with_height(&geometry, 2.0);
    assert_eq!(rubble.geometry.major.to_bits(), 2.0_f32.to_bits());
    assert_eq!(rubble.geometry.minor.to_bits(), 7.0_f32.to_bits());
    assert_eq!(rubble.geometry.height.to_bits(), 24.0_f32.to_bits());
    assert!(rubble.geometry.is_small);
    assert_eq!(rubble.bounds.center_height.to_bits(), 0.0_f32.to_bits());
    assert_eq!(rubble.bounds.sphere_radius.to_bits(), 2.0_f32.to_bits());
    assert_eq!(rubble.bounds.max_height.to_bits(), 2.0_f32.to_bits());
}

#[test]
fn box_and_cylinder_recompute_their_distinct_native_bounding_spheres() {
    for (shape, expected) in [(Shape::Box, 26.0_f32.sqrt()), (Shape::Cylinder, 3.0)] {
        let geometry = Geometry {
            shape,
            is_small: false,
            major: 3.0,
            minor: 4.0,
            height: 24.0,
        };
        let rubble = with_height(&geometry, 2.0);
        assert_eq!(rubble.geometry.major.to_bits(), 3.0_f32.to_bits());
        assert_eq!(rubble.geometry.minor.to_bits(), 4.0_f32.to_bits());
        assert_eq!(rubble.geometry.height.to_bits(), 2.0_f32.to_bits());
        assert_eq!(rubble.bounds.center_height.to_bits(), 1.0_f32.to_bits());
        assert_eq!(rubble.bounds.sphere_radius.to_bits(), expected.to_bits());
    }
}

#[test]
fn changing_dynamic_footprint_preserves_collision_and_replaces_partition_coverage() {
    let (mut game, first, _) = structure();
    game.update_ground_contacts();
    game.set_dynamic_geometry(first, [20.0, 5.0, 7.0]);
    assert!(!game.body_no_collisions(first));
    assert_eq!(game.body_geometry(first).major.to_bits(), 5.0_f32.to_bits());
    assert_eq!(game.body_geometry(first).minor.to_bits(), 7.0_f32.to_bits());
    assert_eq!(
        game.body_geometry_bounds(first).sphere_radius.to_bits(),
        174.0_f32.sqrt().to_bits()
    );
    game.update_ground_contacts();
    assert!(
        game.partition
            .outward_objects(game.entities[first].position)
            .contains(&first)
    );
    game.set_rubble_geometry(first);
    game.set_dynamic_geometry(first, [40.0, 10.0, 10.0]);
    assert!(game.body_no_collisions(first));
}

fn structure() -> (Skirmish, usize, usize) {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.maintenance.rubble_height = Some(2.0);
    unit.geometry = Geometry {
        shape: Shape::Box,
        is_small: false,
        major: 30.0,
        minor: 40.0,
        height: 120.0,
    };
    unit.geometry_bounds = GeometryBounds {
        center_height: 60.0,
        sphere_radius: 6100.0_f32.sqrt(),
        max_height: 120.0,
    };
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let first = game.spawn(kind, game.neutral_owner(), [355.0, 205.0], true);
    let second = game.spawn(kind, game.neutral_owner(), [455.0, 205.0], true);
    (game, first, second)
}

#[test]
fn rubble_is_per_instance_and_positive_bridge_healing_does_not_restore_collision() {
    let (mut game, first, second) = structure();
    let kind = game.entities[first].type_index;
    game.catalog[kind].kind_of.push("BRIDGE".into());
    game.entities[first].health = 0.0;
    assert!(game.set_rubble_geometry(first));
    assert!(!game.set_rubble_geometry(first));
    assert!(game.body_no_collisions(first));
    assert!(!game.body_no_collisions(second));
    assert_eq!(game.target_center(first)[2].to_bits(), 1.0_f32.to_bits());
    assert_eq!(game.target_center(second)[2].to_bits(), 60.0_f32.to_bits());
    game.apply_healing(first, Some(second), 10.0);
    assert!(game.entities[first].alive());
    assert!(game.body_no_collisions(first));
    assert_eq!(
        game.body_geometry(first).height.to_bits(),
        2.0_f32.to_bits()
    );
    assert_eq!(
        game.catalog[kind].geometry.height.to_bits(),
        120.0_f32.to_bits()
    );
}

#[test]
fn lethal_structure_keeps_partition_membership_but_projectiles_escape_rubble() {
    let (mut game, building, _) = structure();
    let launcher = game.spawn(5, 0, [355.0, 205.0], true);
    let missile_geometry = Geometry {
        shape: Shape::Sphere,
        is_small: true,
        major: 1.0,
        minor: 1.0,
        height: 1.0,
    };
    let projectile = Body {
        geometry: &missile_geometry,
        position: [355.0, 205.0, 1.0],
        angle: 0.0,
    };
    assert!(game.missile_contact(&projectile, building, launcher, None, u16::MAX));
    game.apply_hit(building, launcher, 2000.0);
    assert!(game.retained_rubble(building));
    game.update_ground_contacts();
    assert!(
        game.partition
            .outward_objects([355.0, 205.0])
            .contains(&building)
    );
    assert!(!game.missile_contact(&projectile, building, launcher, None, u16::MAX));
    // An explicit victim does not override PartitionContactList's status gate.
    assert!(!game.missile_contact(&projectile, building, launcher, Some(building), u16::MAX));
}
