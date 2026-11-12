//! Zero Hour bridge geometry and `PathfindLayer::classifyLayerMapCell`.
use super::{Cell, CellType};
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

pub struct BridgeGeometry {
    pub from: [f32; 3],
    pub to: [f32; 3],
    /// Native from-left, from-right, to-left, to-right order.
    pub corners: [[f32; 3]; 4],
}

pub struct BridgeCell {
    pub kind: CellType,
    pub connect_ground: bool,
}

pub(super) struct BridgeLayer {
    pub geometry: BridgeGeometry,
    pub cells: BTreeMap<Cell, BridgeCell>,
    original: BTreeMap<Cell, (bool, bool, bool)>,
    pub destroyed: bool,
}

impl BridgeLayer {
    pub fn new(
        geometry: BridgeGeometry,
        width: u16,
        height: u16,
        layer: u8,
    ) -> Result<Self, &'static str> {
        geometry.validate()?;
        // addBridge may use WALL=LAST=15, but newMap's normal-layer loop
        // excludes it. With no wall pieces this slot never allocates cells.
        if layer == 15 {
            return Ok(Self {
                geometry,
                cells: BTreeMap::new(),
                original: BTreeMap::new(),
                destroyed: false,
            });
        }
        let minimum = [0, 1].map(|axis| {
            geometry
                .corners
                .iter()
                .map(|point| point[axis])
                .fold(f32::INFINITY, f32::min)
        });
        let maximum = [0, 1].map(|axis| {
            geometry
                .corners
                .iter()
                .map(|point| point[axis])
                .fold(f32::NEG_INFINITY, f32::max)
        });
        let low = minimum.map(|value| ((value - 0.1) / 10.0).floor() - 1.0);
        let high = maximum.map(|value| ((value + 0.1) / 10.0).ceil() + 1.0);
        let low = [
            low[0].max(0.0).min(f32::from(width - 1)),
            low[1].max(0.0).min(f32::from(height - 1)),
        ]
        .map(|value| value.to_u16().expect("bounded bridge cell"));
        let high = [
            high[0].max(0.0).min(f32::from(width - 1)),
            high[1].max(0.0).min(f32::from(height - 1)),
        ]
        .map(|value| value.to_u16().expect("bounded bridge cell"));
        if high[0] <= low[0] || high[1] <= low[1] {
            return Err("Bridge outside map extent");
        }
        let mut cells = BTreeMap::new();
        let mut original = BTreeMap::new();
        for x in low[0]..high[0] {
            for y in low[1]..high[1] {
                let cell = Cell { x, y };
                let minimum = [f32::from(x) * 10.0, f32::from(y) * 10.0];
                let bounds = [minimum, [minimum[0] + 10.0, minimum[1] + 10.0]];
                let count = [
                    bounds[0],
                    [bounds[0][0], bounds[1][1]],
                    bounds[1],
                    [bounds[1][0], bounds[0][1]],
                ]
                .into_iter()
                .filter(|point| geometry.contains(*point))
                .count();
                let mut kind = match count {
                    4 => CellType::Clear,
                    0 => CellType::Impassable,
                    _ => CellType::BridgeImpassable,
                };
                let mut connect_ground = false;
                if count != 4 {
                    if geometry.side_intersects(bounds) {
                        kind = CellType::BridgeImpassable;
                    } else {
                        if geometry.end_intersects(bounds, false) {
                            kind = CellType::Clear;
                        }
                        if geometry.end_intersects(bounds, true) {
                            kind = CellType::Clear;
                            connect_ground = true;
                        }
                    }
                }
                original.insert(
                    cell,
                    (
                        matches!(kind, CellType::Clear),
                        matches!(kind, CellType::Impassable),
                        connect_ground,
                    ),
                );
                cells.insert(
                    cell,
                    BridgeCell {
                        kind,
                        connect_ground,
                    },
                );
            }
        }
        Ok(Self {
            geometry,
            cells,
            original,
            destroyed: false,
        })
    }

    pub fn set_destroyed(&mut self, destroyed: bool) -> bool {
        if self.destroyed == destroyed {
            return false;
        }
        self.destroyed = destroyed;
        for (cell, data) in &mut self.cells {
            let (clear, impassable, connection) = self.original[cell];
            data.kind = if destroyed {
                CellType::BridgeImpassable
            } else if clear {
                CellType::Clear
            } else if impassable {
                CellType::Impassable
            } else {
                CellType::BridgeImpassable
            };
            // Native upper-cell metadata retains its connection while the
            // corresponding ground cell disconnects during bridge destruction.
            data.connect_ground = connection;
        }
        true
    }
}

impl BridgeGeometry {
    pub(super) fn endpoint_cells(&self) -> Option<[Cell; 2]> {
        let direction = normalized(difference(self.to, self.from));
        let x_offset = direction[0] * (10.0 * 0.7);
        let y_offset = direction[1] * (10.0 * 0.7);
        Some([
            Cell {
                x: ((self.from[0] - x_offset) / 10.0).floor().to_u16()?,
                y: ((self.from[1] - y_offset) / 10.0).floor().to_u16()?,
            },
            Cell {
                x: ((self.to[0] + x_offset) / 10.0).floor().to_u16()?,
                y: ((self.to[1] + y_offset) / 10.0).floor().to_u16()?,
            },
        ])
    }

    fn validate(&self) -> Result<(), &'static str> {
        if !self
            .from
            .iter()
            .chain(&self.to)
            .chain(self.corners.iter().flatten())
            .all(|value| value.is_finite())
        {
            return Err("Nonfinite bridge geometry");
        }
        let direction = difference(self.to, self.from);
        let across = difference(self.corners[1], self.corners[0]);
        let determinant = cross2(direction, across);
        if length(direction) == 0.0 || length(across) == 0.0 || determinant == 0.0 {
            return Err("Degenerate bridge geometry");
        }
        Ok(())
    }

    #[must_use]
    pub fn contains(&self, point: [f32; 2]) -> bool {
        triangle_contains([self.corners[0], self.corners[1], self.corners[2]], point)
            || triangle_contains([self.corners[1], self.corners[2], self.corners[3]], point)
    }

    #[must_use]
    pub fn height(&self, point: [f32; 2]) -> f32 {
        let (normal, distance) = self.plane();
        let bottom = dot2(normal, point);
        let fraction = (distance - bottom) / (normal[2] * 1000.0);
        fraction * 1000.0
    }

    #[must_use]
    pub fn ray_point(&self, from: [f32; 3], to: [f32; 3]) -> Option<[f32; 3]> {
        let (normal, distance) = self.plane();
        let direction = difference(to, from);
        let denominator = dot3(normal, direction);
        if denominator == 0.0 {
            return None;
        }
        let numerator = -(dot3(normal, from) - distance);
        let fraction = numerator / denominator;
        let point = std::array::from_fn(|axis| {
            let displacement = direction[axis] * fraction;
            from[axis] + displacement
        });
        self.contains([point[0], point[1]]).then_some(point)
    }

    pub(super) fn normal(&self) -> [f32; 3] {
        self.plane().0
    }

    fn plane(&self) -> ([f32; 3], f32) {
        let across = difference(self.corners[1], self.corners[0]);
        let direction = difference(self.corners[2], self.corners[0]);
        let cross = cross3(across, direction);
        if cross.iter().all(|component| *component == 0.0) {
            return ([0.0, 0.0, 1.0], 0.0);
        }
        let normal = plane_normal(cross);
        (normal, dot3(normal, self.corners[0]))
    }

    fn side_intersects(&self, bounds: [[f32; 2]; 2]) -> bool {
        let across = normalized(difference(self.corners[1], self.corners[0]));
        let mut left_from = self.corners[0];
        let mut right_from = self.corners[1];
        let mut left_to = self.corners[2];
        let mut right_to = self.corners[3];
        for _ in 0..2 {
            left_from = offset(left_from, across, -5.1);
            right_from = offset(right_from, across, 5.1);
            left_to = offset(left_to, across, -5.1);
            right_to = offset(right_to, across, 5.1);
            if line_intersects(left_from, left_to, bounds)
                || line_intersects(right_from, right_to, bounds)
            {
                return true;
            }
        }
        false
    }

    pub(super) fn end_intersects(&self, bounds: [[f32; 2]; 2], entry: bool) -> bool {
        let across = normalized(difference(self.corners[1], self.corners[0]));
        let direction = normalized(difference(self.to, self.from));
        let [mut left_from, mut right_from, mut left_to, mut right_to] = self.corners;
        if entry {
            left_from = offset(left_from, direction, -5.0);
            right_from = offset(right_from, direction, -5.0);
            left_to = offset(left_to, direction, 5.0);
            right_to = offset(right_to, direction, 5.0);
        }
        left_from = offset(left_from, across, 10.0);
        right_from = offset(right_from, across, -10.0);
        left_to = offset(left_to, across, 10.0);
        right_to = offset(right_to, across, -10.0);
        line_intersects(left_from, right_from, bounds) || line_intersects(left_to, right_to, bounds)
    }
}

fn difference(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot2(a: [f32; 3], b: [f32; 2]) -> f32 {
    let x = a[0] * b[0];
    let y = a[1] * b[1];
    x + y
}

fn dot3(left: [f32; 3], right: [f32; 3]) -> f32 {
    let x_product = left[0] * right[0];
    let y_product = left[1] * right[1];
    let z_product = left[2] * right[2];
    x_product + y_product + z_product
}

fn cross2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let left = a[0] * b[1];
    let right = a[1] * b[0];
    left - right
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let left = [a[1] * b[2], a[2] * b[0], a[0] * b[1]];
    let right = [a[2] * b[1], a[0] * b[2], a[1] * b[0]];
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn length(value: [f32; 3]) -> f32 {
    dot3(value, value).sqrt()
}

fn normalized(value: [f32; 3]) -> [f32; 3] {
    let magnitude = length(value);
    value.map(|component| component / magnitude)
}

// PlaneClass::Set uses Vector3::Normalize, whose reciprocal multiplication
// differs from the Coord3D component divisions used by bridge end tests.
fn plane_normal(value: [f32; 3]) -> [f32; 3] {
    let squared = dot3(value, value);
    if squared == 0.0 {
        return value;
    }
    let inverse = 1.0 / squared.sqrt();
    value.map(|component| component * inverse)
}

fn offset(point: [f32; 3], direction: [f32; 3], factor: f32) -> [f32; 3] {
    let x_offset = direction[0] * factor;
    let y_offset = direction[1] * factor;
    [point[0] + x_offset, point[1] + y_offset, point[2]]
}

fn triangle_contains(points: [[f32; 3]; 3], point: [f32; 2]) -> bool {
    let edge = difference(points[1], points[0]);
    let last = difference(points[2], points[0]);
    let orientation = cross2(edge, last);
    if orientation == 0.0 {
        return degenerate_triangle_contains(points, point);
    }
    let factor = if orientation > 0.0 { 1.0 } else { -1.0 };
    for index in 0..3 {
        let start = points[index];
        let end = points[(index + 1) % 3];
        let edge = difference(end, start);
        let relative = [point[0] - start[0], point[1] - start[1]];
        let left = edge[0] * relative[1];
        let right = edge[1] * relative[0];
        if (left - right) * factor < 0.0 {
            return false;
        }
    }
    true
}

fn degenerate_triangle_contains(points: [[f32; 3]; 3], point: [f32; 2]) -> bool {
    let first = difference(points[1], points[0]);
    let middle = difference(points[2], points[1]);
    let first_squared = dot2(first, [first[0], first[1]]);
    let middle_squared = dot2(middle, [middle[0], middle[1]]);
    // Point_In_Triangle_2D assigns p2p0dist2 from p1p2.Length2 as well.
    // Consequently its outer-segment selection reduces to these two branches.
    let (start, edge, maximum) = if first_squared > middle_squared {
        (points[0], first, first_squared)
    } else {
        (points[2], difference(points[0], points[2]), middle_squared)
    };
    let relative = [point[0] - start[0], point[1] - start[1]];
    let squared = dot2([relative[0], relative[1], 0.0], relative);
    if maximum == 0.0 {
        return squared == 0.0;
    }
    if cross2(edge, [relative[0], relative[1], 0.0]) != 0.0 {
        return false;
    }
    let end = [relative[0] - edge[0], relative[1] - edge[1]];
    squared <= maximum && dot2([end[0], end[1], 0.0], end) <= maximum
}

/// `TerrainLogic::LineInRegion`, including clipping the second endpoint using
/// the first endpoint's already clipped coordinates and original second code.
fn line_intersects(mut first: [f32; 3], mut second: [f32; 3], bounds: [[f32; 2]; 2]) -> bool {
    let code = |point: [f32; 3]| {
        u8::from(point[0] < bounds[0][0])
            | (u8::from(point[0] > bounds[1][0]) << 1)
            | (u8::from(point[1] > bounds[1][1]) << 2)
            | (u8::from(point[1] < bounds[0][1]) << 3)
    };
    let first_code = code(first);
    let second_code = code(second);
    if first_code | second_code == 0 {
        return true;
    }
    if first_code & second_code != 0 {
        return false;
    }
    if !clip(&mut first, second, first_code, bounds) {
        return false;
    }
    if !clip(&mut second, first, second_code, bounds) {
        return false;
    }
    code(first) == 0 && code(second) == 0
}

fn clip(point: &mut [f32; 3], other: [f32; 3], flags: u8, bounds: [[f32; 2]; 2]) -> bool {
    if flags == 0 {
        return true;
    }
    if flags & (4 | 8) != 0 {
        let delta = other[1] - point[1];
        if delta == 0.0 {
            return false;
        }
        let edge = if flags & 8 != 0 {
            bounds[0][1]
        } else {
            bounds[1][1]
        };
        point[0] += (other[0] - point[0]) * (edge - point[1]) / delta;
        point[1] = edge;
    }
    if point[0] < bounds[0][0] || point[0] > bounds[1][0] {
        let delta = other[0] - point[0];
        if delta == 0.0 {
            return false;
        }
        let edge = if point[0] > bounds[1][0] {
            bounds[1][0]
        } else {
            bounds[0][0]
        };
        point[1] += (other[1] - point[1]) * (edge - point[0]) / delta;
        point[0] = edge;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{degenerate_triangle_contains, normalized, plane_normal};

    #[test]
    fn native_degenerate_triangle_preserves_outer_segment_length_quirk() {
        let points = [[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        assert!(degenerate_triangle_contains(points, [1.0, 0.0]));
        assert!(!degenerate_triangle_contains(points, [0.0, 0.0]));
        assert!(!degenerate_triangle_contains(points, [2.0, 0.0]));
        assert!(!degenerate_triangle_contains(points, [1.0, 0.01]));
        assert!(degenerate_triangle_contains([[3.0; 3]; 3], [3.0; 2]));
    }

    #[test]
    fn plane_normal_uses_vector_reciprocal_while_end_vectors_use_coord_division() {
        let vector = [1.0, 7.0, 1.0];
        assert_eq!(
            plane_normal(vector).map(f32::to_bits),
            [0x3e0f_6381, 0x3f7a_ee22, 0x3e0f_6381]
        );
        assert_eq!(
            normalized(vector).map(f32::to_bits),
            [0x3e0f_6381, 0x3f7a_ee21, 0x3e0f_6381]
        );
    }
}
