//! Real paid Cairo delivery and original posed meshes; no GPU/pixel acceptance.
use super::{CameraRig, Match, ScreenCapture};
use crate::{model_animation, physical_debris, playtest_view, reinforcement_pad_capture};
use bevy::{
    camera::{ComputedCameraValues, RenderTargetInfo},
    ecs::system::RunSystemOnce,
    prelude::*,
    window::{PrimaryWindow, WindowResolution},
};
use generals_sim::skirmish::Faction;
use std::time::Duration;

#[ignore = "requires original game installation"]
#[test]
fn original_paid_pad_descent_rejects_plane_occlusion_then_accepts_source_separation() {
    let mut app = original_scene();
    let mut rejected_overlap = false;
    let mut accepted = false;
    // Existing original delivery regression bounds the complete cargo/chute/rally
    // lifecycle by3600 source frames. No wait changes the ordinary game's timing.
    for _ in 0..3600 {
        draw(&mut app);
        let (original_ready, ready) = reinforcement_pad_capture::test_descent(app.world_mut());
        if original_ready && !ready {
            rejected_overlap = true;
        }
        if ready {
            assert!(
                original_ready,
                "all original deployed/rider/body guards remain"
            );
            accepted = true;
            break;
        }
        app.world_mut().resource_mut::<Match>().simulation.step();
    }
    assert!(
        rejected_overlap,
        "the original immediate-open guard must admit an occluded frame"
    );
    assert!(
        accepted,
        "ordinary delivery must separate while its actual vehicle remains suspended"
    );
}

fn original_scene() -> App {
    let root = &generals_assets::game_root().unwrap();
    let mut app = App::new();
    app.add_plugins(TransformPlugin)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<crate::source_material::SourceMaterial>>()
        .init_resource::<Time>()
        .init_resource::<Time<Virtual>>()
        .insert_resource(Time::<Fixed>::from_hz(30.0))
        .insert_resource(model_animation::Clock::new())
        .init_resource::<crate::structure_view::Structures>();
    let settings = super::load_map(
        app.world_mut(),
        root,
        &ScreenCapture::ReinforcementPad(Faction::Usa),
        "Maps\\Cairo Commandos\\Cairo Commandos.map",
        false,
    )
    .unwrap();
    app.insert_resource(CameraRig {
        focus: Vec2::ZERO,
        distance: settings.max_height,
        generation: 0,
        settings,
    });
    let mut battle = app.world_mut().remove_resource::<Match>().unwrap();
    reinforcement_pad_capture::prepare(&mut app, &mut battle, Faction::Usa).unwrap();
    app.insert_resource(battle);
    app.world_mut()
        .run_system_once(playtest_view::setup)
        .unwrap();
    app.world_mut().spawn((
        Window {
            resolution: WindowResolution::new(3600, 2196).with_scale_factor_override(2.0),
            ..default()
        },
        PrimaryWindow,
    ));
    install_hud(&mut app);
    app
}

fn install_hud(app: &mut App) {
    let area = app.world().resource::<crate::control_bar::UiData>().schemes[0].area;
    // Same ControlBarScheme 800x600 scaling as actual playtest_ui::at, with
    // headless computed layout at the native r11 physical window dimensions.
    let scale = Vec2::new(3600.0 / 800.0, 2196.0 / 600.0);
    let size = area.size() * scale;
    let center = area.center() * scale;
    app.world_mut().spawn((
        crate::playtest_ui::Picture::Bar,
        ComputedNode { size, ..default() },
        UiGlobalTransform::from_translation(center),
        InheritedVisibility::VISIBLE,
    ));
}

fn draw(app: &mut App) {
    let world = app.world_mut();
    world.resource_mut::<Time<Virtual>>().unpause();
    world
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_secs_f64(1.0 / 30.0));
    world.run_system_once(playtest_view::render).unwrap();
    world.run_system_once(physical_debris::update).unwrap();
    world
        .run_system_once(model_animation::update_clock)
        .unwrap();
    world.run_system_once(model_animation::update).unwrap();
    reinforcement_pad_capture::test_descent_frame(world);
    world.run_system_once(super::update_camera).unwrap();
    world.run_schedule(PostUpdate);
    world.run_system_once(physical_debris::transform).unwrap();
    world.run_system_once(project_camera).unwrap();
    world.run_system_once(headless_visibility).unwrap();
}

fn project_camera(
    camera: Single<(&mut Camera, &mut Projection), With<playtest_view::WorldCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    let (mut camera, mut projection) = camera.into_inner();
    let window = window.into_inner();
    // CameraPlugin normally fills this from the actual render target. Keep its
    // real viewport, transform and projection supplied by update_camera above.
    let size = camera.viewport.as_ref().unwrap().physical_size.as_vec2();
    projection.update(size.x, size.y);
    camera.computed = ComputedCameraValues {
        clip_from_view: projection.get_clip_from_view(),
        target_info: Some(RenderTargetInfo {
            physical_size: window.resolution.physical_size(),
            scale_factor: window.scale_factor(),
        }),
        ..default()
    };
}

fn headless_visibility(
    roots: Query<(&Visibility, &Children), With<playtest_view::UnitVisual>>,
    mut parts: Query<
        (&Visibility, &mut InheritedVisibility, &mut ViewVisibility),
        Without<playtest_view::UnitVisual>,
    >,
) {
    // Device-free storage adapter only: every source-visible part is available
    // to project. Actual mesh bounds still must satisfy the unaltered viewport,
    // size and separation predicates. GPU occlusion requires fresh native PNGs.
    for (root, children) in &roots {
        for child in children {
            if let Ok((local, mut inherited, mut view)) = parts.get_mut(*child) {
                let visible = *root != Visibility::Hidden && *local != Visibility::Hidden;
                *inherited = if visible {
                    InheritedVisibility::VISIBLE
                } else {
                    InheritedVisibility::HIDDEN
                };
                *view = if visible {
                    ViewVisibility::VISIBLE
                } else {
                    ViewVisibility::HIDDEN
                };
            }
        }
    }
}
