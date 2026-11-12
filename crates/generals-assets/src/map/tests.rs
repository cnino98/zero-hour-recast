use super::*;

fn string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend(
        u16::try_from(value.len())
            .expect("short fixture string")
            .to_le_bytes(),
    );
    bytes.extend(value.as_bytes());
}

fn object_chunk(version: u16, z: f32, properties: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    for value in [1004.8322_f32, 583.634, z, std::f32::consts::FRAC_PI_4] {
        data.extend(value.to_le_bytes());
    }
    data.extend(7_u32.to_le_bytes());
    string(&mut data, "SupplyDock");
    data.extend(properties);
    let mut chunk = 1_u32.to_le_bytes().to_vec();
    chunk.extend(version.to_le_bytes());
    chunk.extend(
        u32::try_from(data.len())
            .expect("small fixture")
            .to_le_bytes(),
    );
    chunk.extend(data);
    chunk
}

#[test]
fn placed_objects_preserve_native_coordinates_typed_properties_and_version_rules() {
    let names = BTreeMap::from([
        (1, "Object".to_owned()),
        (2, "waypointName".to_owned()),
        (3, "waypointID".to_owned()),
    ]);
    let mut dict = 3_u16.to_le_bytes().to_vec();
    dict.extend(((2_u32 << 8) | 3).to_le_bytes());
    string(&mut dict, "Old");
    dict.extend(((2_u32 << 8) | 3).to_le_bytes());
    string(&mut dict, "Player_1_Start");
    dict.extend(((3_u32 << 8) | 1).to_le_bytes());
    dict.extend(28_i32.to_le_bytes());
    let objects = read_objects(&object_chunk(3, 12.5, &dict), &names).expect("valid object");
    assert_eq!(
        objects[0].position.map(f32::to_bits),
        [1004.8322_f32, 583.634, 12.5].map(f32::to_bits)
    );
    assert_eq!(objects[0].waypoint_name(), Some("Player_1_Start"));
    assert_eq!(objects[0].waypoint_id(), Some(28));
    let old = read_objects(&object_chunk(2, 12.5, &dict), &names).expect("old object");
    assert_eq!(old[0].position[2].to_bits(), 0.0_f32.to_bits());
    assert!(
        read_objects(&object_chunk(3, 2000.0, &dict), &names)
            .expect("source drops object")
            .is_empty()
    );
}

#[test]
fn rejects_object_boundary_unknown_dict_type_and_nonfinite_coordinate() {
    let names = BTreeMap::from([(1, "Object".to_owned()), (2, "waypointName".to_owned())]);
    let chunk = object_chunk(3, 0.0, &0_u16.to_le_bytes());
    assert!(read_objects(&chunk[..chunk.len() - 1], &names).is_err());
    assert!(read_objects(&object_chunk(3, f32::NAN, &0_u16.to_le_bytes()), &names).is_err());
    let mut dict = 1_u16.to_le_bytes().to_vec();
    dict.extend(((2_u32 << 8) | 5).to_le_bytes());
    assert!(read_objects(&object_chunk(3, 0.0, &dict), &names).is_err());
}

fn heightmap(width: u32, height: u32) -> HeightMap {
    HeightMap {
        width,
        height,
        border: 0,
        boundaries: vec![[
            i32::try_from(width).unwrap(),
            i32::try_from(height).unwrap(),
        ]],
        heights: vec![0; usize::try_from(width * height).expect("small fixture")],
    }
}

fn tile_bytes(map: &HeightMap, version: u16) -> Vec<u8> {
    let count = map.width * map.height;
    let mut bytes = count.to_le_bytes().to_vec();
    bytes.extend(vec![0; usize::try_from(count * 8).expect("small fixture")]);
    let stride = if version == 7 {
        (map.width + 1) / 8
    } else {
        map.width.div_ceil(8)
    };
    bytes.extend(vec![
        0x81;
        usize::try_from(stride * map.height)
            .expect("small fixture")
    ]);
    for field in [4_u32, 1, 0, 1, 0, 4, 2, 0] {
        bytes.extend(field.to_le_bytes());
    }
    string(&mut bytes, "Sand");
    bytes.extend([0; 8]); // no edge tiles/classes
    bytes
}

#[test]
fn version_seven_cliff_rows_are_padded_without_leaking_to_next_row() {
    let map = heightmap(10, 3);
    let tiles =
        TerrainTiles::parse(&tile_bytes(&map, 7), 7, &map).expect("source version-seven padding");
    assert_eq!(tiles.cliff_flags, [0x81, 0, 0x81, 0, 0x81, 0]);
    assert!(tiles.is_cliff(0, 2));
    assert!(tiles.is_cliff(7, 2));
    assert!(!tiles.is_cliff(8, 2));
    assert!(!tiles.is_cliff(9, 1));
}

#[test]
fn rejects_mismatched_tile_arrays_and_truncated_tables() {
    let map = heightmap(4, 4);
    let bytes = tile_bytes(&map, 8);
    assert!(TerrainTiles::parse(&bytes[..bytes.len() - 1], 8, &map).is_err());
    let mut wrong_count = bytes.clone();
    wrong_count[..4].copy_from_slice(&15_u32.to_le_bytes());
    assert!(TerrainTiles::parse(&wrong_count, 8, &map).is_err());
    let mut invalid_tile = bytes;
    invalid_tile[4..6].copy_from_slice(&16_u16.to_le_bytes());
    assert!(TerrainTiles::parse(&invalid_tile, 8, &map).is_err());
}

#[test]
fn source_tile_quadrants_and_cliff_stretch_are_retained() {
    let mut map = heightmap(4, 4);
    let tiles = TerrainTiles::parse(&tile_bytes(&map, 8), 8, &map).expect("valid tiles");
    let first = tiles
        .texture_uv(0, 0, &map.heights, false)
        .expect("base UV");
    assert_eq!(
        first.corners,
        [[0.0, 1.0], [0.25, 1.0], [0.25, 0.75], [0.0, 0.75]]
    );
    let next = tiles
        .texture_uv(0, 3, &map.heights, false)
        .expect("next quadrant");
    assert_eq!(
        next.corners,
        [[0.25, 0.75], [0.5, 0.75], [0.5, 0.5], [0.25, 0.5]]
    );
    map.heights[0] = 64;
    let cliff = tiles
        .texture_uv(0, 0, &map.heights, true)
        .expect("source peak stretch");
    assert!(cliff.stretched);
    assert_eq!(cliff.corners[0][1].to_bits(), 1.0_f32.to_bits());
    assert_eq!(cliff.corners[1][1].to_bits(), 0.25_f32.to_bits());
    assert!(
        !tiles
            .texture_uv(0, 0, &map.heights, false)
            .expect("disabled adjustment")
            .stretched
    );
}
