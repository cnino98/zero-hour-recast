//! `BaseHeightMap::getHeightMapHeight` and `PartitionManager` terrain cache/line walk.

/// Original heightmap vertices, including the map border, in native units.
pub struct TerrainHeights {
    pub width: u16,
    pub height: u16,
    pub origin: [f32; 2],
    pub samples: Vec<f32>,
}

pub struct Terrain {
    width: u16,
    height: u16,
    origin: [f32; 2],
    samples: Vec<f32>,
    partition_inverse: f32,
    partition_width: u16,
    partition_height: u16,
    high: Vec<f32>,
    revision: u64,
}

impl Terrain {
    // BaseHeightMapRenderObjClass::isClearLineOfSight, DO_BRESENHAM branch.
    pub(in crate::skirmish) fn line_of_sight(&self, start: [f32; 3], end: [f32; 3]) -> bool {
        let cell = |point: [f32; 3]| {
            std::array::from_fn::<_, 2, _>(|axis| {
                let border = signed_index(-self.origin[axis] * 0.1).expect("native map border");
                crate::native_real::floor(point[axis] * 0.1) + border
            })
        };
        let [mut x, mut y] = cell(start);
        let goal = cell(end);
        let dx = (goal[0] - x).abs();
        let dy = (goal[1] - y).abs();
        let sx = if goal[0] >= x { 1 } else { -1 };
        let sy = if goal[1] >= y { 1 } else { -1 };
        let (denominator, add, first, second) = if dx >= dy {
            (dx, dy, [0, sy], [sx, 0])
        } else {
            (dy, dx, [sx, 0], [0, sy])
        };
        if denominator == 0 {
            return true;
        }
        let inverse = 1.0 / f32::from(i16::try_from(denominator).expect("bounded LOS span"));
        let increment = (end[2] - start[2]) * inverse;
        let mut height = start[2];
        let mut numerator = denominator / 2;
        for _ in 0..denominator {
            if x < 0 || y < 0 || x >= i32::from(self.width - 1) || y >= i32::from(self.height - 1) {
                break;
            }
            let index = usize::try_from(y).expect("nonnegative y") * usize::from(self.width)
                + usize::try_from(x).expect("nonnegative x");
            let width = usize::from(self.width);
            let highest = self.samples[index]
                .max(self.samples[index + 1])
                .max(self.samples[index + width])
                .max(self.samples[index + width + 1]);
            if highest > height + 0.5 {
                return false;
            }
            height += increment;
            numerator += add;
            if numerator >= denominator {
                numerator -= denominator;
                x += first[0];
                y += first[1];
            }
            x += second[0];
            y += second[1];
        }
        true
    }

    pub(in crate::skirmish) fn new(
        width: u16,
        height: u16,
        origin: [f32; 2],
        samples: Vec<f32>,
        partition_size: f32,
        extent: [f32; 2],
    ) -> Result<Self, &'static str> {
        if !(2..=2048).contains(&width)
            || !(2..=2048).contains(&height)
            || samples.len() != usize::from(width) * usize::from(height)
            || samples.iter().chain(&origin).any(|v| !v.is_finite())
            || !partition_size.is_finite()
            || partition_size < 1.0
        {
            return Err("invalid projectile terrain samples");
        }
        let partition_inverse = 1.0 / partition_size;
        let partition_width = bounded_index((extent[0] * partition_inverse).ceil())?;
        let partition_height = bounded_index((extent[1] * partition_inverse).ceil())?;
        if usize::from(partition_width) * usize::from(partition_height) > 1_048_576 {
            return Err("projectile partition cache exceeds supported size");
        }
        let mut terrain = Self {
            width,
            height,
            origin,
            samples,
            partition_inverse,
            partition_width,
            partition_height,
            high: Vec::new(),
            revision: 0,
        };
        let sample_count = bounded_index((partition_size / 10.0).ceil())?;
        let step = partition_size / f32::from(sample_count);
        let mut offsets = Vec::with_capacity(usize::from(sample_count) + 1);
        let mut offset = 0.0;
        for _ in 0..=sample_count {
            if offset > partition_size {
                break;
            }
            offsets.push(offset);
            offset += step;
        }
        for y in 0..partition_height {
            for x in 0..partition_width {
                let x_offset = f32::from(x) * partition_size;
                let y_offset = f32::from(y) * partition_size;
                let base = [x_offset, y_offset];
                let mut highest = f32::NEG_INFINITY;
                for &yy in &offsets {
                    for &xx in &offsets {
                        highest = highest.max(terrain.sample([base[0] + xx, base[1] + yy]));
                    }
                }
                terrain.high.push(highest);
            }
        }
        Ok(terrain)
    }

    pub(in crate::skirmish) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(in crate::skirmish) fn height_map(&self) -> TerrainHeights {
        TerrainHeights {
            width: self.width,
            height: self.height,
            origin: self.origin,
            samples: self.samples.clone(),
        }
    }

    // W3DTerrainVisual::setRawMapHeight + WorldHeightMap::setRawHeight.
    // Native bounds are on the linear index, so an edge column can address
    // the adjacent row. Only lower; preserve the init-only partition cache.
    pub(in crate::skirmish) fn lower_raw_height(&mut self, point: [i32; 2], height: i32) {
        use num_traits::ToPrimitive;

        let border = self
            .origin
            .map(|value| (-value * 0.1).to_i32().expect("native map border"));
        let x = point[0] + border[0];
        let y = point[1] + border[1];
        let index = i64::from(y) * i64::from(self.width) + i64::from(x);
        let Some(sample) = usize::try_from(index)
            .ok()
            .and_then(|index| self.samples.get_mut(index))
        else {
            return;
        };
        if *sample / 0.625 > height.to_f32().expect("native raw height") {
            let byte =
                u8::try_from(height.rem_euclid(256)).expect("UnsignedByte height conversion");
            *sample = f32::from(byte) * 0.625;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub(in crate::skirmish) fn sample(&self, point: [f32; 2]) -> f32 {
        let coordinates = point.map(|value| value * 0.1);
        let border = self.origin.map(|value| -value * 0.1);
        let raw_x = coordinates[0].floor() + border[0];
        let raw_y = coordinates[1].floor() + border[1];
        let x = bounded_index(raw_x.clamp(0.0, f32::from(self.width - 1))).unwrap_or(0);
        let y = bounded_index(raw_y.clamp(0.0, f32::from(self.height - 1))).unwrap_or(0);
        let raw =
            |x, y| self.samples[usize::from(y) * usize::from(self.width) + usize::from(x)] / 0.625;
        if x < 1 || y < 1 || x > self.width.saturating_sub(3) || y > self.height.saturating_sub(3) {
            return raw(x, y) * 0.625;
        }
        let dx = coordinates[0] - coordinates[0].floor();
        let dy = coordinates[1] - coordinates[1].floor();
        let near = raw(x, y);
        let far = raw(x + 1, y + 1);
        let height = if dy > dx {
            let left = raw(x, y + 1);
            let vertical = (1.0 - dy) * (near - left);
            let horizontal = dx * (far - left);
            left + vertical + horizontal
        } else {
            let right = raw(x + 1, y);
            let vertical = dy * (far - right);
            let horizontal = (1.0 - dx) * (near - right);
            right + vertical + horizontal
        };
        height * 0.625
    }

    /// `BaseHeightMap`'s twelve-sample smoothed normal, including its repeated X3 term.
    pub(in crate::skirmish) fn normal(&self, point: [f32; 2]) -> [f32; 3] {
        use num_traits::ToPrimitive;

        let coordinates = point.map(|value| value * 0.1);
        let border = self.origin.map(|value| -value * 0.1);
        let raw_x = coordinates[0].floor() + border[0];
        let raw_y = coordinates[1].floor() + border[1];
        if raw_x < 1.0
            || raw_y < 1.0
            || raw_x > f32::from(self.width.saturating_sub(3))
            || raw_y > f32::from(self.height.saturating_sub(3))
        {
            return [0.0, 0.0, 1.0];
        }
        let x = usize::from(bounded_index(raw_x).expect("bounded normal x"));
        let y = usize::from(bounded_index(raw_y).expect("bounded normal y"));
        let width = usize::from(self.width);
        let raw = |x: usize, y: usize| self.samples[y * width + x] / 0.625;
        let d0 = raw(x, y);
        let d1 = raw(x + 1, y);
        let d2 = raw(x + 1, y + 1);
        let d3 = raw(x, y + 1);
        let d4 = raw(x, y - 1);
        let d5 = raw(x + 1, y - 1);
        let d6 = raw(x + 2, y);
        let d7 = raw(x + 2, y + 1);
        let d8 = raw(x + 1, y + 2);
        let d9 = raw(x, y + 2);
        let d11 = raw(x - 1, y);
        let fx = coordinates[0] - coordinates[0].floor();
        let fy = coordinates[1] - coordinates[1].floor();
        let interpolate = |deltas: [f32; 4]| {
            let left_a = deltas[0] * (1.0 - fx);
            let left_b = fx * deltas[3];
            let left = left_a + left_b;
            let right_a = deltas[1] * (1.0 - fx);
            let right_b = fx * deltas[2];
            let right = right_a + right_b;
            // Native 1.0-fy is double, but fy*right is evaluated as Real first.
            let right = fy * right;
            (f64::from(left) * (1.0 - f64::from(fy)) + f64::from(right))
                .to_f32()
                .expect("native smoothed normal")
        };
        let dx = interpolate([d1 - d11, d6 - d0, d7 - d3, d6 - d0]);
        let dy = interpolate([d3 - d4, d2 - d5, d8 - d1, d9 - d0]);
        let x_first = 0.0 * dy;
        let x_second = dx * 32.0;
        let y_first = dx * 0.0;
        let y_second = 32.0 * dy;
        let cross = [x_first - x_second, y_first - y_second, 1024.0];
        let squared = cross.map(|value| value * value);
        let inverse = 1.0 / ((squared[0] + squared[1]) + squared[2]).sqrt();
        cross.map(|value| value * inverse)
    }

    pub(super) fn highest(&self, start: [f32; 3], end: [f32; 3]) -> Option<f32> {
        let cell = |p: [f32; 3]| -> Option<[i32; 2]> {
            let x = signed_index((p[0] * self.partition_inverse).floor()).ok()?;
            let y = signed_index((p[1] * self.partition_inverse).floor()).ok()?;
            Some([x, y])
        };
        let [mut x, mut y] = cell(start)?;
        let goal = cell(end)?;
        let dx = (goal[0] - x).abs();
        let dy = (goal[1] - y).abs();
        let sx = if goal[0] >= x { 1 } else { -1 };
        let sy = if goal[1] >= y { 1 } else { -1 };
        let (denominator, add, first, second) = if dx >= dy {
            (dx, dy, [0, sy], [sx, 0])
        } else {
            (dy, dx, [sx, 0], [0, sy])
        };
        let mut numerator = denominator / 2;
        let mut highest: Option<f32> = None;
        for _ in 0..=denominator {
            if (0..i32::from(self.partition_width)).contains(&x)
                && (0..i32::from(self.partition_height)).contains(&y)
            {
                let index = usize::try_from(y).ok()? * usize::from(self.partition_width)
                    + usize::try_from(x).ok()?;
                highest =
                    Some(highest.map_or(self.high[index], |value| value.max(self.high[index])));
            }
            numerator += add;
            if numerator >= denominator {
                numerator -= denominator;
                x += first[0];
                y += first[1];
            }
            x += second[0];
            y += second[1];
        }
        highest
    }
}

pub(super) fn bounded_index(value: f32) -> Result<u16, &'static str> {
    if !value.is_finite() || !(0.0..=f32::from(u16::MAX)).contains(&value) {
        return Err("projectile index outside supported bounds");
    }
    // The checked nonnegative range fits the IEEE significand; dropping its
    // fractional bits is the source integer conversion without unchecked casts.
    let bits = value.to_bits();
    let exponent = (bits >> 23) & 0xff;
    if exponent < 127 {
        return Ok(0);
    }
    let significand = (bits & 0x7f_ffff) | 0x80_0000;
    u16::try_from(significand >> (150 - exponent)).map_err(|_| "invalid projectile index")
}

fn signed_index(value: f32) -> Result<i32, &'static str> {
    let magnitude = i32::from(bounded_index(value.abs())?);
    Ok(if value < 0.0 { -magnitude } else { magnitude })
}

#[cfg(test)]
mod tests {
    use super::Terrain;

    #[test]
    fn partition_line_can_enter_from_outside_playable_boundary() {
        let terrain = Terrain::new(12, 12, [0.0; 2], vec![12.5; 144], 40.0, [100.0; 2]).unwrap();
        let height = terrain
            .highest([-5.0, 20.0, 0.0], [90.0, 20.0, 0.0])
            .unwrap();
        assert_eq!(height.to_bits(), 12.5_f32.to_bits());
        assert!(
            terrain
                .highest([-100.0, -5.0, 0.0], [-20.0, -5.0, 0.0])
                .is_none()
        );
    }

    #[test]
    fn raw_lowering_updates_live_queries_but_retains_native_partition_extrema() {
        let mut terrain =
            Terrain::new(12, 12, [0.0; 2], vec![12.5; 144], 40.0, [100.0; 2]).unwrap();
        for x in 0..12 {
            for y in 0..12 {
                terrain.lower_raw_height([x, y], 4);
            }
        }
        assert_eq!(terrain.sample([40.0, 40.0]).to_bits(), 2.5_f32.to_bits());
        assert_eq!(
            terrain
                .highest([40.0, 40.0, 0.0], [40.0, 40.0, 0.0])
                .unwrap()
                .to_bits(),
            12.5_f32.to_bits()
        );
    }
}
