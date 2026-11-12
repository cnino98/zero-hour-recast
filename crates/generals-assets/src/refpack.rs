use crate::{
    AssetError,
    binary::{Reader, usize_from_u32},
};

const MAX_DECOMPRESSED_BYTES: u32 = 128 * 1024 * 1024;

/// Decode an EAR-wrapped or bare EA `RefPack` stream, with a 128 MiB output limit.
///
/// # Errors
/// Returns an error for unsupported compression, truncated input, invalid
/// back-references, mismatched declared lengths, or excessive output sizes.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, AssetError> {
    if bytes.starts_with(b"EAR\0") {
        let mut reader = Reader::new(bytes);
        reader.take(4)?;
        let outer_length = reader.le_u32()?;
        check_length(outer_length)?;
        let decoded = decode(&bytes[8..])?;
        if decoded.len() != usize_from_u32(outer_length)? {
            return Err(AssetError::Invalid("EAR and RefPack lengths disagree"));
        }
        Ok(decoded)
    } else {
        decode(bytes)
    }
}

fn check_length(length: u32) -> Result<usize, AssetError> {
    if length > MAX_DECOMPRESSED_BYTES {
        return Err(AssetError::Invalid("decompression exceeds 128 MiB limit"));
    }
    usize_from_u32(length)
}

fn read_length(reader: &mut Reader<'_>, count: usize) -> Result<u32, AssetError> {
    let bytes = reader.take(count)?;
    Ok(bytes
        .iter()
        .fold(0, |value, byte| (value << 8) | u32::from(*byte)))
}

fn decode(bytes: &[u8]) -> Result<Vec<u8>, AssetError> {
    let mut reader = Reader::new(bytes);
    let flags = reader.byte()?;
    if flags & 0x7e != 0x10 || reader.byte()? != 0xfb {
        return Err(AssetError::Invalid("unsupported RefPack signature"));
    }
    let size_bytes = if flags & 0x80 == 0 { 3 } else { 4 };
    if flags & 1 != 0 {
        reader.take(size_bytes)?;
    }
    let limit = check_length(read_length(&mut reader, size_bytes)?)?;
    let mut output = Vec::with_capacity(limit);
    loop {
        let command = Command::read(&mut reader)?;
        append_literals(&mut reader, &mut output, command.literals, limit)?;
        if command.end {
            if output.len() != limit || !reader.is_empty() {
                return Err(AssetError::Invalid("RefPack size or end marker mismatch"));
            }
            return Ok(output);
        }
        append_match(&mut output, command.distance, command.length, limit)?;
    }
}

struct Command {
    literals: usize,
    distance: usize,
    length: usize,
    end: bool,
}

impl Command {
    fn read(reader: &mut Reader<'_>) -> Result<Self, AssetError> {
        let first = usize::from(reader.byte()?);
        let (literals, distance, length, end) = match first {
            0x00..=0x7f => {
                let second = usize::from(reader.byte()?);
                (
                    first & 3,
                    ((first & 0x60) << 3) + second + 1,
                    ((first & 0x1c) >> 2) + 3,
                    false,
                )
            }
            0x80..=0xbf => {
                let second = usize::from(reader.byte()?);
                let third = usize::from(reader.byte()?);
                (
                    second >> 6,
                    ((second & 0x3f) << 8) + third + 1,
                    (first & 0x3f) + 4,
                    false,
                )
            }
            0xc0..=0xdf => {
                let second = usize::from(reader.byte()?);
                let third = usize::from(reader.byte()?);
                let fourth = usize::from(reader.byte()?);
                (
                    first & 3,
                    ((first & 0x10) << 12) + (second << 8) + third + 1,
                    ((first & 0x0c) << 6) + fourth + 5,
                    false,
                )
            }
            0xe0..=0xfb => (((first & 0x1f) << 2) + 4, 0, 0, false),
            _ => (first & 3, 0, 0, true),
        };
        Ok(Self {
            literals,
            distance,
            length,
            end,
        })
    }
}

fn append_literals(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    count: usize,
    limit: usize,
) -> Result<(), AssetError> {
    if count > limit - output.len() {
        return Err(AssetError::Invalid("literal run exceeds declared output"));
    }
    output.extend_from_slice(reader.take(count)?);
    Ok(())
}

fn append_match(
    output: &mut Vec<u8>,
    distance: usize,
    length: usize,
    limit: usize,
) -> Result<(), AssetError> {
    if length == 0 {
        return Ok(());
    }
    if distance == 0 || distance > output.len() || length > limit - output.len() {
        return Err(AssetError::Invalid("invalid RefPack back-reference"));
    }
    // Byte-at-a-time copying intentionally supports overlapping LZ matches.
    for _ in 0..length {
        let byte = output[output.len() - distance];
        output.push(byte);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::decompress;

    #[test]
    fn overlapping_match_repeats_just_written_bytes() {
        let bytes = [0x10, 0xfb, 0, 0, 8, 1, 0, b'a', 4, 0, 0xfc];
        assert_eq!(decompress(&bytes).expect("valid overlap"), b"aaaaaaaa");
    }

    #[test]
    fn rejects_invalid_back_reference_and_overflow() {
        let no_history = [0x10, 0xfb, 0, 0, 3, 0, 0, 0xfc];
        assert!(decompress(&no_history).is_err());
        let overflow = [0x10, 0xfb, 0, 0, 3, 1, 0, b'a', 0xfc];
        assert!(decompress(&overflow).is_err());
    }

    #[test]
    fn rejects_truncated_commands_and_length_mismatches() {
        assert!(decompress(&[0x10, 0xfb, 0, 0, 1, 0x80]).is_err());
        assert!(decompress(&[0x10, 0xfb, 0, 0, 1, 0xfc]).is_err());
        assert!(decompress(&[0x90, 0xfb, 0xff, 0xff, 0xff, 0xff]).is_err());
        let mut outer = b"EAR\0".to_vec();
        outer.extend(2_u32.to_le_bytes());
        outer.extend([0x10, 0xfb, 0, 0, 1, 0xfd, b'a']);
        assert!(decompress(&outer).is_err());
    }
}
