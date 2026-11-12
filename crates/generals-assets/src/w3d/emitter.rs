//! WW3D v2 particle-emitter records, SH `part_ldr.cpp` and `w3d_file.h`.
//!
//! This admits standalone quad emitters with box randomizers and linear color,
//! opacity and size properties. Other render/property forms remain explicit errors.
use super::{Chunks, Error, read_float, read_name, read_u32, read_vector};

#[derive(Clone)]
pub struct Definition {
    pub name: String,
    pub texture: String,
    pub emission: Emission,
    pub motion: Motion,
    pub properties: Properties,
    /// Bytes retain native `W3dShaderStruct` field order.
    pub shader: [u8; 16],
}

#[derive(Clone)]
pub struct Emission {
    pub rate: f32,
    pub burst: u32,
    pub lifetime: f32,
    pub maximum: f32,
}

#[derive(Clone)]
pub struct Motion {
    pub position_box: [f32; 3],
    pub velocity_box: [f32; 3],
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
    pub outward: f32,
    pub inherit: f32,
}

#[derive(Clone)]
pub struct Properties {
    pub colors: Vec<ColorKey>,
    pub opacity: Vec<ScalarKey>,
    pub size: Vec<ScalarKey>,
    pub size_random: f32,
}

#[derive(Clone)]
pub struct ColorKey {
    pub time: f32,
    pub color: [u8; 3],
}

#[derive(Clone)]
pub struct ScalarKey {
    pub time: f32,
    pub value: f32,
}

pub(super) fn parse(bytes: &[u8]) -> Result<Definition, Error> {
    let mut chunks = Chunks::new(bytes);
    // Native Load_W3D reads these five records in order before optional chunks.
    let header = required(&mut chunks, 0x501, 20)?;
    if read_u32(header, 0)? != 0x0002_0000 {
        return Err(Error::Unsupported("emitter version"));
    }
    let name = read_name(&header[4..20])?;
    let user = required(&mut chunks, 0x502, 12)?;
    if read_u32(user, 0)? != 0 || read_u32(user, 4)? != 0 {
        return Err(Error::Unsupported("emitter user data"));
    }
    let info = required(&mut chunks, 0x503, 332)?;
    let v2 = required(&mut chunks, 0x504, 124)?;
    let props = chunks.next().ok_or(Error::Truncated)??;
    if props.id != 0x505 {
        return Err(Error::InvalidChunk("missing emitter properties"));
    }
    let properties = properties(props.body)?;
    let shader: [u8; 16] = v2[76..92].try_into().map_err(|_| Error::Truncated)?;
    validate_shader(shader)?;
    if read_u32(v2, 92)? != 1 || read_u32(v2, 96)? != 0 {
        return Err(Error::Unsupported("emitter render or texture-frame mode"));
    }
    optional(chunks)?;
    let texture = read_name(&info[..260])?;
    if name.is_empty() || texture.is_empty() {
        return Err(Error::InvalidChunk("unnamed emitter or texture"));
    }
    // V2 replaces legacy V1 size/color/random/gravity fields with properties
    // and volume randomizers; velocity, acceleration and emission stay in Info.
    Ok(Definition {
        name,
        texture,
        emission: Emission {
            rate: read_float(info, 272)?,
            burst: read_u32(v2, 0)?,
            lifetime: read_float(info, 268)?,
            maximum: read_float(info, 276)?,
        },
        motion: Motion {
            position_box: box_volume(v2, 4)?,
            velocity_box: box_volume(v2, 36)?,
            velocity: read_vector(info, 300)?,
            acceleration: read_vector(info, 312)?,
            outward: read_float(v2, 68)?,
            inherit: read_float(v2, 72)?,
        },
        properties,
        shader,
    })
}

fn required<'a>(chunks: &mut Chunks<'a>, id: u32, size: usize) -> Result<&'a [u8], Error> {
    let chunk = chunks.next().ok_or(Error::Truncated)??;
    if chunk.id != id || chunk.body.len() != size || chunk.container {
        return Err(Error::InvalidChunk("emitter record order or size"));
    }
    Ok(chunk.body)
}

fn box_volume(bytes: &[u8], offset: usize) -> Result<[f32; 3], Error> {
    if read_u32(bytes, offset)? != 0 {
        return Err(Error::Unsupported("emitter randomizer volume"));
    }
    // Vector3SolidBoxRandomizer's constructor clamps each negative extent.
    Ok(read_vector(bytes, offset + 4)?.map(|value| value.max(0.0)))
}

fn properties(bytes: &[u8]) -> Result<Properties, Error> {
    if bytes.len() < 40 {
        return Err(Error::Truncated);
    }
    let counts = [
        read_u32(bytes, 0)?,
        read_u32(bytes, 4)?,
        read_u32(bytes, 8)?,
    ];
    if counts.iter().any(|&count| count == 0 || count > 256) {
        return Err(Error::LimitExceeded("emitter property key count"));
    }
    let total: u32 = counts.iter().sum();
    let expected = 40 + usize::try_from(total).map_err(|_| Error::Truncated)? * 8;
    if bytes.len() != expected {
        return Err(Error::InvalidChunk("emitter property count or size"));
    }
    if bytes[12..15] != [0; 3] || read_float(bytes, 16)? != 0.0 {
        return Err(Error::Unsupported("emitter color or opacity randomizer"));
    }
    let mut offset = 40;
    let mut colors = Vec::new();
    for _ in 0..counts[0] {
        colors.push(ColorKey {
            time: if colors.is_empty() {
                0.0
            } else {
                read_float(bytes, offset)?
            },
            color: bytes[offset + 4..offset + 7]
                .try_into()
                .map_err(|_| Error::Truncated)?,
        });
        offset += 8;
    }
    validate_times(colors.iter().map(|key| key.time))?;
    let opacity = scalar_keys(bytes, &mut offset, counts[1])?;
    let size = scalar_keys(bytes, &mut offset, counts[2])?;
    Ok(Properties {
        colors,
        opacity,
        size,
        size_random: read_float(bytes, 20)?,
    })
}

fn scalar_keys(bytes: &[u8], offset: &mut usize, count: u32) -> Result<Vec<ScalarKey>, Error> {
    let mut keys = Vec::new();
    for _ in 0..count {
        keys.push(ScalarKey {
            time: if keys.is_empty() {
                0.0
            } else {
                read_float(bytes, *offset)?
            },
            value: read_float(bytes, *offset + 4)?,
        });
        *offset += 8;
    }
    validate_times(keys.iter().map(|key| key.time))?;
    Ok(keys)
}

fn validate_times(times: impl Iterator<Item = f32>) -> Result<(), Error> {
    let mut previous = None;
    for time in times {
        if time < 0.0 || previous.is_some_and(|previous| time <= previous) {
            return Err(Error::InvalidChunk("emitter property times"));
        }
        previous = Some(time);
    }
    Ok(())
}

fn validate_shader(shader: [u8; 16]) -> Result<(), Error> {
    // Required source form: depth-tested/writing textured alpha-test quad,
    // opaque ONE/ZERO blending and disabled secondary/detail stages.
    for (index, value) in [
        (0, 3),
        (1, 1),
        (3, 0),
        (5, 0),
        (6, 0),
        (7, 1),
        (8, 1),
        (9, 0),
        (10, 0),
        (12, 1),
        (13, 0),
        (14, 0),
    ] {
        if shader[index] != value {
            return Err(Error::Unsupported("emitter shader"));
        }
    }
    Ok(())
}

fn optional(chunks: Chunks<'_>) -> Result<(), Error> {
    let mut rotation = false;
    let mut frame = false;
    let mut extra = false;
    for chunk in chunks {
        let chunk = chunk?;
        let seen = match chunk.id {
            0x50a => &mut rotation,
            0x50b => &mut frame,
            0x50d => &mut extra,
            _ => return Err(Error::Unsupported("emitter optional chunk")),
        };
        if *seen || chunk.container {
            return Err(Error::InvalidChunk("duplicate or nested emitter property"));
        }
        *seen = true;
        let size = if chunk.id == 0x50d { 40 } else { 24 };
        if chunk.body.len() != size {
            return Err(Error::InvalidChunk("emitter constant property size"));
        }
        let nonzero = if chunk.id == 0x50d {
            read_float(chunk.body, 0)? != 0.0
        } else {
            read_u32(chunk.body, 0)? != 0
                || read_float(chunk.body, 4)? != 0.0
                || (chunk.id == 0x50a && read_float(chunk.body, 8)? != 0.0)
                || read_float(chunk.body, 20)? != 0.0
        };
        // Native ignores reserved words and the first (start-value) key time.
        if nonzero {
            return Err(Error::Unsupported(
                "emitter rotation/frame/future-start property",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
