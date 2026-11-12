//! Choose ordinary camera yaw/focus; retain the original height and field of view.
use super::Check;
use crate::{
    playtest::{CameraRig, Match},
    terrain::Terrain,
};
use bevy::prelude::*;

pub(super) fn frame(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let points: Vec<_> = check
        .setup
        .technicals
        .into_iter()
        .chain([check.setup.marauder, check.crate_id])
        .map(|id| {
            let [x, y, z] = game.object_position(id);
            Vec3::new(x, z, -y) * 0.1
        })
        .collect();
    let minimum = points
        .iter()
        .copied()
        .reduce(Vec3::min)
        .expect("prepared subjects");
    let maximum = points
        .iter()
        .copied()
        .reduce(Vec3::max)
        .expect("prepared subjects");
    let center = (minimum + maximum) * 0.5;
    let mut windows = app.world_mut().query::<&Window>();
    let window = windows
        .single(app.world())
        .expect("configured capture window");
    let rig = app.world().resource::<CameraRig>();
    let height = rig.settings.max_height;
    let pitch = rig.settings.pitch;
    let aspect = window.width() / (window.height() * rig.settings.viewport_height);
    let tangent = Vec2::new(
        25.0_f32.to_radians().tan(),
        25.0_f32.to_radians().tan() / aspect,
    );
    let horizontal = height / pitch.tan();
    let margin = (horizontal - height / (pitch + tangent.y.atan()).tan()).abs() * 0.85;
    let terrain = app.world().resource::<Terrain>();
    let extent = Vec2::new(f32::from(terrain.width), f32::from(terrain.height));
    let lower = Vec2::splat(margin.min(extent.x * 0.5).min(extent.y * 0.5) + 0.01);
    let mut best = (f32::INFINITY, rig.focus, rig.settings.yaw);
    for angle in 0..24_i16 {
        let yaw = f32::from(angle) * std::f32::consts::TAU / 24.0;
        let (sine, cosine) = yaw.sin_cos();
        let offset = Vec3::new(horizontal * sine, height, horizontal * cosine);
        for x in -4..=4_i16 {
            for y in -4..=4_i16 {
                let focus = (Vec2::new(center.x, -center.z)
                    + Vec2::new(f32::from(x), f32::from(y)) * 2.5)
                    .clamp(lower, extent - lower);
                let target = Vec3::new(focus.x, terrain.sample(focus), -focus.y);
                let inverse = Transform::from_translation(target + offset)
                    .looking_at(target, Vec3::Y)
                    .compute_affine()
                    .inverse();
                let score = points
                    .iter()
                    .map(|&point| {
                        let local = inverse.transform_point3(point);
                        if local.z >= 0.0 {
                            f32::INFINITY
                        } else {
                            (local.x / (-local.z * tangent.x))
                                .abs()
                                .max((local.y / (-local.z * tangent.y)).abs())
                        }
                    })
                    .fold(0.0_f32, f32::max);
                if score < best.0 {
                    best = (score, focus, yaw);
                }
            }
        }
    }
    let mut rig = app.world_mut().resource_mut::<CameraRig>();
    rig.focus = best.1;
    rig.settings.yaw = best.2;
    rig.distance = height;
    println!(
        "Technical capture framing: original height {height}, yaw {}, focus {:?}, maximum projected extent {}",
        best.2, best.1, best.0
    );
}
