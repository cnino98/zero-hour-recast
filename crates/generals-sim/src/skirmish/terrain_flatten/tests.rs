use super::*;
use crate::skirmish::{Faction, Kind, tests};

fn fixture(shape: Shape, radius: f32) -> (Skirmish, usize) {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.geometry = Geometry {
        is_small: false,
        shape,
        major: radius,
        minor: radius,
        height: 10.0,
    };
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let mut heights = vec![12.5; 81 * 61];
    heights[30 * 81 + 30] = 2.5;
    game.set_projectile_terrain(81, 61, [0.0; 2], heights, 40.0)
        .unwrap();
    let id = game.spawn(kind, 0, [300.0, 300.0], true);
    (game, id)
}

#[test]
fn box_clamps_to_center_lowers_neighbors_and_updates_live_height_queries() {
    let (mut game, id) = fixture(Shape::Box, 10.0);
    game.flatten_object_terrain(id);
    assert_eq!(
        game.ground_height([320.0, 320.0]).to_bits(),
        2.5_f32.to_bits()
    );
    assert_eq!(
        game.ground_height([330.0, 320.0]).to_bits(),
        12.5_f32.to_bits()
    );
    assert_eq!(
        game.movement_terrain
            .cell(crate::Cell { x: 32, y: 32 })
            .unwrap()
            .height
            .to_bits(),
        2.5_f32.to_bits()
    );
    let revision = game.terrain_height_revision();
    assert!(revision > 0);
    game.flatten_object_terrain(id);
    assert_eq!(game.terrain_height_revision(), revision);
}

#[test]
fn circle_uses_strict_radius_and_average_without_raising_its_lower_center() {
    let (mut game, id) = fixture(Shape::Cylinder, 20.0);
    game.flatten_object_terrain(id);
    assert_eq!(
        game.ground_height([300.0, 300.0]).to_bits(),
        2.5_f32.to_bits()
    );
    assert_eq!(
        game.ground_height([320.0, 300.0]).to_bits(),
        11.25_f32.to_bits()
    );
    assert_eq!(
        game.ground_height([330.0, 300.0]).to_bits(),
        12.5_f32.to_bits()
    );
}

#[test]
fn corrected_border_scan_includes_negative_rows_and_native_linear_index_writes() {
    let (mut game, id) = fixture(Shape::Cylinder, 1.0);
    let mut heights = vec![12.5; 8 * 8];
    heights[8 + 2] = 2.5;
    game.set_projectile_terrain(8, 8, [-20.0; 2], heights, 40.0)
        .unwrap();
    game.set_object_position(id, [0.0, -10.0, 0.0]);
    game.flatten_object_terrain(id);
    assert_eq!(
        game.ground_height([0.0, -20.0]).to_bits(),
        2.5_f32.to_bits()
    );
    let terrain = game.projectile_terrain.as_mut().unwrap();
    terrain.lower_raw_height([-3, 1], 1);
    // x+border=-1 aliases the preceding row's final vertex; native only bounds ndx.
    assert_eq!(
        terrain.height_map().samples[3 * 8 - 1].to_bits(),
        0.625_f32.to_bits()
    );
}

#[test]
fn rotated_box_and_degenerate_triangle_preserve_source_edge_inclusion() {
    let geometry = Geometry {
        is_small: false,
        shape: Shape::Box,
        major: 20.0,
        minor: 5.0,
        height: 1.0,
    };
    let points = footprint(&geometry, [0.0; 2], std::f32::consts::FRAC_PI_4);
    assert!(points.contains(&[1, 1]) && points.contains(&[-1, -1]));
    assert!(!points.contains(&[1, -1]));
    assert!(triangle_contains(
        [[0.0, 0.0], [0.0, 0.0], [0.0, 10.0]],
        [0.0, 5.0]
    ));
    assert!(!triangle_contains([[0.0; 2]; 3], [1.0, 0.0]));
}
