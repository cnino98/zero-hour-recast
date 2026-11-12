//! Process-static `WWMath` volume and WW3D particle-property random streams.
use num_traits::ToPrimitive;

pub(super) struct Random {
    volume_index: i32,
    property: [u32; 624],
    property_index: usize,
}

impl Random {
    pub fn new() -> Self {
        let mut property = [0_u32; 624];
        property[0] = 4357;
        for index in 1..624 {
            property[index] = property[index - 1].wrapping_mul(69069);
        }
        Self {
            volume_index: 0,
            property,
            property_index: 624,
        }
    }

    // Random3Class uses signed shifts and wrapping 32-bit integer arithmetic.
    fn volume(&mut self) -> i32 {
        let mut low = 0_i32;
        let mut high = self.volume_index;
        self.volume_index = self.volume_index.wrapping_add(1);
        for (mix1, mix2) in [
            (0xbaa9_6887_u32, 0x4b0f_3b58_u32),
            (0x1e17_d32c, 0xe874_f0c3),
            (0x03bc_dc3c, 0x6955_c5a6),
            (0x0f33_d1b2, 0x55a7_ca46),
        ] {
            let held = high;
            let value = held ^ i32::from_ne_bytes(mix1.to_ne_bytes());
            let lower = value & 0xffff;
            let upper = value >> 16;
            let square = lower
                .wrapping_mul(lower)
                .wrapping_add(!upper.wrapping_mul(upper));
            let swapped = (square >> 16) | square.wrapping_shl(16);
            let mixed = swapped ^ i32::from_ne_bytes(mix2.to_ne_bytes());
            high = low ^ mixed.wrapping_add(lower.wrapping_mul(upper));
            low = held;
        }
        high
    }

    pub fn box_vector(&mut self, extents: [f32; 3]) -> [f32; 3] {
        extents.map(|extent| {
            let random = self.volume().to_f32().expect("native signed random");
            let unit = random * (1.0 / 2_147_483_648.0_f32);
            unit * extent
        })
    }

    pub fn property(&mut self) -> i32 {
        if self.property_index == 624 {
            for index in 0..624 {
                let pair = (self.property[index] & 0x8000_0000)
                    | (self.property[(index + 1) % 624] & 0x7fff_ffff);
                let matrix = if pair & 1 == 0 { 0 } else { 0x9908_b0df };
                self.property[index] = self.property[(index + 397) % 624] ^ (pair >> 1) ^ matrix;
            }
            self.property_index = 0;
        }
        let mut value = self.property[self.property_index];
        self.property_index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^= value >> 18;
        i32::from_ne_bytes(value.to_ne_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::Random;

    #[test]
    fn native_signed_volume_and_old_mt_seed_streams_match_source_word_corpus() {
        let mut random = Random::new();
        let expected_volume: [u32; 12] = [
            0xad2e_aa18,
            0x6670_51bb,
            0xbeb8_385d,
            0x1293_a6d6,
            0x9a1c_341d,
            0x4868_b99c,
            0x803d_17d0,
            0x569b_65af,
            0xb762_acdd,
            0xf517_1076,
            0x873f_d40c,
            0x2f1e_be17,
        ];
        for expected in expected_volume {
            assert_eq!(random.volume().to_ne_bytes(), expected.to_ne_bytes());
        }
        let expected_property: [u32; 12] = [
            0xd13c_8af5,
            0xffc2_7482,
            0x82a6_958b,
            0x21ac_240a,
            0x0911_0bba,
            0xfe12_7bc0,
            0xa02e_7212,
            0x1006_0698,
            0x6924_52f5,
            0x2280_870e,
            0xfb1a_7281,
            0xd65f_6aa9,
        ];
        for expected in expected_property {
            assert_eq!(random.property().to_ne_bytes(), expected.to_ne_bytes());
        }
    }
}
