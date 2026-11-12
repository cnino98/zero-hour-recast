//! Timecoded W3D motion from `HCompressedAnimClass` and
//! `TimeCodedMotionChannelClass`/`TimeCodedBitChannelClass`, EA GPL-3.0-or-later,
//! pinned `SuperHackers` 2bfaea60. Adaptive delta remains explicitly unsupported.
use super::{
    Chunks, Error, MAX_PIVOTS, animation::slerp, read_float, read_name, read_u32, set_once,
};
use std::collections::BTreeMap;

pub(super) struct Compressed {
    pub name: String,
    pub hierarchy: String,
    pub frame_count: u16,
    pub frame_rate: u16,
    channels: BTreeMap<(usize, u8), Vec<Key>>,
    visibility: BTreeMap<usize, Vec<(u16, bool)>>,
}

struct Key {
    frame: u16,
    binary: bool,
    value: [f32; 4],
}

impl Compressed {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut header = None;
        for chunk in Chunks::new(bytes) {
            let chunk = chunk?;
            if chunk.id == 0x281 {
                set_once(&mut header, chunk.body)?;
            }
        }
        let header = header.ok_or(Error::InvalidChunk("compressed animation header missing"))?;
        if header.len() != 44 || read_u32(header, 0)? != 1 {
            return Err(Error::Unsupported(
                "compressed animation header version/size",
            ));
        }
        if u16_at(header, 42)? != 0 {
            return Err(Error::Unsupported("adaptive-delta animation"));
        }
        let frame_count = u16::try_from(read_u32(header, 36)?)
            .map_err(|_| Error::LimitExceeded("compressed animation frames"))?;
        let frame_rate = u16_at(header, 40)?;
        if frame_count == 0 || frame_rate == 0 {
            return Err(Error::InvalidChunk("empty compressed animation duration"));
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
        };
        for chunk in Chunks::new(bytes) {
            let chunk = chunk?;
            match chunk.id {
                0x282 => {
                    let (pivot, kind, keys) = motion(chunk.body, frame_count)?;
                    clip.channels.insert((pivot, kind), keys);
                }
                0x283 => {
                    let (pivot, bits) = visibility(chunk.body, frame_count)?;
                    clip.visibility.insert(pivot, bits);
                }
                _ => {}
            }
        }
        Ok(clip)
    }

    pub fn sample(&self, pivot: usize, frame: f32) -> ([f32; 3], [f32; 4], bool) {
        let translation = std::array::from_fn(|axis| {
            let kind = u8::try_from(axis).expect("three translation axes");
            self.channels.get(&(pivot, kind)).map_or(0.0, |keys| {
                let (first, next) = interval(keys, frame);
                next.map_or(first.value[0], |next| {
                    let ratio =
                        (frame - f32::from(first.frame)) / f32::from(next.frame - first.frame);
                    // WWMath::Lerp uses a + (b-a)*t, with separate float operations.
                    let offset = (next.value[0] - first.value[0]) * ratio;
                    first.value[0] + offset
                })
            })
        });
        let rotation = self
            .channels
            .get(&(pivot, 6))
            .map_or([0.0, 0.0, 0.0, 1.0], |keys| {
                let (first, next) = interval(keys, frame);
                next.map_or(first.value, |next| {
                    let ratio =
                        (frame - f32::from(first.frame)) / f32::from(next.frame - first.frame);
                    slerp(first.value, next.value, ratio)
                })
            });
        let visible = self.visibility.get(&pivot).is_none_or(|keys| {
            // TimeCodedBitChannelClass uses the first key even before its time;
            // it loads DefaultVal but does not use it when sampling.
            let index = keys.partition_point(|(time, _)| f32::from(*time) <= frame);
            keys[index.saturating_sub(1)].1
        });
        (translation, rotation, visible)
    }
}

fn interval(keys: &[Key], frame: f32) -> (&Key, Option<&Key>) {
    let index = keys
        .partition_point(|key| f32::from(key.frame) <= frame)
        .saturating_sub(1);
    (&keys[index], keys.get(index + 1).filter(|key| !key.binary))
}

fn motion(bytes: &[u8], frames: u16) -> Result<(usize, u8, Vec<Key>), Error> {
    let (count, pivot) = channel_header(bytes, frames)?;
    let width = usize::from(*bytes.get(6).ok_or(Error::Truncated)?);
    let kind = *bytes.get(7).ok_or(Error::Truncated)?;
    if !matches!((kind, width), (0..=2, 1) | (6, 4)) {
        return Err(Error::Unsupported(
            "compressed animation channel kind/dimension",
        ));
    }
    let packet = (width + 1) * 4;
    if bytes.len() < 8 + count * packet {
        return Err(Error::Truncated);
    }
    let mut keys = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 8 + index * packet;
        let encoded = read_u32(bytes, offset)?;
        let frame = key_time(encoded, frames)?;
        if index == 0 && frame != 0 || keys.last().is_some_and(|key: &Key| key.frame >= frame) {
            return Err(Error::InvalidChunk("compressed motion key order"));
        }
        let mut value = [0.0; 4];
        for (component, target) in value.iter_mut().take(width).enumerate() {
            *target = read_float(bytes, offset + 4 + component * 4)?;
        }
        if kind == 6 && value.iter().map(|value| value * value).sum::<f32>() < 0.000_001 {
            return Err(Error::InvalidChunk("zero compressed animation quaternion"));
        }
        keys.push(Key {
            frame,
            binary: encoded & 0x8000_0000 != 0,
            value,
        });
    }
    Ok((pivot, kind, keys))
}

fn visibility(bytes: &[u8], frames: u16) -> Result<(usize, Vec<(u16, bool)>), Error> {
    let (count, pivot) = channel_header(bytes, frames)?;
    if *bytes.get(6).ok_or(Error::Truncated)? != 0 {
        return Err(Error::Unsupported("compressed animation visibility kind"));
    }
    if *bytes.get(7).ok_or(Error::Truncated)? > 1 {
        return Err(Error::InvalidChunk(
            "compressed animation visibility default",
        ));
    }
    if bytes.len() < 8 + count * 4 {
        return Err(Error::Truncated);
    }
    let mut keys = Vec::with_capacity(count);
    for index in 0..count {
        let encoded = read_u32(bytes, 8 + index * 4)?;
        let frame = key_time(encoded, frames)?;
        if keys.last().is_some_and(|(previous, _)| *previous >= frame) {
            return Err(Error::InvalidChunk("compressed visibility key order"));
        }
        keys.push((frame, encoded & 0x8000_0000 != 0));
    }
    Ok((pivot, keys))
}

fn channel_header(bytes: &[u8], frames: u16) -> Result<(usize, usize), Error> {
    let count = usize::try_from(read_u32(bytes, 0)?)
        .map_err(|_| Error::LimitExceeded("compressed animation keys"))?;
    if count == 0 || count > usize::from(frames) {
        return Err(Error::InvalidChunk("compressed animation key count"));
    }
    let pivot = usize::from(u16_at(bytes, 4)?);
    if pivot >= MAX_PIVOTS {
        return Err(Error::LimitExceeded("compressed animation pivot"));
    }
    Ok((count, pivot))
}

fn key_time(encoded: u32, frames: u16) -> Result<u16, Error> {
    let frame = u16::try_from(encoded & 0x7fff_ffff)
        .map_err(|_| Error::InvalidChunk("compressed animation key time"))?;
    if frame >= frames {
        return Err(Error::InvalidChunk("compressed animation key time"));
    }
    Ok(frame)
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

#[cfg(test)]
mod tests {
    use super::{Compressed, Error};

    fn chunk(id: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = id.to_le_bytes().to_vec();
        bytes.extend(u32::try_from(body.len()).unwrap().to_le_bytes());
        bytes.extend(body);
        bytes
    }

    fn animation(keys: &[(u32, f32)], bits: &[u32]) -> Vec<u8> {
        let mut header = [0; 44];
        header[..4].copy_from_slice(&1_u32.to_le_bytes());
        header[4..8].copy_from_slice(b"MOVE");
        header[20..23].copy_from_slice(b"RIG");
        header[36..40].copy_from_slice(&6_u32.to_le_bytes());
        header[40..42].copy_from_slice(&30_u16.to_le_bytes());
        let mut bytes = chunk(0x281, &header);
        let mut channel = u32::try_from(keys.len()).unwrap().to_le_bytes().to_vec();
        channel.extend([1, 0, 1, 0]);
        for (time, value) in keys {
            channel.extend(time.to_le_bytes());
            channel.extend(value.to_le_bytes());
        }
        bytes.extend(chunk(0x282, &channel));
        if !bits.is_empty() {
            let mut visibility = u32::try_from(bits.len()).unwrap().to_le_bytes().to_vec();
            visibility.extend([1, 0, 0, 1]);
            for bit in bits {
                visibility.extend(bit.to_le_bytes());
            }
            bytes.extend(chunk(0x283, &visibility));
        }
        bytes
    }

    #[test]
    fn uneven_timecodes_interpolate_and_hold_after_last_key() {
        let clip = Compressed::parse(&animation(&[(0, 0.0), (4, 8.0)], &[])).unwrap();
        assert!((clip.sample(1, 2.5).0[0] - 5.0).abs() < f32::EPSILON);
        assert!((clip.sample(1, 5.5).0[0] - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn next_binary_flag_delays_jump_and_visibility_uses_first_bit_before_time() {
        let clip = Compressed::parse(&animation(
            &[(0, 2.0), (0x8000_0004, 8.0)],
            &[2, 0x8000_0004],
        ))
        .unwrap();
        assert!((clip.sample(1, 3.9).0[0] - 2.0).abs() < f32::EPSILON);
        assert!(!clip.sample(1, 0.0).2);
        let (position, _, visible) = clip.sample(1, 4.0);
        assert!((position[0] - 8.0).abs() < f32::EPSILON);
        assert!(visible);
    }

    #[test]
    fn binary_and_final_quaternions_preserve_stored_values() {
        let first = [0.0_f32, 0.0, 0.0, 0.999_95];
        let last = [0.0_f32, 0.0, 0.600_1, 0.800_1];
        let mut bytes = animation(&[(0, 0.0)], &[]);
        let mut channel = 2_u32.to_le_bytes().to_vec();
        channel.extend([1, 0, 4, 6]);
        for (time, value) in [(0_u32, first), (0x8000_0004, last)] {
            channel.extend(time.to_le_bytes());
            for component in value {
                channel.extend(component.to_le_bytes());
            }
        }
        bytes.extend(chunk(0x282, &channel));
        let clip = Compressed::parse(&bytes).unwrap();
        assert_eq!(
            clip.sample(1, 3.5).1.map(f32::to_bits),
            first.map(f32::to_bits)
        );
        assert_eq!(
            clip.sample(1, 5.5).1.map(f32::to_bits),
            last.map(f32::to_bits)
        );
    }

    #[test]
    fn malformed_key_ranges_and_truncated_payload_fail() {
        for keys in [
            [(1, 0.0), (4, 8.0)],
            [(0, 0.0), (0, 8.0)],
            [(0, 0.0), (6, 8.0)],
        ] {
            assert!(matches!(
                Compressed::parse(&animation(&keys, &[])),
                Err(Error::InvalidChunk(_))
            ));
        }
        let mut bytes = animation(&[(0, 0.0), (4, 8.0)], &[]);
        bytes.pop();
        assert!(matches!(Compressed::parse(&bytes), Err(Error::Truncated)));
    }
}
