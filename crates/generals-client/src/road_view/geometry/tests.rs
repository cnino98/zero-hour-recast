use super::*;
use bevy::mesh::VertexAttributeValues;
use generals_assets::{HeightMap, map::TerrainTiles};
use std::str::SplitWhitespace;

fn number(words: &mut SplitWhitespace<'_>) -> f32 {
    words.next().unwrap().parse().unwrap()
}

fn count(words: &mut SplitWhitespace<'_>) -> usize {
    words.next().unwrap().parse().unwrap()
}

fn vector(words: &mut SplitWhitespace<'_>) -> Vec2 {
    Vec2::new(number(words), number(words))
}

fn terrain(mode: u16) -> Terrain {
    let heights = (0..64_u16)
        .flat_map(|y| {
            (0..64).map(move |x| match mode {
                0 => 7,
                1 => x + y,
                2 => {
                    if x == 9 {
                        23
                    } else {
                        6
                    }
                }
                3 => x / 4,
                4 => (x * 7 + y * 11) % 31,
                _ => unreachable!("native reference terrain mode"),
            })
        })
        .collect();
    Terrain::new(
        HeightMap {
            width: 64,
            height: 64,
            border: 2,
            boundaries: vec![[60, 60]],
            heights,
        },
        TerrainTiles {
            width: 64,
            height: 64,
            base_tiles: vec![0; 4096],
            blend_tiles: vec![0; 4096],
            extra_blend_tiles: vec![0; 4096],
            cliff_tiles: vec![0; 4096],
            cliff_flags: vec![0; 512],
            texture_classes: Vec::new(),
            edge_classes: Vec::new(),
            blends: vec![None],
            cliffs: vec![None],
        },
        true,
        true,
    )
    .unwrap()
}

#[test]
fn road_mesh_matches_extracted_native_flat_slope_crest_border_and_short_sections() {
    // Generated only by check-road-geometry-reference.py from unchanged pinned
    // native functions. Compare actual Bevy mesh output, not a duplicate port.
    let mut words = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/reference/road-geometry-reference.txt"
    ))
    .split_whitespace();
    for case in 0..20 {
        let terrain = terrain(u16::try_from(count(&mut words)).unwrap());
        let section = Section {
            origin: vector(&mut words),
            normal: vector(&mut words),
            direction: vector(&mut words),
            corners: std::array::from_fn(|_| vector(&mut words)),
            offset: vector(&mut words),
            scale: vector(&mut words),
        };
        let vertices = count(&mut words);
        let indices = count(&mut words);
        let mesh = mesh(&section, &terrain).unwrap();
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("road positions")
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("road UVs")
        };
        assert_eq!(
            positions.len(),
            vertices,
            "native vertex count, case {case}"
        );
        for vertex in 0..vertices {
            let values = std::array::from_fn::<_, 5, _>(|_| {
                f32::from_bits(u32::try_from(count(&mut words)).unwrap())
            });
            let position = [values[0] * 0.1, values[2] * 0.1, -values[1] * 0.1];
            assert_eq!(
                positions[vertex].map(f32::to_bits),
                position.map(f32::to_bits),
                "native position, case {case}, vertex {vertex}"
            );
            assert_eq!(
                uvs[vertex].map(f32::to_bits),
                [values[3].to_bits(), values[4].to_bits()],
                "native atlas UV, case {case}, vertex {vertex}"
            );
        }
        let expected: Vec<_> = (0..indices).map(|_| count(&mut words)).collect();
        assert_eq!(
            mesh.indices().unwrap().iter().collect::<Vec<_>>(),
            expected,
            "native winding/connectivity, case {case}"
        );
    }
    assert!(words.next().is_none());
}
