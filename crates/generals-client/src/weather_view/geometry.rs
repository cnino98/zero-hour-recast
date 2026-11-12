//! `W3DSnow` camera-relative lattice and D3D8 point-size attenuation.
use super::data::Settings;
use crate::water_view::geometry::fast_sin;
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use num_traits::ToPrimitive;

pub(super) fn noise(dimension: f32, seed: &mut u32) -> Vec<f32> {
    let dimension = dimension.to_u32().expect("validated snow cube");
    (0..4096)
        .map(|_| {
            // Original Windows CRT rand(), independent of GameLogicRandomValue.
            *seed = seed.wrapping_mul(214_013).wrapping_add(2_531_011);
            (((*seed >> 16) & 0x7fff) % dimension)
                .to_f32()
                .expect("snow starting height")
        })
        .collect()
}

pub(super) fn mesh(
    settings: &Settings,
    noise: &[f32],
    time: f32,
    camera: &Transform,
    projection: Mat4,
    viewport: Vec2,
) -> Mesh {
    let camera_native = Vec3::new(
        camera.translation.x,
        -camera.translation.z,
        camera.translation.y,
    ) * 10.0;
    let spacing = 1.0 / settings.density;
    let half = (settings.dimension / spacing * 0.5)
        .floor()
        .to_i32()
        .expect("weather half extent");
    let center = (camera_native.truncate() / spacing).floor().as_ivec2();
    let mut lattice = Vec::new();
    let minimum = [center.x - half, center.y - half];
    let maximum = [center.x + half, center.y + half];
    if settings.points {
        points_in_box(minimum, maximum, &mut lattice);
    } else {
        for y in minimum[1]..maximum[1] {
            for x in minimum[0]..maximum[0] {
                lattice.push([x, y]);
            }
        }
    }
    let world = camera.compute_affine();
    let inverse = world.inverse();
    let half_height = settings.dimension * 0.5;
    let ceiling = camera_native.z + half_height;
    let fall = time * settings.velocity;
    let traveled = fall + camera_native.z % settings.dimension;
    let mut positions = Vec::new();
    let mut uv = Vec::new();
    let mut indices = Vec::new();
    for [x, y] in lattice {
        let index = usize::try_from((x + 100_000) & 63).expect("noise column")
            + usize::try_from((y + 100_000) & 63).expect("noise row") * 64;
        let height = ceiling - (traveled + noise[index]) % settings.dimension;
        let sx = x.to_f32().expect("snow lattice x");
        let sy = y.to_f32().expect("snow lattice y");
        let drift = Vec2::new(
            settings.amplitude * fast_sin(height * settings.frequency[0] + sx),
            settings.amplitude * fast_sin(height * settings.frequency[1] + sy),
        );
        let mut point = Vec3::new(sx * spacing, height, -sy * spacing) * 0.1;
        if settings.points {
            point += Vec3::new(drift.x, 0.0, -drift.y) * 0.1;
        }
        let mut view = inverse.transform_point3(point);
        if !settings.points {
            let horizontal = drift.x * 0.1;
            let vertical = drift.y * 0.1;
            view.x += horizontal;
            view.y += vertical;
        }
        let clip = projection * view.extend(1.0);
        if clip.w <= 0.0 || view.z >= 0.0 || settings.points && (clip.z < 0.0 || clip.z > clip.w) {
            continue;
        }
        let size = if settings.points {
            let pixels = point_pixels(settings, viewport.y, view.length() * 10.0);
            Vec2::new(
                pixels * clip.w / (viewport.x * projection.x_axis.x),
                pixels * clip.w / (viewport.y * projection.y_axis.y),
            )
        } else {
            Vec2::splat(settings.quad_size * 0.05)
        };
        let base = u32::try_from(positions.len()).expect("weather vertices");
        for corner in [
            Vec2::new(-1.0, 1.0),
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
        ] {
            positions.push(
                world
                    .transform_point3(view + (corner * size).extend(0.0))
                    .to_array(),
            );
        }
        uv.extend([[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        indices.extend([3, 0, 2, 2, 0, 1].map(|index| base + index));
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(indices))
}

pub(super) fn point_pixels(settings: &Settings, height: f32, distance: f32) -> f32 {
    (height * settings.point_size / distance)
        .clamp(settings.point_bounds[0], settings.point_bounds[1])
}

fn points_in_box(minimum: [i32; 2], maximum: [i32; 2], result: &mut Vec<[i32; 2]>) {
    let width = maximum[0] - minimum[0];
    let height = maximum[1] - minimum[1];
    let middle = [minimum[0] + (width + 1) / 2, minimum[1] + (height + 1) / 2];
    if width > 45 {
        if height > 45 {
            points_in_box([minimum[0], middle[1]], [middle[0], maximum[1]], result);
            points_in_box(middle, maximum, result);
            points_in_box(minimum, middle, result);
            points_in_box([middle[0], minimum[1]], [maximum[0], middle[1]], result);
        } else {
            points_in_box(minimum, [middle[0], maximum[1]], result);
            points_in_box([middle[0], minimum[1]], maximum, result);
        }
    } else if height > 45 {
        points_in_box([minimum[0], middle[1]], maximum, result);
        points_in_box(minimum, [maximum[0], middle[1]], result);
    } else {
        for y in minimum[1]..maximum[1] {
            for x in minimum[0]..maximum[0] {
                result.push([x, y]);
            }
        }
    }
}

pub(super) fn screen() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0; 2]; 4])
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}
