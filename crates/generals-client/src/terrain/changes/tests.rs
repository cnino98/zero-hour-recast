use super::*;
use std::collections::VecDeque;

#[test]
fn updated_vertices_rebuild_terrain_normals_and_scorch_surface_with_original_uvs() {
    let mut terrain = crate::terrain::tests::terrain().unwrap();
    terrain.heights.fill(16);
    terrain.adjust_cliff_textures = false;
    let before = terrain.batches().remove(0).mesh;
    let old_uv = before.attribute(Mesh::ATTRIBUTE_UV_0).unwrap().clone();
    let mut map = terrain.height_map();
    map.samples[27] = 0.0;
    terrain.apply_height_map(&map);
    assert_eq!(terrain.sample(Vec2::ONE).to_bits(), 0.0_f32.to_bits());
    let after = terrain.batches().remove(0).mesh;
    assert_ne!(
        after.attribute(Mesh::ATTRIBUTE_POSITION),
        before.attribute(Mesh::ATTRIBUTE_POSITION)
    );
    assert_ne!(
        after.attribute(Mesh::ATTRIBUTE_NORMAL),
        before.attribute(Mesh::ATTRIBUTE_NORMAL)
    );
    assert_eq!(after.attribute(Mesh::ATTRIBUTE_UV_0).unwrap(), &old_uv);
    let scorch = terrain.scorch_mesh(
        &VecDeque::from([crate::scorch::Mark {
            position: [5.0, 5.0, 0.0],
            radius: 5.0,
            kind: 0,
        }]),
        [1.0; 4],
    );
    let Some(VertexAttributeValues::Float32x3(vertices)) =
        scorch.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("scorch vertices")
    };
    assert_eq!(vertices[3][1].to_bits(), 0.00625_f32.to_bits());
}
