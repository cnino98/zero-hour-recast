//! Ground starting units: `GameLogic::placeNetworkBuildingsForPlayer`,
//! `PartitionManager::findPositionAround/tryPosition` and `Pathfinder::adjustDestination`.
//! Source revision is pinned in the parent module. Water, bridges and group goals
//! are not represented by this map adapter, nor is the rest of engine RNG consumption.

use std::f32::consts::TAU;

use super::movement::{bounding_radius, footprints_overlap};
use super::{CELL_SIZE, Cell, Footprint, Skirmish};

impl Skirmish {
    pub(super) fn snap_unit_position(
        &self,
        type_index: usize,
        position: [f32; 2],
    ) -> Option<[f32; 2]> {
        let (_, center) = radius_and_center(&self.catalog[type_index].footprint(position, 0.0));
        let before = if center { 0.0 } else { CELL_SIZE / 2.0 };
        let cell = self.cell_at([position[0] + before, position[1] + before])?;
        let after = if center { 0.5 } else { 0.05 };
        Some([
            (f32::from(cell.x) + after) * CELL_SIZE,
            (f32::from(cell.y) + after) * CELL_SIZE,
        ])
    }

    pub(super) fn starting_builder_position(
        &self,
        type_index: usize,
        center: [f32; 2],
        sphere_radius: f32,
        random: &mut [u32; 6],
    ) -> Option<[f32; 2]> {
        // GeneralsMD/.../GameLogic.cpp supplies exactly these two radii.
        let start_angle = random_angle(random);
        let minimum = sphere_radius * 0.7;
        let maximum = sphere_radius * 1.3;
        let mut distance = minimum;
        loop {
            if distance > maximum {
                break;
            }
            // The first ring tests only its starting angle. Later rings alternate
            // positive/negative offsets before increasing radius by five units.
            let spacing = if distance <= minimum {
                TAU
            } else {
                (5.0 / (distance + 1.0)) * (TAU / 6.0)
            };
            let samples = ((TAU / spacing) / 2.0).ceil();
            let mut sample = 0_u16;
            loop {
                if f32::from(sample) >= samples {
                    break;
                }
                let offset = spacing * f32::from(sample);
                for angle in [start_angle + offset, start_angle - offset]
                    .into_iter()
                    .take(if sample == 0 { 1 } else { 2 })
                {
                    let (sine, cosine) = angle.sin_cos();
                    let displacement = [distance * cosine, distance * sine];
                    let position = [center[0] + displacement[0], center[1] + displacement[1]];
                    let probe = Footprint {
                        position,
                        angle,
                        major: 5.0,
                        minor: 5.0,
                        shape: crate::collision_geometry::Shape::Sphere,
                    };
                    if self
                        .cell_at(position)
                        .is_some_and(|cell| !self.grid.is_blocked(cell))
                        && self.clear_of_objects(&probe)
                    {
                        // placeObjectAtPosition immediately invokes adjustDestination
                        // after findPositionAround. This is not a nearest-cell search.
                        return self.adjust_starting_destination(type_index, position);
                    }
                }
                sample += 1;
            }
            distance += 5.0;
        }
        None
    }

    pub(super) fn adjust_starting_destination(
        &self,
        type_index: usize,
        position: [f32; 2],
    ) -> Option<[f32; 2]> {
        let geometry = self.catalog[type_index].footprint(position, 0.0);
        let (radius, center_in_cell) = radius_and_center(&geometry);
        let offset = if center_in_cell { 0.0 } else { CELL_SIZE / 2.0 };
        let cell = self.cell_at([position[0] + offset, position[1] + offset])?;
        let mut x = i32::from(cell.x);
        let mut y = i32::from(cell.y);
        if let Some(position) =
            self.check_starting_destination(type_index, x, y, radius, center_in_cell)
        {
            return Some(position);
        }
        // Core/.../AIPathfind.cpp: adjustDestination, MAX_ADJUSTMENT_CELL_COUNT=400.
        // Newly created units have no single-unit selection group, so the fixed
        // profile's useExactDestination exception does not apply during startup.
        let mut remaining = 400;
        let mut segment = 1;
        while remaining > 0 {
            for (direction, [dx, dy]) in [[1, 0], [0, 1], [-1, 0], [0, -1]].into_iter().enumerate()
            {
                for _ in 0..segment {
                    x += dx;
                    y += dy;
                    remaining -= 1;
                    if let Some(position) =
                        self.check_starting_destination(type_index, x, y, radius, center_in_cell)
                    {
                        return Some(position);
                    }
                }
                if direction % 2 == 1 {
                    segment += 1;
                }
            }
        }
        None
    }

    fn check_starting_destination(
        &self,
        type_index: usize,
        x: i32,
        y: i32,
        radius: i32,
        center_in_cell: bool,
    ) -> Option<[f32; 2]> {
        let cell = Cell {
            x: u16::try_from(x).ok()?,
            y: u16::try_from(y).ok()?,
        };
        let above = radius + i32::from(center_in_cell);
        for column in x - radius..x + above {
            for row in y - radius..y + above {
                let occupied = Cell {
                    x: u16::try_from(column).ok()?,
                    y: u16::try_from(row).ok()?,
                };
                if self.navigation_for_type(type_index).is_blocked(occupied) {
                    return None;
                }
            }
        }
        // adjustCoordToCell deliberately uses .05, not zero, for even diameters.
        let offset = if center_in_cell { 0.5 } else { 0.05 };
        let position = [
            (f32::from(cell.x) + offset) * CELL_SIZE,
            (f32::from(cell.y) + offset) * CELL_SIZE,
        ];
        let unit = &self.catalog[type_index];
        let footprint = unit.footprint(position, unit.placement_angle);
        // The full source obstruction raster and mobile goal reservations are not
        // ported. Exact source geometry overlap also closes the current raster's
        // boundary-cell gaps; this restriction is explicit, not parity evidence.
        self.clear_of_objects(&footprint).then_some(position)
    }

    fn clear_of_objects(&self, footprint: &Footprint) -> bool {
        self.entities
            .iter()
            .filter(|entity| entity.alive())
            .all(|entity| {
                !footprints_overlap(
                    footprint,
                    &self.catalog[entity.type_index].footprint(entity.position, entity.facing),
                )
            })
            && self
                .layout
                .obstacles
                .iter()
                .all(|other| !footprints_overlap(footprint, other))
    }
}

/// Core/.../AIPathfind.cpp: getRadiusAndCenter, including its two-cell radius cap.
pub(super) fn radius_and_center(geometry: &Footprint) -> (i32, bool) {
    let mut diameter = 2.0 * bounding_radius(geometry);
    if diameter > CELL_SIZE && diameter < 2.0 * CELL_SIZE {
        diameter = 2.0 * CELL_SIZE;
    }
    let cells = (diameter / CELL_SIZE + 0.3).floor().max(1.0);
    let half = (cells / 2.0).floor();
    let radius = if half >= 2.0 {
        2
    } else {
        i32::from(half >= 1.0)
    };
    (radius, half > 2.0 || cells % 2.0 > 0.0)
}

pub(super) fn seed_random(seed: u32) -> [u32; 6] {
    // Core/.../Common/RandomValue.cpp: seedRandom's sequential differences reduce
    // to adding the same base seed to these six initial words, modulo 2^32.
    [
        0xf22d_0e56,
        0x8831_26e9,
        0xc624_dd2f,
        0x0702_c49c,
        0x9e35_3f7d,
        0x6fdf_3b64,
    ]
    .map(|word: u32| word.wrapping_add(seed))
}

pub(super) fn random_angle(seed: &mut [u32; 6]) -> f32 {
    random_real(seed, 0.0, TAU)
}

pub(super) fn random_between(seed: &mut [u32; 6], maximum: u32) -> u32 {
    // RETAIL_COMPATIBLE_CRC=0: equal bounds consume no random draw.
    if maximum == 0 {
        0
    } else if maximum == u32::MAX {
        random_value(seed)
    } else {
        random_value(seed) % (maximum + 1)
    }
}

pub(super) fn random_real(seed: &mut [u32; 6], low: f32, high: f32) -> f32 {
    if low >= high {
        return high;
    }
    let value = random_value(seed);
    let bytes = value.to_le_bytes();
    // Both integer halves and the high product are exact f32 values; their sum
    // rounds once, like the source's UnsignedInt -> Real conversion.
    let integer_high = f32::from(u16::from_le_bytes([bytes[2], bytes[3]])) * 65_536.0;
    let real = integer_high + f32::from(u16::from_le_bytes([bytes[0], bytes[1]]));
    let fraction = real * (1.0 / 4_294_967_296.0);
    let scaled = fraction * (high - low);
    scaled + low
}

pub(super) fn random_value(seed: &mut [u32; 6]) -> u32 {
    // Preserve the original ADC macro's assignment ordering, including its ax
    // alias: after the first addition its carry test compares the new ax to B.
    let mut value = seed[5].wrapping_add(seed[4]);
    let mut carry = u32::from(value < seed[5] || value < seed[4]);
    seed[4] = value;
    for index in (0..4).rev() {
        value = value.wrapping_add(seed[index]).wrapping_add(carry);
        carry = u32::from(value < seed[index]);
        seed[index] = value;
    }
    for index in (0..6).rev() {
        seed[index] = seed[index].wrapping_add(1);
        if seed[index] != 0 {
            break;
        }
        if index == 0 {
            value = value.wrapping_add(1);
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::{random_real, seed_random};

    #[test]
    fn real_range_preserves_requested_bounds_and_equal_bounds_do_not_consume_rng() {
        let mut seed = seed_random(1);
        for _ in 0..64 {
            let value = random_real(&mut seed, -0.5, 0.75);
            assert!((-0.5..=0.75).contains(&value));
        }
        let before = seed;
        assert_eq!(
            random_real(&mut seed, 2.0, 2.0).to_bits(),
            2.0_f32.to_bits()
        );
        assert_eq!(seed, before);
    }
}
