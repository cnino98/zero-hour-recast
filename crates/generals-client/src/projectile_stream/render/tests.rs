use super::*;
use crate::projectile_stream::inspection::mesh_positions;
use bevy::mesh::VertexAttributeValues;

#[test]
fn rendered_stream_supplies_material_normals_without_changing_prelit_strip() {
    let template = Template {
        width: 1.5,
        tile: 2.0,
        scroll: 6.0,
        maximum: 14,
        image: None,
        material: None,
    };
    let mut mesh = geometry(
        &template,
        &[
            [-20.0, 200.0, 10.0],
            [0.0, 200.0, 10.0],
            [20.0, 200.0, 10.0],
        ],
        0.25,
        &Transform::IDENTITY,
    )
    .unwrap();
    let vertices = mesh_positions(&mesh).unwrap().len();
    assert_eq!(vertices, 6);
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        panic!("stream material requires normals");
    };
    assert!(
        normals
            .iter()
            .all(|normal| normal.map(f32::to_bits) == [0.0_f32, 1.0, 0.0].map(f32::to_bits))
    );
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("original stream texture coordinates");
    };
    assert_eq!(
        uv.iter()
            .map(|value| value.map(f32::to_bits))
            .collect::<Vec<_>>(),
        [
            [0.0_f32, 0.25],
            [1.0, 0.25],
            [0.0, 2.25],
            [1.0, 2.25],
            [0.0, 4.25],
            [1.0, 4.25]
        ]
        .map(|value| value.map(f32::to_bits))
    );
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("original prelit strip colors");
    };
    assert!(
        colors
            .iter()
            .all(|color| color.map(f32::to_bits) == [1.0_f32.to_bits(); 4])
    );
    let normals = mesh.remove_attribute(Mesh::ATTRIBUTE_NORMAL).unwrap();
    assert!(mesh_positions(&mesh).is_err());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    assert!(mesh_positions(&mesh).is_ok());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; vertices - 1]);
    assert!(mesh_positions(&mesh).is_err());
}
