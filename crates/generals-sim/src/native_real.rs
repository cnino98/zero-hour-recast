//! Core BaseFunctions.h, `RTS_GENERALS=0`: binary32 ceil/floor then signed long.
use num_traits::ToPrimitive;

#[must_use]
pub fn ceil(mut value: f32) -> i32 {
    if value.to_bits() & 0x8000_0000 == 0 {
        value += f32::from_bits(0x3f7f_ffff);
    }
    truncated_integer(value)
}

#[must_use]
pub fn floor(mut value: f32) -> i32 {
    if value.to_bits() & 0x8000_0000 != 0 {
        value -= f32::from_bits(0x3f7f_ffff);
    }
    truncated_integer(value)
}

/// Windows x86 `WWMath::Inv_Sqrt`, with the game's 24-bit x87 precision.
///
/// Requires positive inputs from 2^-125 through 2^125 so all instruction
/// intermediates retain normal binary32 exponents. The emitter validates its
/// randomizer extents against that domain; other x87 exponent cases are unbound.
#[must_use]
pub fn inverse_sqrt(value: f32) -> f32 {
    let estimate = f32::from_bits(0xbe6e_b508_u32.wrapping_sub(value.to_bits()) >> 1);
    let half = f32::from_bits(value.to_bits().wrapping_sub(0x0080_0000));
    let square = estimate * estimate;
    let first_error = square * half;
    let first_correction = 1.5 - first_error;
    let first_product = first_error * first_correction;
    let second_error = first_product * first_correction;
    let second_estimate = estimate * first_correction;
    let second_correction = 1.5 - second_error;
    let second_product = second_error * second_correction;
    let third_error = second_product * second_correction;
    let third_estimate = second_estimate * second_correction;
    let third_correction = 1.5 - third_error;
    third_estimate * third_correction
}

fn truncated_integer(value: f32) -> i32 {
    let bits = value.to_bits();
    let exponent = (bits >> 23) & 0xff;
    let mask = if exponent < 127 {
        0
    } else {
        let shift = exponent.wrapping_sub(127) & 31;
        u32::from_ne_bytes((-0x0080_0000_i32 >> shift).to_ne_bytes())
    };
    f32::from_bits(bits & mask).to_i32().unwrap_or(i32::MIN)
}

#[cfg(test)]
mod tests {
    use super::{ceil, floor};

    #[test]
    fn emitter_inverse_sqrt_matches_independent_native_x87_instruction_corpus() {
        // source_oracle.py executes the instruction register stack independently.
        for (input, expected) in [
            (0x0100_0000, 0x5eb5_04f2),
            (0x3a83_126e, 0x41fc_fb73),
            (0x3b44_9ba6, 0x4192_0f33),
            (0x3c23_d70a, 0x411f_ffff),
            (0x3f00_0000, 0x3fb5_04f2),
            (0x3f80_0000, 0x3f7f_ffff),
            (0x4000_0000, 0x3f35_04f2),
            (0x7e00_0000, 0x2035_04f2),
        ] {
            assert_eq!(
                super::inverse_sqrt(f32::from_bits(input)).to_bits(),
                expected
            );
        }
    }

    #[test]
    fn fast_rounding_preserves_binary32_sum_and_native_integer_indefinite() {
        assert_eq!(ceil(0.0), 0);
        assert_eq!(ceil(1.0), 2);
        assert_eq!(ceil(1.25), 2);
        assert_eq!(ceil(2.75), 3);
        assert_eq!(ceil(-1.25), -1);
        assert_eq!(ceil(f32::INFINITY), i32::MIN);
        assert_eq!(ceil(f32::NAN), i32::MIN);
        assert_eq!(floor(0.0), 0);
        assert_eq!(floor(-1.0), -2);
        assert_eq!(floor(-1.25), -2);
        assert_eq!(floor(1.25), 1);
        assert_eq!(floor(f32::NEG_INFINITY), i32::MIN);
    }
}
