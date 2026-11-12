//! `W3DTreeBuffer::updateTexture` / `WorldHeightMap` tile decoding and native atlas order.
use generals_assets::{AssetError, virtual_assets::VirtualAssets};
use std::error::Error;

pub struct Texture {
    pub width: usize,
    pub half: bool,
    pixels: Vec<[u8; 4]>,
}

impl Texture {
    pub(super) fn load(assets: &VirtualAssets, name: &str) -> Result<Self, Box<dyn Error>> {
        let bytes = match assets.read(&format!("art/terrain/{name}")) {
            Ok(asset) => asset.bytes,
            Err(AssetError::Missing(_)) => match assets.read(&format!("art/textures/{name}")) {
                Ok(asset) => asset.bytes,
                Err(AssetError::Missing(_)) => {
                    // updateTexture keeps a fresh tree type's mesh/origin but
                    // assigns no tiles. It does not ask WW3D for a DDS substitute.
                    return Ok(Self {
                        width: 0,
                        half: false,
                        pixels: Vec::new(),
                    });
                }
                Err(error) => return Err(error.into()),
            },
            Err(error) => return Err(error.into()),
        };
        Self::decode(&bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let header = bytes.get(..18).ok_or("truncated tree TGA header")?;
        let width = usize::from(u16::from_le_bytes([header[12], header[13]]));
        let height = usize::from(u16::from_le_bytes([header[14], header[15]]));
        if header[1] != 0 || !matches!(header[2], 2 | 10) || !matches!(header[16], 24 | 32) {
            return Err("unsupported native tree TGA format".into());
        }
        let half = width == 32 && height == 32;
        let tiles = (width / 64).min(height / 64).min(10);
        if !half && (tiles == 0 || width / 64 > 10 || height / 64 > 10) {
            return Err("unsupported native tree TGA extent or ID".into());
        }
        let tile_width = if half { 1 } else { tiles };
        let extent = tile_width * 64;
        let mut pixels = vec![[0; 4]; extent * extent];
        let mut cursor = 18;
        let mut remaining = 0;
        let mut run = false;
        let mut color = [0; 4];
        let pixel_bytes = usize::from(header[16] / 8);
        for y in 0..extent {
            for x in 0..width {
                if y >= height {
                    continue;
                }
                if header[2] == 10 && remaining == 0 {
                    let flag = *bytes.get(cursor).ok_or("truncated tree TGA run")?;
                    cursor += 1;
                    remaining = usize::from(flag & 127) + 1;
                    run = flag & 128 != 0;
                    if run {
                        color = read_pixel(bytes, &mut cursor, pixel_bytes)?;
                    }
                }
                if header[2] == 10 {
                    remaining -= 1;
                }
                if !run {
                    color = read_pixel(bytes, &mut cursor, pixel_bytes)?;
                }
                if x < extent {
                    pixels[(extent - 1 - y) * extent + x] = color;
                }
            }
        }
        Ok(Self {
            width: tile_width,
            half,
            pixels,
        })
    }
}

fn read_pixel(bytes: &[u8], cursor: &mut usize, count: usize) -> Result<[u8; 4], Box<dyn Error>> {
    let pixel = bytes
        .get(*cursor..*cursor + count)
        .ok_or("truncated tree TGA pixels")?;
    *cursor += count;
    Ok([
        pixel[2],
        pixel[1],
        pixel[0],
        if count == 4 { pixel[3] } else { 255 },
    ])
}

pub(super) struct Atlas {
    pub extent: usize,
    pub origins: Vec<[usize; 2]>,
    pub rgba: Vec<u8>,
}

impl Atlas {
    pub(super) fn new(types: &[std::sync::Arc<super::data::Data>]) -> Result<Self, Box<dyn Error>> {
        let mut total = 0;
        for (i, data) in types.iter().enumerate() {
            if !types[..i]
                .iter()
                .any(|old| old.texture.eq_ignore_ascii_case(&data.texture))
            {
                total += data.pixels.width * data.pixels.width;
            }
        }
        if total > 512 {
            return Err("native optimized tree tile capacity exceeded".into());
        }
        let mut tiles = 8;
        while tiles * tiles < total {
            tiles *= 2;
        }
        let extent = tiles * 64;
        let mut free = vec![true; tiles * tiles];
        let mut origins = vec![[0; 2]; types.len()];
        let mut rgba = vec![0; extent * extent * 4];
        for size in (1..=tiles).rev() {
            for (index, data) in types
                .iter()
                .enumerate()
                .filter(|(_, data)| data.pixels.width == size)
            {
                if let Some(previous) = types[..index]
                    .iter()
                    .position(|old| old.texture.eq_ignore_ascii_case(&data.texture))
                {
                    origins[index] = origins[previous];
                    continue;
                }
                let mut found = None;
                'rows: for y in 0..=tiles - size {
                    for x in 0..=tiles - size {
                        if free[y * tiles + x] {
                            if (0..size)
                                .all(|dy| (0..size).all(|dx| free[(y + dy) * tiles + x + dx]))
                            {
                                found = Some([x, y]);
                                break 'rows;
                            }
                            // Native inner search stops at the first free anchor even
                            // when its complete square is occupied.
                            break;
                        }
                    }
                }
                let Some([x, y]) = found else {
                    return Err("native optimized tree atlas has no available block".into());
                };
                origins[index] = [x * 64, y * 64];
                for dy in 0..size {
                    for dx in 0..size {
                        free[(y + dy) * tiles + x + dx] = false;
                    }
                }
                let source_extent = size * 64;
                for row in 0..source_extent {
                    for column in 0..source_extent {
                        let dest = ((y * 64 + row) * extent + x * 64 + column) * 4;
                        rgba[dest..dest + 4]
                            .copy_from_slice(&data.pixels.pixels[row * source_extent + column]);
                    }
                }
            }
        }
        Ok(Self {
            extent,
            origins,
            rgba,
        })
    }
}
