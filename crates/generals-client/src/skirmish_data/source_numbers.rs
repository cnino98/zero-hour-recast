//! Core `INI.cpp::scanType` / `scanUnsignedInt` and `parseDurationUnsignedInt`.
//! The pinned C++20 branch parses an `Int64` prefix, then narrows to `UnsignedInt`.
use num_traits::ToPrimitive;
use std::error::Error;

pub fn unsigned(token: &str) -> Result<u32, Box<dyn Error>> {
    let token = token.strip_prefix('+').unwrap_or(token);
    let negative = usize::from(token.starts_with('-'));
    let digits = token.as_bytes()[negative..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return Err("invalid native unsigned integer".into());
    }
    // from_chars does not require the returned pointer to reach token.end().
    let value: i64 = token[..negative + digits].parse()?;
    Ok(u32::try_from(value.rem_euclid(1_i64 << 32)).expect("native UnsignedInt narrowing"))
}

pub fn duration_frames(token: &str) -> Result<u32, Box<dyn Error>> {
    let milliseconds = unsigned(token)?
        .to_f32()
        .expect("UnsignedInt converts to Real");
    let frames = milliseconds * (30.0_f32 / 1000.0);
    Ok(frames
        .ceil()
        .to_u32()
        .expect("UnsignedInt milliseconds fit frame count"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_native_prefix_sign_and_narrowing_boundaries() {
        for (token, expected) in [
            ("-1", u32::MAX),
            ("4294967296", 0),
            ("100ms", 100),
            ("+-1", u32::MAX),
            ("+17.5", 17),
            ("0x10", 0),
            ("9223372036854775807", u32::MAX),
            ("-9223372036854775808", 0),
        ] {
            assert_eq!(unsigned(token).unwrap(), expected, "{token}");
        }
        for token in [
            "",
            "+",
            "-",
            "--1",
            " 1",
            "9223372036854775808",
            "-9223372036854775809",
        ] {
            assert!(unsigned(token).is_err(), "{token}");
        }
    }

    #[test]
    fn duration_narrows_before_binary32_conversion_multiply_and_ceil() {
        assert_eq!(duration_frames("-1").unwrap(), 128_849_016);
        assert_eq!(duration_frames("4294967296").unwrap(), 0);
        assert_eq!(duration_frames("101ms").unwrap(), 4);
    }
}
