//! `GameLogic` map Z, `Thing` height, weapon snapshots and immutable structure lifetime.
use super::*;
use crate::skirmish::{
    Kind, MapObstacle,
    tests::{fixture_type, game},
};

fn placed_structure(offset: f32) -> Skirmish {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.name = "PlacedGarage".into();
    unit.maintenance.rubble_height = Some(5.0);
    unit.scoring.victory = false;
    game.catalog.push(unit);
    let neutral = game.layout.neutral_objects.len();
    game.layout.neutral_objects.push(NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        original_team: "teamPlyrCivilian".into(),
        script_name: Some("AuthoredGarage".into()),
        name: "PlacedGarage".into(),
        position: [355.0, 205.0, offset],
        angle: 0.25,
        script_targetable: false,
        indestructible: false,
        selectable: None,
    });
    game.layout
        .obstacle_order
        .push(MapObstacle::Neutral(neutral));
    game
}

#[test]
fn negative_map_height_survives_terrain_installation_capture_death_and_restart() {
    let mut game = placed_structure(-2.0);
    game.reset_sandbox(Faction::Gla).unwrap();
    let building = game.named_map_object("AuthoredGarage").unwrap();
    assert_eq!(
        game.object_position(building)[2].to_bits(),
        (-2.0_f32).to_bits()
    );
    // Client installs original height samples after creating the simulation.
    game.set_projectile_terrain(81, 61, [0.0; 2], vec![18.75; 81 * 61], 40.0)
        .unwrap();
    assert_eq!(
        game.object_position(building)[2].to_bits(),
        16.75_f32.to_bits()
    );
    assert_eq!(
        game.height_above_ground(building).to_bits(),
        (-2.0_f32).to_bits()
    );
    assert!(
        game.navigation
            .is_blocked(game.cell_at([355.0, 205.0]).unwrap())
    );
    assert!(game.move_to(building, [400.0, 205.0]).is_err());
    game.transfer_maintenance_owner(building, 0);
    assert_eq!(
        game.object_position(building)[2].to_bits(),
        16.75_f32.to_bits()
    );
    game.apply_native_damage(building, None, 2000.0, crate::skirmish::DeathType::Normal);
    assert!(game.retained_rubble(building));
    assert_eq!(
        game.object_position(building)[2].to_bits(),
        16.75_f32.to_bits()
    );
    game.reset_sandbox(Faction::Gla).unwrap();
    let restarted = game.named_map_object("AuthoredGarage").unwrap();
    assert!(game.entities[restarted].alive());
    assert_eq!(game.entities[restarted].owner, game.neutral_owner());
    assert_eq!(
        game.object_position(restarted)[2].to_bits(),
        16.75_f32.to_bits()
    );
}

#[test]
fn authored_height_reaches_real_weapon_launch_and_target_snapshots() {
    let mut game = placed_structure(-2.0);
    let building_type = game.catalog.len() - 1;
    game.catalog[building_type].weapon = fixture_type(Faction::Gla, Kind::Tank).weapon;
    game.reset_sandbox(Faction::Gla).unwrap();
    let building = game.named_map_object("AuthoredGarage").unwrap();
    let infantry = game.spawn(5, 0, [305.0, 205.0], true);
    game.fire_weapon(infantry, building);
    let shot = game.presentation.shots.last().unwrap();
    assert_eq!(shot.target, Some(building));
    assert_eq!(shot.target_position[2].to_bits(), 3.0_f32.to_bits());
    assert_eq!(
        game.launch_transform(
            building,
            crate::skirmish::WeaponBinding {
                type_index: game.entities[building].type_index,
                weapon_index: 0,
                weapon_slot: 0,
            },
            0
        )
        .position[2]
            .to_bits(),
        (-2.0_f32).to_bits()
    );
    game.fire_weapon(building, infantry);
    let shot = game.presentation.shots.last().unwrap();
    assert_eq!(shot.source, building);
    assert_eq!(shot.source_position[2].to_bits(), (-2.0_f32).to_bits());
}

#[test]
fn positive_authored_structure_height_preserves_ground_offset_and_footprint_boundary() {
    for offset in [0.25, 10.0, 10.25] {
        let mut game = placed_structure(offset);
        game.set_projectile_terrain(81, 61, [0.0; 2], vec![18.75; 81 * 61], 40.0)
            .unwrap();
        for _ in 0..2 {
            game.reset_sandbox(Faction::Gla).unwrap();
            let id = game.named_map_object("AuthoredGarage").unwrap();
            assert_eq!(
                game.object_position(id)[2].to_bits(),
                (18.75 + offset).to_bits()
            );
            assert_eq!(game.height_above_ground(id).to_bits(), offset.to_bits());
            let cell = game.cell_at([355.0, 205.0]).unwrap();
            assert_eq!(game.attack_obstacle(1, cell).is_some(), offset <= 10.0);
        }
    }
}

#[test]
fn authored_height_rejects_nonfinite_values_before_spawning_the_record() {
    for offset in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut game = placed_structure(offset);
        assert_eq!(
            game.reset_sandbox(Faction::Gla).err(),
            Some("Nonfinite neutral map object height is unsupported")
        );
        assert!(game.named_map_object("AuthoredGarage").is_none());
    }
}

#[test]
fn finite_authored_height_is_applied_to_mobile_physical_map_objects() {
    for offset in [-2.0, 12.0] {
        let mut game = placed_structure(offset);
        let mut unit = fixture_type(Faction::Civilian, Kind::Tank);
        unit.name = "PlacedGarage".into();
        assert!(!unit.kind_of.iter().any(|kind| kind == "IMMOBILE"));
        assert!(unit.crush.physics_module.is_some());
        let index = game.catalog.len() - 1;
        game.catalog[index] = unit;
        game.reset_sandbox(Faction::Gla).unwrap();
        let id = game.named_map_object("AuthoredGarage").unwrap();
        assert_eq!(game.object_position(id)[2].to_bits(), offset.to_bits());
        assert_eq!(game.height_above_ground(id).to_bits(), offset.to_bits());
    }
}

#[test]
fn authored_body_controls_are_applied_to_live_map_objects_and_restored_on_restart() {
    let mut game = placed_structure(0.0);
    let record = game
        .layout
        .neutral_objects
        .iter_mut()
        .find(|record| record.script_name.as_deref() == Some("AuthoredGarage"))
        .unwrap();
    record.script_targetable = true;
    record.indestructible = true;
    record.selectable = Some(false);
    game.reset_sandbox(Faction::Gla).unwrap();
    let id = game.named_map_object("AuthoredGarage").unwrap();
    assert!(game.entities[id].script_targetable());
    assert!(game.entities[id].indestructible());
    assert!(!game.entities[id].selectable(&game.catalog[game.entities[id].type_index]));
    let health = game.entities[id].health;
    game.apply_native_damage(id, None, f32::MAX, super::super::DeathType::Normal);
    assert_eq!(game.entities[id].health.to_bits(), health.to_bits());
    game.set_map_body_controls(id, false, false);
    game.reset_sandbox(Faction::Gla).unwrap();
    let restarted = game.named_map_object("AuthoredGarage").unwrap();
    assert!(game.entities[restarted].script_targetable());
    assert!(game.entities[restarted].indestructible());
    assert!(
        !game.entities[restarted].selectable(&game.catalog[game.entities[restarted].type_index])
    );
}

#[test]
fn zero_geometry_defensive_arch_keeps_signed_map_height_without_fence_or_structure_claims() {
    let mut game = placed_structure(2.0);
    let index = game.catalog.len() - 1;
    let unit = &mut game.catalog[index];
    unit.kind_of = ["IMMOBILE", "DEFENSIVE_WALL", "FORCEATTACKABLE"]
        .map(str::to_owned)
        .to_vec();
    unit.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    unit.geometry.shape = crate::collision_geometry::Shape::Box;
    unit.geometry.major = 0.0;
    unit.geometry.minor = 0.0;
    unit.geometry.height = 0.0;
    unit.geometry_bounds.center_height = 0.0;
    unit.geometry_bounds.max_height = 0.0;
    unit.geometry_bounds.sphere_radius = 0.0;
    unit.maintenance.rubble_height = None;
    unit.fence_width = 44.0;
    crate::skirmish::validate_catalog(&game.catalog, &game.rules).unwrap();
    game.reset_sandbox(Faction::Gla).unwrap();
    let id = game.named_map_object("AuthoredGarage").unwrap();
    assert_eq!(game.object_position(id)[2].to_bits(), 2.0_f32.to_bits());
    let center = crate::Cell { x: 35, y: 20 };
    assert!(game.attack_obstacle(1, center).is_none());
    for x in 33..=37 {
        assert!(game.attack_obstacle(1, crate::Cell { x, y: 20 }).is_none());
    }
    assert_eq!(game.body_geometry(id).major.to_bits(), 0.0_f32.to_bits());
    assert_eq!(game.object_radius(id).to_bits(), 0.0_f32.to_bits());
    let probe = crate::collision_geometry::Geometry {
        shape: crate::collision_geometry::Shape::Box,
        is_small: false,
        major: 1.0,
        minor: 1.0,
        height: 1.0,
    };
    let point = crate::collision_geometry::Body {
        geometry: game.body_geometry(id),
        position: game.object_position(id),
        angle: 0.0,
    };
    let mut other = crate::collision_geometry::Body {
        geometry: &probe,
        position: point.position,
        angle: 0.0,
    };
    assert!(crate::collision_geometry::intersects(&point, &other));
    other.position[0] += 2.0;
    assert!(!crate::collision_geometry::intersects(&point, &other));
    game.apply_native_damage(id, None, f32::MAX, crate::skirmish::DeathType::Normal);
    assert!(!game.entities[id].alive());
    assert!(game.attack_obstacle(1, center).is_none());
    game.layout
        .neutral_objects
        .iter_mut()
        .find(|record| record.script_name.as_deref() == Some("AuthoredGarage"))
        .unwrap()
        .position[2] = -5.0;
    game.reset_sandbox(Faction::Gla).unwrap();
    let restarted = game.named_map_object("AuthoredGarage").unwrap();
    assert_eq!(
        game.object_position(restarted)[2].to_bits(),
        (-5.0_f32).to_bits()
    );
    assert!(game.entities[restarted].alive());
    assert!(game.attack_obstacle(1, center).is_none());
    game.catalog[index].geometry.major = -0.1;
    assert!(crate::skirmish::validate_catalog(&game.catalog, &game.rules).is_err());
    game.catalog[index].geometry.major = f32::NAN;
    assert!(crate::skirmish::validate_catalog(&game.catalog, &game.rules).is_err());
}
