// Core/Common/RandomValue.cpp: separate six-word audio generator.
pub struct Random([u32; 6]);

impl Random {
    pub fn new(seed: u32) -> Self {
        Self(
            [
                0xf22d_0e56_u32,
                0x8831_26e9,
                0xc624_dd2f,
                0x0702_c49c,
                0x9e35_3f7d,
                0x6fdf_3b64,
            ]
            .map(|value| value.wrapping_add(seed)),
        )
    }

    fn next(&mut self) -> u32 {
        let mut value = self.0[5];
        let mut carry = 0;
        for index in (0..5).rev() {
            let right = self.0[index];
            let sum = value.wrapping_add(right).wrapping_add(carry);
            carry = u32::from(sum < value || sum < right);
            value = sum;
            self.0[index] = sum;
        }
        for index in (0..6).rev() {
            self.0[index] = self.0[index].wrapping_add(1);
            if self.0[index] != 0 {
                break;
            }
            if index == 0 {
                value = value.wrapping_add(1);
            }
        }
        value
    }

    pub fn index(&mut self, count: usize) -> usize {
        if count < 2 {
            return 0;
        }
        let [a, b, c, d] = self.next().to_le_bytes();
        let value = usize::from(a)
            | (usize::from(b) << 8)
            | (usize::from(c) << 16)
            | (usize::from(d) << 24);
        value % count
    }

    pub fn real(&mut self, low: f32, high: f32) -> f32 {
        if low >= high {
            return high;
        }
        let value = self.next();
        let [a, b, c, d] = value.to_le_bytes();
        let high_word = f32::from(u16::from_le_bytes([c, d]));
        let low_word = f32::from(u16::from_le_bytes([a, b]));
        let high_scaled = high_word * 65_536.0;
        let combined = high_scaled + low_word;
        let fraction = combined * (1.0 / 4_294_967_295.0);
        let scaled = fraction * (high - low);
        scaled + low
    }
}

pub fn choice(random: &mut Random, count: usize, previous: Option<usize>, shuffled: bool) -> usize {
    if shuffled {
        let mut index = random.index(count);
        if count > 2 && previous == Some(index) {
            index = (index + 1) % count;
        }
        index
    } else {
        previous.map_or(0, |index| (index + 1) % count)
    }
}

#[cfg(test)]
mod tests {
    use super::{Random, choice};

    #[test]
    fn random_voice_does_not_repeat_with_three_variants() {
        let mut random = Random::new(0);
        let mut previous = None;
        for _ in 0..100 {
            let picked = choice(&mut random, 3, previous, true);
            assert_ne!(Some(picked), previous);
            previous = Some(picked);
        }
    }
}
