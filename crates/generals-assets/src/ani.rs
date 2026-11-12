//! Decodes the installed Windows ANI resource used by `Win32Mouse::init`.
//! This backend adapter retains CUR pixels/hotspots and ANI sequence/jiffy rates.
use std::time::Duration;

pub struct Animation {
    pub frames: Vec<Frame>,
    sequence: Vec<usize>,
    rates: Vec<u32>,
    total: u64,
}

pub struct Frame {
    pub rgba: Vec<u8>,
    pub hotspot: [u16; 2],
    pub width: u16,
    pub height: u16,
}

impl Animation {
    /// Decode the installed palette-based Windows animated cursor format.
    ///
    /// # Errors
    /// Rejects malformed boundaries, invalid sequences and unsupported bitmap modes.
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("ANI resource exceeds the cursor allocation limit");
        }
        if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"ACON") {
            return Err("attack cursor is not a RIFF ACON resource");
        }
        let length = usize::try_from(word(bytes, 4)?).map_err(|_| "ANI length overflow")?;
        // The installed SCCAttack.ani declares the complete file length here,
        // including the eight-byte RIFF header. Windows accepts this legacy file.
        if length != bytes.len() && length.checked_add(8) != Some(bytes.len()) {
            return Err("ANI RIFF length does not match the resource");
        }
        let mut header = None;
        let mut rates = None;
        let mut sequence = None;
        let mut frames = Vec::new();
        for (tag, data) in chunks(&bytes[12..])? {
            match tag {
                b"anih" if header.is_none() => header = Some(data),
                b"rate" if rates.is_none() => rates = Some(words(data)?),
                b"seq " if sequence.is_none() => sequence = Some(words(data)?),
                b"LIST" if data.get(..4) == Some(b"fram") => {
                    for (kind, frame) in chunks(&data[4..])? {
                        if kind != b"icon" || frames.len() >= 256 {
                            return Err("unsupported ANI frame list");
                        }
                        frames.push(decode_frame(frame)?);
                    }
                }
                b"LIST" if data.get(..4) == Some(b"INFO") => {}
                _ => return Err("unsupported or duplicate ANI chunk"),
            }
        }
        let header = header.ok_or("ANI header missing")?;
        if header.len() != 36 || word(header, 0)? != 36 {
            return Err("unsupported ANI header or sequence flags");
        }
        let sequence = match (word(header, 32)?, sequence) {
            // AF_ICON without AF_SEQUENCE uses the frames in their stored order.
            // SCCEnter and SCCNoAction use this original Windows ANI branch.
            (1, None) => {
                (0..u32::try_from(frames.len()).map_err(|_| "ANI frame count overflow")?).collect()
            }
            (3, Some(sequence)) => sequence,
            _ => return Err("unsupported ANI header or sequence flags"),
        };
        if frames.is_empty()
            || usize::try_from(word(header, 4)?).ok() != Some(frames.len())
            || sequence.is_empty()
            || sequence.len() > 4096
            || usize::try_from(word(header, 8)?).ok() != Some(sequence.len())
        {
            return Err("ANI frame or step count mismatch");
        }
        let default_rate = word(header, 28)?;
        let rates = rates.unwrap_or_else(|| vec![default_rate; sequence.len()]);
        if rates.len() != sequence.len() || rates.contains(&0) {
            return Err("ANI has invalid frame durations");
        }
        let sequence = sequence
            .into_iter()
            .map(|index| {
                usize::try_from(index)
                    .ok()
                    .filter(|index| *index < frames.len())
                    .ok_or("ANI sequence references a missing cursor frame")
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            total: rates.iter().map(|rate| u64::from(*rate)).sum(),
            frames,
            sequence,
            rates,
        })
    }

    /// Return the authored frame at an elapsed time, looping in 1/60-second jiffies.
    #[must_use]
    pub fn frame_at(&self, elapsed: Duration) -> usize {
        let jiffies = elapsed.as_nanos() * 60 / 1_000_000_000;
        let mut remaining = jiffies % u128::from(self.total);
        for (&frame, &rate) in self.sequence.iter().zip(&self.rates) {
            if remaining < u128::from(rate) {
                return frame;
            }
            remaining -= u128::from(rate);
        }
        unreachable!("positive ANI durations cover their total")
    }
}

type Chunk<'a> = (&'a [u8], &'a [u8]);

fn chunks(mut bytes: &[u8]) -> Result<Vec<Chunk<'_>>, &'static str> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let size = usize::try_from(word(bytes, 4)?).map_err(|_| "ANI chunk size overflow")?;
        let end = 8_usize.checked_add(size).ok_or("ANI chunk size overflow")?;
        let data = bytes.get(8..end).ok_or("truncated ANI chunk")?;
        result.push((&bytes[..4], data));
        bytes = bytes
            .get(end..)
            .and_then(|tail| tail.get(size % 2..))
            .ok_or("missing ANI chunk padding")?;
    }
    Ok(result)
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or("truncated ANI integer")?;
    Ok(u32::from_le_bytes(slice.try_into().expect("four bytes")))
}

fn short(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or("truncated CUR integer")?;
    Ok(u16::from_le_bytes(slice.try_into().expect("two bytes")))
}

fn words(bytes: &[u8]) -> Result<Vec<u32>, &'static str> {
    if !bytes.len().is_multiple_of(4) {
        return Err("unaligned ANI integer list");
    }
    (0..bytes.len())
        .step_by(4)
        .map(|offset| word(bytes, offset))
        .collect()
}

fn decode_frame(bytes: &[u8]) -> Result<Frame, &'static str> {
    if short(bytes, 0)? != 0 || short(bytes, 2)? != 2 || short(bytes, 4)? != 1 {
        return Err("ANI frame must contain one CUR bitmap");
    }
    let dimension = |offset| {
        bytes
            .get(offset)
            .map(|value| if *value == 0 { 256 } else { u16::from(*value) })
            .ok_or("missing CUR dimension")
    };
    let dimensions = [dimension(6)?, dimension(7)?];
    let [width, height] = dimensions.map(usize::from);
    let hotspot = [short(bytes, 10)?, short(bytes, 12)?];
    let offset = usize::try_from(word(bytes, 18)?).map_err(|_| "CUR offset overflow")?;
    let length = usize::try_from(word(bytes, 14)?).map_err(|_| "CUR length overflow")?;
    let end = offset
        .checked_add(length)
        .ok_or("CUR bitmap length overflow")?;
    let bitmap = bytes.get(offset..end).ok_or("truncated CUR bitmap")?;
    if word(bitmap, 0)? != 40
        || usize::try_from(word(bitmap, 4)?).ok() != Some(width)
        || usize::try_from(word(bitmap, 8)?).ok() != Some(height * 2)
        || short(bitmap, 12)? != 1
        || short(bitmap, 14)? != 4
        || word(bitmap, 16)? != 0
        || !matches!(word(bitmap, 32)?, 0 | 16)
        || usize::from(hotspot[0]) >= width
        || usize::from(hotspot[1]) >= height
    {
        return Err("unsupported cursor bitmap or hotspot");
    }
    let stride = (width * 4).div_ceil(32) * 4;
    let mask_stride = width.div_ceil(32) * 4;
    let mask_offset = 104 + stride * height;
    if bitmap.len() != mask_offset + mask_stride * height {
        return Err("cursor bitmap/palette/mask size mismatch");
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in (0..height).rev() {
        for x in 0..width {
            let packed = bitmap[104 + y * stride + x / 2];
            let index = usize::from((packed >> if x % 2 == 0 { 4 } else { 0 }) & 15);
            let [blue, green, red, _] = bitmap[40 + index * 4..44 + index * 4] else {
                unreachable!()
            };
            let transparent = bitmap[mask_offset + y * mask_stride + x / 8] & (128 >> (x % 8)) != 0;
            if transparent && [red, green, blue] != [0, 0, 0] {
                return Err("cursor uses unsupported destination XOR inversion");
            }
            rgba.extend([red, green, blue, if transparent { 0 } else { 255 }]);
        }
    }
    Ok(Frame {
        rgba,
        hotspot,
        width: dimensions[0],
        height: dimensions[1],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor() -> Vec<u8> {
        let mut bitmap = vec![0; 120];
        bitmap[..4].copy_from_slice(&40_u32.to_le_bytes());
        bitmap[4..8].copy_from_slice(&2_u32.to_le_bytes());
        bitmap[8..12].copy_from_slice(&4_u32.to_le_bytes());
        bitmap[12..14].copy_from_slice(&1_u16.to_le_bytes());
        bitmap[14..16].copy_from_slice(&4_u16.to_le_bytes());
        bitmap[46] = 255; // Palette 1 red (BGR).
        bitmap[49] = 255; // Palette 2 green.
        bitmap[104] = 0x11; // Bottom row red.
        bitmap[108] = 0x20; // Top row green / transparent black.
        bitmap[116] = 0x40; // Top-right AND transparency bit.
        let mut result = vec![0, 0, 2, 0, 1, 0, 2, 2, 0, 0, 1, 0, 1, 0];
        result.extend(120_u32.to_le_bytes());
        result.extend(22_u32.to_le_bytes());
        result.extend(bitmap);
        result
    }

    fn chunk(tag: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = tag.to_vec();
        bytes.extend(
            u32::try_from(payload.len())
                .expect("small fixture")
                .to_le_bytes(),
        );
        bytes.extend(payload);
        if !payload.len().is_multiple_of(2) {
            bytes.push(0);
        }
        bytes
    }

    fn animation(sequence: [u32; 3], rates: [u32; 3]) -> Vec<u8> {
        let mut contents = b"ACON".to_vec();
        let header: Vec<_> = [36_u32, 2, 3, 0, 0, 4, 1, 6, 3]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        contents.extend(chunk(*b"anih", &header));
        contents.extend(chunk(
            *b"seq ",
            &sequence
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ));
        contents.extend(chunk(
            *b"rate",
            &rates
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ));
        let mut frames = b"fram".to_vec();
        frames.extend(chunk(*b"icon", &cursor()));
        frames.extend(chunk(*b"icon", &cursor()));
        contents.extend(chunk(*b"LIST", &frames));
        chunk(*b"RIFF", &contents)
    }

    fn implicit_animation() -> Vec<u8> {
        let mut contents = b"ACON".to_vec();
        let header: Vec<_> = [36_u32, 2, 2, 0, 0, 4, 1, 10, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        contents.extend(chunk(*b"anih", &header));
        let mut frames = b"fram".to_vec();
        frames.extend(chunk(*b"icon", &cursor()));
        frames.extend(chunk(*b"icon", &cursor()));
        contents.extend(chunk(*b"LIST", &frames));
        chunk(*b"RIFF", &contents)
    }

    #[test]
    fn original_enter_cursor_uses_implicit_frame_order_and_header_rate() {
        let data = Animation::parse(&implicit_animation()).expect("implicit ANI");
        assert_eq!(data.frame_at(Duration::ZERO), 0);
        assert_eq!(data.frame_at(Duration::from_millis(166)), 0);
        assert_eq!(data.frame_at(Duration::from_nanos(166_666_667)), 1);
        assert_eq!(data.frame_at(Duration::from_nanos(333_333_334)), 0);
    }

    #[test]
    fn explicit_sequence_flag_requires_sequence_and_implicit_steps_match_frames() {
        let mut bytes = implicit_animation();
        bytes[52..56].copy_from_slice(&3_u32.to_le_bytes());
        assert!(matches!(
            Animation::parse(&bytes),
            Err("unsupported ANI header or sequence flags")
        ));
        bytes[52..56].copy_from_slice(&1_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&3_u32.to_le_bytes());
        assert!(matches!(
            Animation::parse(&bytes),
            Err("ANI frame or step count mismatch")
        ));
    }

    #[test]
    fn cursor_retains_hotspot_palette_orientation_and_transparent_mask() {
        let frame = decode_frame(&cursor()).expect("source-style cursor");
        assert_eq!(frame.hotspot, [1, 1]);
        let pixels = &frame.rgba;
        assert_eq!(&pixels[..8], &[0, 255, 0, 255, 0, 0, 0, 0]);
        assert_eq!(&pixels[8..], &[255, 0, 0, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn animation_uses_authored_sequence_jiffy_rates_and_loop_boundary() {
        let data = Animation::parse(&animation([1, 0, 1], [2, 4, 6])).expect("ANI");
        assert_eq!(data.frame_at(Duration::ZERO), 1);
        assert_eq!(data.frame_at(Duration::from_millis(33)), 1);
        assert_eq!(data.frame_at(Duration::from_nanos(33_333_334)), 0);
        assert_eq!(data.frame_at(Duration::from_millis(99)), 0);
        assert_eq!(data.frame_at(Duration::from_millis(100)), 1);
        assert_eq!(data.frame_at(Duration::from_millis(234)), 0);
    }

    #[test]
    fn malformed_frames_and_timelines_are_rejected() {
        assert!(Animation::parse(&animation([0, 2, 0], [4, 4, 4])).is_err());
        assert!(Animation::parse(&animation([0, 1, 0], [4, 0, 4])).is_err());
        let valid = animation([0, 1, 0], [4, 4, 4]);
        assert!(Animation::parse(&valid[..valid.len() - 1]).is_err());
        let mut bad = cursor();
        bad[10..12].copy_from_slice(&2_u16.to_le_bytes());
        assert!(decode_frame(&bad).is_err());
        let mut invert = cursor();
        invert[22 + 108] = 0x21;
        assert!(decode_frame(&invert).is_err());
    }
}
