//! Original falling model projection, separate from the mandatory native pixel gate.
use bevy::{
    camera::CameraProjection, ecs::system::RunSystemOnce, mesh::VertexAttributeValues, prelude::*,
};
use generals_assets::{map::MapData, w3d::Model};

#[ignore = "requires original game installation"]
#[test]
fn original_collapse_drawable_tracking_keeps_full_falling_mesh_above_hud() {
    let Scene {
        mut app,
        points,
        position,
        yaw,
    } = original_scene();
    let xy = [position[0], position[1]];
    let ground = position[2];
    // Native r9: logical1800x1098 at scale2; physical viewport truncates to1756.
    let settings = &app
        .world()
        .resource::<crate::playtest::CameraRig>()
        .settings;
    let source_size = Vec2::new(1_800.0, 1_098.0 * settings.viewport_height);
    let world_size = Vec2::new(1_800.0, (2_196.0 * settings.viewport_height).floor() * 0.5);
    let projection = PerspectiveProjection {
        fov: 2.0 * (25.0_f32.to_radians().tan() / (source_size.x / source_size.y)).atan(),
        aspect_ratio: world_size.x / world_size.y,
        near: 1.0,
        far: 1_000.0,
        ..default()
    }
    .get_clip_from_view();
    let mut old_clipped = false;
    // Real source collapse recurrence: gravity -64 / 30 / 30, damping0,
    // geometry height75. Inspect every callback lying in the unchanged20–80% band.
    let mut height = 0.0_f32;
    let mut velocity = 0.0_f32;
    let mut samples = 0;
    for _ in 0..50 {
        height -= velocity;
        velocity -= -64.0_f32 / 30.0 / 30.0;
        if !(-60.0..-15.0).contains(&height) {
            continue;
        }
        samples += 1;
        app.world_mut()
            .run_system_once_with(record_pose, height)
            .unwrap();
        // Exercise the actual changed system with its real generation-scoped
        // Structures offset. Reverting frame's tracking hunk must fail this test.
        app.world_mut().run_system_once(super::frame).unwrap();
        let terrain = app.world().resource::<crate::terrain::Terrain>();
        let rig = app.world().resource::<crate::playtest::CameraRig>();
        let target = app.world().resource::<super::Check>().setup.target;
        let offset = app
            .world()
            .resource::<crate::structure_view::Structures>()
            .offset(1, target);
        assert_eq!(offset[2].to_bits(), height.to_bits());
        let model = Transform::from_translation(Vec3::new(xy[0], ground, -xy[1]) * 0.1)
            .with_rotation(Quat::from_rotation_y(yaw))
            * Transform::from_translation(Vec3::new(offset[0], height, -offset[1]) * 0.1);
        let bounds = project(rig, terrain, projection, world_size, &points, model);
        assert!(
            contained(bounds),
            "source height={height}, actual tracked bounds={bounds:?}"
        );
        let terrain = app
            .world_mut()
            .remove_resource::<crate::terrain::Terrain>()
            .unwrap();
        let mut rig = app.world_mut().resource_mut::<crate::playtest::CameraRig>();
        // Keep the original frame's separately rounded height offset.
        let height_offset = 75.0_f32 * 0.3;
        crate::capture_framing::focus_on(
            &mut rig,
            &terrain,
            [xy[0], xy[1], ground + height_offset],
        );
        let old = project(&rig, &terrain, projection, world_size, &points, model);
        old_clipped |= !contained(old);
        app.insert_resource(terrain);
    }
    assert!(samples >= 20);
    assert!(
        old_clipped,
        "old collision focus must reproduce full-mesh HUD clipping"
    );
}

struct Scene {
    app: App,
    points: Vec<[f32; 3]>,
    position: [f32; 3],
    yaw: f32,
}

fn original_scene() -> Scene {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let root = &generals_assets::game_root().unwrap();
    let settings = crate::world_data::camera_settings(&assets, root).unwrap();
    let map = MapData::parse(&assets.read(super::map_name(false)).unwrap().bytes).unwrap();
    // This is the actual Golden Oasis placement from the native r9 diagnostic.
    let placed = map
        .objects
        .iter()
        .find(|object| {
            object.template == "MogadishuHighrise01"
                && (object.position[0] - 1_745.542).abs() < 0.01
                && (object.position[1] - 2_031.030_3).abs() < 0.01
        })
        .unwrap();
    let xy = [placed.position[0], placed.position[1]];
    let yaw = placed.angle;
    let terrain = crate::terrain::Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    // The original RUBBLE state uses E, with the same two actual material meshes
    // (60 and 900 submitted vertices) observed during r9, rather than pristine art.
    let model = Model::load(&assets, "CBMogdis01_E").unwrap();
    let parts = crate::model_render::parts(&model).unwrap();
    let counts: Vec<_> = parts
        .iter()
        .map(|part| part.mesh.count_vertices())
        .collect();
    assert_eq!(counts, [60, 900]);
    let points: Vec<_> = parts
        .iter()
        .flat_map(|part| {
            let Some(VertexAttributeValues::Float32x3(points)) =
                part.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("original collapse mesh lacks positions")
            };
            points.iter().copied()
        })
        .collect();
    let ground = terrain
        .sample(Vec2::from_array(xy) * 0.1)
        .mul_add(10.0, placed.position[2]);
    let rig = crate::playtest::CameraRig {
        focus: Vec2::ZERO,
        distance: settings.max_height,
        generation: 1,
        settings,
    };
    battle.simulation = crate::playtest::map_fixture::Fixture::load(root, super::map_name(false))
        .unwrap()
        .into_simulation()
        .unwrap();
    battle
        .simulation
        .reset(
            generals_sim::skirmish::Faction::Usa,
            generals_sim::skirmish::Faction::China,
        )
        .unwrap();
    battle.generation = 1;
    let target = battle
        .simulation
        .entities
        .iter()
        .position(|actor| {
            battle.simulation.catalog[actor.type_index].name == "MogadishuHighrise01"
                && (actor.position[0] - xy[0]).abs() < 0.01
                && (actor.position[1] - xy[1]).abs() < 0.01
        })
        .unwrap();
    let position = battle.simulation.object_position(target);
    assert_eq!(position[2].to_bits(), ground.to_bits());
    let app = camera_app(&assets, root, battle, target, terrain, rig);
    Scene {
        app,
        points,
        position,
        yaw,
    }
}

fn project(
    rig: &crate::playtest::CameraRig,
    terrain: &crate::terrain::Terrain,
    projection: Mat4,
    viewport: Vec2,
    points: &[[f32; 3]],
    model: Transform,
) -> Rect {
    let horizontal = rig.distance / rig.settings.pitch.tan();
    let (sine, cosine) = rig.settings.yaw.sin_cos();
    let focus = Vec3::new(rig.focus.x, terrain.sample(rig.focus), -rig.focus.y);
    let camera = Transform::from_translation(
        focus + Vec3::new(horizontal * sine, rig.distance, horizontal * cosine),
    )
    .looking_at(focus, Vec3::Y);
    // update_camera keeps its native map-edge clamp. This original interior
    // placement never reaches it, for either the old or tracked focus.
    let half_fov = (25.0_f32.to_radians().tan() / (viewport.x / viewport.y)).atan();
    let bottom = rig.distance / (rig.settings.pitch + half_fov).tan();
    let extent = Vec2::new(f32::from(terrain.width), f32::from(terrain.height));
    let margin = ((horizontal - bottom).abs() * 0.85)
        .min(extent.x * 0.5)
        .min(extent.y * 0.5);
    assert!(rig.focus.cmpge(Vec2::splat(margin)).all());
    assert!(rig.focus.cmple(extent - Vec2::splat(margin)).all());
    let clip = projection * camera.to_matrix().inverse();
    let mut bounds = Rect::from_corners(Vec2::splat(f32::INFINITY), Vec2::splat(f32::INFINITY));
    bounds.max = Vec2::splat(f32::NEG_INFINITY);
    for point in points {
        let ndc = clip.project_point3(model.transform_point(Vec3::from_array(*point)));
        let pixel = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * viewport * 0.5;
        bounds.min = bounds.min.min(pixel);
        bounds.max = bounds.max.max(pixel);
    }
    bounds
}

fn camera_app(
    assets: &generals_assets::virtual_assets::VirtualAssets,
    root: &std::path::Path,
    battle: crate::playtest::Match,
    target: usize,
    terrain: crate::terrain::Terrain,
    rig: crate::playtest::CameraRig,
) -> App {
    let setup = super::setup::Setup {
        topple: false,
        target,
        tanks: Vec::new(),
        position: battle.simulation.object_position(target),
        height: 75.0,
    };
    let check = super::Check {
        setup,
        phase: 1,
        started: None,
        dead_frame: None,
        attacker: None,
        audio: false,
        particles: false,
        bone: false,
        effect: None,
        falling_frames: 0,
        visible_falling_frames: 0,
        effect_falling_frames: 0,
    };
    let particles = crate::particles::load(assets, &[]).unwrap();
    let audio = crate::source_audio::load(assets, &[], &root.join("UserData/Options.ini")).unwrap();
    let mut app = App::new();
    app.insert_resource(battle)
        .insert_resource(check)
        .insert_resource(terrain)
        .insert_resource(rig)
        .insert_resource(crate::structure_view::Structures::default())
        .insert_resource(crate::model_animation::Animations::effect_fixture(1))
        .insert_resource(particles)
        .insert_resource(audio);
    app
}

fn record_pose(
    In(height): In<f32>,
    check: Res<super::Check>,
    battle: Res<crate::playtest::Match>,
    mut structures: ResMut<crate::structure_view::Structures>,
    mut animations: ResMut<crate::model_animation::Animations>,
    mut particles: ResMut<crate::particles::Particles>,
    mut audio: ResMut<crate::source_audio::SourceAudio>,
) {
    use generals_sim::skirmish::structure_death::{Event, VisualAction};
    let check = check.into_inner();
    let battle = battle.into_inner();
    structures.record(
        battle,
        &Event {
            object: check.setup.target,
            frame: battle.simulation.tick,
            action: VisualAction::CollapseShudder {
                height,
                amplitude: 0.5,
            },
            effect: None,
        },
        &mut animations,
        &mut particles,
        &mut audio,
    );
}

fn contained(bounds: Rect) -> bool {
    bounds.min.cmpge(Vec2::splat(12.0)).all()
        && bounds.max.cmple(Vec2::new(1_788.0, 734.5)).all()
        && bounds.size().cmpge(Vec2::new(16.0, 12.0)).all()
}
