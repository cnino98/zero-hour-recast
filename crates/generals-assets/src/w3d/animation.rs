//! Raw and timecoded W3D animation: EA `HRawAnimClass`, `MotionChannelClass`, `BitChannelClass`
//! and `Fast_Slerp`, GPL-3.0-or-later, `SuperHackers` revision 2bfaea60.
use super::{
    Chunks, Error, MAX_PIVOTS, hierarchy_header, parse_hierarchy, read_float, read_name, read_u32,
    set_once, top_level_chunk,
};
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

pub struct Clip {
    pub name: String,
    pub hierarchy: String,
    pub frame_count: u16,
    pub frame_rate: u16,
    channels: BTreeMap<(usize, u16), Channel>,
    visibility: BTreeMap<usize, Visibility>,
    compressed: Option<super::compressed::Compressed>,
    pivot_count: Option<usize>,
}

struct Channel {
    first: u16,
    last: u16,
    width: usize,
    values: Vec<f32>,
}

struct Visibility {
    first: u16,
    last: u16,
    default: bool,
    bits: Vec<u8>,
}

impl Clip {
    /// Reads one raw/timecoded animation, including chunks embedded in a model.
    /// # Errors
    /// Rejects adaptive-delta/old formats, invalid frame ranges, nonfinite channels,
    /// invalid quaternion dimensions and truncated channel payloads.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::load(bytes)?.ok_or(Error::Unsupported("animation chunk missing"))
    }

    /// Returns no handle for framed data without hierarchy animation chunks,
    /// as `WW3DAssetManager::Get_HAnim` does after loading a static model.
    /// # Errors
    /// Retains malformed and unsupported animation errors; morphs are not absence.
    pub fn load(bytes: &[u8]) -> Result<Option<Self>, Error> {
        if bytes.is_empty() {
            return Err(Error::InvalidChunk("empty animation input"));
        }
        if top_level_chunk(bytes, 0x2c0)?.is_some() {
            return Err(Error::Unsupported("morph animation"));
        }
        let Some(bytes) = top_level_chunk(bytes, 0x200)? else {
            let Some(body) = top_level_chunk(bytes, 0x280)? else {
                return Ok(None);
            };
            let compressed = super::compressed::Compressed::parse(body)?;
            return Ok(Some(Self {
                name: compressed.name.clone(),
                hierarchy: compressed.hierarchy.clone(),
                frame_count: compressed.frame_count,
                frame_rate: compressed.frame_rate,
                channels: BTreeMap::new(),
                visibility: BTreeMap::new(),
                compressed: Some(compressed),
                pivot_count: None,
            }));
        };
        let mut header = None;
        for chunk in Chunks::new(bytes) {
            let chunk = chunk?;
            if chunk.id == 0x201 {
                set_once(&mut header, chunk.body)?;
            }
        }
        let header = header.ok_or(Error::InvalidChunk("animation header missing"))?;
        if header.len() != 44 || read_u32(header, 0)? < 0x0003_0000 {
            return Err(Error::Unsupported("animation header version/size"));
        }
        let frame_count = u16::try_from(read_u32(header, 36)?)
            .map_err(|_| Error::LimitExceeded("animation frames"))?;
        let frame_rate = u16::try_from(read_u32(header, 40)?)
            .map_err(|_| Error::LimitExceeded("animation frame rate"))?;
        if frame_count == 0 || frame_rate == 0 {
            return Err(Error::InvalidChunk("empty animation duration"));
        }
        let hierarchy = read_name(&header[20..36])?;
        let name = format!("{hierarchy}.{}", read_name(&header[4..20])?);
        let mut clip = Self {
            name,
            hierarchy,
            frame_count,
            frame_rate,
            channels: BTreeMap::new(),
            visibility: BTreeMap::new(),
            compressed: None,
            pivot_count: None,
        };
        for chunk in Chunks::new(bytes) {
            let chunk = chunk?;
            match chunk.id {
                0x202 => {
                    let (pivot, kind, channel) = channel(chunk.body, frame_count)?;
                    clip.channels.insert((pivot, kind), channel);
                }
                0x203 => {
                    let (pivot, visibility) = visibility(chunk.body, frame_count)?;
                    clip.visibility.insert(pivot, visibility);
                }
                _ => {}
            }
        }
        Ok(Some(clip))
    }

    /// Resolves the animation's declared hierarchy, as the original asset manager
    /// does when loading an animation. It may differ from a drawable's hierarchy.
    /// # Errors
    /// Rejects missing/misnamed hierarchies and malformed pivot data.
    pub fn bind_hierarchy(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let body = top_level_chunk(bytes, 0x100)?
            .ok_or(Error::InvalidChunk("animation hierarchy missing"))?;
        let header = hierarchy_header(body)?;
        if !read_name(&header[4..20])?.eq_ignore_ascii_case(&self.hierarchy) {
            return Err(Error::InvalidChunk(
                "animation hierarchy asset name mismatch",
            ));
        }
        self.pivot_count = Some(parse_hierarchy(body)?.len());
        Ok(())
    }

    pub(super) fn pivot_limit(&self, model_name: &str, model_count: usize) -> Result<usize, Error> {
        self.pivot_count.map_or_else(
            || {
                if self.hierarchy.eq_ignore_ascii_case(model_name) {
                    // Matching model metadata already resolves this same hierarchy.
                    Ok(model_count)
                } else {
                    Err(Error::InvalidChunk(
                        "animation hierarchy metadata unresolved",
                    ))
                }
            },
            Ok,
        )
    }

    pub(super) fn sample(&self, pivot: usize, frame: f32) -> ([f32; 3], [f32; 4], bool) {
        if let Some(compressed) = &self.compressed {
            return compressed.sample(pivot, frame);
        }
        // Original Win32 Float_To_Long uses x87 nearest-even conversion.
        let first = floor_index(
            (frame - 0.499_999).round_ties_even().max(0.0),
            self.frame_count - 1,
        );
        let next = if first + 1 == self.frame_count {
            0
        } else {
            first + 1
        };
        let ratio = frame - f32::from(first);
        let translation = std::array::from_fn(|axis| {
            let kind = u16::try_from(axis).expect("three translation axes");
            let a = self.value(pivot, kind, first)[0];
            let b = self.value(pivot, kind, next)[0];
            let scaled = (b - a) * ratio;
            a + scaled
        });
        let a = self.value(pivot, 6, first);
        let b = self.value(pivot, 6, next);
        let rotation = if ratio.to_bits().trailing_zeros() >= 31 {
            a
        } else {
            slerp(a, b, ratio)
        };
        let visible = self.visibility.get(&pivot).is_none_or(|channel| {
            let frame = floor_index(frame, self.frame_count - 1);
            if frame < channel.first || frame > channel.last {
                channel.default
            } else {
                let bit = usize::from(frame - channel.first);
                channel.bits[bit / 8] & (1 << (bit % 8)) != 0
            }
        });
        (translation, rotation, visible)
    }

    fn value(&self, pivot: usize, kind: u16, frame: u16) -> [f32; 4] {
        let identity = if kind == 6 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [0.0; 4]
        };
        let Some(channel) = self.channels.get(&(pivot, kind)) else {
            return identity;
        };
        if frame < channel.first || frame > channel.last {
            return identity;
        }
        let start = usize::from(frame - channel.first) * channel.width;
        let mut value = identity;
        value[..channel.width].copy_from_slice(&channel.values[start..start + channel.width]);
        value
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn range(bytes: &[u8], frames: u16) -> Result<(u16, u16), Error> {
    let first = u16_at(bytes, 0)?;
    let last = u16_at(bytes, 2)?;
    if first > last || last >= frames {
        return Err(Error::InvalidChunk("animation channel frame range"));
    }
    Ok((first, last))
}

fn channel(bytes: &[u8], frames: u16) -> Result<(usize, u16, Channel), Error> {
    let (first, last) = range(bytes, frames)?;
    let width = usize::from(u16_at(bytes, 4)?);
    let kind = u16_at(bytes, 6)?;
    let pivot = usize::from(u16_at(bytes, 8)?);
    if pivot >= MAX_PIVOTS {
        return Err(Error::LimitExceeded("animation pivot"));
    }
    if !matches!((kind, width), (0..=2, 1) | (6, 4)) {
        return Err(Error::Unsupported("animation channel kind/dimension"));
    }
    let count = usize::from(last - first + 1) * width;
    if bytes.len() < 12 + count * 4 {
        return Err(Error::Truncated);
    }
    // The original exporter sometimes appends excess floats; the reader skips them.
    let values = (0..count)
        .map(|index| read_float(bytes, 12 + index * 4))
        .collect::<Result<Vec<_>, _>>()?;
    if kind == 6
        && values
            .chunks_exact(4)
            .any(|q| q.iter().map(|v| v * v).sum::<f32>() < 0.000_001)
    {
        return Err(Error::InvalidChunk("zero animation quaternion"));
    }
    Ok((
        pivot,
        kind,
        Channel {
            first,
            last,
            width,
            values,
        },
    ))
}

fn visibility(bytes: &[u8], frames: u16) -> Result<(usize, Visibility), Error> {
    let (first, last) = range(bytes, frames)?;
    if u16_at(bytes, 4)? != 0 {
        return Err(Error::Unsupported("animation visibility kind"));
    }
    let pivot = usize::from(u16_at(bytes, 6)?);
    if pivot >= MAX_PIVOTS {
        return Err(Error::LimitExceeded("animation pivot"));
    }
    let default = *bytes.get(8).ok_or(Error::Truncated)?;
    if default > 1 {
        return Err(Error::InvalidChunk("animation visibility default"));
    }
    let count = (usize::from(last - first) + 1).div_ceil(8);
    let bits = bytes.get(9..9 + count).ok_or(Error::Truncated)?.to_vec();
    Ok((
        pivot,
        Visibility {
            first,
            last,
            default: default != 0,
            bits,
        },
    ))
}

fn floor_index(value: f32, maximum: u16) -> u16 {
    let mut low = 0;
    let mut high = maximum;
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if f32::from(middle) <= value {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

fn fast_sin(value: f32) -> f32 {
    let value = (value * (1024.0 / (2.0 * std::f32::consts::PI))).rem_euclid(1024.0);
    let index = floor_index(value, 1023);
    let fraction = value - f32::from(index);
    let table = |index: u16| {
        let angle = f32::from(index) * 2.0 * std::f32::consts::PI / 1024.0;
        angle.sin()
    };
    let left = (1.0 - fraction) * table(index);
    let right = fraction * table((index + 1) % 1024);
    left + right
}

fn fast_acos(value: f32) -> f32 {
    if value.abs() > 0.975 {
        return value.acos();
    }
    let scaled = value * 512.0;
    let index = floor_index(scaled, 511);
    let fraction = scaled - f32::from(index);
    let left = (1.0 - fraction) * (f32::from(index) / 512.0).acos();
    let right = fraction * (f32::from(index + 1) / 512.0).acos();
    left + right
}

/// `quat.cpp::Cached_Slerp`, including its retained theta denominator.
/// `ParticleEmitter` uses this cached path; animation channels use ordinary slerp.
///
/// # Panics
/// Panics if a trigonometric result cannot be converted to native binary32.
#[must_use]
pub fn cached_slerp(a: [f32; 4], b: [f32; 4], mut fraction: f32) -> [f32; 4] {
    let products = std::array::from_fn::<_, 4, _>(|index| a[index] * b[index]);
    let first = products[0] + products[1];
    let second = first + products[2];
    let dot = second + products[3];
    let cosine = dot.abs();
    let weight = if f64::from(1.0_f32 - cosine) < 0.001 {
        1.0 - fraction
    } else {
        let theta = f64::from(cosine).acos().to_f32().expect("native acos");
        let inverse = 1.0 / theta;
        let angle = fraction * theta;
        let weight = f64::from(theta - angle)
            .sin()
            .to_f32()
            .expect("native sine")
            * inverse;
        fraction = f64::from(angle).sin().to_f32().expect("native sine") * inverse;
        weight
    };
    if dot < 0.0 {
        fraction = -fraction;
    }
    std::array::from_fn(|index| {
        let left = weight * a[index];
        let right = fraction * b[index];
        left + right
    })
}

pub(super) fn slerp(a: [f32; 4], b: [f32; 4], mut fraction: f32) -> [f32; 4] {
    let dot = a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
    let cosine = dot.abs();
    let weight = if 1.0 - cosine < 0.000_001 {
        1.0 - fraction
    } else {
        let angle = fast_acos(cosine);
        let inverse = 1.0 / fast_sin(angle);
        let scaled_angle = fraction * angle;
        let weight = fast_sin(angle - scaled_angle) * inverse;
        fraction = fast_sin(scaled_angle) * inverse;
        weight
    };
    if dot < 0.0 {
        fraction = -fraction;
    }
    std::array::from_fn(|index| {
        let left = weight * a[index];
        let right = fraction * b[index];
        left + right
    })
}

#[cfg(test)]
mod tests {
    use super::{Clip, Error};

    #[test]
    fn cached_emitter_slerp_uses_native_double_epsilon_and_retains_linear_weighting() {
        let scalar = 0.9995_f32;
        let square = scalar * scalar;
        let rotation = [0.0, 0.0, (1.0 - square).sqrt(), scalar];
        let sampled = super::cached_slerp([0.0, 0.0, 0.0, 1.0], rotation, 0.25);
        let z = 0.25 * rotation[2];
        let w_product = 0.25 * rotation[3];
        let w = 0.75 + w_product;
        assert_eq!(
            sampled.map(f32::to_bits),
            [0.0_f32, 0.0, z, w].map(f32::to_bits)
        );
    }

    fn chunk(id: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = id.to_le_bytes().to_vec();
        let size = u32::try_from(body.len()).unwrap() | if id == 0x200 { 0x8000_0000 } else { 0 };
        bytes.extend(size.to_le_bytes());
        bytes.extend(body);
        bytes
    }

    fn animation(channel: &[u8], visibility: &[u8]) -> Vec<u8> {
        let mut header = [0; 44];
        header[..4].copy_from_slice(&0x0004_0001_u32.to_le_bytes());
        header[4..8].copy_from_slice(b"MOVE");
        header[20..23].copy_from_slice(b"RIG");
        header[36..40].copy_from_slice(&3_u32.to_le_bytes());
        header[40..].copy_from_slice(&30_u32.to_le_bytes());
        let mut body = chunk(0x201, &header);
        body.extend(chunk(0x202, channel));
        if !visibility.is_empty() {
            body.extend(chunk(0x203, visibility));
        }
        chunk(0x200, &body)
    }

    fn x_channel() -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [0_u16, 2, 1, 0, 1, 0] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [0_f32, 10.0, 20.0] {
            bytes.extend(value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn fractional_translation_wraps_to_first_frame_and_uses_visibility_default() {
        let visibility = [1, 0, 1, 0, 0, 0, 1, 0, 1, 0];
        let clip = Clip::parse(&animation(&x_channel(), &visibility)).unwrap();
        let (translation, _, visible) = clip.sample(1, 1.5);
        assert!((translation[0] - 15.0).abs() < 0.000_01);
        assert!(!visible);
        let (translation, _, visible) = clip.sample(1, 2.5);
        assert!((translation[0] - 10.0).abs() < 0.000_01);
        assert!(visible);
    }

    #[test]
    fn truncated_channel_and_out_of_duration_range_are_rejected() {
        let mut channel = x_channel();
        channel.pop();
        assert!(matches!(
            Clip::parse(&animation(&channel, &[])),
            Err(Error::Truncated)
        ));
        let mut channel = x_channel();
        channel[2..4].copy_from_slice(&3_u16.to_le_bytes());
        assert!(matches!(
            Clip::parse(&animation(&channel, &[])),
            Err(Error::InvalidChunk("animation channel frame range"))
        ));
    }

    #[test]
    fn quaternion_shortest_arc_handles_opposite_encodings() {
        let q = [0.0, 0.0, 0.707_106_77, 0.707_106_77];
        let interpolated = super::slerp(q, q.map(|value| -value), 0.5);
        for (actual, expected) in interpolated.into_iter().zip(q) {
            assert!((actual - expected).abs() < 0.000_001);
        }
    }
    #[test]
    fn optional_animation_load_distinguishes_static_data_from_invalid_animation() {
        let static_data = chunk(0x100, &[0; 4]);
        assert!(Clip::load(&static_data).unwrap().is_none());
        assert!(Clip::parse(&static_data).is_err());
        let raw = animation(&x_channel(), &[]);
        assert!(Clip::load(&raw).unwrap().is_some());
        assert!(Clip::load(&raw[..raw.len() - 1]).is_err());
        assert!(Clip::load(&chunk(0x200, &[])).is_err());
        assert!(Clip::load(&[]).is_err());
        let morph = [0xc0, 2, 0, 0, 0, 0, 0, 0x80];
        assert!(matches!(
            Clip::load(&morph),
            Err(Error::Unsupported("morph animation"))
        ));
        let unsupported = [0x80, 2, 0, 0, 0, 0, 0, 0x80];
        assert!(Clip::load(&unsupported).is_err());
    }
}
