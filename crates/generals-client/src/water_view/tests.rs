use super::{data::Settings, geometry, shore};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use generals_assets::map::PolygonTrigger;

fn settings() -> Settings {
    Settings {
        diffuse: [0.5, 0.4, 0.3, 1.0],
        texture: String::new(),
        additive: false,
        depth: 3.0,
        opacity: 1.0,
        soft: true,
        feather: 0,
    }
}

#[test]
fn original_water_subdivision_preserves_sloped_polygon_height_and_fan_order() {
    let trigger = PolygonTrigger {
        id: 1,
        layer: String::new(),
        name: "native trapezoid".into(),
        water: true,
        river: false,
        river_start: 0,
        points: vec![[0, 0, 10], [80, 0, 20], [80, 80, 30], [0, 80, 40]],
    };
    let polygons = geometry::polygons(&[trigger]).unwrap();
    let mesh = geometry::mesh(&polygons[0], &settings(), 0.0, |_| 255);
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("water positions");
    };
    assert_eq!(points.len(), 9);
    assert_eq!(
        points[0].map(f32::to_bits),
        [0.0, 4.0, -8.0].map(f32::to_bits)
    );
    assert_eq!(
        points[4].map(f32::to_bits),
        [4.0, 2.5, -4.0].map(f32::to_bits)
    );
    // Native Y=+0 maps to backend Z=-0; retain that exact coordinate conversion.
    assert_eq!(
        points[8].map(f32::to_bits),
        [8.0, 2.0, -0.0].map(f32::to_bits)
    );
    assert_eq!(
        mesh.indices().unwrap().iter().take(6).collect::<Vec<_>>(),
        [0, 4, 3, 0, 1, 4]
    );
}

#[test]
fn river_start_pairs_original_banks_and_applies_shroud_to_rgb_and_alpha_once() {
    let trigger = PolygonTrigger {
        id: 1,
        layer: String::new(),
        name: "native river".into(),
        water: true,
        river: true,
        river_start: 0,
        points: vec![[0, 0, 10], [100, 0, 10], [100, 100, 10], [0, 100, 10]],
    };
    let polygons = geometry::polygons(&[trigger]).unwrap();
    let mesh = geometry::mesh(&polygons[0], &settings(), 0.0, |_| 128);
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("river positions")
    };
    assert_eq!(
        points,
        &vec![
            [10.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [10.0, 1.0, -10.0],
            [0.0, 1.0, -10.0]
        ]
    );
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("river colors")
    };
    assert_eq!(
        colors[0].map(f32::to_bits),
        [64.0 / 255.0, 51.0 / 255.0, 38.0 / 255.0, 128.0 / 255.0].map(f32::to_bits)
    );
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("river UV")
    };
    assert_eq!(uv[0].map(f32::to_bits), [0.5, 0.0].map(f32::to_bits));
    assert_eq!(uv[1].map(f32::to_bits), [0.0, 0.0].map(f32::to_bits));
    assert_eq!(
        mesh.indices().unwrap().iter().collect::<Vec<_>>(),
        [0, 1, 3, 0, 3, 2]
    );
}

#[test]
fn shore_lookup_uses_original_byte_gradient_capped_at_minimum_opacity() {
    let image = shore::lookup(0.5);
    let pixels = image.data.unwrap();
    assert_eq!(pixels[3], 0);
    assert_eq!(pixels[126 * 4 + 3], 126);
    assert_eq!(pixels[127 * 4 + 3], 127);
    assert_eq!(pixels[255 * 4 + 3], 127);
}

#[test]
fn original_feather_water_raises_only_first_corner_between_layers() {
    let trigger = PolygonTrigger {
        id: 1,
        layer: String::new(),
        name: "layered".into(),
        water: true,
        river: false,
        river_start: 0,
        points: vec![[0, 0, 10], [40, 0, 10], [40, 40, 10], [0, 40, 10]],
    };
    let polygons = geometry::polygons(&[trigger]).unwrap();
    let mut settings = settings();
    settings.feather = 2;
    let mesh = geometry::mesh(&polygons[0], &settings, 0.0, |_| 255);
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("feather positions")
    };
    assert_eq!(points.len(), 8);
    assert!((points[4][1] - points[0][1] - 0.2).abs() < 0.000_001);
    assert_eq!(points[7][1].to_bits(), points[3][1].to_bits());
}
