use std::{borrow::Cow, collections::BTreeMap};

use crate::{
    AssetError,
    binary::{Reader, usize_from_u32},
    decompress,
};

/// Original row-major terrain samples, including the map's outer border.
pub struct HeightMap {
    /// Number of samples along the original world's X axis.
    pub width: u32,
    /// Number of samples along the original world's Y axis.
    pub height: u32,
    /// Outer border thickness in samples; world origin is offset by this amount.
    pub border: u32,
    /// Signed native `ICoord2D` boundary sizes; inactive entries may be negative.
    pub boundaries: Vec<[i32; 2]>,
    /// Unsigned eight-bit source samples widened to u16 without alteration.
    pub heights: Vec<u16>,
}

impl HeightMap {
    /// Horizontal distance between adjacent samples, in original world units.
    pub const CELL_SIZE: f32 = 10.0;

    /// Vertical distance represented by one raw height increment.
    pub const HEIGHT_SCALE: f32 = 0.625;

    /// Read `HeightMapData` from a `CkMp` map, optionally wrapped in EAR/`RefPack`.
    ///
    /// Supports height chunk versions 2–4. Unknown map chunks are skipped using
    /// their declared lengths; terrain textures, placed objects and scripts are
    /// intentionally not interpreted by this importer yet.
    ///
    /// # Errors
    /// Returns an error for malformed compression, chunk tables, sizes, borders,
    /// unsupported height chunk versions, or maps larger than 4096² samples.
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
        let mut heightmap = None;
        while !reader.is_empty() {
            let identifier = reader.le_u32()?;
            let version = reader.le_u16()?;
            let size = usize_from_u32(reader.le_u32()?)?;
            let payload = reader.take(size)?;
            let name = symbols
                .get(&identifier)
                .ok_or(AssetError::Invalid("map chunk uses an unknown symbol"))?;
            if name == "HeightMapData" {
                if heightmap.is_some() {
                    return Err(AssetError::Invalid("duplicate HeightMapData chunk"));
                }
                heightmap = Some(parse_heights(payload, version)?);
            }
        }
        heightmap.ok_or(AssetError::Invalid("map has no HeightMapData chunk"))
    }
}

pub fn read_symbols(reader: &mut Reader<'_>) -> Result<BTreeMap<u32, String>, AssetError> {
    let count = reader.le_u32()?;
    if count == 0 || count > 4096 {
        return Err(AssetError::Invalid("invalid map symbol count"));
    }
    let mut symbols = BTreeMap::new();
    for _ in 0..count {
        let length = usize::from(reader.byte()?);
        let name = std::str::from_utf8(reader.take(length)?)
            .map_err(|_| AssetError::Invalid("invalid map symbol encoding"))?
            .to_owned();
        let id = reader.le_u32()?;
        if symbols.insert(id, name).is_some() {
            return Err(AssetError::Invalid("duplicate map symbol identifier"));
        }
    }
    Ok(symbols)
}

pub fn parse_heights(bytes: &[u8], version: u16) -> Result<HeightMap, AssetError> {
    if !(2..=4).contains(&version) {
        return Err(AssetError::Invalid("unsupported height chunk version"));
    }
    let mut reader = Reader::new(bytes);
    let width = reader.le_u32()?;
    let height = reader.le_u32()?;
    if !(2..=4096).contains(&width) || !(2..=4096).contains(&height) {
        return Err(AssetError::Invalid(
            "terrain dimensions outside supported range",
        ));
    }
    let border = if version >= 3 { reader.le_u32()? } else { 0 };
    if border >= width.div_ceil(2) || border >= height.div_ceil(2) {
        return Err(AssetError::Invalid("terrain border consumes entire map"));
    }
    let boundaries = if version >= 4 {
        let count = reader.le_u32()?;
        if count > 4096 {
            return Err(AssetError::Invalid("too many terrain boundaries"));
        }
        (0..count)
            .map(|_| {
                Ok([
                    i32::from_le_bytes(reader.le_u32()?.to_le_bytes()),
                    i32::from_le_bytes(reader.le_u32()?.to_le_bytes()),
                ])
            })
            .collect::<Result<Vec<_>, AssetError>>()?
    } else {
        vec![[
            i32::from_le_bytes((width - 2 * border).to_le_bytes()),
            i32::from_le_bytes((height - 2 * border).to_le_bytes()),
        ]]
    };
    let count = reader.le_u32()?;
    if count != width * height {
        return Err(AssetError::Invalid(
            "terrain sample count differs from dimensions",
        ));
    }
    let heights = reader
        .take(usize_from_u32(count)?)?
        .iter()
        .copied()
        .map(u16::from)
        .collect();
    if !reader.is_empty() {
        return Err(AssetError::Invalid("unexpected trailing height data"));
    }
    Ok(HeightMap {
        width,
        height,
        border,
        boundaries,
        heights,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_heights;

    fn samples(width: u32, height: u32, count: u32) -> Vec<u8> {
        let mut bytes = width.to_le_bytes().to_vec();
        bytes.extend(height.to_le_bytes());
        bytes.extend(count.to_le_bytes());
        bytes.extend([1, 2, 3, 255]);
        bytes
    }

    #[test]
    fn preserves_row_major_sample_values() {
        let map = parse_heights(&samples(2, 2, 4), 2).expect("valid terrain");
        assert_eq!((map.width, map.height, map.border), (2, 2, 0));
        assert_eq!(map.heights, [1, 2, 3, 255]);
    }

    #[test]
    fn final_crusade_version_four_preserves_negative_inactive_boundary() {
        let mut bytes = Vec::new();
        for value in [340_u32, 390, 70, 3] {
            bytes.extend(value.to_le_bytes());
        }
        for boundary in [[200_i32, 218], [-6, 2], [1, 1]] {
            for value in boundary {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes.extend((340_u32 * 390).to_le_bytes());
        bytes.resize(bytes.len() + 340 * 390, 0);
        let map = parse_heights(&bytes, 4).unwrap();
        assert_eq!(map.boundaries, [[200, 218], [-6, 2], [1, 1]]);
    }

    #[test]
    fn rejects_wrong_sample_counts_and_excessive_dimensions() {
        assert!(parse_heights(&samples(2, 3, 4), 2).is_err());
        assert!(parse_heights(&samples(u32::MAX, 3, 4), 2).is_err());
        assert!(parse_heights(&samples(2, 2, 4)[..15], 2).is_err());
        assert!(parse_heights(&samples(2, 2, 4), 5).is_err());
    }
}
