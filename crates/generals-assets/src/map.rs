//! Source map records, retaining native world coordinates and typed properties.
//!
//! Ported from `SuperHackers` 2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed:
//! `WorldHeightMap::{ParseObjectData,ParseObjectsDataChunk,ParseWorldDictDataChunk}`,
//! `DataChunkInput::readDict`, and `TerrainLogic::parseWaypointData`.

mod lighting;
#[cfg(test)]
mod tests;
mod tiles;
mod triggers;

pub use lighting::{LightingSet, MapLight, MapLighting};
pub use tiles::{Blend, CliffMapping, TerrainTiles, TextureClass, TextureUv};
pub use triggers::PolygonTrigger;

use std::{borrow::Cow, collections::BTreeMap};

use crate::{
    AssetError, HeightMap,
    binary::{Reader, usize_from_u32},
    decompress,
    heightmap::{parse_heights, read_symbols},
};

/// The five serialized `Dict` value types, without interpreting map scripts.
pub enum MapValue {
    Bool(bool),
    Int(i32),
    Real(f32),
    Ascii(String),
    Unicode(String),
}

/// A placed object or waypoint. Z is an offset above the terrain, not world Z.
pub struct MapObject {
    pub template: String,
    pub position: [f32; 3],
    pub angle: f32,
    pub flags: u32,
    pub properties: BTreeMap<String, MapValue>,
}

impl MapObject {
    #[must_use]
    pub fn waypoint_id(&self) -> Option<i32> {
        match self.properties.get("waypointID") {
            Some(MapValue::Int(id)) => Some(*id),
            _ => None,
        }
    }

    #[must_use]
    pub fn waypoint_name(&self) -> Option<&str> {
        match self.properties.get("waypointName") {
            Some(MapValue::Ascii(name)) => Some(name),
            _ => None,
        }
    }
}

/// Map geometry and placed records. Scripts and teams remain explicit
/// unparsed chunks; reading these records does not implement their behavior.
pub struct MapData {
    pub heightmap: HeightMap,
    pub objects: Vec<MapObject>,
    pub tiles: TerrainTiles,
    pub waypoint_links: Vec<[i32; 2]>,
    pub world_properties: BTreeMap<String, MapValue>,
    pub lighting: Option<MapLighting>,
    pub triggers: Vec<PolygonTrigger>,
    pub unparsed_chunks: Vec<String>,
}

impl MapData {
    /// Read an uncompressed or EAR/`RefPack` wrapped map.
    ///
    /// # Errors
    /// Rejects malformed chunk boundaries, dictionaries, unsupported versions,
    /// nonfinite coordinates and inconsistent terrain array dimensions.
    pub fn parse(bytes: &[u8]) -> Result<Self, AssetError> {
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
        let mut chunks = BTreeMap::new();
        while !reader.is_empty() {
            let chunk = read_chunk(&mut reader, &symbols)?;
            if chunks.insert(chunk.name, chunk).is_some() {
                return Err(AssetError::Invalid("duplicate top-level map chunk"));
            }
        }
        let height = chunks
            .remove("HeightMapData")
            .ok_or(AssetError::Invalid("map has no HeightMapData"))?;
        let heightmap = parse_heights(height.bytes, height.version)?;
        let blend = chunks
            .remove("BlendTileData")
            .ok_or(AssetError::Invalid("map has no BlendTileData"))?;
        let tiles = TerrainTiles::parse(blend.bytes, blend.version, &heightmap)?;
        let objects = chunks
            .remove("ObjectsList")
            .ok_or(AssetError::Invalid("map has no ObjectsList"))?;
        let objects = read_objects(objects.bytes, &symbols)?;
        let waypoint_links = chunks
            .remove("WaypointsList")
            .map_or_else(|| Ok(Vec::new()), |chunk| read_links(chunk.bytes))?;
        let world_properties = chunks.remove("WorldInfo").map_or_else(
            || Ok::<_, AssetError>(BTreeMap::new()),
            |chunk| {
                let mut reader = Reader::new(chunk.bytes);
                let dict = read_dict(&mut reader, &symbols)?;
                finish(&reader)?;
                Ok(dict)
            },
        )?;
        let lighting = chunks
            .remove("GlobalLighting")
            .map(|chunk| MapLighting::parse(chunk.bytes, chunk.version))
            .transpose()?;
        let triggers = chunks.remove("PolygonTriggers").map_or_else(
            || Ok(Vec::new()),
            |chunk| triggers::read(chunk.bytes, chunk.version),
        )?;
        Ok(Self {
            heightmap,
            objects,
            tiles,
            waypoint_links,
            world_properties,
            lighting,
            triggers,
            unparsed_chunks: chunks.keys().map(ToString::to_string).collect(),
        })
    }
}

pub(crate) struct Chunk<'a, 'b> {
    pub name: &'a str,
    pub version: u16,
    pub bytes: &'b [u8],
}

pub(crate) fn read_chunk<'a, 'b>(
    reader: &mut Reader<'b>,
    symbols: &'a BTreeMap<u32, String>,
) -> Result<Chunk<'a, 'b>, AssetError> {
    let name = symbols
        .get(&reader.le_u32()?)
        .ok_or(AssetError::Invalid("map chunk uses unknown symbol"))?;
    let version = reader.le_u16()?;
    let size = usize_from_u32(reader.le_u32()?)?;
    Ok(Chunk {
        name,
        version,
        bytes: reader.take(size)?,
    })
}

fn read_objects(
    bytes: &[u8],
    symbols: &BTreeMap<u32, String>,
) -> Result<Vec<MapObject>, AssetError> {
    let mut reader = Reader::new(bytes);
    let mut objects = Vec::new();
    while !reader.is_empty() {
        let chunk = read_chunk(&mut reader, symbols)?;
        if chunk.name != "Object" || !(1..=3).contains(&chunk.version) {
            return Err(AssetError::Invalid("unsupported placed object chunk"));
        }
        if objects.len() >= 100_000 {
            return Err(AssetError::Invalid("too many placed map objects"));
        }
        let mut object = Reader::new(chunk.bytes);
        let mut position = [real(&mut object)?, real(&mut object)?, real(&mut object)?];
        if chunk.version <= 2 {
            position[2] = 0.0;
        }
        let angle = real(&mut object)?;
        let flags = object.le_u32()?;
        let template = ascii(&mut object)?;
        let properties = if chunk.version >= 2 {
            read_dict(&mut object, symbols)?
        } else {
            BTreeMap::new()
        };
        finish(&object)?;
        // The source loader drops objects outside the representable terrain Z range.
        if (-1000.0..=1593.75).contains(&position[2]) {
            objects.push(MapObject {
                template,
                position,
                angle,
                flags,
                properties,
            });
        }
    }
    Ok(objects)
}

fn read_links(bytes: &[u8]) -> Result<Vec<[i32; 2]>, AssetError> {
    let mut reader = Reader::new(bytes);
    let count = reader.le_u32()?;
    if count > 1_000_000 {
        return Err(AssetError::Invalid("too many waypoint links"));
    }
    let links = (0..count)
        .map(|_| Ok([integer(&mut reader)?, integer(&mut reader)?]))
        .collect::<Result<Vec<_>, AssetError>>()?;
    finish(&reader)?;
    Ok(links)
}

pub(crate) fn read_dict(
    reader: &mut Reader<'_>,
    symbols: &BTreeMap<u32, String>,
) -> Result<BTreeMap<String, MapValue>, AssetError> {
    let count = reader.le_u16()?;
    let mut dict = BTreeMap::new();
    for _ in 0..count {
        let key = reader.le_u32()?;
        let name = symbols
            .get(&(key >> 8))
            .ok_or(AssetError::Invalid("map dictionary uses unknown key"))?;
        let value = match key & 255 {
            0 => MapValue::Bool(reader.byte()? != 0),
            1 => MapValue::Int(integer(reader)?),
            2 => MapValue::Real(real(reader)?),
            3 => MapValue::Ascii(ascii(reader)?),
            4 => {
                let length = reader.le_u16()?;
                let units = (0..length)
                    .map(|_| reader.le_u16())
                    .collect::<Result<Vec<_>, _>>()?;
                MapValue::Unicode(
                    String::from_utf16(&units)
                        .map_err(|_| AssetError::Invalid("invalid map Unicode string"))?,
                )
            }
            _ => return Err(AssetError::Invalid("unknown map dictionary value type")),
        };
        // Dict::set* replaces earlier values for an existing name key.
        dict.insert(name.clone(), value);
    }
    Ok(dict)
}

pub(crate) fn ascii(reader: &mut Reader<'_>) -> Result<String, AssetError> {
    let length = usize::from(reader.le_u16()?);
    Ok(reader
        .take(length)?
        .iter()
        .copied()
        .map(char::from)
        .collect())
}

pub(crate) fn integer(reader: &mut Reader<'_>) -> Result<i32, AssetError> {
    Ok(i32::from_le_bytes(reader.le_u32()?.to_le_bytes()))
}

pub(crate) fn real(reader: &mut Reader<'_>) -> Result<f32, AssetError> {
    let value = f32::from_bits(reader.le_u32()?);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AssetError::Invalid("nonfinite map number"))
    }
}

pub(crate) const fn finish(reader: &Reader<'_>) -> Result<(), AssetError> {
    if reader.is_empty() {
        Ok(())
    } else {
        Err(AssetError::Invalid("unexpected trailing map chunk data"))
    }
}
