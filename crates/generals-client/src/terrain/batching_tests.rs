//! Preserve old terrain geometry while bounding only opaque spatial uploads.
use super::{Batch, HeightMap, Terrain, TerrainBatch, TerrainTiles, VERTEX_BUFFER_TILE_LENGTH};
use bevy::{mesh::VertexAttributeValues, prelude::Mesh};
use generals_assets::map::{Blend, CliffMapping, TextureClass};
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

type Quads = BTreeMap<(String, u32, u32), (Vec<u8>, Vec<usize>)>;

fn terrain(adjust_cliffs: bool, three_way: bool) -> Terrain {
    let width = 66_u16;
    let height = 35_u16;
    let count = usize::from(width) * usize::from(height);
    let map = HeightMap {
        width: u32::from(width),
        height: u32::from(height),
        border: 2,
        boundaries: vec![[61, 30]],
        heights: (0..height)
            .flat_map(|y| (0..width).map(move |x| (17 * x + 13 * y) % 256))
            .collect(),
    };
    let tiles = TerrainTiles {
        width: u32::from(width),
        height: u32::from(height),
        base_tiles: (0..height)
            .flat_map(|y| (0..width).map(move |x| ((x + y) % 2) * 4 + x % 4))
            .collect(),
        blend_tiles: (0..count)
            .map(|index| if index % 3 == 0 { 1 } else { 2 })
            .collect(),
        extra_blend_tiles: (0..count).map(|index| u16::from(index % 5 == 0)).collect(),
        cliff_tiles: (0..count).map(|index| u16::from(index % 7 == 0)).collect(),
        cliff_flags: vec![0; usize::from(width.div_ceil(8)) * usize::from(height)],
        texture_classes: vec![
            TextureClass {
                name: "SourceSand".to_owned(),
                first_tile: 0,
                tile_count: 1,
                width: 1,
            },
            TextureClass {
                name: "SourceRock".to_owned(),
                first_tile: 1,
                tile_count: 1,
                width: 1,
            },
        ],
        edge_classes: Vec::new(),
        blends: vec![
            None,
            Some(Blend {
                tile: 4,
                directions: [true, false, true, false],
                inverted: 1,
                long_diagonal: true,
                custom_edge: -1,
            }),
            Some(Blend {
                tile: 0,
                directions: [false, true, false, true],
                inverted: 2,
                long_diagonal: false,
                custom_edge: -1,
            }),
        ],
        cliffs: vec![
            None,
            Some(CliffMapping {
                tile: 0,
                uv: [[0.0, 0.0], [0.03, 0.0], [0.02, 0.04], [0.0, 0.03]],
                flip: true,
                mutant: false,
            }),
        ],
    };
    Terrain::new(map, tiles, adjust_cliffs, three_way).unwrap()
}

fn positions(mesh: &Mesh) -> &[[f32; 3]] {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("positions");
    };
    positions
}

fn opaque_quads(batches: &[TerrainBatch]) -> Quads {
    let mut quads = BTreeMap::new();
    for batch in batches.iter().filter(|batch| batch.layer == 0) {
        let mesh = &batch.mesh;
        let packed = mesh.create_packed_vertex_buffer_data();
        let stride = packed.len() / mesh.count_vertices();
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for (quad, points) in positions(mesh).chunks_exact(4).enumerate() {
            let key = (
                batch.texture_class.clone(),
                points[0][0].to_bits(),
                points[0][2].to_bits(),
            );
            let bytes = packed[quad * 4 * stride..(quad + 1) * 4 * stride].to_vec();
            let directed = indices[quad * 6..(quad + 1) * 6]
                .iter()
                .map(|index| index - quad * 4)
                .collect();
            assert!(
                quads.insert(key, (bytes, directed)).is_none(),
                "every original cell occurs once"
            );
        }
    }
    quads
}

fn assert_blended_identical(actual: &[TerrainBatch], expected: &[TerrainBatch]) {
    let actual: Vec<_> = actual.iter().filter(|batch| batch.layer > 0).collect();
    let expected: Vec<_> = expected.iter().filter(|batch| batch.layer > 0).collect();
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_eq!(
            (actual.layer, &actual.texture_class, actual.pixel_extent),
            (
                expected.layer,
                &expected.texture_class,
                expected.pixel_extent
            )
        );
        assert_eq!(
            actual.mesh.create_packed_vertex_buffer_data(),
            expected.mesh.create_packed_vertex_buffer_data()
        );
        assert_eq!(actual.mesh.indices(), expected.mesh.indices());
    }
}

fn spatial_keys(terrain: &Terrain, batches: &[TerrainBatch]) -> Vec<(usize, u16, u16)> {
    let mut keys = Vec::new();
    for batch in batches.iter().filter(|batch| batch.layer == 0) {
        let class = terrain
            .tiles
            .texture_classes
            .iter()
            .position(|class| class.name == batch.texture_class)
            .unwrap();
        assert_eq!(
            batch.pixel_extent,
            u32::from(terrain.tiles.texture_classes[class].width) * 64
        );
        let mut key = None;
        let mut previous = None;
        for quad in positions(&batch.mesh).chunks_exact(4) {
            let x = (quad[0][0] + f32::from(terrain.border)).to_u16().unwrap();
            let y = (f32::from(terrain.border) - quad[0][2]).to_u16().unwrap();
            if let Some(previous) = previous {
                assert!(
                    previous < (y, x),
                    "source row-major cell order within a tile"
                );
            }
            previous = Some((y, x));
            let current = (
                class,
                y / VERTEX_BUFFER_TILE_LENGTH,
                x / VERTEX_BUFFER_TILE_LENGTH,
            );
            if let Some(key) = key {
                assert_eq!(key, current, "one source spatial tile per opaque mesh");
            }
            key = Some(current);
        }
        keys.push(key.unwrap());
        assert!(batch.mesh.count_vertices() <= 4 * usize::from(VERTEX_BUFFER_TILE_LENGTH).pow(2));
    }
    assert!(
        keys.windows(2).all(|pair| pair[0] < pair[1]),
        "stable class-major, tile-y/x order"
    );
    keys
}

#[test]
fn spatial_opaque_preserves_every_border_quad_and_complete_blend_streams() {
    for (adjust_cliffs, three_way) in [(false, false), (false, true), (true, false), (true, true)] {
        let terrain = terrain(adjust_cliffs, three_way);
        let expected = old_batches(&terrain);
        let actual = terrain.batches();
        let quads = opaque_quads(&actual);
        assert_eq!(
            quads.len(),
            65 * 34,
            "includes all stored border cells and partial edge tiles"
        );
        assert_eq!(quads, opaque_quads(&expected));
        assert_blended_identical(&actual, &expected);
        let keys = spatial_keys(&terrain, &actual);
        assert_eq!(
            keys.len(),
            2 * 3 * 2,
            "both texture classes occur in all six spatial tiles"
        );
        assert_eq!(keys.first(), Some(&(0, 0, 0)));
        assert_eq!(keys.last(), Some(&(1, 1, 2)));
    }
}

#[test]
fn height_rebuild_keeps_surface_slots_and_global_neighbors_across_tile_edges() {
    let mut terrain = terrain(true, true);
    let before = terrain.batches();
    let keys = spatial_keys(&terrain, &before);
    for (x, y) in [(31, 31), (32, 31), (33, 31), (31, 32), (32, 32), (33, 33)] {
        terrain.heights[y * usize::from(terrain.raw_width) + x] = 0;
    }
    let after = terrain.batches();
    assert_eq!(
        after.len(),
        before.len(),
        "Surface(index) still names the same batch"
    );
    assert_eq!(spatial_keys(&terrain, &after), keys);
    for (before, after) in before.iter().zip(&after) {
        assert_eq!(
            (&before.texture_class, before.layer, before.pixel_extent),
            (&after.texture_class, after.layer, after.pixel_extent)
        );
    }
    let changed = before
        .iter()
        .zip(&after)
        .filter(|(before, after)| {
            before.layer == 0
                && before.mesh.create_packed_vertex_buffer_data()
                    != after.mesh.create_packed_vertex_buffer_data()
        })
        .count();
    assert!(
        changed >= 4,
        "neighbor normals and heights change on both sides of X/Y tile edges"
    );
    let expected = old_batches(&terrain);
    assert_eq!(opaque_quads(&after), opaque_quads(&expected));
    assert_blended_identical(&after, &expected);
}

#[ignore = "requires original game installation"]
#[test]
fn installed_dark_mountain_bounds_large_opaque_classes_by_source_tile_size() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = generals_assets::map::MapData::parse(
        &assets
            .read("Maps\\Dark Mountain\\Dark Mountain.map")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let terrain = Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
    let batches = terrain.batches();
    let keys = spatial_keys(&terrain, &batches);
    let tile_count = usize::from((terrain.raw_width - 1).div_ceil(VERTEX_BUFFER_TILE_LENGTH))
        * usize::from((terrain.raw_height - 1).div_ceil(VERTEX_BUFFER_TILE_LENGTH));
    assert!(keys.len() <= tile_count * terrain.tiles.texture_classes.len());
    let mut class_vertices = BTreeMap::<&str, usize>::new();
    for batch in batches.iter().filter(|batch| batch.layer == 0) {
        *class_vertices.entry(&batch.texture_class).or_default() += batch.mesh.count_vertices();
        assert!(batch.mesh.get_vertex_buffer_size() <= 4096 * 48);
    }
    let (class, vertices) = class_vertices
        .iter()
        .max_by_key(|(_, count)| **count)
        .unwrap();
    eprintln!(
        "terrain-opaque-profile-map class={class} unsplit_vertices={vertices} unsplit_vertex_bytes={}",
        vertices * 48
    );
    assert!(
        class_vertices.values().any(|&count| count > 4096 * 100),
        "fixture contains a genuinely large original class mesh"
    );
    assert_eq!(
        class_vertices.values().sum::<usize>(),
        usize::from(terrain.raw_width - 1) * usize::from(terrain.raw_height - 1) * 4
    );
}

// Pre-partition batching/append functions, with self replaced by terrain only.
// They retain the original complete blend order and numerical geometry oracle.
fn old_batches(terrain: &Terrain) -> Vec<TerrainBatch> {
    let count = terrain.tiles.texture_classes.len();
    let mut batches: Vec<_> = (0..count * 3).map(|_| Batch::default()).collect();
    // The source draws the stored surrounding border as well as playable cells.
    for y in 0..terrain.raw_height - 1 {
        for x in 0..terrain.raw_width - 1 {
            old_append_cell(terrain, &mut batches, x, y);
        }
    }
    batches
        .into_iter()
        .enumerate()
        .filter_map(|(index, batch)| {
            if batch.positions.is_empty() {
                return None;
            }
            let texture_class = terrain.tiles.texture_classes[index % count].name.clone();
            let layer = u8::try_from(index / count).expect("three terrain layers");
            Some(TerrainBatch {
                texture_class,
                pixel_extent: u32::from(terrain.tiles.texture_classes[index % count].width) * 64,
                layer,
                mesh: batch.mesh(),
            })
        })
        .collect()
}

fn old_append_cell(terrain: &Terrain, batches: &mut [Batch], x: u16, y: u16) {
    let index = terrain.index(x, y);
    let primary = terrain.tiles.blends[usize::from(terrain.tiles.blend_tiles[index])].as_ref();
    let primary_tile = primary.map_or(terrain.tiles.base_tiles[index], |blend| blend.tile);
    let primary_uv = terrain
        .tiles
        .texture_uv(
            index,
            primary_tile,
            &terrain.heights,
            terrain.adjust_cliff_textures,
        )
        .expect("validated terrain cell");
    let positions = terrain.corners(x, y);
    let normals =
        [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)].map(|(x, y)| terrain.normal(x, y));
    let cliff_flip =
        (positions[0][1] - positions[2][1]).abs() > (positions[1][1] - positions[3][1]).abs();
    let flip = terrain.primary_flip(x, y, primary_uv.stretched);
    let base_uv = terrain
        .tiles
        .texture_uv(
            index,
            terrain.tiles.base_tiles[index],
            &terrain.heights,
            terrain.adjust_cliff_textures,
        )
        .expect("validated terrain cell");
    batches[base_uv.texture_class].append(positions, normals, base_uv.corners, [255; 4], flip);
    if let Some(primary) = primary {
        let (alpha, _) = primary.alpha_and_flip();
        batches[terrain.tiles.texture_classes.len() + primary_uv.texture_class].append(
            positions,
            normals,
            primary_uv.corners,
            alpha,
            flip,
        );
    }
    if terrain.use_three_way_blends
        && let Some(extra) =
            terrain.tiles.blends[usize::from(terrain.tiles.extra_blend_tiles[index])].as_ref()
    {
        let uv = terrain
            .tiles
            .texture_uv(
                index,
                extra.tile,
                &terrain.heights,
                terrain.adjust_cliff_textures,
            )
            .expect("validated terrain cell");
        let (alpha, extra_flip) = extra.alpha_and_flip();
        batches[2 * terrain.tiles.texture_classes.len() + uv.texture_class].append(
            positions,
            normals,
            uv.corners,
            alpha,
            extra_flip || uv.stretched && cliff_flip,
        );
    }
}
