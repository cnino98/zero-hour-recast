use super::*;
use crate::{
    skirmish::{
        CanAttackResult, DamageType, DeathType, Faction, Kind, WeaponBinding,
        damage::{AreaDamage, Source},
        tests::{fixture_type, game, register_update},
    },
    terrain::{BridgeGeometry, TerrainMap},
};

fn bridge_game(immortal: bool) -> (Skirmish, usize, u8) {
    bridge_game_with_water(immortal, false)
}

fn bridge_game_with_water(immortal: bool, water_barrier: bool) -> (Skirmish, usize, u8) {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.name = if immortal {
        "GenericBridge"
    } else {
        "MonumentTrainBridge"
    }
    .into();
    unit.behavior_modules.clear();
    unit.update_modules.clear();
    unit.kind_of = ["STRUCTURE", "IMMOBILE", "BRIDGE"]
        .map(str::to_owned)
        .to_vec();
    if !immortal {
        unit.kind_of
            .extend(["LANDMARK_BRIDGE", "UNATTACKABLE"].map(str::to_owned));
    }
    unit.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    unit.max_health = if immortal { 1.0 } else { 2000.0 };
    unit.initial_health = unit.max_health;
    unit.minimum_health = if immortal { 1.0 } else { 0.0 };
    unit.behavior.bridges.push(Rules {
        lateral_scaffold_speed: 100.0 * (1.0 / 30.0),
        vertical_scaffold_speed: 70.0 * (1.0 / 30.0),
    });
    register_update(&mut unit, "BridgeBehavior");
    crate::skirmish::module_updates::validate_registrations(&unit).unwrap();
    game.catalog.push(unit);
    let water: Vec<_> = if water_barrier {
        (32..41)
            .flat_map(|x| (0..60).map(move |y| crate::grid::Cell { x, y }))
            .collect()
    } else {
        Vec::new()
    };
    let mut terrain = TerrainMap::classify(80, 60, [], water).unwrap();
    let layer = terrain
        .add_bridge(
            BridgeGeometry {
                from: [300.0, 205.0, 80.0],
                to: [410.0, 205.0, 80.0],
                corners: [
                    [300.0, 230.0, 80.0],
                    [300.0, 180.0, 80.0],
                    [410.0, 230.0, 80.0],
                    [410.0, 180.0, 80.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    if water_barrier {
        game.grid = crate::grid::Grid::new(80, 60, terrain.blocked_ground_cells()).unwrap();
    }
    game.configure_movement_terrain(terrain).unwrap();
    let record = crate::skirmish::NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        name: if immortal {
            "GenericBridge"
        } else {
            "MonumentTrainBridge"
        }
        .into(),
        original_team: "teamPlyrCivilian".into(),
        position: [355.0, 205.0, 80.0],
        angle: 0.0,
        script_name: Some("BridgeFixture".into()),
        script_targetable: false,
        indestructible: false,
        selectable: None,
    };
    if immortal {
        game.layout
            .regular_bridges
            .push(crate::skirmish::RegularBridge {
                layer,
                template_name: "Concrete".into(),
                object: record,
            });
    } else {
        game.layout.neutral_objects.push(record);
    }
    game.spawn_map_bridges().unwrap();
    let id = game.map_objects["BridgeFixture"];
    (game, id, layer)
}

#[test]
fn bridge_area_damage_obeys_unattackable_and_changes_layer_before_authored_die() {
    let (mut game, bridge, layer) = bridge_game(false);
    let weapon = &mut game.catalog[6].weapon;
    weapon.damage = 2500.0;
    weapon.damage_type = DamageType::Unresistable;
    weapon.area = Some(AreaDamage {
        at_source: false,
        primary_radius: 10.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 4,
    });
    let source = game.spawn(6, 0, [280.0, 205.0], true);
    let occupant = game.spawn(6, 1, [355.0, 205.0], true);
    game.entities[occupant].layer = layer;
    game.update_ground_contacts();
    assert!(matches!(
        game.player_attack_result(source, bridge),
        CanAttackResult::NotPossible
    ));
    assert!(matches!(
        game.force_attack_result(source, Some(bridge), game.object_position(bridge)),
        CanAttackResult::NotPossible
    ));
    game.tick = 7;
    game.deal_bound_weapon_damage_with_radius(
        &Source::Object(source),
        WeaponBinding {
            type_index: 6,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        None,
        game.object_position(bridge),
        1.0,
        1.0,
    );
    assert!(!game.entities[bridge].alive());
    assert!(!game.entities[occupant].alive());
    assert!(matches!(
        game.entities[occupant].damage_state.damage_type,
        DamageType::Falling
    ));
    assert!(matches!(
        game.entities[occupant].damage_state.death_type,
        DeathType::Splatted
    ));
    assert_eq!(
        game.entities[occupant].damage_state.last_attacker,
        Some(occupant)
    );
    assert_eq!(game.entities[bridge].behaviors.bridges[0].death_frame, 7);
    assert!(game.entities[bridge].behaviors.bridges[0].fx_resolved);
    assert!(
        game.movement_terrain
            .bridge_height(layer, [355.0, 205.0])
            .is_none()
    );
    assert!(
        game.model_flags(&game.entities[bridge], [0.5, 0.25])
            .contains("RUBBLE")
    );
    assert!(game.entities[bridge].exists());
    game.run_object_updates();
    assert_eq!(game.update_remaining_sleep(bridge, 0), 1);
}

#[test]
fn bridge_collapse_preserves_below_deck_ground_occupant() {
    let (mut game, bridge, layer) = bridge_game(false);
    let lower = game.spawn(6, 1, [355.0, 205.0], true);
    assert_eq!(game.entities[lower].layer, 1);
    assert!(matches!(
        game.movement_terrain
            .cell(crate::grid::Cell { x: 35, y: 20 })
            .unwrap()
            .kind,
        crate::terrain::CellType::Clear
    ));
    assert!(game.object_position(lower)[2] < 80.0);

    // BridgeBehavior snapshots passengers from the current native partition;
    // materialize the ground actor there before testing its below-deck exclusion.
    game.update_ground_contacts();
    let bridge_position = game.object_position(bridge);
    let bridge_radius = game
        .movement_terrain
        .bridge_geometry(layer)
        .unwrap()
        .corners[2];
    let dx = bridge_radius[0] - bridge_position[0];
    let dy = bridge_radius[1] - bridge_position[1];
    let squared = [dx, dy].map(|value| value * value);
    let radius = (squared[0] + squared[1]).sqrt();
    let lower_position = game.object_position(lower);
    let squared = [0, 1].map(|axis| {
        let delta = lower_position[axis] - bridge_position[axis];
        delta * delta
    });
    assert!(squared[0] + squared[1] < radius * radius);
    assert!(
        game.movement_terrain
            .bridge_contains(layer, [lower_position[0], lower_position[1]],)
    );
    assert!(lower_position[2] < game.movement_terrain.bridge_minimum_height(layer).unwrap());
    let position = lower_position.map(f32::to_bits);
    let health = game.entities[lower].health.to_bits();
    assert!(
        game.partition
            .fastest_range_objects([bridge_position[0], bridge_position[1]], radius)
            .contains(&lower)
    );

    game.apply_native_damage(
        bridge,
        None,
        game.entities[bridge].max_health,
        DeathType::Normal,
    );

    assert!(game.entities[lower].alive());
    assert_eq!(game.entities[lower].layer, 1);
    assert_eq!(game.entities[lower].health.to_bits(), health);
    assert_eq!(game.object_position(lower).map(f32::to_bits), position);
}

#[test]
fn bridge_collapse_invalidates_queued_deck_route() {
    let (mut game, bridge, layer) = bridge_game_with_water(false, true);
    let crosser = game.spawn(6, 0, [275.0, 205.0], true);
    let goal = [435.0, 205.0];
    let route = game
        .movement_path(crosser, game.entities[crosser].position, goal)
        .unwrap();
    assert!(route.raw.iter().any(|node| node.layer == layer));
    game.move_to(crosser, goal).unwrap();
    game.process_path_service();
    assert!(
        game.entities[crosser]
            .ai
            .path
            .raw
            .iter()
            .any(|node| node.layer == layer)
    );

    game.kill_object(bridge, DeathType::Normal);

    assert!(game.entities[crosser].ai.path.raw.is_empty());
    assert!(game.entities[crosser].ai.ground_path.is_none());
    assert!(
        game.movement_path(crosser, game.entities[crosser].position, goal)
            .is_none()
    );
}

#[test]
fn original_immortal_bridge_retains_health_without_transition_or_death() {
    let (mut game, bridge, layer) = bridge_game(true);
    let random = game.random;
    assert_eq!(game.update_remaining_sleep(bridge, 0), 1);
    game.apply_native_damage(bridge, None, 999_999.0, DeathType::Normal);
    assert_eq!(game.entities[bridge].health.to_bits(), 1.0_f32.to_bits());
    assert!(!game.entities[bridge].behaviors.bridges[0].fx_resolved);
    assert_eq!(game.entities[bridge].behaviors.bridges[0].death_frame, 0);
    assert!(
        game.movement_terrain
            .bridge_height(layer, [355.0, 205.0])
            .is_some()
    );
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.update_remaining_sleep(bridge, 0), 1);
    assert_eq!(game.random, random);
}

#[test]
fn bridge_healing_resets_death_frame_and_restoration_does_not_damage_occupants() {
    let (mut game, bridge, layer) = bridge_game(false);
    game.tick = 4;
    game.kill_object(bridge, DeathType::Normal);
    assert_eq!(game.entities[bridge].behaviors.bridges[0].death_frame, 4);
    let occupant = game.spawn(6, 1, [355.0, 205.0], true);
    game.entities[occupant].layer = layer;
    let health = game.entities[occupant].health;
    // ActiveBody::attemptHealing explicitly permits dead bridges to revive.
    game.apply_healing(bridge, None, 2000.0);
    assert_eq!(game.entities[bridge].behaviors.bridges[0].death_frame, 0);
    assert!(
        game.movement_terrain
            .bridge_height(layer, [355.0, 205.0])
            .is_some()
    );
    assert_eq!(game.entities[occupant].health.to_bits(), health.to_bits());
}

#[test]
fn bridge_layer_surface_survives_destruction_but_height_query_clips_to_polygon() {
    let (mut game, bridge, layer) = bridge_game(false);
    assert_eq!(
        game.movement_layer_height(layer, [355.0, 205.0]).to_bits(),
        80.0_f32.to_bits()
    );
    assert_eq!(
        game.movement_layer_height(layer, [500.0, 205.0]).to_bits(),
        0.0_f32.to_bits()
    );
    game.kill_object(bridge, DeathType::Normal);
    assert!(
        game.movement_terrain
            .bridge_height(layer, [355.0, 205.0])
            .is_none()
    );
    assert_eq!(
        game.movement_layer_height(layer, [355.0, 205.0]).to_bits(),
        80.0_f32.to_bits()
    );
    assert_eq!(
        game.movement_layer_height(layer, [500.0, 205.0]).to_bits(),
        0.0_f32.to_bits()
    );
}

#[test]
fn bridge_radar_refresh_overwrites_only_rubble_edges_and_keeps_native_zero_and_90_boundaries() {
    let (mut game, bridge, _) = bridge_game(false);
    game.kill_object(bridge, DeathType::Normal);
    assert_eq!(game.radar_terrain_refresh_frame, 0);
    game.tick = 100;
    assert!(!game.take_radar_terrain_refresh());
    game.apply_healing(bridge, None, 2000.0);
    assert_eq!(game.radar_terrain_refresh_frame, 100);
    game.tick = 101;
    game.apply_native_damage(bridge, None, 1100.0, DeathType::Normal);
    assert_eq!(game.radar_terrain_refresh_frame, 100);
    game.tick = 190;
    assert!(!game.take_radar_terrain_refresh());
    game.kill_object(bridge, DeathType::Normal);
    assert_eq!(game.radar_terrain_refresh_frame, 190);
    game.tick = 280;
    assert!(!game.take_radar_terrain_refresh());
    game.tick = 281;
    assert!(game.take_radar_terrain_refresh());
    assert!(!game.take_radar_terrain_refresh());
    game.apply_healing(bridge, None, 2000.0);
    game.tick = 372;
    assert!(game.take_radar_terrain_refresh());
    game.tick = u64::from(u32::MAX) - 20;
    game.kill_object(bridge, DeathType::Normal);
    game.tick += 90;
    assert!(!game.take_radar_terrain_refresh());
    game.tick += 1;
    assert!(game.take_radar_terrain_refresh());
}

#[test]
fn radar_first_bridge_identity_survives_death_without_revealing_an_older_overlapping_bridge() {
    let (mut game, first, layer) = bridge_game(false);
    let first_geometry = game.movement_terrain.bridge_geometry(layer).unwrap();
    let geometry = BridgeGeometry {
        from: first_geometry.from,
        to: first_geometry.to,
        corners: first_geometry.corners,
    };
    let second_layer = game.movement_terrain.add_bridge(geometry, |_| 0.0).unwrap();
    let index = game.layout.neutral_objects.len();
    game.layout
        .neutral_objects
        .push(crate::skirmish::NeutralObject {
            initial_health: None,
            attitude: None,
            recruitable: None,
            name: "MonumentTrainBridge".into(),
            original_team: "team".into(),
            position: [355.0, 205.0, 80.0],
            angle: 0.0,
            script_name: None,
            script_targetable: false,
            indestructible: false,
            selectable: None,
        });
    let second = game.spawn_neutral_object(index, true).unwrap();
    game.bind_map_bridges();
    assert_eq!(
        game.radar_bridge_at([355.0, 205.0]).unwrap().0,
        second_layer
    );
    assert_eq!(
        game.radar_bridge_at([355.0, 205.0]).unwrap().1,
        "MonumentTrainBridge"
    );
    assert!(game.radar_bridge_working(second_layer));
    game.kill_object(second, DeathType::Normal);
    assert!(game.entities[first].alive());
    assert!(game.radar_bridge_working(layer));
    assert!(!game.radar_bridge_working(second_layer));
    assert_eq!(
        game.radar_bridge_at([355.0, 205.0]).unwrap().0,
        second_layer
    );
    game.apply_healing(second, None, 2000.0);
    assert!(game.radar_bridge_working(second_layer));
    game.destroy_maintenance_object(second);
    game.process_update_destruction();
    assert!(!game.radar_bridge_working(second_layer));
    assert_eq!(
        game.radar_bridge_at([355.0, 205.0]).unwrap().0,
        second_layer
    );
}
