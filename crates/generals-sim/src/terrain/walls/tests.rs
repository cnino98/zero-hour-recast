use super::*;

fn piece(position: [f32; 2], major: f32, minor: f32, shape: Shape) -> Footprint {
    Footprint {
        position,
        major,
        minor,
        shape,
        angle: 0.0,
    }
}

#[test]
fn wall_cells_require_four_covered_corners_and_do_not_connect_to_ground() {
    let mut terrain = TerrainMap::classify(30, 30, [], []).unwrap();
    let pieces = [piece([105.0, 105.0], 15.0, 15.0, Shape::Box)];
    terrain.add_wall_piece(7);
    terrain.allocate_wall_layer(80.0, &pieces);
    assert!(terrain.is_point_on_wall([95.0, 95.0]));
    assert!(!terrain.is_point_on_wall([85.0, 95.0]));
    assert!(matches!(
        terrain.bridge_cell(15, Cell { x: 8, y: 9 }).unwrap().kind,
        CellType::BridgeImpassable
    ));
    assert!(
        !terrain
            .bridge_cell(15, Cell { x: 9, y: 9 })
            .unwrap()
            .connect_ground
    );
    assert_eq!(
        terrain.get_layer_for_destination([95.0, 95.0, 80.0], 0.0),
        15
    );
    assert_eq!(
        terrain.get_layer_for_destination([95.0, 95.0, 40.0], 0.0),
        1
    );
    assert_eq!(
        terrain.get_highest_layer_for_destination([95.0, 95.0, 79.0], 0.0, true),
        1
    );
    // W3D's wall branch returns the authored plane before the bridge/ground max.
    assert_eq!(
        terrain
            .destination_layer_height(15, [95.0, 95.0], 100.0)
            .to_bits(),
        80.0_f32.to_bits()
    );
    terrain.invalidate_wall_piece(7);
    terrain.classify_wall_cells(&[]);
    assert!(!terrain.is_point_on_wall([95.0, 95.0]));
    assert!(terrain.bridge_layer_active(15));
}

#[test]
fn wall_geometry_uses_sphere_square_but_cylinder_minor_radius() {
    let sphere = piece([0.0; 2], 10.0, 2.0, Shape::Sphere);
    let cylinder = piece([0.0; 2], 10.0, 2.0, Shape::Cylinder);
    assert!(contains(&sphere, [9.0, 9.0]));
    assert!(!contains(&cylinder, [9.0, 9.0]));
    assert!(contains(&cylinder, [10.0, 2.0]));
}

#[test]
fn wall_removal_keeps_invalid_slots_and_native_insertion_capacity() {
    let mut terrain = TerrainMap::classify(30, 30, [], []).unwrap();
    for id in 0..128 {
        terrain.add_wall_piece(id);
    }
    assert_eq!(terrain.wall_piece_ids().count(), 127);
    terrain.invalidate_wall_piece(0);
    terrain.add_wall_piece(128);
    assert_eq!(terrain.wall_piece_ids().count(), 126);
    terrain.remove_wall_piece(1);
    terrain.add_wall_piece(128);
    let ids: Vec<_> = terrain.wall_piece_ids().collect();
    assert_eq!(ids[0], 126);
    assert_eq!(ids.last(), Some(&128));
}

#[test]
fn wall_routes_stay_on_the_wall_and_have_no_implicit_ground_ramp() {
    use crate::{
        Grid,
        grid::{LayerCell, LayerQuery},
    };
    let mut terrain = TerrainMap::classify(30, 30, [], []).unwrap();
    terrain.add_wall_piece(4);
    terrain.allocate_wall_layer(80.0, &[piece([150.0, 150.0], 60.0, 30.0, Shape::Box)]);
    let grid = Grid::new(30, 30, []).unwrap().with_bridge_layers(&terrain);
    let mut query = LayerQuery {
        start: LayerCell {
            cell: Cell { x: 10, y: 15 },
            layer: 15,
        },
        goal: LayerCell {
            cell: Cell { x: 19, y: 15 },
            layer: 15,
        },
        radius: 0,
        centered: true,
        closest_cost: None,
    };
    let empty = BTreeMap::new();
    let result = grid.path_for_layers_counted(&query, &empty, &BTreeMap::new());
    let route = result.route.expect("walkable wall route");
    assert!(route.raw.iter().all(|node| node.layer == 15));
    query.start.layer = 1;
    assert!(
        grid.path_for_layers_counted(&query, &empty, &BTreeMap::new())
            .route
            .is_none()
    );
}
