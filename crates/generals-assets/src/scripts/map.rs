//! Serialized map-authored sides, build lists, teams, and player script lists.
//!
//! This preserves `SidesList::ParseSidesDataChunk` records for admission checks;
//! it does not execute map scripts or implement build-list behavior.

use std::{borrow::Cow, collections::BTreeMap};

use crate::{
    AssetError,
    binary::Reader,
    decompress,
    heightmap::read_symbols,
    map::{MapValue, ascii, finish, integer, read_chunk, read_dict, real},
};

use super::{ScriptList, read_list};

/// Parsed `SidesList` metadata. Raw team dictionaries retain singleton flags
/// and other source fields so callers can distinguish native defaults.
pub struct MapScripts {
    pub version: u16,
    pub sides: Vec<MapSide>,
    pub teams: Vec<BTreeMap<String, MapValue>>,
    /// Script lists are bound by side order, matching the native reader.
    pub script_lists: Vec<ScriptList>,
}

pub struct MapSide {
    pub properties: BTreeMap<String, MapValue>,
    pub build_list: Vec<MapBuildEntry>,
}

/// One source `BuildListInfo` entry; version-3 fields are absent in older maps.
pub struct MapBuildEntry {
    pub building_name: String,
    pub template_name: String,
    /// Serialized coordinates. The native reader preserves X/Y but forces Z to
    /// ground level before installing this record into the live side list.
    pub position: [f32; 3],
    pub angle: f32,
    pub initially_built: u8,
    pub rebuilds: i32,
    pub script: Option<String>,
    pub health: Option<i32>,
    pub whiner: Option<u8>,
    pub unsellable: Option<u8>,
    pub repairable: Option<u8>,
}

/// Parse the map's `SidesList` chunk from a raw or RefPack-wrapped map.
///
/// # Errors
/// Rejects missing or repeated sides chunks, unsupported versions, malformed
/// records, invalid nested script lists, and truncated input.
pub fn map_scripts(bytes: &[u8]) -> Result<MapScripts, AssetError> {
    let decoded = if bytes.starts_with(b"CkMp") {
        Cow::Borrowed(bytes)
    } else {
        Cow::Owned(decompress(bytes)?)
    };
    let mut reader = Reader::new(&decoded);
    if reader.take(4)? != b"CkMp" {
        return Err(AssetError::Invalid("map does not have a CkMp signature"));
    }
    let symbols = read_symbols(&mut reader)?;
    let mut sides_result = None;
    while !reader.is_empty() {
        let chunk = read_chunk(&mut reader, &symbols)?;
        if chunk.name == "SidesList" {
            if sides_result.is_some() {
                return Err(AssetError::Invalid("duplicate SidesList chunk"));
            }
            sides_result = Some(read_sides(chunk.bytes, chunk.version, &symbols)?);
        }
    }
    sides_result.ok_or(AssetError::Invalid("map has no SidesList chunk"))
}

fn read_sides(
    bytes: &[u8],
    version: u16,
    symbols: &BTreeMap<u32, String>,
) -> Result<MapScripts, AssetError> {
    if !(1..=3).contains(&version) {
        return Err(AssetError::Invalid("unsupported SidesList version"));
    }
    let mut reader = Reader::new(bytes);
    let side_count = bounded_count(&mut reader, 16, "invalid SidesList side count")?;
    let mut sides = Vec::with_capacity(side_count);
    for _ in 0..side_count {
        let properties = read_dict(&mut reader, symbols)?;
        let build_count =
            bounded_count(&mut reader, 100_000, "invalid SidesList build-list count")?;
        let mut build_list = Vec::with_capacity(build_count);
        for _ in 0..build_count {
            let building_name = ascii(&mut reader)?;
            let template_name = ascii(&mut reader)?;
            let position = [real(&mut reader)?, real(&mut reader)?, real(&mut reader)?];
            let angle = real(&mut reader)?;
            let initially_built = reader.byte()?;
            let rebuilds = integer(&mut reader)?;
            let (script, health, whiner, unsellable, repairable) = if version >= 3 {
                (
                    Some(ascii(&mut reader)?),
                    Some(integer(&mut reader)?),
                    Some(reader.byte()?),
                    Some(reader.byte()?),
                    Some(reader.byte()?),
                )
            } else {
                (None, None, None, None, None)
            };
            build_list.push(MapBuildEntry {
                building_name,
                template_name,
                position,
                angle,
                initially_built,
                rebuilds,
                script,
                health,
                whiner,
                unsellable,
                repairable,
            });
        }
        sides.push(MapSide {
            properties,
            build_list,
        });
    }
    let teams = if version >= 2 {
        let count = bounded_count(&mut reader, 100_000, "invalid SidesList team count")?;
        let mut teams = Vec::with_capacity(count);
        for _ in 0..count {
            teams.push(read_dict(&mut reader, symbols)?);
        }
        teams
    } else {
        Vec::new()
    };

    // The enclosing SidesList chunk embeds a PlayerScriptsList chunk. The
    // native reader permits extra player lists and drops lists beyond sides.
    let nested = read_chunk(&mut reader, symbols)?;
    if nested.name != "PlayerScriptsList" {
        return Err(AssetError::Invalid(
            "invalid SidesList player scripts chunk",
        ));
    }
    let mut nested_reader = Reader::new(nested.bytes);
    let mut script_lists = Vec::new();
    while !nested_reader.is_empty() {
        let list = read_chunk(&mut nested_reader, symbols)?;
        if list.name != "ScriptList" || list.version != 1 || script_lists.len() >= 16 {
            return Err(AssetError::Invalid("invalid SidesList script list"));
        }
        script_lists.push(read_list(list.bytes, symbols)?);
    }
    // Native binding associates only the first list per side and drops extras.
    script_lists.truncate(sides.len());
    finish(&reader)?;
    Ok(MapScripts {
        version,
        sides,
        teams,
        script_lists,
    })
}

fn bounded_count(
    reader: &mut Reader<'_>,
    maximum: usize,
    message: &'static str,
) -> Result<usize, AssetError> {
    let count = usize::try_from(integer(reader)?).map_err(|_| AssetError::Invalid(message))?;
    if count > maximum {
        return Err(AssetError::Invalid(message));
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(symbol: u32, version: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = symbol.to_le_bytes().to_vec();
        bytes.extend(version.to_le_bytes());
        bytes.extend(u32::try_from(payload.len()).unwrap().to_le_bytes());
        bytes.extend(payload);
        bytes
    }

    fn fixture(version: u16) -> Vec<u8> {
        fixture_with_script_versions(version, 5, 1)
    }

    fn fixture_with_script_versions(
        version: u16,
        player_scripts_version: u16,
        script_list_version: u16,
    ) -> Vec<u8> {
        let mut nested_payload = Vec::new();
        nested_payload.extend(chunk(3, script_list_version, &[]));
        let nested = chunk(2, player_scripts_version, &nested_payload);
        let mut sides = 1_i32.to_le_bytes().to_vec(); // side count
        sides.extend(0_u16.to_le_bytes()); // empty side dict
        sides.extend(1_i32.to_le_bytes()); // build count
        sides.extend(1_u16.to_le_bytes());
        sides.extend(b"B");
        sides.extend(1_u16.to_le_bytes());
        sides.extend(b"T");
        for value in [1.0_f32, 2.0, 3.0, 0.5] {
            sides.extend(value.to_le_bytes());
        }
        sides.push(1);
        sides.extend(4_i32.to_le_bytes());
        if version >= 3 {
            sides.extend(0_u16.to_le_bytes()); // no build script
            sides.extend(100_i32.to_le_bytes());
            sides.extend([0, 1, 1]);
        }
        if version >= 2 {
            sides.extend(1_i32.to_le_bytes()); // one default singleton team
            sides.extend(1_u16.to_le_bytes());
            sides.extend((4_u32 << 8).to_le_bytes());
            sides.push(1);
        }
        sides.extend(nested);

        let mut bytes = b"CkMp".to_vec();
        bytes.extend(4_u32.to_le_bytes());
        for (id, name) in [
            (1_u32, "SidesList"),
            (2, "PlayerScriptsList"),
            (3, "ScriptList"),
            (4, "teamIsSingleton"),
        ] {
            bytes.push(u8::try_from(name.len()).unwrap());
            bytes.extend(name.as_bytes());
            bytes.extend(id.to_le_bytes());
        }
        bytes.extend(chunk(1, version, &sides));
        bytes
    }

    #[test]
    fn parses_source_versions_and_preserves_team_and_build_list_records() {
        for version in 1..=3 {
            let parsed = map_scripts(&fixture(version)).expect("versioned side metadata");
            assert_eq!(parsed.version, version);
            assert_eq!(parsed.sides.len(), 1);
            let build = &parsed.sides[0].build_list[0];
            assert_eq!(build.building_name, "B");
            assert_eq!(build.template_name, "T");
            // Preserve serialized Z; native SidesList forces it to zero.
            assert_eq!(
                build.position.map(f32::to_bits),
                [1.0_f32, 2.0, 3.0].map(f32::to_bits)
            );
            assert_eq!(build.initially_built, 1);
            assert_eq!(build.rebuilds, 4);
            assert_eq!(build.script.is_some(), version >= 3);
            assert_eq!(parsed.teams.len(), usize::from(version >= 2));
            if version >= 2 {
                assert!(matches!(
                    parsed.teams[0].get("teamIsSingleton"),
                    Some(MapValue::Bool(true))
                ));
            }
            assert_eq!(parsed.script_lists.len(), 1);
        }
    }

    #[test]
    fn malformed_sides_versions_counts_and_nested_lists_are_rejected() {
        assert!(matches!(
            read_sides(&[], 4, &BTreeMap::new()),
            Err(AssetError::Invalid("unsupported SidesList version"))
        ));
        assert!(matches!(
            read_sides(&[1, 0, 0], 3, &BTreeMap::new()),
            Err(AssetError::Invalid("truncated input"))
        ));
        let valid = fixture(3);
        assert!(matches!(
            map_scripts(&valid[..valid.len() - 1]),
            Err(AssetError::Invalid("truncated input"))
        ));
        let names = [
            "SidesList",
            "PlayerScriptsList",
            "ScriptList",
            "teamIsSingleton",
        ];
        let chunks_start = 8 + names.iter().map(|name| 1 + name.len() + 4).sum::<usize>();
        let mut duplicate = valid.clone();
        duplicate.extend_from_slice(&valid[chunks_start..]);
        assert!(matches!(
            map_scripts(&duplicate),
            Err(AssetError::Invalid("duplicate SidesList chunk"))
        ));
        assert!(matches!(
            map_scripts(&fixture_with_script_versions(3, 5, 2)),
            Err(AssetError::Invalid("invalid SidesList script list"))
        ));
    }

    #[test]
    fn player_scripts_chunk_versions_do_not_change_nested_list_parsing() {
        for player_scripts_version in [4, 5] {
            let parsed = map_scripts(&fixture_with_script_versions(3, player_scripts_version, 1))
                .expect("native parser accepts enclosing script-list versions");
            assert_eq!(parsed.version, 3);
            assert_eq!(parsed.sides.len(), 1);
            assert_eq!(parsed.script_lists.len(), 1);
            assert_eq!(parsed.sides[0].build_list[0].building_name, "B");
            assert_eq!(
                parsed.sides[0].build_list[0].position.map(f32::to_bits),
                [1.0_f32, 2.0, 3.0].map(f32::to_bits)
            );
            assert!(matches!(
                parsed.teams[0].get("teamIsSingleton"),
                Some(MapValue::Bool(true))
            ));
            assert!(parsed.script_lists[0].scripts.is_empty());
            assert!(parsed.script_lists[0].groups.is_empty());
        }
    }

    #[test]
    #[ignore = "requires original game installation"]
    fn installed_zero_hour_map_sides_metadata_is_readable() {
        use crate::virtual_assets::VirtualAssets;
        let game_data = crate::game_root().unwrap().join("GameData");
        let assets = VirtualAssets::for_zero_hour(
            game_data.join("Command and Conquer Generals"),
            game_data.join("Command and Conquer Generals Zero Hour"),
        )
        .expect("installed Generals and Zero Hour archives");
        let asset = assets
            .read("Maps/Alpine Assault/Alpine Assault.map")
            .expect("installed Alpine Assault map");
        let parsed = map_scripts(&asset.bytes).expect("installed map side metadata");
        assert!(
            !parsed.sides.is_empty(),
            "{} should contain a side list",
            asset.path
        );
    }
}
