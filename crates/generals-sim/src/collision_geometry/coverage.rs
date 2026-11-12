//! Ordered native partition coverage for the zero-origin W3D terrain extent.

use super::{Body, Shape};

pub fn coverage(
    body: &Body<'_>,
    is_small: bool,
    cell_size: f32,
    dimensions: [u16; 2],
) -> Vec<[u16; 2]> {
    assert!(cell_size.is_finite() && cell_size > 0.0);
    let mut cells = Vec::new();
    if is_small {
        let radius = body.geometry.major.min(cell_size * 0.5);
        for x in cell_range(body.position[0], radius, cell_size, dimensions[0]) {
            for y in cell_range(body.position[1], radius, cell_size, dimensions[1]) {
                cells.push([x, y]);
            }
        }
    } else if matches!(body.geometry.shape, Shape::Box) {
        rectangle(body, cell_size, dimensions, &mut cells);
    } else {
        let radius = body.geometry.major;
        for x in cell_range(body.position[0], radius, cell_size, dimensions[0]) {
            for y in cell_range(body.position[1], radius, cell_size, dimensions[1]) {
                let origin = [f32::from(x) * cell_size, f32::from(y) * cell_size];
                let delta = std::array::from_fn::<_, 2, _>(|axis| {
                    let nearest =
                        origin[axis].max(body.position[axis].min(origin[axis] + cell_size));
                    body.position[axis] - nearest
                });
                let squares = delta.map(|value| value * value);
                if squares[0] + squares[1] < radius * radius {
                    cells.push([x, y]);
                }
            }
        }
    }
    cells
}

fn rectangle(body: &Body<'_>, cell_size: f32, dimensions: [u16; 2], cells: &mut Vec<[u16; 2]>) {
    let cosine = body.angle.cos();
    let sine = body.angle.sin();
    let step_size = cell_size * 0.5;
    let step_x = [cosine * step_size, sine * step_size];
    let step_y = [sine * step_size, -cosine * step_size];
    let inverse_steps = 2.0 * (1.0 / step_size);
    let count = [
        step_count(body.geometry.major * inverse_steps),
        step_count(body.geometry.minor * inverse_steps),
    ];
    let products = [
        body.geometry.major * cosine,
        body.geometry.minor * cosine,
        body.geometry.major * sine,
        body.geometry.minor * sine,
    ];
    let mut row = [
        body.position[0] - products[0] - products[3],
        body.position[1] + products[1] - products[2],
    ];
    for _ in 0..count[1] {
        let mut point = row;
        for _ in 0..count[0] {
            if let (Some(x), Some(y)) = (
                cell(point[0] * (1.0 / cell_size), dimensions[0]),
                cell(point[1] * (1.0 / cell_size), dimensions[1]),
            ) {
                // addSubPixToCoverage retains the first encounter of each cell.
                if !cells.contains(&[x, y]) {
                    cells.push([x, y]);
                }
            }
            point[0] += step_x[0];
            point[1] += step_x[1];
        }
        row[0] += step_y[0];
        row[1] += step_y[1];
    }
}

fn cell_range(center: f32, radius: f32, cell_size: f32, limit: u16) -> std::ops::Range<u16> {
    let inverse = 1.0 / cell_size;
    let first = (center - radius) * inverse;
    let last = (center + radius) * inverse;
    if limit == 0 || last < 0.0 || first >= f32::from(limit) {
        return 0..0;
    }
    let start = cell(first, limit).unwrap_or(0);
    let end = cell(last, limit).map_or(limit, |value| value + 1);
    start..end
}

fn cell(value: f32, limit: u16) -> Option<u16> {
    if value < 0.0 || value >= f32::from(limit) {
        return None;
    }
    let (mut low, mut high) = (0, limit);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if f32::from(middle) <= value {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(low)
}

fn step_count(value: f32) -> u16 {
    // Grounded actor extents are bounded to fewer than 65,536 half-cell samples.
    assert!(value.is_finite() && (0.0..=f32::from(u16::MAX)).contains(&value));
    u16::try_from(crate::native_real::ceil(value)).expect("native rectangle sample count")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision_geometry::Geometry;

    #[test]
    fn small_fill_clamps_radius_and_preserves_x_major_order() {
        let geometry = Geometry {
            is_small: false,
            shape: Shape::Box,
            major: 100.0,
            minor: 1.0,
            height: 1.0,
        };
        let body = Body {
            geometry: &geometry,
            position: [10.0, 10.0, 0.0],
            angle: 0.7,
        };
        assert_eq!(
            coverage(&body, true, 10.0, [4, 4]),
            [[0, 0], [0, 1], [1, 0], [1, 1]]
        );
    }

    #[test]
    fn precise_circle_excludes_tangent_cells_and_clips_negative_coordinates() {
        let geometry = Geometry {
            is_small: false,
            shape: Shape::Cylinder,
            major: 5.0,
            minor: 1.0,
            height: 1.0,
        };
        let mut body = Body {
            geometry: &geometry,
            position: [5.0, 5.0, 0.0],
            angle: 0.0,
        };
        assert_eq!(coverage(&body, false, 10.0, [4, 4]), [[0, 0]]);
        body.position = [-1.0, 10.0, 0.0];
        assert_eq!(coverage(&body, false, 10.0, [4, 4]), [[0, 0], [0, 1]]);
    }

    #[test]
    fn rectangle_samples_half_cells_in_top_to_bottom_first_encounter_order() {
        let geometry = Geometry {
            is_small: false,
            shape: Shape::Box,
            major: 10.0,
            minor: 10.0,
            height: 1.0,
        };
        let body = Body {
            geometry: &geometry,
            position: [15.0, 15.0, 0.0],
            angle: 0.0,
        };
        assert_eq!(
            coverage(&body, false, 10.0, [4, 4]),
            // Native fast_float_ceil(4.0) is 5: both far boundary samples
            // participate, unlike mathematical ceil's four samples.
            [
                [0, 2],
                [1, 2],
                [2, 2],
                [0, 1],
                [1, 1],
                [2, 1],
                [0, 0],
                [1, 0],
                [2, 0]
            ]
        );
    }
}
