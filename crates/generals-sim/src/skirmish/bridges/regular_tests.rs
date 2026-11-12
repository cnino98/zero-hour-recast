use super::*;
use crate::{
    Cell,
    collision_geometry::Shape,
    skirmish::{
        Faction, Kind, UnitAiUpdate,
        tests::{fixture_type, game},
    },
    terrain::{BridgeGeometry, CellType, TerrainMap},
};

fn record(layer: u8, x: f32, z: f32, name: &str) -> RegularBridge {
    RegularBridge {
        layer,
        template_name: "Concrete".into(),
        object: NeutralObject {
            initial_health: None,
            attitude: None,
            recruitable: None,
            name: "GenericBridge".into(),
            original_team: "teamplayer0".into(),
            position: [x, 205.0, z],
            angle: 0.0,
            script_name: Some(name.into()),
            script_targetable: false,
            indestructible: false,
            selectable: None,
        },
    }
}

fn geometry(z: f32) -> BridgeGeometry {
    BridgeGeometry {
        from: [300.0, 205.0, z],
        to: [410.0, 205.0, z],
        corners: [
            [300.0, 230.0, z],
            [300.0, 180.0, z],
            [410.0, 230.0, z],
            [410.0, 180.0, z],
        ],
    }
}

fn bridge_catalog_game() -> Skirmish {
    let mut game = game();
    let mut bridge = fixture_type(Faction::Civilian, Kind::TechBuilding);
    bridge.name = "GenericBridge".into();
    bridge.kind_of = ["STRUCTURE", "IMMOBILE", "BRIDGE"]
        .map(str::to_owned)
        .to_vec();
    bridge.ai.update = UnitAiUpdate::Absent;
    bridge.selectable = false;
    bridge.max_health = 1.0;
    bridge.initial_health = 1.0;
    bridge.minimum_health = 1.0;
    bridge.geometry.shape = Shape::Box;
    bridge.geometry.is_small = false;
    bridge.geometry.major = 1.0;
    bridge.geometry.minor = 1.0;
    bridge.geometry.height = 1.0;
    bridge.geometry_bounds.center_height = 0.5;
    bridge.geometry_bounds.max_height = 1.0;
    bridge.geometry_bounds.sphere_radius = 1.5;
    game.catalog.push(bridge);
    game
}

fn regular_game(height: f32, overlap: bool) -> Skirmish {
    let mut game = bridge_catalog_game();
    if overlap {
        let mut ordinary = fixture_type(Faction::Civilian, Kind::TechBuilding);
        ordinary.name = "LaterMapObject".into();
        ordinary.kind_of = ["STRUCTURE", "IMMOBILE"].map(str::to_owned).to_vec();
        ordinary.ai.update = UnitAiUpdate::Absent;
        ordinary.geometry.shape = Shape::Box;
        ordinary.geometry.major = 1.0;
        ordinary.geometry.minor = 1.0;
        game.catalog.push(ordinary);
        game.layout.neutral_objects.push(NeutralObject {
            initial_health: None,
            attitude: None,
            recruitable: None,
            name: "LaterMapObject".into(),
            original_team: "team".into(),
            position: [355.0, 205.0, 0.0],
            angle: 0.0,
            script_name: Some("ordinary".into()),
            script_targetable: false,
            indestructible: false,
            selectable: None,
        });
    }
    let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
    let layer = terrain.add_bridge(geometry(height), |_| 0.0).unwrap();
    game.layout
        .regular_bridges
        .push(record(layer, 355.0, height, "first"));
    if overlap {
        let layer = terrain.add_bridge(geometry(height), |_| 0.0).unwrap();
        game.layout
            .regular_bridges
            .push(record(layer, 355.0, height, "second"));
    }
    game.configure_movement_terrain(terrain).unwrap();
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    game
}

#[test]
fn regular_bridge_actor_precedes_bases_and_preserves_exact_height_identity_and_immortality() {
    let mut game = regular_game(80.125, false);
    assert_eq!(game.radar_bridge_at([355.0, 205.0]).unwrap().1, "Concrete");
    assert_eq!(
        game.catalog[game.entities[0].type_index].name,
        "GenericBridge"
    );
    assert_eq!(game.entities[0].owner, game.neutral_owner());
    assert_eq!(game.entities[0].layer, 1);
    assert_eq!(game.map_objects.get("first"), Some(&0));
    assert_eq!(
        game.object_position(0).map(f32::to_bits),
        [355.0_f32, 205.0, 80.125].map(f32::to_bits)
    );
    let picked = game
        .pick_bridge([355.0, 205.0, 500.0], [355.0, 205.0, -100.0])
        .unwrap();
    assert_eq!(
        picked.map(f32::to_bits),
        game.object_position(0).map(f32::to_bits)
    );
    for _ in 0..4 {
        game.step();
    }
    let index = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[index].locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Treads,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    let tank = game.spawn(index, 0, [295.0, 205.0], true);
    game.move_to_world(tank, picked).unwrap();
    assert_eq!(
        game.entities[tank]
            .ai
            .path_request
            .pending
            .as_ref()
            .unwrap()
            .goal
            .map(f32::to_bits),
        picked.map(f32::to_bits)
    );
    game.process_path_service();
    assert_eq!(
        game.entities[tank].ai.path.optimized.back().unwrap().layer,
        2
    );
    game.apply_native_damage(0, None, 999_999.0, DeathType::Normal);
    game.update_bridge_states();
    assert_eq!(game.entities[0].health.to_bits(), 1.0_f32.to_bits());
    assert!(game.movement_terrain.bridge_layer_active(2));
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert_eq!(game.map_objects.get("first"), Some(&0));
    assert_eq!(game.bridge_objects.len(), 1);
    assert!(
        game.pick_bridge([355.0, 205.0, 500.0], [355.0, 205.0, -100.0])
            .is_some()
    );
}

#[test]
fn initial_low_decks_precede_footprints_but_exact_ten_unit_overlap_uses_newest_bridge_owner() {
    let center = Cell { x: 35, y: 20 };
    let adjacent = Cell { x: 36, y: 20 };
    let mut low = regular_game(0.25, true);
    assert!(low.attack_obstacle(1, center).is_none());
    assert!(matches!(
        low.movement_terrain.cell(center).unwrap().kind,
        CellType::BridgeImpassable
    ));
    assert!(matches!(
        low.movement_terrain.cell(adjacent).unwrap().kind,
        CellType::BridgeImpassable
    ));
    low.reset(Faction::Gla, Faction::Gla).unwrap();
    assert!(low.attack_obstacle(1, center).is_none());
    let mut exact = regular_game(10.0, true);
    assert!(matches!(
        exact.movement_terrain.cell(center).unwrap().kind,
        CellType::Clear
    ));
    assert!(matches!(
        exact.attack_obstacle(1, center).unwrap().id,
        crate::skirmish::movement::obstacles::OwnerId::Actor(1)
    ));
    assert!(exact.map_objects["ordinary"] > 1);
    // The constructor's provisional ordinary reservation was evicted before
    // the native newest-first existing-bridge registration. A runtime later object cannot replace the existing source owner.
    let later = exact.spawn(
        exact.entities[0].type_index,
        exact.neutral_owner(),
        [355.0, 205.0],
        true,
    );
    exact.register_object_obstacle(later);
    assert!(matches!(
        exact.attack_obstacle(1, center).unwrap().id,
        crate::skirmish::movement::obstacles::OwnerId::Actor(1)
    ));
    exact.reclassify_navigation_obstacles();
    assert!(matches!(
        exact.attack_obstacle(1, center).unwrap().id,
        crate::skirmish::movement::obstacles::OwnerId::Actor(1)
    ));
    exact.reset(Faction::Gla, Faction::Gla).unwrap();
    assert!(matches!(
        exact.attack_obstacle(1, center).unwrap().id,
        crate::skirmish::movement::obstacles::OwnerId::Actor(1)
    ));
    assert_eq!(exact.map_objects.get("second"), Some(&1));
}

#[test]
fn landmark_prepass_keeps_fixed_neutral_owner_and_does_not_spawn_twice() {
    let mut game = game();
    let mut landmark = fixture_type(Faction::Civilian, Kind::TechBuilding);
    landmark.name = "MapLandmark".into();
    landmark.kind_of = ["STRUCTURE", "IMMOBILE", "BRIDGE", "LANDMARK_BRIDGE"]
        .map(str::to_owned)
        .to_vec();
    landmark.ai.update = UnitAiUpdate::Absent;
    landmark.geometry.shape = Shape::Box;
    landmark.geometry.major = 55.0;
    landmark.geometry.minor = 25.0;
    game.catalog.push(landmark);
    game.layout.neutral_objects.push(NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        name: "MapLandmark".into(),
        original_team: "teamplayer0".into(),
        position: [355.0, 205.0, 10.0],
        angle: 0.0,
        script_name: Some("landmark".into()),
        script_targetable: false,
        indestructible: false,
        selectable: None,
    });
    let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
    terrain.add_bridge(geometry(10.0), |_| 0.0).unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    let id = *game.map_objects.get("landmark").unwrap();
    assert_eq!(id, 0);
    assert_eq!(game.entities[id].owner, game.neutral_owner());
    assert_eq!(game.object_position(id)[2].to_bits(), 10.0_f32.to_bits());
    assert_eq!(
        game.attack_obstacle(1, Cell { x: 35, y: 20 })
            .unwrap()
            .player,
        game.neutral_owner()
    );
    assert_eq!(
        game.entities
            .iter()
            .filter(|actor| game.catalog[actor.type_index].name == "MapLandmark")
            .count(),
        1
    );
    assert!(!game.civilian_objects.contains(&id));
    assert!(
        game.pick_bridge([355.0, 205.0, 500.0], [355.0, 205.0, -100.0])
            .is_some()
    );
}

#[test]
fn wall_slot_bridge_keeps_its_actual_generic_object_and_pick_but_has_ground_path_fallback() {
    let mut game = bridge_catalog_game();
    let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
    for index in 0..14 {
        let (x, height) = if index == 13 {
            (355.0, 0.25)
        } else {
            (455.0, 80.0)
        };
        let mut deck = geometry(height);
        if index != 13 {
            for point in deck
                .corners
                .iter_mut()
                .chain([&mut deck.from, &mut deck.to])
            {
                point[0] += 100.0;
            }
        }
        let layer = terrain.add_bridge(deck, |_| 0.0).unwrap();
        game.layout
            .regular_bridges
            .push(record(layer, x, height, &format!("bridge{index}")));
    }
    game.configure_movement_terrain(terrain).unwrap();
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert_eq!(game.map_objects.get("bridge13"), Some(&13));
    assert_eq!(game.object_position(13)[2].to_bits(), 0.25_f32.to_bits());
    assert_eq!(
        game.pick_bridge([355.0, 205.0, 500.0], [355.0, 205.0, -100.0])
            .unwrap()[2]
            .to_bits(),
        0.25_f32.to_bits()
    );
    assert!(
        game.movement_terrain
            .bridge_cell(15, Cell { x: 35, y: 20 })
            .is_none()
    );
    assert_eq!(
        game.navigation
            .resolve_layer(crate::grid::LayerCell {
                cell: Cell { x: 35, y: 20 },
                layer: 15
            })
            .layer,
        1
    );
    assert!(matches!(
        game.attack_obstacle(1, Cell { x: 35, y: 20 }).unwrap().id,
        crate::skirmish::movement::obstacles::OwnerId::Actor(13)
    ));
    assert_eq!(game.bridge_objects.len(), 14);
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert_eq!(game.bridge_objects.len(), 14);
    assert_eq!(game.map_objects.get("bridge13"), Some(&13));
    assert!(
        game.movement_terrain
            .bridge_cell(15, Cell { x: 35, y: 20 })
            .is_none()
    );
    assert_eq!(
        game.navigation
            .resolve_layer(crate::grid::LayerCell {
                cell: Cell { x: 35, y: 20 },
                layer: 15
            })
            .layer,
        1
    );
    assert!(matches!(
        game.attack_obstacle(1, Cell { x: 35, y: 20 }).unwrap().id,
        crate::skirmish::movement::obstacles::OwnerId::Actor(13)
    ));
}
