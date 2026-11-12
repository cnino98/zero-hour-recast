//! SH TerrainLogic.cpp2619, W3DTerrainVisual.cpp894 and `WWMath` tri.h101.
use super::{Skirmish, TerrainHeights};
use crate::collision_geometry::{Geometry, Shape};
use num_traits::ToPrimitive;

#[cfg(test)]
mod tests;

impl Skirmish {
    /// Revision of the authoritative raw vertices, for terrain rendering.
    #[must_use]
    pub fn terrain_height_revision(&self) -> u64 {
        self.projectile_terrain
            .as_ref()
            .map_or(0, super::projectiles::Terrain::revision)
    }

    /// Original raw vertices after gameplay deformation, in native map units.
    #[must_use]
    pub fn terrain_height_map(&self) -> Option<TerrainHeights> {
        self.projectile_terrain
            .as_ref()
            .map(super::projectiles::Terrain::height_map)
    }

    pub(in crate::skirmish) fn flatten_object_terrain(&mut self, id: usize) {
        let geometry = self.body_geometry(id);
        if geometry.is_small {
            return;
        }
        let position = self.entities[id].position;
        let points = footprint(geometry, position, self.entities[id].facing);
        let box_shape = matches!(geometry.shape, Shape::Box);
        let Some(terrain) = self.projectile_terrain.as_mut() else {
            // An absent heightmap is the simulation's uniform zero terrain.
            return;
        };
        if points.is_empty() {
            return;
        }
        let mut total = 0.0;
        for &point in &points {
            total += terrain.sample(
                point.map(|value| value.to_f32().expect("native terrain coordinate") * 10.0),
            );
        }
        let count = i32::try_from(points.len()).expect("native terrain sample count");
        let average = total / count.to_f32().expect("Int sample count to Real");
        let mut raw = (0.5 + average / 0.625)
            .floor()
            .to_i32()
            .expect("native raw terrain height");
        if box_shape {
            let center = (terrain.sample(position) / 0.625)
                .floor()
                .to_i32()
                .expect("native center height");
            raw = raw.min(center);
        }
        for [x, y] in points {
            for [dx, dy] in [
                [0, 0],
                [-1, 0],
                [1, 0],
                [0, -1],
                [0, 1],
                [-1, -1],
                [1, 1],
                [1, -1],
                [-1, 1],
            ] {
                terrain.lower_raw_height([x + dx, y + dy], raw);
            }
        }
        // These heights stand in for native live terrain queries. Cell types
        // and the native init-only partition extrema cache are retained.
        self.movement_terrain
            .set_ground_heights(|point| terrain.sample(point));
    }
}

fn footprint(geometry: &Geometry, position: [f32; 2], angle: f32) -> Vec<[i32; 2]> {
    let corners = box_corners(position, [geometry.major, geometry.minor], angle);
    let bounds = if matches!(geometry.shape, Shape::Box) {
        std::array::from_fn(|axis| {
            let mut low = corners[0][axis];
            let mut high = low;
            for corner in &corners[1..] {
                low = low.min(corner[axis]);
                high = high.max(corner[axis]);
            }
            [low, high]
        })
    } else {
        position.map(|coordinate| [coordinate - geometry.major, coordinate + geometry.major])
    };
    let bounds = bounds.map(|axis| {
        axis.map(|value| {
            (value / 10.0)
                .floor()
                .to_i32()
                .expect("native footprint grid coordinate")
        })
    });
    let radius_squared = geometry.major * geometry.major;
    let mut points = Vec::new();
    // CRC=0 includes negative rows. Preserve the native X-outer/Y-inner sum order.
    for x in bounds[0][0]..=bounds[0][1] {
        for y in bounds[1][0]..=bounds[1][1] {
            let point =
                [x, y].map(|value| value.to_f32().expect("native terrain coordinate") * 10.0);
            let covered = if matches!(geometry.shape, Shape::Box) {
                triangle_contains([corners[0], corners[1], corners[3]], point)
                    || triangle_contains([corners[1], corners[2], corners[3]], point)
            } else {
                let dx = point[0] - position[0];
                let dy = point[1] - position[1];
                let y_squared = dy * dy;
                dx * dx + y_squared < radius_squared
            };
            if covered {
                points.push([x, y]);
            }
        }
    }
    points
}

fn box_corners([x, y]: [f32; 2], [major, minor]: [f32; 2], angle: f32) -> [[f32; 2]; 4] {
    let cosine = f64::from(angle).cos().to_f32().expect("native Cos");
    let sine = f64::from(angle).sin().to_f32().expect("native Sin");
    let major_cosine = major * cosine;
    let major_sine = major * sine;
    let minor_cosine = minor * cosine;
    let minor_sine = minor * sine;
    [
        [x - major_cosine - minor_sine, y + minor_cosine - major_sine],
        [x + major_cosine - minor_sine, y + minor_cosine + major_sine],
        [x + major_cosine + minor_sine, y - minor_cosine + major_sine],
        [x - major_cosine + minor_sine, y - minor_cosine - major_sine],
    ]
}

fn difference(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn perpendicular(a: [f32; 2], b: [f32; 2]) -> f32 {
    let second = a[1] * b[0];
    let first = a[0] * b[1];
    first - second
}
fn squared_length(point: [f32; 2]) -> f32 {
    let squared = point.map(|value| value * value);
    squared[0] + squared[1]
}

fn triangle_contains(points: [[f32; 2]; 3], point: [f32; 2]) -> bool {
    let edges = [
        difference(points[1], points[0]),
        difference(points[2], points[1]),
        difference(points[0], points[2]),
    ];
    let orientation = perpendicular(edges[0], difference(points[2], points[0]));
    if orientation != 0.0 {
        let side = if orientation > 0.0 { 1.0 } else { -1.0 };
        return (0..3).all(|index| {
            perpendicular(edges[index], difference(point, points[index])) * side >= 0.0
        });
    }
    // Preserve tri.h's repeated p1p2 length for the degenerate p2p0 case.
    let first = squared_length(edges[0]);
    let second = squared_length(edges[1]);
    let (edge, delta, maximum) = if first > second {
        (edges[0], difference(point, points[0]), first)
    } else {
        (edges[2], difference(point, points[2]), second)
    };
    if maximum == 0.0 {
        return squared_length(delta) == 0.0;
    }
    perpendicular(edge, delta) == 0.0
        && squared_length(delta) <= maximum
        && squared_length(difference(delta, edge)) <= maximum
}
