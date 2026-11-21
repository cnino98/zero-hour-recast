//! Actual original prepared/posed bridge parts through the capture camera system.
use super::super::{Check, frame, setup};
use crate::{
    model_render,
    playtest::{CameraRig, Match},
    playtest_view::{UnitVisual, WorldCamera},
    source_material::SourceMaterial,
    terrain::Terrain,
};
use bevy::{
    camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo},
    ecs::system::RunSystemOnce,
    mesh::VertexAttributeValues,
    prelude::*,
    window::{PrimaryWindow, WindowResolution},
};
use generals_assets::{
    map::MapData,
    w3d::{Model, animation::Clip},
};

struct Scene {
    app: App,
    model: Model,
    clip: Clip,
    bindings: Vec<(Entity, Handle<Mesh>, model_render::PartBinding)>,
    position: [f32; 3],
    basis: [[f32; 3]; 3],
    camera: Entity,
    id: usize,
    original: [f32; 3],
    rubble_speed: f32,
}

#[ignore = "requires original game installation"]
#[test]
fn installed_revealed_original_bridge_parts_fit_intact_rubble_and_terminal_capture() {
    let Scene {
        mut app,
        model,
        clip,
        bindings,
        position,
        basis,
        camera,
        id,
        original,
        rubble_speed,
    } = original_scene();
    let [original_max, original_pitch, original_yaw] = original;
    let mut old_clipped = false;
    for (phase, source_frame) in capture_snapshots(&mut app, &clip, rubble_speed) {
        if phase != 0 {
            sample_pose(&mut app, &model, &clip, &bindings, source_frame);
        }
        app.world_mut().resource_mut::<Check>().phase = phase;
        app.world_mut().run_system_once(frame).unwrap();
        let points = world_points(&app, &bindings);
        assert!(points.len() > 100);
        let rig = app.world().resource::<CameraRig>();
        let bounds = projected(
            rig,
            app.world().resource::<Terrain>(),
            app.world().get::<Camera>(camera).unwrap(),
            &points,
            rig.distance,
            app.world()
                .resource::<crate::particles::Particles>()
                .camera_shake(),
        );
        assert!(
            contained(bounds),
            "phase={phase} source_frame={source_frame} actual full model={bounds:?} distance={}",
            rig.distance
        );
        old_clipped |= !contained(projected(
            rig,
            app.world().resource::<Terrain>(),
            app.world().get::<Camera>(camera).unwrap(),
            &points,
            original_max,
            app.world()
                .resource::<crate::particles::Particles>()
                .camera_shake(),
        ));
        assert_eq!(rig.settings.max_height.to_bits(), original_max.to_bits());
        assert_eq!(rig.settings.pitch.to_bits(), original_pitch.to_bits());
        assert_eq!(rig.settings.yaw.to_bits(), original_yaw.to_bits());
        let (current_position, current_basis) = app
            .world()
            .resource::<Match>()
            .simulation
            .object_transform(id);
        assert_eq!(
            current_position.map(f32::to_bits),
            position.map(f32::to_bits)
        );
        assert_eq!(
            current_basis.map(|row| row.map(f32::to_bits)),
            basis.map(|row| row.map(f32::to_bits))
        );
        println!(
            "Original bridge phase={phase} frame={source_frame} vertices={} bounds={bounds:?} fit_distance={}",
            points.len(),
            rig.distance
        );
    }
    assert!(
        old_clipped,
        "old native-max focus must reproduce actual original-model clipping"
    );
    sample_pose(&mut app, &model, &clip, &bindings, 0.0);
    let points = world_points(&app, &bindings);
    verify_shaken_fit(&mut app, camera, &points);
    app.world_mut().resource_mut::<Check>().phase = 3;
    app.world_mut().run_system_once(frame).unwrap();
    assert_eq!(
        app.world().resource::<CameraRig>().distance.to_bits(),
        original_max.to_bits(),
        "hydrant view must retain native distance"
    );
}

fn verify_shaken_fit(app: &mut App, camera: Entity, points: &[Vec3]) {
    app.world_mut().resource_mut::<Check>().phase = 11;
    app.world_mut().run_system_once(frame).unwrap();
    let unshaken_distance = app.world().resource::<CameraRig>().distance;
    let mut old_clipped = false;
    for angle in [
        std::f32::consts::FRAC_PI_4,
        -3.0 * std::f32::consts::FRAC_PI_4,
    ] {
        let focus = app.world().resource::<CameraRig>().focus;
        app.world_mut()
            .resource_mut::<crate::particles::Particles>()
            .test_normal_camera_impulse(focus, angle);
        let shake = app
            .world()
            .resource::<crate::particles::Particles>()
            .camera_shake();
        assert!(shake.length_squared() > 0.0);
        app.world_mut().run_system_once(frame).unwrap();
        let rig = app.world().resource::<CameraRig>();
        let terrain = app.world().resource::<Terrain>();
        let camera = app.world().get::<Camera>(camera).unwrap();
        let bounds = projected(rig, terrain, camera, points, rig.distance, shake);
        assert!(
            contained(bounds),
            "source NORMAL shake={shake:?} bounds={bounds:?}"
        );
        old_clipped |= !contained(projected(
            rig,
            terrain,
            camera,
            points,
            unshaken_distance,
            shake,
        ));
        assert_eq!(
            app.world()
                .resource::<crate::particles::Particles>()
                .camera_shake()
                .to_array()
                .map(f32::to_bits),
            shake.to_array().map(f32::to_bits),
            "capture fitting must not consume or alter source shake",
        );
    }
    assert!(
        old_clipped,
        "unshaken fit must expose the actual camera mismatch"
    );
}

// Native capture observes death while paused, then the first >90-tick radar
// refresh. It never requests the ONCE clip's fully buried final frame.
fn capture_snapshots(app: &mut App, clip: &Clip, speed: f32) -> Vec<(u8, f32)> {
    let mut snapshots = vec![(0, 0.0)];
    app.world_mut().resource_mut::<Check>().phase = 6;
    for _ in 0..18000 {
        advance_scenario(app);
        if app.world().resource::<Check>().phase == 11 {
            break;
        }
        render_tick(app);
    }
    assert_eq!(app.world().resource::<Check>().phase, 11);
    assert!(app.world().resource::<Time<Virtual>>().is_paused());
    let death = app.world().resource::<Check>().destroyed.unwrap();
    assert!(
        !app.world_mut()
            .resource_mut::<Match>()
            .simulation
            .take_radar_terrain_refresh()
    );
    let death_ms = app
        .world()
        .resource::<crate::model_animation::Clock>()
        .milliseconds();
    for _ in 0..2 {
        render_tick(app);
        assert_eq!(app.world().resource::<Match>().simulation.tick, death);
        assert_eq!(
            app.world()
                .resource::<crate::model_animation::Clock>()
                .milliseconds(),
            death_ms
        );
        snapshots.push((11, 0.0));
    }
    // inspect_bridge advances 11 -> 12 only after the original body fits.
    app.world_mut().resource_mut::<Check>().phase = 12;
    advance_scenario(app);
    assert_eq!(app.world().resource::<Check>().phase, 13);
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    let mut source_frame = 0.0;
    let mut previous_ms = death_ms;
    while app.world().resource::<Match>().simulation.tick - death <= 90 {
        render_tick(app);
        let now = app
            .world()
            .resource::<crate::model_animation::Clock>()
            .milliseconds();
        let elapsed = f32::from(u16::try_from(now - previous_ms).unwrap());
        let animation_ms = f32::from(clip.frame_rate) * speed * elapsed;
        let delta = animation_ms * 0.001;
        source_frame += delta;
        previous_ms = now;
    }
    assert_eq!(app.world().resource::<Match>().simulation.tick - death, 91);
    assert!(
        app.world_mut()
            .resource_mut::<Match>()
            .simulation
            .take_radar_terrain_refresh()
    );
    assert!(source_frame > 0.0 && source_frame < f32::from(clip.frame_count - 1));
    snapshots.push((13, source_frame));
    // The successful phase13 observer pauses before requesting its asynchronous
    // screenshot; phase255 must retain exactly that clock and fitted pose.
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.world_mut().resource_mut::<Check>().phase = 255;
    for _ in 0..2 {
        render_tick(app);
        assert_eq!(app.world().resource::<Match>().simulation.tick - death, 91);
        assert_eq!(
            app.world()
                .resource::<crate::model_animation::Clock>()
                .milliseconds(),
            previous_ms
        );
        snapshots.push((255, source_frame));
    }
    snapshots
}

fn advance_scenario(app: &mut App) {
    let mut check = app.world_mut().remove_resource::<Check>().unwrap();
    let mut time = app.world_mut().remove_resource::<Time<Virtual>>().unwrap();
    super::super::advance_scenario(
        &mut check,
        &mut app.world_mut().resource_mut::<Match>().simulation,
        &mut time,
    )
    .unwrap();
    app.insert_resource(check);
    app.insert_resource(time);
}

fn render_tick(app: &mut App) {
    if !app.world().resource::<Time<Virtual>>().is_paused() {
        app.world_mut().resource_mut::<Match>().simulation.step();
    }
    let step = app.world().resource::<Time<Fixed>>().timestep();
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(step);
    app.world_mut()
        .run_system_once(crate::model_animation::update_clock)
        .unwrap();
}

fn rubble_speed(lines: &[generals_assets::content::ContentLine]) -> f32 {
    let draws = crate::skirmish_data::blocks(lines, 0, "Draw").unwrap();
    let states = crate::skirmish_data::blocks(draws[0], 1, "ConditionState").unwrap();
    let rubble = states
        .into_iter()
        .find(|state| state[0].tokens[1..] == ["RUBBLE"])
        .unwrap();
    let mode = crate::skirmish_data::field(rubble, 2, "AnimationMode").unwrap();
    assert_eq!(crate::skirmish_data::scalar(mode).unwrap(), "ONCE");
    let flags = crate::skirmish_data::field(rubble, 2, "Flags").unwrap();
    assert!(flags.tokens.iter().any(|flag| flag == "START_FRAME_FIRST"));
    let range = crate::skirmish_data::field(rubble, 2, "AnimationSpeedFactorRange").unwrap();
    let speed: [f32; 2] = [
        range.tokens[1].parse().unwrap(),
        range.tokens[2].parse().unwrap(),
    ];
    assert_eq!(speed.map(f32::to_bits), [0.5_f32.to_bits(); 2]);
    speed[0]
}

fn original_scene() -> Scene {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut game = super::original_game();
    let setup = setup::prepare(&mut game).unwrap();
    assert!(matches!(
        game.visibility_status(0, setup.bridge),
        generals_sim::skirmish::visibility::Status::Clear
            | generals_sim::skirmish::visibility::Status::Partial
    ));
    let id = setup.bridge;
    let (position, basis) = game.object_transform(id);
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
    let name = &game.catalog[game.entities[id].type_index].name;
    assert_eq!(name, "MonumentTrainBridge");
    let (files, _) =
        crate::skirmish_data::source_files::effective(&assets, &[name.as_str()]).unwrap();
    let object = crate::skirmish_data::definition(&files, "Object", name).unwrap();
    let rubble_speed = rubble_speed(&object.lines);
    let model = crate::skirmish_data::normal_model(&assets, object).unwrap();
    let parts = model_render::parts(&model).unwrap();
    let clip = Clip::load(&assets.read("art/w3d/cmtbridge_a1.w3d").unwrap().bytes)
        .unwrap()
        .unwrap();
    let settings = crate::world_data::camera_settings(&assets, root).unwrap();
    let original_max = settings.max_height;
    let original_pitch = settings.pitch;
    let original_yaw = settings.yaw;
    let map = MapData::parse(&assets.read(super::super::map_name()).unwrap().bytes).unwrap();
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let ui = crate::control_bar::load(&assets, &game.catalog, &root.join("UserData/Options.ini"))
        .unwrap();
    let area = ui.schemes[0].area;
    let mut app = App::new();
    app.insert_resource(crate::particles::load(&assets, &game.catalog).unwrap());
    install_match(&mut app, game, setup, settings, terrain);
    let camera = install_camera(&mut app, area);
    let bindings = install_parts(&mut app, parts, actor, id);
    Scene {
        app,
        model,
        clip,
        bindings,
        position,
        basis,
        camera,
        id,
        original: [original_max, original_pitch, original_yaw],
        rubble_speed,
    }
}

fn install_match(
    app: &mut App,
    game: generals_sim::skirmish::Skirmish,
    setup: setup::Setup,
    settings: crate::world_data::CameraSettings,
    terrain: Terrain,
) {
    app.add_message::<AppExit>();
    app.insert_resource(CameraRig {
        focus: Vec2::ZERO,
        distance: settings.max_height,
        generation: 1,
        settings,
    });
    app.insert_resource(terrain);
    app.insert_resource(Match {
        observer_player: None,
        observer_players: Vec::new(),
        simulation: game,
        selected: Vec::new(),
        logic_selection: Vec::new(),
        commands: crate::input::commands::Queue::default(),
        placing: None,
        started: true,
        paused: false,
        promotions_open: false,
        generation: 1,
        opponent_difficulty: None,
        opponent_faction: generals_sim::skirmish::Faction::China,
        game_speed: 30,
    });
    app.insert_resource(Check {
        setup,
        phase: 0,
        started: None,
        water: None,
        destroyed: None,
        radar: Vec::new(),
        tail_seen: false,
        emitter_id: None,
        waiting: None,
    });
    app.insert_resource(Time::<Virtual>::default());
    app.insert_resource(Time::<Fixed>::from_hz(30.0));
    app.insert_resource(crate::model_animation::Clock::new());
    app.insert_resource(Assets::<Mesh>::default());
    app.insert_resource(Assets::<SourceMaterial>::default());
}

fn install_camera(app: &mut App, area: Rect) -> Entity {
    let window = Window {
        resolution: WindowResolution::new(3600, 2196).with_scale_factor_override(2.0),
        ..default()
    };
    let projection = PerspectiveProjection {
        fov: 2.0 * (25.0_f32.to_radians().tan() / (1800.0 / (1098.0 * 0.8))).atan(),
        aspect_ratio: 1800.0 / 878.0,
        near: 1.0,
        far: 1000.0,
        ..default()
    }
    .get_clip_from_view();
    let computed = ComputedCameraValues {
        clip_from_view: projection,
        target_info: Some(RenderTargetInfo {
            physical_size: window.resolution.physical_size(),
            scale_factor: window.scale_factor(),
        }),
        ..default()
    };
    let camera = app
        .world_mut()
        .spawn((
            Camera {
                viewport: Some(bevy::camera::Viewport {
                    physical_position: UVec2::ZERO,
                    physical_size: UVec2::new(3600, 1756),
                    depth: 0.0..1.0,
                }),
                computed,
                ..default()
            },
            GlobalTransform::default(),
            WorldCamera,
        ))
        .id();
    app.world_mut().spawn((window, PrimaryWindow));
    let scale = Vec2::new(3600.0 / 800.0, 2196.0 / 600.0);
    // Native UI layout rounds physical edges before deriving size and center.
    let minimum = (area.min * scale).round();
    let maximum = (area.max * scale).round();
    app.world_mut().spawn((
        crate::playtest_ui::Picture::Bar,
        ComputedNode {
            size: maximum - minimum,
            ..default()
        },
        UiGlobalTransform::from_translation((minimum + maximum) * 0.5),
        InheritedVisibility::VISIBLE,
    ));
    camera
}

fn install_parts(
    app: &mut App,
    parts: Vec<model_render::Part>,
    actor: Mat4,
    id: usize,
) -> Vec<(Entity, Handle<Mesh>, model_render::PartBinding)> {
    let render_root = app
        .world_mut()
        .spawn((
            UnitVisual {
                index: id,
                generation: 1,
            },
            GlobalTransform::from(actor),
        ))
        .id();
    let mut bindings = Vec::new();
    for mut part in parts {
        if part.material.parameters.opacity <= 0.0 {
            continue;
        }
        let binding = part.binding.take().unwrap();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(part.mesh);
        let material = app
            .world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .add(part.material);
        let entity = app
            .world_mut()
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material),
                GlobalTransform::from(actor),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        app.world_mut().entity_mut(render_root).add_child(entity);
        bindings.push((entity, mesh, binding));
    }
    assert!(!bindings.is_empty());
    bindings
}

fn sample_pose(
    app: &mut App,
    model: &Model,
    clip: &Clip,
    bindings: &[(Entity, Handle<Mesh>, model_render::PartBinding)],
    source_frame: f32,
) {
    let pose = model.pose(clip, source_frame).unwrap();
    for (entity, mesh, binding) in bindings {
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        model_render::deform(&mut meshes.get_mut(mesh).unwrap(), binding, &pose).unwrap();
        app.world_mut()
            .entity_mut(*entity)
            .insert(if pose[binding.mesh].visible {
                InheritedVisibility::VISIBLE
            } else {
                InheritedVisibility::HIDDEN
            });
    }
}

fn world_points(
    app: &App,
    bindings: &[(Entity, Handle<Mesh>, model_render::PartBinding)],
) -> Vec<Vec3> {
    let mut points = Vec::new();
    for (entity, mesh, _) in bindings {
        if !app
            .world()
            .get::<InheritedVisibility>(*entity)
            .unwrap()
            .get()
        {
            continue;
        }
        let mesh = app.world().resource::<Assets<Mesh>>().get(mesh).unwrap();
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("source positions missing");
        };
        let world = app.world().get::<GlobalTransform>(*entity).unwrap();
        points.extend(
            vertices
                .iter()
                .map(|point| world.transform_point(Vec3::from_array(*point))),
        );
    }
    points
}
fn projected(
    rig: &CameraRig,
    terrain: &Terrain,
    camera: &Camera,
    points: &[Vec3],
    distance: f32,
    shake: Vec2,
) -> Rect {
    let horizontal = distance / rig.settings.pitch.tan();
    let half_fov =
        (25.0_f32.to_radians().tan() / (1800.0 / (1098.0 * rig.settings.viewport_height))).atan();
    let margin = ((horizontal - distance / (rig.settings.pitch + half_fov).tan()).abs() * 0.85)
        .min(f32::from(terrain.width) * 0.5)
        .min(f32::from(terrain.height) * 0.5);
    assert!(
        rig.focus.cmpge(Vec2::splat(margin)).all()
            && rig
                .focus
                .cmple(
                    Vec2::new(f32::from(terrain.width), f32::from(terrain.height))
                        - Vec2::splat(margin)
                )
                .all(),
        "original bridge camera must remain unclamped: distance={distance} focus={:?} margin={margin} extent=({}, {}) world_min={:?} world_max={:?} vertices={}",
        rig.focus,
        terrain.width,
        terrain.height,
        points.iter().copied().reduce(Vec3::min),
        points.iter().copied().reduce(Vec3::max),
        points.len(),
    );
    let (sine, cosine) = rig.settings.yaw.sin_cos();
    let shaken = rig.focus + shake;
    let focus = Vec3::new(shaken.x, terrain.sample(rig.focus), -shaken.y);
    let transform = Transform::from_translation(
        focus + Vec3::new(horizontal * sine, distance, horizontal * cosine),
    )
    .looking_at(focus, Vec3::Y);
    let world = GlobalTransform::from(transform);
    let mut bounds = Rect {
        min: Vec2::splat(f32::INFINITY),
        max: Vec2::splat(f32::NEG_INFINITY),
    };
    for point in points {
        let pixel = camera.world_to_viewport(&world, *point).unwrap();
        bounds.min = bounds.min.min(pixel);
        bounds.max = bounds.max.max(pixel);
    }
    bounds
}
fn contained(bounds: Rect) -> bool {
    bounds.min.cmpge(Vec2::splat(12.0)).all()
        && bounds.max.cmple(Vec2::new(1788.0, 734.5)).all()
        && bounds.size().cmpge(Vec2::new(16.0, 12.0)).all()
}
