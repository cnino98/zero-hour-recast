use super::*;
use bevy::mesh::VertexAttributeValues;

fn mark(position: [f32; 3], radius: f32, kind: i32) -> Mark {
    Mark {
        position,
        radius,
        kind,
    }
}

#[test]
fn sloped_source_shadow_matches_extracted_native_vertex_uv_and_index_bits() {
    let mut terrain = crate::terrain::tests::terrain().unwrap();
    terrain.heights[27] = 16;
    let mesh = terrain.scorch_mesh(
        &VecDeque::from([mark([5.0, 5.0, 99.0], 5.0, 4)]),
        [0.5, 0.25, 0.75, 1.0],
    );
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("scorch positions");
    };
    // output/scorch-native-reference.log, copied native writeScorchToBuffer.
    assert_eq!(
        positions
            .iter()
            .map(|value| value.map(f32::to_bits))
            .collect::<Vec<_>>(),
        [
            [0, 0x3bcc_cccd, 0x8000_0000],
            [0x3f80_0000, 0x3bcc_cccd, 0x8000_0000],
            [0, 0x3bcc_cccd, 0xbf80_0000],
            [0x3f80_0000, 0x3f80_cccd, 0xbf80_0000],
        ]
    );
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("scorch UVs");
    };
    assert_eq!(
        uv,
        &[
            [0.375, 0.375],
            [0.625, 0.375],
            [0.375, 0.625],
            [0.625, 0.625]
        ]
    );
    assert_eq!(
        mesh.indices().unwrap().iter().collect::<Vec<_>>(),
        [0, 3, 2, 0, 1, 3]
    );
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("scorch colors");
    };
    assert!(
        colors
            .iter()
            .all(|color| color.map(f32::to_bits) == [0.5_f32, 0.25, 0.75, 1.0].map(f32::to_bits))
    );
}

#[test]
fn source_blend_flip_and_negative_clipping_keep_native_diagonal_and_uv_rounding() {
    let mut terrain = crate::terrain::tests::terrain().unwrap();
    terrain.tiles.blends.push(Some(generals_assets::map::Blend {
        tile: 0,
        directions: [true, false, false, false],
        inverted: 2,
        long_diagonal: false,
        custom_edge: -1,
    }));
    terrain.tiles.blend_tiles[18] = 1;
    let mesh = terrain.scorch_mesh(&VecDeque::from([mark([5.0, 5.0, 0.0], 5.0, 2)]), [1.0; 4]);
    assert_eq!(
        mesh.indices().unwrap().iter().collect::<Vec<_>>(),
        [1, 2, 0, 1, 3, 2]
    );
    let mesh = terrain.scorch_mesh(
        &VecDeque::from([mark([-19.0, -19.0, 0.0], 15.0, 4)]),
        [1.0; 4],
    );
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("scorch UVs");
    };
    assert_eq!(uv.len(), 9);
    assert_eq!(uv[0].map(f32::to_bits), [0x3efb_bbbc; 2]);
    assert_eq!(uv[4].map(f32::to_bits), [0x3f13_3333; 2]);
    assert_eq!(uv[8].map(f32::to_bits), [0x3f28_8888; 2]);
    assert_eq!(mesh.indices().unwrap().len(), 24);
}

#[test]
fn buffers_reject_whole_mark_and_prioritize_newest_before_stopping_at_native_limit() {
    let terrain = crate::terrain::tests::terrain().unwrap();
    let mut vertices = Geometry {
        positions: vec![[0.0; 3]; 32_765],
        uv: Vec::new(),
        indices: Vec::new(),
    };
    let source = mark([5.0, 5.0, 0.0], 5.0, 0);
    assert!(!vertices.append(&terrain, &source));
    assert_eq!(vertices.positions.len(), 32_765);
    let mut indices = Geometry {
        positions: Vec::new(),
        uv: Vec::new(),
        indices: vec![0; 65_530],
    };
    assert!(!indices.append(&terrain, &source));
    assert!(indices.positions.is_empty());
    let marks = (0..500)
        .map(|index| mark([5.0, 5.0, 0.0], 40.0, index % 9))
        .collect();
    let mesh = terrain.scorch_mesh(&marks, [1.0; 4]);
    assert_eq!(mesh.count_vertices(), 222 * 64);
    assert_eq!(mesh.indices().unwrap().len(), 222 * 294);
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("scorch UVs");
    };
    assert_eq!(uv[0].map(f32::to_bits), [0.421_875_f32.to_bits(); 2]);
    let outside = terrain.scorch_mesh(&VecDeque::from([mark([100.0; 3], 1.0, 0)]), [1.0; 4]);
    assert_eq!(outside.count_vertices(), 0);
    assert!(outside.indices().unwrap().is_empty());
}
