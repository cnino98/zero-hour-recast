use super::{RenderResources, Stream, StreamVisual, Streams};
use crate::{playtest::Match, render_performance::Digest, source_material::SourceMaterial};
use bevy::{
    asset::{AssetApp, AssetPlugin},
    prelude::*,
};
use std::collections::BTreeMap;

// Previous complete per-frame builder, retained independently to compare all
// mesh bytes against the cached geometry + exact unscrolled-V path.
fn reference(
    source: &Streams,
    stream: &Stream,
    terrain: &crate::terrain::Terrain,
    camera: &Transform,
) -> Mesh {
    let mut combined = None;
    for index in 0..source.segments {
        let points = super::arc_segment(
            stream.points,
            index,
            source.segments,
            source.arc,
            source.overlap,
            |point| terrain.sample(Vec2::new(point[0], point[1]) / 10.0) * 10.0,
        )
        .unwrap();
        let length = super::vector_length(std::array::from_fn(|axis| {
            points[1][axis] - points[0][axis]
        }));
        let tile = length / source.beams[0].width * source.aspect * source.tile_scale;
        let mut segment =
            super::geometry(std::slice::from_ref(&source.beams[0]), points, camera).unwrap();
        if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) =
            segment.attribute_mut(Mesh::ATTRIBUTE_UV_0)
        {
            for point in uv {
                let tiled = point[1] * tile;
                point[1] = tiled + stream.offset;
            }
        }
        if let Some(mesh) = &mut combined {
            Mesh::merge(mesh, &segment).unwrap();
        } else {
            combined = Some(segment);
        }
    }
    combined.unwrap()
}

#[derive(Resource)]
struct Fixture {
    streams: Streams,
    camera: Transform,
    delta: f32,
}

fn draw(
    mut commands: Commands,
    mut fixture: ResMut<Fixture>,
    battle: Res<Match>,
    mut resources: RenderResources,
    mut visuals: Query<(Entity, &mut StreamVisual, &Mesh3d)>,
) {
    let camera = fixture.camera;
    let delta = fixture.delta;
    fixture.streams.render(
        &mut commands,
        battle.into_inner(),
        &camera,
        &mut resources,
        &mut visuals,
        delta,
    );
}

fn scene() -> App {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let map = generals_assets::map::MapData::parse(
        &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
    )
    .unwrap();
    let terrain = crate::terrain::Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
    let mut streams = super::load_template(&assets, "PatriotBinaryDataStream").unwrap();
    for serial in 1..=128_u16 {
        streams.active.push(Stream {
            object: usize::from(serial),
            serial: u64::from(serial),
            points: [[100.0, f32::from(serial), 0.0], [200.0, 300.0, 0.0]],
            width_scale: 1.0,
            origin: [0.0; 3],
            offset: 0.0,
            visible: serial % 2 == 0,
        });
    }
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<SourceMaterial>>()
        .init_resource::<Assets<crate::particles::smudge::SceneMaterial>>()
        .insert_resource(battle)
        .insert_resource(terrain)
        .insert_resource(Fixture {
            streams,
            camera: Transform::IDENTITY,
            delta: 1.0 / 30.0,
        })
        .add_systems(Update, draw);
    app
}

fn check(app: &mut App) -> BTreeMap<u64, Handle<Mesh>> {
    let expected = expected(app);
    check_expected(app, &expected)
}

fn expected(app: &App) -> BTreeMap<u64, Mesh> {
    let fixture = app.world().resource::<Fixture>();
    let terrain = app.world().resource::<crate::terrain::Terrain>();
    fixture
        .streams
        .active
        .iter()
        .filter(|stream| stream.visible)
        .map(|stream| {
            (
                stream.serial,
                reference(&fixture.streams, stream, terrain, &fixture.camera),
            )
        })
        .collect()
}

fn check_expected(app: &mut App, expected: &BTreeMap<u64, Mesh>) -> BTreeMap<u64, Handle<Mesh>> {
    let mut query = app.world_mut().query::<(&StreamVisual, &Mesh3d)>();
    let streams = &app.world().resource::<Fixture>().streams;
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mut result = BTreeMap::new();
    for (visual, mesh) in query.iter(app.world()) {
        assert_eq!(visual.generation, streams.generation);
        let stream = streams
            .active
            .iter()
            .find(|stream| stream.serial == visual.serial)
            .expect("rendered current stream");
        assert!(stream.visible);
        let mut before = Digest::new();
        let mut after = Digest::new();
        let original = &expected[&stream.serial];
        let actual = meshes.get(&mesh.0).unwrap();
        before.mesh(original, true);
        after.mesh(actual, true);
        assert_eq!(before.value(), after.value());
        let indices = original.indices().unwrap();
        let actual_indices = actual.indices().unwrap();
        assert_eq!(
            std::mem::discriminant(indices),
            std::mem::discriminant(actual_indices)
        );
        assert!(indices.iter().eq(actual_indices.iter()));
        assert!(result.insert(visual.serial, mesh.0.clone()).is_none());
    }
    assert_eq!(
        result.len(),
        streams
            .active
            .iter()
            .filter(|stream| stream.visible)
            .count()
    );
    result
}

fn position_buffers(app: &mut App) -> BTreeMap<u64, *const [f32; 3]> {
    let mut query = app.world_mut().query::<(&StreamVisual, &Mesh3d)>();
    let meshes = app.world().resource::<Assets<Mesh>>();
    query
        .iter(app.world())
        .map(|(visual, handle)| {
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) = meshes
                .get(&handle.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("original stream positions missing");
            };
            (visual.serial, points.as_ptr())
        })
        .collect()
}

#[ignore = "requires original game installation"]
#[test]
fn stream_membership_preserves_arc_uv_identity_hidden_recreation_and_generation_retirement() {
    let mut app = scene();
    app.update();
    let before = check(&mut app);
    assert_eq!(before.len(), 64);
    app.update();
    assert_eq!(before, check(&mut app));
    {
        let mut fixture = app.world_mut().resource_mut::<Fixture>();
        fixture.streams.active[0].visible = true;
        fixture.streams.active[1].visible = false;
    }
    app.update();
    let changed = check(&mut app);
    assert!(changed.contains_key(&1));
    assert!(!changed.contains_key(&2));
    assert_eq!(before[&4], changed[&4]);
    app.world_mut().resource_mut::<Fixture>().streams.active[1].visible = true;
    app.update();
    let shown = check(&mut app);
    assert_ne!(before[&2], shown[&2]);
    assert_eq!(app.world().resource::<Assets<SourceMaterial>>().len(), 1);
    {
        let mut fixture = app.world_mut().resource_mut::<Fixture>();
        fixture.streams.generation += 1;
        fixture.streams.active.clear();
    }
    app.update();
    assert!(check(&mut app).is_empty());
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
}

#[ignore = "requires original game installation"]
#[test]
fn camera_affine_endpoints_and_missing_assets_rebuild_without_changing_native_mesh_bytes() {
    let mut app = scene();
    app.update();
    let handles = check(&mut app);
    for camera in [
        Transform::from_translation(Vec3::new(14.0, 35.0, -66.0)),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, 0.3, 1.1, -0.6)),
        Transform::from_scale(Vec3::new(0.8, 1.1, 1.4)),
        Transform::from_translation(Vec3::new(-24.0, 65.0, 36.0))
            .with_rotation(Quat::from_euler(EulerRot::XYZ, -0.7, 0.2, 0.4))
            .with_scale(Vec3::new(1.6, 0.7, 0.5)),
    ] {
        app.world_mut().resource_mut::<Fixture>().camera = camera;
        app.update();
        assert_eq!(handles, check(&mut app));
    }
    {
        let mut fixture = app.world_mut().resource_mut::<Fixture>();
        for stream in &mut fixture.streams.active {
            stream.points[0][0] += 14.75;
            stream.points[1][1] -= 26.3;
            stream.points[1][2] += 37.125;
        }
    }
    app.update();
    assert_eq!(handles, check(&mut app));
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .remove(handles[&2].id());
    app.update();
    assert_eq!(handles, check(&mut app));
}

#[ignore = "requires original game installation"]
#[test]
fn scroll_wrap_matches_original_arithmetic_and_unchanged_offset_does_not_modify_mesh_assets() {
    let mut app = scene();
    let mut offset = 0.0_f32;
    for delta in [1.0 / 30.0, 0.0, 10.75, 0.001, 72.4, 0.0] {
        app.world_mut().resource_mut::<Fixture>().delta = delta;
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .clear();
        let previous = offset;
        let scroll = app.world().resource::<Fixture>().streams.scroll;
        let distance = scroll * delta;
        offset += distance;
        offset -= offset.floor();
        app.update();
        for stream in &app.world().resource::<Fixture>().streams.active {
            if stream.visible {
                assert_eq!(stream.offset.to_bits(), offset.to_bits());
            }
        }
        check(&mut app);
        if previous.to_bits() == offset.to_bits() {
            assert!(
                !app.world_mut()
                    .resource_mut::<Messages<AssetEvent<Mesh>>>()
                    .drain()
                    .any(|event| matches!(event, AssetEvent::Modified { .. }))
            );
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn sustained_128_stream_scroll_matches_uncached_geometry_for_240_frames() {
    let mut app = scene();
    for stream in &mut app.world_mut().resource_mut::<Fixture>().streams.active {
        stream.visible = true;
    }
    app.update();
    let handles = check(&mut app);
    assert_eq!(handles.len(), 128);
    let buffers = position_buffers(&mut app);
    let mut cached = std::time::Duration::ZERO;
    let mut uncached = std::time::Duration::ZERO;
    for _ in 0..240 {
        let started = std::time::Instant::now();
        app.update();
        cached += started.elapsed();
        let started = std::time::Instant::now();
        let meshes = expected(&app);
        uncached += started.elapsed();
        assert_eq!(handles, check_expected(&mut app, &meshes));
        assert_eq!(buffers, position_buffers(&mut app));
    }
    println!(
        "128 original Patriot streams × 240 display frames: cached ECS update {:.3} ms, uncached geometry {:.3} ms; every attribute/index digest matched, all 128 position buffers retained (zero geometry rebuilds)",
        cached.as_secs_f64() * 1000.0,
        uncached.as_secs_f64() * 1000.0,
    );
}

#[ignore = "requires original game installation"]
#[test]
fn original_feedback_streams_follow_real_laser_objects_and_deferred_deletion() {
    let (assets, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let mut particles = crate::particles::load(&assets, &battle.simulation.catalog).unwrap();
    let mut native = super::super::native::Lasers::new();
    let mut world = World::new();
    let mut state = bevy::ecs::system::SystemState::<
        Query<(
            &crate::playtest_view::UnitVisual,
            &crate::model_animation::Players,
            &Transform,
        )>,
    >::new(&mut world);
    let models = state.get(&world).unwrap();
    let (pair, target, _) =
        crate::weapon_verification::defenses::prepare(&mut battle.simulation).unwrap();
    let mut streams = super::load_template(&assets, "PatriotBinaryDataStream").unwrap();
    for event in &battle.simulation.presentation.laser_initializations {
        native.initialize(&battle, event, &mut animations, &mut particles, &models);
    }
    native.update(&battle, &mut particles, &models);
    streams.record_native(&battle, &native);
    assert_eq!(streams.active.len(), 2);
    let assistant = battle.simulation.entities[pair[1]]
        .drawable_position()
        .unwrap()
        .map(f32::to_bits);
    // AssistedTargetingUpdate passes an explicit endpoint. LaserUpdate
    // keeps that position even when the target moves later in this frame.
    let init = battle
        .simulation
        .presentation
        .laser_initializations
        .iter()
        .find(|event| event.object == streams.active[1].object)
        .unwrap();
    assert_eq!(init.init.target, Some(target));
    let destination = init.target_position.unwrap().map(f32::to_bits);
    assert_eq!(init.init.end.unwrap().map(f32::to_bits), destination);
    assert_eq!(
        streams.active[0]
            .points
            .map(|point| point.map(f32::to_bits)),
        [assistant; 2]
    );
    assert_eq!(
        streams.active[1]
            .points
            .map(|point| point.map(f32::to_bits)),
        [assistant, destination]
    );
    assert!(streams.active[0].serial < streams.active[1].serial);
    let objects: Vec<_> = streams.active.iter().map(|stream| stream.object).collect();
    native.update(&battle, &mut particles, &models);
    streams.record_native(&battle, &native);
    assert_eq!(streams.active.len(), 2);
    for unit in &mut battle.simulation.catalog {
        unit.ai.acquisition = 0;
    }
    for id in pair {
        battle.simulation.stop(id).unwrap();
    }
    for _ in 0..120 {
        battle.simulation.step();
        for event in &battle.simulation.presentation.weapon_fx {
            if let generals_sim::skirmish::WeaponFxEvent::DrawableDestroyed(object) = event {
                native.retire(*object, &mut particles);
            }
        }
        native.update(&battle, &mut particles, &models);
        streams.record_native(&battle, &native);
        for &object in &objects {
            assert_eq!(
                streams.active.iter().any(|stream| stream.object == object),
                battle.simulation.entities[object].exists()
            );
        }
        if objects
            .iter()
            .all(|&object| !battle.simulation.entities[object].exists())
        {
            break;
        }
    }
    assert!(
        objects
            .iter()
            .all(|&object| !battle.simulation.entities[object].exists())
    );
    assert!(streams.active.is_empty());
}

#[ignore = "requires original game installation"]
#[test]
fn feedback_resources_follow_actual_catalog_assistance_and_resolved_variations() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let catalog = &mut battle.simulation.catalog;
    for unit in catalog.iter_mut() {
        unit.behavior.assisted_targeting.clear();
    }
    let laser = catalog
        .iter()
        .position(|unit| unit.name == "PatriotBinaryDataStream")
        .unwrap();
    assert!(catalog[laser].laser_update.is_some());
    catalog[0].name = "MapAddedAssistant".into();
    catalog[1].name = "MapFeedbackChoice".into();
    catalog[1].variations = vec![catalog[laser].name.clone(), catalog[2].name.clone()];
    catalog[0]
        .behavior
        .assisted_targeting
        .push(generals_sim::skirmish::AssistedTargeting {
            clip_size: 1,
            weapon_slot: 0,
            lasers: ["MapFeedbackChoice".into(), "MissingOptionalLaser".into()],
        });
    // Keep the variation, its nonlaser alternative, and a separately used
    // special-ability laser in this custom catalog. Both drawable templates
    // must load exactly once, even though only one is the Patriot's feedback.
    let decoy = catalog[2].name.clone();
    catalog.retain(|unit| {
        unit.name == decoy
            || matches!(
                unit.name.as_str(),
                "MapAddedAssistant"
                    | "MapFeedbackChoice"
                    | "PatriotBinaryDataStream"
                    | "BinaryDataStream"
            )
    });
    let mut templates = crate::skirmish_data::feedback_templates(catalog);
    templates.sort_unstable();
    assert_eq!(templates, ["BinaryDataStream", "PatriotBinaryDataStream"]);
    let streams = super::load_feedback(&assets, catalog).unwrap();
    let mut loaded = streams
        .iter()
        .map(|stream| stream.template.as_str())
        .collect::<Vec<_>>();
    loaded.sort_unstable();
    assert_eq!(loaded, templates);
}

#[ignore = "requires original game installation"]
#[test]
fn feedback_visibility_uses_the_laser_object_instead_of_its_parent() {
    let (assets, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let mut particles = crate::particles::load(&assets, &battle.simulation.catalog).unwrap();
    let mut native = super::super::native::Lasers::new();
    let mut world = World::new();
    let mut state = bevy::ecs::system::SystemState::<
        Query<(
            &crate::playtest_view::UnitVisual,
            &crate::model_animation::Players,
            &Transform,
        )>,
    >::new(&mut world);
    let models = state.get(&world).unwrap();
    let (pair, _, _) =
        crate::weapon_verification::defenses::prepare(&mut battle.simulation).unwrap();
    let mut streams = super::load_template(&assets, "PatriotBinaryDataStream").unwrap();
    for event in &battle.simulation.presentation.laser_initializations {
        native.initialize(&battle, event, &mut animations, &mut particles, &models);
    }
    native.update(&battle, &mut particles, &models);
    streams.record_native(&battle, &native);
    assert_eq!(streams.active.len(), 2);
    let object = streams.active[0].object;
    let parent = pair[1];
    let game = &mut battle.simulation;
    for unit in &mut game.catalog {
        unit.sight.clearing_range = 0.0;
        unit.sight.reveal_to_all_range = 0.0;
    }
    game.catalog[game.entities[parent].type_index]
        .sight
        .clearing_range = 100.0;
    // Partition fixture positions are independent of the retained Drawable endpoints.
    game.entities[parent].position = [1000.0, 1000.0];
    game.entities[pair[0]].position = [1500.0, 1500.0];
    game.entities[object].position = [100.0, 100.0];
    game.configure_visibility(generals_sim::skirmish::visibility::VisibilityRules {
        cell_size: 10.0,
        unlook_persist_frames: 0,
        use_shroud: true,
    })
    .unwrap();
    assert!(game.visible_to(0, parent));
    assert!(!game.visible_to(0, object));
    native.update(&battle, &mut particles, &models);
    streams.record_native(&battle, &native);
    assert!(!streams.active[0].visible);
    assert_eq!(
        streams.active[0].points[0].map(f32::to_bits),
        streams.active[1].points[0].map(f32::to_bits)
    );
    assert_eq!(
        streams.active[1].points[0].map(f32::to_bits),
        battle.simulation.entities[parent]
            .drawable_position()
            .unwrap()
            .map(f32::to_bits)
    );
}

#[test]
fn native_draw_keeps_all_authored_beam_layers_and_the_source_outer_to_inner_order() {
    let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
        path: "laser.ini".into(), origin: "native W3DLaserDraw regression".into(),
        bytes: b"Object Beam\nDraw=W3DLaserDraw DrawTag\nNumBeams=65tail\nInnerBeamWidth=4\nOuterBeamWidth=12\nInnerColor=R:255 G:255 B:255 A:255\nOuterColor=R:255 G:255 B:255 A:255\nEnd\nEnd\n".to_vec(),
    }).unwrap();
    let draw = crate::skirmish_data::blocks(&file.declarations[0].lines, 0, "Draw").unwrap()[0];
    let beams = super::beam_layers(draw).unwrap();
    assert_eq!(beams.len(), 65);
    assert_eq!(beams[0].width.to_bits(), 12.0_f32.to_bits());
    assert_eq!(beams[64].width.to_bits(), 4.0_f32.to_bits());
    assert!(
        beams
            .iter()
            .all(|beam| beam.color.map(f32::to_bits) == [1.0_f32.to_bits(); 3])
    );
}
