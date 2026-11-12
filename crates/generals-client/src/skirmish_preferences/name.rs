//! Original UTF-16 quoted-printable player names.
use std::error::Error;

pub(super) fn encode(value: &str) -> String {
    let mut output = String::new();
    for unit in value.encode_utf16().take_while(|unit| *unit != 0) {
        if output.len() >= 1021 {
            break;
        }
        for byte in unit.to_le_bytes() {
            if byte.is_ascii_alphanumeric() {
                output.push(char::from(byte));
            } else {
                output.push('_');
                output.push(char::from(b"0123456789ABCDEF"[usize::from(byte >> 4)]));
                output.push(char::from(b"0123456789ABCDEF"[usize::from(byte & 15)]));
            }
        }
    }
    output
}

pub(super) fn decode_name(value: &str) -> Result<String, Box<dyn Error>> {
    // QuotedPrintable.cpp uses '_' escapes over little-endian UTF-16 bytes,
    // not RFC quoted-printable or UTF-8. Invalid hex digits become zero.
    let mut input = value.bytes();
    let mut bytes = Vec::new();
    while let Some(mut byte) = input.next() {
        if byte == b'_' {
            let Some(first) = input.next() else {
                break;
            };
            byte = hex(first);
            if let Some(second) = input.next() {
                byte = (byte << 4) | hex(second);
            }
        }
        if bytes.len() >= 2046 {
            return Err("skirmish name exceeds native buffer".into());
        }
        bytes.push(byte);
    }
    if !bytes.len().is_multiple_of(2) {
        bytes.push(0);
    }
    let units = bytes
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .take_while(|word| *word != 0)
        .collect::<Vec<_>>();
    Ok(String::from_utf16(&units)?)
}

const fn hex(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_name_encoding_is_utf16_with_underscore_escapes() {
        assert_eq!(
            decode_name("C_00a_00f_00_E9_00").expect("source encoded name"),
            "Café"
        );
        assert_eq!(decode_name("_3D_D8_80_DE").expect("UTF-16 pair"), "🚀");
        assert_eq!(
            decode_name("A_").expect("source trailing escape handling"),
            "A"
        );
        assert_eq!(
            decode_name("_GG_00").expect("source invalid hex becomes zero"),
            ""
        );
    }

    #[test]
    fn malformed_utf16_and_oversize_names_do_not_escape_native_bounds() {
        assert!(decode_name("_00_D8").is_err());
        assert!(decode_name(&"A".repeat(2047)).is_err());
    }
}
