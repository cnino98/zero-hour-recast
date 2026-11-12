//! Original train-span connections are inaccessible to ground-only infantry.
use super::*;
use generals_sim::{Cell, terrain::CellType};

#[ignore = "requires original game installation"]
#[test]
fn original_killing_fields_bridge_connections_are_on_cliffs() {
    let root = &generals_assets::game_root().unwrap();
    let fixture =
        Fixture::load(root, crate::radar::bridge_destruction_capture::map_name()).unwrap();
    let landmark = fixture
        .layout
        .neutral_objects
        .iter()
        .find(|object| object.name == "MonumentTrainBridge")
        .unwrap();
    let ranger = fixture
        .data
        .types
        .iter()
        .find(|unit| unit.name == "AmericaInfantryRanger")
        .unwrap();
    // Original BasicHumanLocomotor: GROUND RUBBLE, without CLIFF.
    assert_eq!(ranger.locomotor.legal_surfaces, 1 | 16);
    let terrain = &fixture.movement;
    let layer = terrain
        .bridge_layers()
        .find(|&layer| {
            terrain
                .bridge_geometry(layer)
                .unwrap()
                .contains([landmark.position[0], landmark.position[1]])
        })
        .unwrap();
    let mut connections = Vec::new();
    for x in 0..terrain.width() {
        for y in 0..terrain.height() {
            let cell = Cell { x, y };
            let Some(upper) = terrain.bridge_cell(layer, cell) else {
                continue;
            };
            if !upper.connect_ground {
                continue;
            }
            connections.push([x, y]);
            assert!(matches!(upper.kind, CellType::Clear));
            assert_eq!(terrain.ground_bridge(cell), Some(layer));
            assert!(matches!(terrain.cell(cell).unwrap().kind, CellType::Cliff));
            let center = [
                f32::from(x).mul_add(10.0, 5.0),
                f32::from(y).mul_add(10.0, 5.0),
            ];
            assert!(!terrain.valid_movement(center, ranger.locomotor.legal_surfaces));
        }
    }
    // Actual source entry lines produce three south and four north connectors.
    assert_eq!(
        connections,
        [
            [52, 165],
            [53, 164],
            [53, 165],
            [72, 201],
            [73, 200],
            [73, 201],
            [74, 200]
        ]
    );
}
