//! `PartitionData::collidesWith` and its shape predicates, `SuperHackers` 2bfaea60.
//!
//! These are the original tests, including bounding-square circle/box tests and
//! corner-only box intersections. Contact policy and forces belong to callers.

mod coverage;
pub(crate) use coverage::coverage;

#[derive(Clone, Copy)]
pub enum Shape {
    Sphere,
    Cylinder,
    Box,
}

pub struct Geometry {
    pub is_small: bool,
    pub shape: Shape,
    pub major: f32,
    pub minor: f32,
    pub height: f32,
}

pub struct Body<'a> {
    pub geometry: &'a Geometry,
    pub position: [f32; 3],
    pub angle: f32,
}

impl Geometry {
    const fn above(&self) -> f32 {
        if matches!(self.shape, Shape::Sphere) {
            self.major
        } else {
            self.height
        }
    }

    const fn below(&self) -> f32 {
        if matches!(self.shape, Shape::Sphere) {
            self.major
        } else {
            0.0
        }
    }
}

/// Tests validated source geometry at the current object positions. Kind/status
/// exclusions and broadphase coverage are intentionally separate from geometry.
#[must_use]
pub fn intersects(a: &Body<'_>, b: &Body<'_>) -> bool {
    let a_top = a.position[2] + a.geometry.above();
    let a_bottom = a.position[2] - a.geometry.below();
    let b_top = b.position[2] + b.geometry.above();
    let b_bottom = b.position[2] - b.geometry.below();
    if !(a_top >= b_bottom && a_bottom <= b_top) {
        return false;
    }
    shape_intersects(a, b)
}

/// `PartitionFilterWouldCollide` intentionally checks positive geometry height;
/// unlike ordinary contact it does not extend spheres below their positions.
pub(crate) fn would_collide(a: &Body<'_>, b: &Body<'_>) -> bool {
    a.position[2] + a.geometry.above() >= b.position[2]
        && a.position[2] <= b.position[2] + b.geometry.above()
        && shape_intersects(a, b)
}

fn shape_intersects(a: &Body<'_>, b: &Body<'_>) -> bool {
    match (&a.geometry.shape, &b.geometry.shape) {
        (Shape::Sphere, Shape::Sphere) => {
            let delta = std::array::from_fn::<_, 3, _>(|axis| b.position[axis] - a.position[axis]);
            let squares = delta.map(|value| value * value);
            let radius = a.geometry.major + b.geometry.major;
            squares[0] + squares[1] + squares[2] <= radius * radius
        }
        (Shape::Sphere, _) => sphere_nonsphere(a, b),
        (_, Shape::Sphere) => sphere_nonsphere(b, a),
        _ => {
            let delta = [a.position[0] - b.position[0], a.position[1] - b.position[1]];
            let squares = delta.map(|value| value * value);
            let minimum = a
                .geometry
                .major
                .min(a.geometry.minor)
                .min(b.geometry.major)
                .min(b.geometry.minor);
            minimum * minimum > squares[0] + squares[1] || xy_intersects(a, b)
        }
    }
}

fn sphere_nonsphere(sphere: &Body<'_>, other: &Body<'_>) -> bool {
    let bottom = other.position[2];
    let top = bottom + other.geometry.above();
    let center = sphere.position[2];
    if center >= bottom && center <= top {
        return xy_intersects(sphere, other);
    }
    let plane = if center < bottom && center + sphere.geometry.major >= bottom {
        bottom
    } else if center > top && center - sphere.geometry.major <= top {
        top
    } else {
        return false;
    };
    let distance = (plane - center).abs();
    let radius_squared = sphere.geometry.major * sphere.geometry.major;
    let distance_squared = distance * distance;
    let geometry = Geometry {
        is_small: false,
        shape: Shape::Sphere,
        major: (radius_squared - distance_squared).sqrt(),
        minor: sphere.geometry.minor,
        height: sphere.geometry.height,
    };
    let slice = Body {
        geometry: &geometry,
        position: [sphere.position[0], sphere.position[1], plane],
        angle: sphere.angle,
    };
    xy_intersects(&slice, other)
}

fn xy_intersects(a: &Body<'_>, b: &Body<'_>) -> bool {
    match (&a.geometry.shape, &b.geometry.shape) {
        (Shape::Box, Shape::Box) => rectangles(a, b),
        (Shape::Box, _) => rectangle_circle(a, b),
        (_, Shape::Box) => rectangle_circle(b, a),
        _ => {
            let delta = [b.position[0] - a.position[0], b.position[1] - a.position[1]];
            let squares = delta.map(|value| value * value);
            let radius = a.geometry.major + b.geometry.major;
            squares[0] + squares[1] <= radius * radius
        }
    }
}

fn rectangle_circle(rectangle: &Body<'_>, circle: &Body<'_>) -> bool {
    // The native approximation retains even the circle object's orientation.
    let square = Geometry {
        is_small: false,
        shape: Shape::Box,
        major: circle.geometry.major,
        minor: circle.geometry.major,
        height: circle.geometry.height,
    };
    rectangles(
        rectangle,
        &Body {
            geometry: &square,
            position: circle.position,
            angle: circle.angle,
        },
    )
}

fn rectangles(a: &Body<'_>, b: &Body<'_>) -> bool {
    points_inside(&corners(a), b) || points_inside(&corners(b), a)
}

fn corners(body: &Body<'_>) -> [[f32; 2]; 4] {
    let cosine = body.angle.cos();
    let sine = body.angle.sin();
    let products = [
        body.geometry.major * cosine,
        body.geometry.minor * cosine,
        body.geometry.major * sine,
        body.geometry.minor * sine,
    ];
    let [x, y, _] = body.position;
    [
        [x - products[0] - products[3], y + products[1] - products[2]],
        [x + products[0] - products[3], y + products[1] + products[2]],
        [x - products[0] + products[3], y - products[1] - products[2]],
        [x + products[0] + products[3], y - products[1] + products[2]],
    ]
}

fn points_inside(points: &[[f32; 2]; 4], body: &Body<'_>) -> bool {
    let minor = if matches!(body.geometry.shape, Shape::Sphere) {
        body.geometry.major
    } else {
        body.geometry.minor
    };
    let cosine = (-body.angle).cos();
    let sine = (-body.angle).sin();
    points.iter().any(|point| {
        let x = point[0] - body.position[0];
        let y = point[1] - body.position[1];
        let products = [x * cosine, y * sine, x * sine, y * cosine];
        (products[0] - products[1]).abs() <= body.geometry.major
            && (products[2] + products[3]).abs() <= minor
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn circle_box_keeps_original_square_approximation() {
        let box_geometry = Geometry {
            is_small: false,
            shape: Shape::Box,
            major: 1.0,
            minor: 1.0,
            height: 1.0,
        };
        let circle = Geometry {
            is_small: false,
            shape: Shape::Cylinder,
            major: 1.0,
            minor: 1.0,
            height: 1.0,
        };
        let rectangle = Body {
            geometry: &box_geometry,
            position: [0.0; 3],
            angle: 0.0,
        };
        let circle = Body {
            geometry: &circle,
            position: [1.8, 1.8, 0.0],
            angle: 0.0,
        };
        // The mathematical circle misses the box corner; native bounding square hits.
        assert!(intersects(&rectangle, &circle));
        assert!(intersects(&circle, &rectangle));
    }

    #[test]
    fn crossed_box_edges_need_a_contained_corner_unless_centers_are_close() {
        let geometry = Geometry {
            is_small: false,
            shape: Shape::Box,
            major: 3.0,
            minor: 0.2,
            height: 1.0,
        };
        let horizontal = Body {
            geometry: &geometry,
            position: [0.0; 3],
            angle: 0.0,
        };
        let mut vertical = Body {
            geometry: &geometry,
            position: [1.0, 0.0, 0.0],
            angle: FRAC_PI_2,
        };
        assert!(!intersects(&horizontal, &vertical));
        vertical.position[0] = 0.19;
        assert!(intersects(&horizontal, &vertical));
        vertical.position[0] = 0.2;
        assert!(!intersects(&horizontal, &vertical));
    }

    #[test]
    fn sphere_uses_horizontal_slice_at_nonsphere_top_and_bottom() {
        let sphere = Geometry {
            is_small: false,
            shape: Shape::Sphere,
            major: 2.0,
            minor: 2.0,
            height: 2.0,
        };
        let cylinder = Geometry {
            is_small: false,
            shape: Shape::Cylinder,
            major: 1.0,
            minor: 1.0,
            height: 1.0,
        };
        let other = Body {
            geometry: &cylinder,
            position: [0.0; 3],
            angle: 0.0,
        };
        let mut ball = Body {
            geometry: &sphere,
            position: [2.5, 0.0, 2.0],
            angle: 0.0,
        };
        assert!(intersects(&ball, &other));
        ball.position[2] = 3.0;
        assert!(!intersects(&ball, &other));
        ball.position = [1.0, 0.0, -2.0];
        assert!(intersects(&ball, &other));
        ball.position[2] = -2.001;
        assert!(!intersects(&ball, &other));
    }
}
