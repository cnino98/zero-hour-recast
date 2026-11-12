//! `W3DWater::renderWater`, drawTrapezoidWater and drawRiverWater vertex order.
use super::data::Settings;
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use generals_assets::map::PolygonTrigger;
use num_traits::ToPrimitive;
use std::error::Error;

pub(super) struct Polygon {
    pub points: Vec<Vec3>,
    pub river: Option<usize>,
}

pub(super) fn polygons(triggers: &[PolygonTrigger]) -> Result<Vec<Polygon>, Box<dyn Error>> {
    triggers
        .iter()
        .filter(|trigger| trigger.water && trigger.points.len() > 2)
        .map(|trigger| {
            let points = trigger
                .points
                .iter()
                .map(|point| {
                    Vec3::from_array(
                        point.map(|value| value.to_f32().expect("source i32 converts to Real")),
                    )
                })
                .collect();
            let river = if trigger.river {
                let start = usize::try_from(trigger.river_start)?;
                if start >= trigger.points.len() - 1 || trigger.points.len() < 4 {
                    return Err("invalid source river start or point count".into());
                }
                if trigger.points[start][..2] == trigger.points[start + 1][..2] {
                    return Err("zero-width original river start".into());
                }
                Some(start)
            } else {
                None
            };
            Ok(Polygon { points, river })
        })
        .collect()
}

pub(super) struct Vertices {
    pub positions: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub uv_b: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}
impl Vertices {
    pub const fn new() -> Self {
        Self {
            positions: Vec::new(),
            uv: Vec::new(),
            uv_b: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
        }
    }
    pub fn vertex(&mut self, point: Vec3, uv: [f32; 2], uv_b: [f32; 2], color: [f32; 4]) {
        self.positions
            .push([point.x * 0.1, point.z * 0.1, -point.y * 0.1]);
        self.uv.push(uv);
        self.uv_b.push(uv_b);
        self.colors.push(color);
    }
    pub fn mesh(self) -> Mesh {
        let count = self.positions.len();
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, self.uv_b)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

pub(super) fn mesh(
    polygon: &Polygon,
    settings: &Settings,
    origin: f32,
    mut shroud: impl FnMut([f32; 2]) -> u8,
) -> Mesh {
    let mut vertices = Vertices::new();
    if let Some(start) = polygon.river {
        river(
            &mut vertices,
            &polygon.points,
            start,
            settings.diffuse,
            origin,
            &mut shroud,
        );
    } else {
        for k in (1..polygon.points.len() - 1).step_by(2) {
            let mut points = [
                polygon.points[(k + 2).min(polygon.points.len() - 1)],
                polygon.points[k + 1],
                polygon.points[k],
                polygon.points[0],
            ];
            for _ in 0..settings.feather.max(1) {
                trapezoid(&mut vertices, points, settings, origin);
                if settings.feather != 0 {
                    points[0].z += 4.0 / settings.feather.to_f32().expect("source feather count");
                }
            }
        }
    }
    vertices.mesh()
}

fn trapezoid(vertices: &mut Vertices, points: [Vec3; 4], settings: &Settings, time: f32) {
    let [origin, p1, p2, p3] = points;
    let u = p1 - origin;
    let v = p3 - origin;
    let u2 = p2 - p3;
    let v2 = p2 - p1;
    let count = |length: f32| {
        (length / 80.0)
            .trunc()
            .clamp(1.0, 50.0)
            .to_u16()
            .expect("bounded water subdivision")
    };
    let columns = count(u.length() + u2.length());
    let rows = count(v.length() + v2.length());
    let base = u32::try_from(vertices.positions.len()).expect("water vertex count");
    for y in 0..=rows {
        for x in 0..=columns {
            let du = f32::from(x) * (1.0 / f32::from(columns));
            let dv = f32::from(y) * (1.0 / f32::from(rows));
            let mut point = origin;
            point += u * du;
            point += v * dv;
            point += (dv * du) * (v2 - v);
            let mut color = settings.diffuse;
            let uv = if settings.feather == 0 {
                let horizontal_amplitude = 0.02 * (11.0 * time).cos();
                let vertical_amplitude = 0.02 * (5.0 * time).cos();
                let phase = 25.0 * time;
                let x_phase = point.x * (std::f32::consts::PI / 40.0);
                let y_phase = point.y * (std::f32::consts::PI / 40.0);
                let x_wave = horizontal_amplitude * fast_sin(phase + x_phase);
                let y_wave = vertical_amplitude * fast_sin(phase + y_phase);
                let x = point.x * (1.0 / 150.0);
                let y = point.y * (1.0 / 150.0);
                [x + x_wave, y + y_wave]
            } else {
                color[3] = match settings.feather {
                    1 => 255.0,
                    2 => 200.0,
                    3 => 140.0,
                    4 => 110.0,
                    5 => 80.0,
                    _ => 0.0,
                } / 255.0;
                let phase = 25.0 * time;
                let x_phase = point.x * (std::f32::consts::PI / 40.0);
                let wave = ((phase + x_phase).sin() - 1.0) * 0.5;
                point.z += wave;
                let x_wave = 0.02 * (11.0 * time).cos() * wave;
                let y_wave = 0.02 * (5.0 * time).cos() * wave;
                [point.x / 150.0 + x_wave, point.y / 150.0 + y_wave]
            };
            let shear = 0.3 * point.x;
            vertices.vertex(point, uv, [point.x / 50.0, (point.y + shear) / 50.0], color);
        }
    }
    let stride = u32::from(columns) + 1;
    for y in 0..u32::from(rows) {
        for x in 0..u32::from(columns) {
            let index = base + y * stride + x;
            vertices.indices.extend([
                index,
                index + stride + 1,
                index + stride,
                index,
                index + 1,
                index + stride + 1,
            ]);
        }
    }
}

fn river(
    vertices: &mut Vertices,
    points: &[Vec3],
    start: usize,
    diffuse: [f32; 4],
    time: f32,
    shroud: &mut impl FnMut([f32; 2]) -> u8,
) {
    let rectangles = points.len() / 2 - 1;
    let mut length = 0.0;
    let mut end = 0.0;
    for (index, pair) in points.windows(2).enumerate() {
        let delta = pair[0] - pair[1];
        let x_squared = delta.x * delta.x;
        let y_squared = delta.y * delta.y;
        let current = (x_squared + y_squared).sqrt();
        length += current;
        if index == start {
            end = current;
        }
    }
    assert!(end > 0.0, "validated nonzero river start width");
    let scale = (length * 0.5 - end) / end / rectangles.to_f32().expect("river segments");
    let mut inner = start;
    let mut outer = start + 1;
    for index in 0..=rectangles {
        let row = scale * index.to_f32().expect("river row");
        let phase = 2.0 * std::f32::consts::PI * row;
        let shift = 3.0 * time;
        let uv = -time + row + fast_sin(phase - shift) / 22.0;
        for (point, u, edge) in [(points[outer], 0.5, 1.0), (points[inner], 0.0, 0.0)] {
            let factor = f32::from(shroud([point.x, point.y])) / 255.0;
            let color = diffuse.map(|channel| (channel * 255.0 * factor).trunc() / 255.0);
            vertices.vertex(point, [u, uv], [edge, uv], color);
        }
        inner = if inner == 0 {
            points.len() - 1
        } else {
            inner - 1
        };
        outer = (outer + 1) % points.len();
    }
    for index in 0..u32::try_from(rectangles).expect("river segments") {
        let base = index * 2;
        vertices
            .indices
            .extend([base, base + 1, base + 3, base, base + 3, base + 2]);
    }
}

pub fn fast_sin(value: f32) -> f32 {
    let scaled = value * (1024.0 / (2.0 * std::f32::consts::PI));
    let base = scaled.floor();
    let fraction = scaled - base;
    let index = base.rem_euclid(1024.0).to_u16().expect("sine table index");
    let table = |index: u16| (f32::from(index) * 2.0 * std::f32::consts::PI / 1024.0).sin();
    let first = (1.0 - fraction) * table(index);
    let second = fraction * table((index + 1) % 1024);
    first + second
}
