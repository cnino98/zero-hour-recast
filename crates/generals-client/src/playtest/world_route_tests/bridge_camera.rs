//! Actual paid bridge arrival, original tank geometry and phase-specific camera.
use crate::{
    playtest::{CameraRig, Match},
    terrain::Terrain,
};
use bevy::{camera::CameraProjection, prelude::*};

pub(super) fn assert_far_bank(app: &mut App, battle: Match, tank: usize) {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let settings = crate::world_data::camera_settings(&assets, root).unwrap();
    let name = &battle.simulation.catalog[battle.simulation.entities[tank].type_index].name;
    assert_eq!(name, "AmericaTankCrusader");
    let (files, _) =
        crate::skirmish_data::source_files::effective(&assets, &[name.as_str()]).unwrap();
    let object = crate::skirmish_data::definition(&files, "Object", name).unwrap();
    let model = crate::skirmish_data::normal_model(&assets, object).unwrap();
    // Model::load applies each original HLOD base-pose pivot; normal_model also
    // hides idle muzzle flashes/headlights, matching the idle arrival body.
    let points: Vec<_> = model
        .meshes
        .iter()
        .filter(|mesh| mesh.attributes & 0x1000 == 0)
        .flat_map(|mesh| &mesh.positions)
        .map(|point| Vec3::new(point[0], point[2], -point[1]) * 0.1)
        .collect();
    assert!(points.len() > 100);
    let (position, basis) = battle.simulation.object_transform(tank);
    let actor = Mat4::from_cols(
        Vec4::new(basis[0][0], basis[2][0], -basis[1][0], 0.0),
        Vec4::new(basis[0][2], basis[2][2], -basis[1][2], 0.0),
        Vec4::new(-basis[0][1], -basis[2][1], basis[1][1], 0.0),
        Vec4::new(
            position[0] * 0.1,
            position[2] * 0.1,
            -position[1] * 0.1,
            1.0,
        ),
    );
    app.insert_resource(battle).insert_resource(CameraRig {
        focus: Vec2::ZERO,
        distance: settings.max_height,
        generation: 1,
        settings,
    });
    // The crossing camera remains on the actual bridge centre, even when this
    // already-proven paid actor has reached the far bank.
    crate::world_capture::test_camera_frame(app.world_mut(), 1);
    let old = projected(app, &points, actor);
    assert!(
        !contained(old),
        "old bridge-centre framing must reproduce far-bank clipping: {old:?}"
    );
    crate::world_capture::test_camera_frame(app.world_mut(), 3);
    let tracked = projected(app, &points, actor);
    assert!(
        contained(tracked),
        "phase3 full original tank bounds={tracked:?}; old={old:?}"
    );
    // PostUpdate inspection marks completion before capture can request its
    // asynchronous screenshot in a later Update. Keep the validated camera.
    let arrival_focus = app.world().resource::<CameraRig>().focus;
    for _ in 0..3 {
        crate::world_capture::test_camera_frame(app.world_mut(), 255);
        assert_eq!(app.world().resource::<CameraRig>().focus, arrival_focus);
        let terminal = projected(app, &points, actor);
        assert!(
            contained(terminal),
            "terminal capture lost original far-bank body: {terminal:?}"
        );
    }
}

fn projected(app: &App, points: &[Vec3], actor: Mat4) -> Rect {
    let rig = app.world().resource::<CameraRig>();
    let terrain = app.world().resource::<Terrain>();
    let logical = Vec2::new(1_800.0, 1_098.0 * rig.settings.viewport_height);
    let viewport = Vec2::new(
        1_800.0,
        (2_196.0 * rig.settings.viewport_height).floor() * 0.5,
    );
    let half_fov = (25.0_f32.to_radians().tan() / (logical.x / logical.y)).atan();
    let projection = PerspectiveProjection {
        fov: half_fov * 2.0,
        aspect_ratio: viewport.x / viewport.y,
        near: 1.0,
        far: 1_000.0,
        ..default()
    }
    .get_clip_from_view();
    let horizontal = rig.distance / rig.settings.pitch.tan();
    let bottom = rig.distance / (rig.settings.pitch + half_fov).tan();
    let extent = Vec2::new(f32::from(terrain.width), f32::from(terrain.height));
    let margin = ((horizontal - bottom).abs() * 0.85)
        .min(extent.x * 0.5)
        .min(extent.y * 0.5);
    let lower = Vec2::splat(margin);
    let upper = extent - lower;
    let mut point = rig.focus;
    if point.cmplt(lower).any() || point.cmpgt(upper).any() {
        point = point.clamp(
            (lower + Vec2::splat(0.01)).min(extent * 0.5),
            (upper - Vec2::splat(0.01)).max(extent * 0.5),
        );
    }
    let (sine, cosine) = rig.settings.yaw.sin_cos();
    let focus = Vec3::new(point.x, terrain.sample(point), -point.y);
    let camera = Transform::from_translation(
        focus + Vec3::new(horizontal * sine, rig.distance, horizontal * cosine),
    )
    .looking_at(focus, Vec3::Y);
    let clip = projection * camera.to_matrix().inverse() * actor;
    let mut bounds = Rect::from_corners(Vec2::splat(f32::INFINITY), Vec2::splat(f32::INFINITY));
    bounds.max = Vec2::splat(f32::NEG_INFINITY);
    for &point in points {
        let ndc = clip.project_point3(point);
        let pixel = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * viewport * 0.5;
        bounds.min = bounds.min.min(pixel);
        bounds.max = bounds.max.max(pixel);
    }
    bounds
}

fn contained(bounds: Rect) -> bool {
    bounds.min.cmpge(Vec2::splat(12.0)).all()
        && bounds.max.cmple(Vec2::new(1_788.0, 734.5)).all()
        && bounds.size().cmpge(Vec2::new(16.0, 12.0)).all()
}
