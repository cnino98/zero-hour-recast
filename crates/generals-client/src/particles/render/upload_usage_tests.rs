//! CPU sorting inputs remain available while only direct particle meshes extract.
use super::{ParticleVisual, equivalence, geometry, mesh};
use crate::particles::{Particles, Pose, Shader, Style, tests};
use bevy::{
    app::SubApp,
    asset::{AssetEventSystems, RenderAssetUsages},
    camera::visibility::RenderLayers,
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, RenderApp,
        mesh::RenderMesh,
        render_asset::{ExtractedAssets, RenderAssetPlugin},
    },
};

fn scene() -> App {
    let mut app = equivalence::scene();
    app.add_systems(
        PostUpdate,
        crate::playtest_view::prepared_meshes::upload_used.before(AssetEventSystems),
    );
    let mut particles = app.world_mut().resource_mut::<Particles>();
    let template = &mut particles.templates[0];
    template.drawing.style = Style::Streak;
    template.drawing.ground_aligned = true;
    template.burst_count.low = 2.0;
    template.burst_count.high = 2.0;
    particles.create(0, Pose::at([10.0, 20.0, 30.0]), None, &mut |low, _| low);
    particles.logic(0, &mut |low, _| low, &|_| 0.0);
    assert_eq!(particles.emitters[0].particles.len(), 2);
    particles.emitters[0].particles[1].position[0] += 10.0;
    app
}

fn generated(app: &App) -> Mesh {
    let particles = app.world().resource::<Particles>();
    mesh(
        &particles.emitters[0],
        &particles.templates[0],
        bevy::math::Affine3A::IDENTITY,
        bevy::math::Affine3A::IDENTITY,
    )
    .unwrap()
}

#[ignore = "requires original game installation"]
#[test]
fn particle_source_usage_follows_native_direct_and_sorted_membership() {
    let app = scene();
    let particles = app.world().resource::<Particles>();
    let emitter = &particles.emitters[0];
    let mut template = tests::template("usage");
    for (style, shader, ground, direct) in [
        (Style::Billboard, Shader::Alpha, false, false),
        (Style::Billboard, Shader::Additive, false, false),
        (Style::Billboard, Shader::AlphaTest, false, true),
        (Style::Billboard, Shader::Alpha, true, true),
        (Style::Volume(1), Shader::Alpha, true, true),
        (Style::Volume(2), Shader::Alpha, true, false),
        (Style::Streak, Shader::Alpha, true, false),
        (Style::Streak, Shader::AlphaTest, true, true),
    ] {
        template.drawing.style = style;
        template.drawing.shader = shader;
        template.drawing.ground_aligned = ground;
        let source = mesh(
            emitter,
            &template,
            bevy::math::Affine3A::IDENTITY,
            bevy::math::Affine3A::IDENTITY,
        )
        .unwrap();
        assert!(source.asset_usage.contains(RenderAssetUsages::MAIN_WORLD));
        assert_eq!(
            source.asset_usage.contains(RenderAssetUsages::RENDER_WORLD),
            direct
        );
        assert!(source.count_vertices() > 0);
        assert!(!source.get_index_buffer_bytes().unwrap().is_empty());
    }
}

// Invoke the actual RenderAssetPlugin extraction without a device or window.
fn extract(app: &mut App) {
    let mut main = MainWorld::default();
    std::mem::swap(&mut *main, app.world_mut());
    let render = app.sub_app_mut(RenderApp);
    render.world_mut().insert_resource(main);
    render.world_mut().run_schedule(ExtractSchedule);
    let mut main = render.world_mut().remove_resource::<MainWorld>().unwrap();
    std::mem::swap(app.world_mut(), &mut *main);
}

#[ignore = "requires original game installation"]
#[test]
fn streak_direct_sorted_transitions_extract_current_bytes_under_one_handle() {
    let mut app = scene();
    app.insert_sub_app(RenderApp, SubApp::new());
    app.add_plugins(RenderAssetPlugin::<RenderMesh>::default());
    let mut saved = None;
    let mut identity = None;
    for direct in [false, true, false, true] {
        let count = app.world().resource::<Particles>().emitters[0]
            .particles
            .len();
        if direct && count == 2 {
            saved = app.world_mut().resource_mut::<Particles>().emitters[0]
                .particles
                .pop();
        } else if !direct && count == 1 {
            app.world_mut().resource_mut::<Particles>().emitters[0]
                .particles
                .push(saved.take().unwrap());
        }
        app.update();
        let mut query = app
            .world_mut()
            .query::<(Entity, &ParticleVisual, &Mesh3d, &RenderLayers)>();
        let (entity, visual, handle, layer) = query.single(app.world()).unwrap();
        let handle = handle.0.clone();
        assert_eq!(visual.bounded, !direct);
        assert_eq!(layer, &RenderLayers::layer(if direct { 7 } else { 3 }));
        let current = (entity, handle.id());
        let previous = identity.replace(current);
        if let Some(previous) = previous {
            assert_eq!(previous, current);
        }
        let expected = generated(&app);
        assert!(geometry::matches(
            app.world().resource::<Assets<Mesh>>().get(&handle).unwrap(),
            &expected
        ));
        extract(&mut app);
        let extracted = app
            .sub_app(RenderApp)
            .world()
            .resource::<ExtractedAssets<RenderMesh>>();
        assert_eq!(
            extracted.modified.contains(&handle.id()),
            previous.is_some()
        );
        assert_eq!(extracted.added.contains(&handle.id()), direct);
        assert!(extracted.removed.is_empty());
        if direct {
            assert_eq!(extracted.extracted.len(), 1);
            assert_eq!(extracted.extracted[0].0, handle.id());
            assert_eq!(
                extracted.extracted[0].1.create_packed_vertex_buffer_data(),
                expected.create_packed_vertex_buffer_data()
            );
            assert_eq!(
                extracted.extracted[0].1.get_index_buffer_bytes(),
                expected.get_index_buffer_bytes()
            );
        } else {
            assert!(extracted.extracted.is_empty());
        }
    }
    check_usage_change_and_retirement(&mut app, identity.unwrap().1);
}

fn check_usage_change_and_retirement(app: &mut App, id: AssetId<Mesh>) {
    // Usage alone must emit Modified even when the vertex and index bytes match.
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_strong_handle(id)
        .unwrap();
    let mut next = app
        .world()
        .resource::<Assets<Mesh>>()
        .get(id)
        .unwrap()
        .clone();
    next.asset_usage = RenderAssetUsages::MAIN_WORLD;
    geometry::update(
        &mut app.world_mut().resource_mut::<Assets<Mesh>>(),
        &handle,
        next,
    );
    // Flush queued asset events without regenerating the direct particle mesh.
    app.world_mut().run_schedule(PostUpdate);
    extract(app);
    let extracted = app
        .sub_app(RenderApp)
        .world()
        .resource::<ExtractedAssets<RenderMesh>>();
    assert!(extracted.modified.contains(&id));
    assert!(extracted.extracted.is_empty() && !extracted.added.contains(&id));
    // The same live handle can return to direct rendering on the next update.
    app.update();
    extract(app);
    let extracted = app
        .sub_app(RenderApp)
        .world()
        .resource::<ExtractedAssets<RenderMesh>>();
    assert!(extracted.modified.contains(&id) && extracted.added.contains(&id));
    assert_eq!(
        extracted.extracted[0].1.get_index_buffer_bytes(),
        generated(app).get_index_buffer_bytes()
    );
    drop(handle);
    app.world_mut()
        .resource_mut::<crate::playtest::Match>()
        .started = false;
    let mut retired = false;
    for _ in 0..4 {
        app.update();
        extract(app);
        retired |= app
            .sub_app(RenderApp)
            .world()
            .resource::<ExtractedAssets<RenderMesh>>()
            .removed
            .contains(&id);
    }
    assert!(
        retired,
        "last particle handle must retire render-world state"
    );
    assert!(app.world().resource::<Assets<Mesh>>().get(id).is_none());
}
