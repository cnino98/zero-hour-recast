//! `WorldHeightMap::ParseBlendTileData` from the revision cited in `map`.

use crate::{AssetError, HeightMap, binary::Reader};

use super::{ascii, finish, integer, real};

mod uv;
pub use uv::TextureUv;

/// A named Terrain.ini class and its 64-pixel source tiles.
pub struct TextureClass {
    pub name: String,
    pub first_tile: u16,
    pub tile_count: u16,
    pub width: u16,
}

/// One serialized terrain blend. Directions are horizontal, vertical, right
/// diagonal, left diagonal in source order; they are not mutually exclusive.
pub struct Blend {
    pub tile: u16,
    pub directions: [bool; 4],
    pub inverted: u8,
    pub long_diagonal: bool,
    pub custom_edge: i32,
}

impl Blend {
    /// Source `getAlphaUVData` corner order: (0,0), (1,0), (1,1), (0,1).
    #[must_use]
    pub fn alpha_and_flip(&self) -> ([u8; 4], bool) {
        let mut alpha = [0; 4];
        let mut flip = false;
        let inverted = self.inverted & 1 != 0;
        if self.directions[0] {
            flip = self.inverted & 2 != 0;
            for index in if inverted { [0, 3] } else { [1, 2] } {
                alpha[index] = 255;
            }
        }
        if self.directions[1] {
            flip = self.inverted & 2 != 0;
            for index in if inverted { [0, 1] } else { [2, 3] } {
                alpha[index] = 255;
            }
        }
        if self.directions[2] {
            if inverted {
                alpha[1] = 255;
                if self.long_diagonal {
                    alpha[0] = 255;
                    alpha[2] = 255;
                }
            } else {
                flip = true;
                alpha[2] = 255;
                if self.long_diagonal {
                    alpha[1] = 255;
                    alpha[3] = 255;
                }
            }
        }
        if self.directions[3] {
            if inverted {
                flip = true;
                alpha[0] = 255;
                if self.long_diagonal {
                    alpha[1] = 255;
                    alpha[3] = 255;
                }
            } else {
                alpha[3] = 255;
                if self.long_diagonal {
                    alpha[0] = 255;
                    alpha[2] = 255;
                }
            }
        }
        if self.custom_edge >= 0 {
            alpha = [0; 4];
            flip = false;
        }
        (alpha, flip)
    }
}

/// Stored cliff texture mapping, before legacy atlas placement.
pub struct CliffMapping {
    pub tile: u16,
    pub uv: [[f32; 2]; 4],
    pub flip: bool,
    pub mutant: bool,
}

/// Arrays retain the map's complete sample extent, including its border.
/// Entry zero in the blend/cliff tables means no blend/custom cliff mapping.
pub struct TerrainTiles {
    pub width: u32,
    pub height: u32,
    pub base_tiles: Vec<u16>,
    pub blend_tiles: Vec<u16>,
    pub extra_blend_tiles: Vec<u16>,
    pub cliff_tiles: Vec<u16>,
    pub cliff_flags: Vec<u8>,
    pub texture_classes: Vec<TextureClass>,
    pub edge_classes: Vec<TextureClass>,
    pub blends: Vec<Option<Blend>>,
    pub cliffs: Vec<Option<CliffMapping>>,
}

impl TerrainTiles {
    pub(super) fn parse(bytes: &[u8], version: u16, map: &HeightMap) -> Result<Self, AssetError> {
        if !(2..=8).contains(&version) {
            return Err(AssetError::Invalid("unsupported blend tile version"));
        }
        let mut reader = Reader::new(bytes);
        if reader.le_u32()? != map.width * map.height {
            return Err(AssetError::Invalid(
                "terrain tile count differs from heightmap",
            ));
        }
        let count = map.heights.len();
        let base_tiles = read_indices(&mut reader, count)?;
        let blend_tiles = read_indices(&mut reader, count)?;
        let extra_blend_tiles = if version >= 6 {
            read_indices(&mut reader, count)?
        } else {
            vec![0; count]
        };
        let cliff_tiles = if version >= 5 {
            read_indices(&mut reader, count)?
        } else {
            vec![0; count]
        };
        let cliff_flags = read_cliffs(&mut reader, version, map)?;
        let tile_count = bounded_count(&mut reader, 2047, false)?;
        let blend_count = bounded_count(&mut reader, 16_192, false)?;
        let cliff_count = if version >= 5 {
            bounded_count(&mut reader, 32_767, true)?
        } else {
            1
        };
        let class_count = bounded_count(&mut reader, 199, false)?;
        let texture_classes = read_classes(&mut reader, class_count, tile_count, true)?;
        let edge_classes = if version >= 4 {
            let edge_tiles = bounded_count(&mut reader, 2047, true)?;
            let edge_count = bounded_count(&mut reader, 199, true)?;
            read_classes(&mut reader, edge_count, edge_tiles, false)?
        } else {
            Vec::new()
        };
        let mut blends = vec![None];
        for _ in 1..blend_count {
            let tile = tile_index(&mut reader)?;
            let directions = [
                reader.byte()? != 0,
                reader.byte()? != 0,
                reader.byte()? != 0,
                reader.byte()? != 0,
            ];
            let inverted = reader.byte()?;
            let long_diagonal = version >= 3 && reader.byte()? != 0;
            let custom_edge = if version >= 4 {
                integer(&mut reader)?
            } else {
                -1
            };
            if reader.le_u32()? != 0x7ada_0000 {
                return Err(AssetError::Invalid("invalid blend record marker"));
            }
            blends.push(Some(Blend {
                tile,
                directions,
                inverted,
                long_diagonal,
                custom_edge,
            }));
        }
        let mut cliffs = vec![None];
        for _ in 1..cliff_count {
            let tile = tile_index(&mut reader)?;
            let mut uv = [[0.0; 2]; 4];
            for point in &mut uv {
                *point = [real(&mut reader)?, real(&mut reader)?];
            }
            cliffs.push(Some(CliffMapping {
                tile,
                uv,
                flip: reader.byte()? != 0,
                mutant: reader.byte()? != 0,
            }));
        }
        finish(&reader)?;
        let result = Self {
            width: map.width,
            height: map.height,
            base_tiles,
            blend_tiles,
            extra_blend_tiles,
            cliff_tiles,
            cliff_flags,
            texture_classes,
            edge_classes,
            blends,
            cliffs,
        };
        result.validate()?;
        Ok(result)
    }

    fn validate(&self) -> Result<(), AssetError> {
        if self
            .base_tiles
            .iter()
            .any(|&tile| self.class_for_tile(tile).is_none())
            || self
                .blends
                .iter()
                .flatten()
                .any(|blend| self.class_for_tile(blend.tile).is_none())
        {
            return Err(AssetError::Invalid("terrain tile has no texture class"));
        }
        if self
            .blend_tiles
            .iter()
            .chain(&self.extra_blend_tiles)
            .any(|&index| usize::from(index) >= self.blends.len())
            || self
                .cliff_tiles
                .iter()
                .any(|&index| usize::from(index) >= self.cliffs.len())
        {
            // The original release repairs invalid indices; reject corrupt data
            // here instead of hiding damage to an installed map.
            return Err(AssetError::Invalid(
                "terrain blend or cliff index out of range",
            ));
        }
        Ok(())
    }

    fn class_for_tile(&self, tile: u16) -> Option<usize> {
        let tile = tile >> 2;
        self.texture_classes.iter().position(|class| {
            tile >= class.first_tile && tile - class.first_tile < class.tile_count
        })
    }

    /// `getCliffState` uses packed bits, with padding at the end of each row.
    #[must_use]
    pub fn is_cliff(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return true;
        }
        let offset = y * self.width.div_ceil(8) + x / 8;
        usize::try_from(offset)
            .ok()
            .and_then(|offset| self.cliff_flags.get(offset))
            .is_none_or(|flags| flags & (1 << (x & 7)) != 0)
    }
}

fn read_indices(reader: &mut Reader<'_>, count: usize) -> Result<Vec<u16>, AssetError> {
    (0..count).map(|_| reader.le_u16()).collect()
}

fn bounded_count(
    reader: &mut Reader<'_>,
    maximum: u32,
    allow_zero: bool,
) -> Result<u32, AssetError> {
    let value = reader.le_u32()?;
    if value > maximum || (value == 0 && !allow_zero) {
        return Err(AssetError::Invalid("invalid terrain table count"));
    }
    Ok(value)
}

fn tile_index(reader: &mut Reader<'_>) -> Result<u16, AssetError> {
    u16::try_from(reader.le_u32()?).map_err(|_| AssetError::Invalid("invalid terrain tile index"))
}

fn read_classes(
    reader: &mut Reader<'_>,
    count: u32,
    tiles: u32,
    legacy: bool,
) -> Result<Vec<TextureClass>, AssetError> {
    (0..count)
        .map(|_| {
            let first_tile = tile_index(reader)?;
            let tile_count = tile_index(reader)?;
            let width = tile_index(reader)?;
            if width == 0
                || width > 10
                || u32::from(tile_count) != u32::from(width).pow(2)
                || u32::from(first_tile) + u32::from(tile_count) > tiles
            {
                return Err(AssetError::Invalid("invalid terrain texture class bounds"));
            }
            if legacy {
                reader.le_u32()?;
            }
            Ok(TextureClass {
                name: ascii(reader)?,
                first_tile,
                tile_count,
                width,
            })
        })
        .collect()
}

fn read_cliffs(
    reader: &mut Reader<'_>,
    version: u16,
    map: &HeightMap,
) -> Result<Vec<u8>, AssetError> {
    let stride = usize::try_from(map.width.div_ceil(8)).expect("bounded terrain width");
    let height = usize::try_from(map.height).expect("bounded terrain height");
    let mut flags = vec![0; stride * height];
    if version >= 7 {
        // Version 7 wrote the wrong row width; source explicitly pads it out.
        let stored_stride = if version == 7 {
            usize::try_from((map.width + 1) / 8).expect("bounded terrain width")
        } else {
            stride
        };
        for row in flags.chunks_exact_mut(stride) {
            row[..stored_stride].copy_from_slice(reader.take(stored_stride)?);
        }
    } else {
        let width = usize::try_from(map.width).expect("bounded terrain width");
        for y in 0..height - 1 {
            for x in 0..width - 1 {
                let offset = y * width + x;
                let heights = [
                    map.heights[offset],
                    map.heights[offset + 1],
                    map.heights[offset + width],
                    map.heights[offset + width + 1],
                ];
                let lo = heights.iter().min().copied().expect("four corners");
                let hi = heights.iter().max().copied().expect("four corners");
                // setCellCliffFlagFromHeights: PATHFIND_CLIFF_SLOPE_LIMIT_F.
                if f32::from(hi - lo) * HeightMap::HEIGHT_SCALE > 9.8 {
                    flags[y * stride + x / 8] |= 1 << (x & 7);
                }
            }
        }
    }
    Ok(flags)
}
