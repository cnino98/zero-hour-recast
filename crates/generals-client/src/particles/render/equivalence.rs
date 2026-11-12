use super::{ParticleVisual, mesh, update};
use crate::{
    particles::{Particles, Pose, Texture, tests},
    playtest::Match,
    playtest_view::WorldCamera,
    render_performance::Digest,
    source_material::SourceMaterial,
};
use bevy::{asset::AssetPlugin, prelude::*, time::TimeUpdateStrategy};
use std::{collections::BTreeMap, time::Duration};

pub(super) fn scene() -> App {
    let (_, battle) = crate::cleanup_verification::tests::installed();
    let mut templates: Vec<_> = ["smoke", "second-smoke", "flare", "second-texture"]
        .map(tests::template)
        .into_iter()
        .collect();
    templates[2].drawing.shader = crate::particles::Shader::Additive;
    templates[3].texture = 1;
    let mut particles = tests::system(templates);
    particles.textures = (0..2)
        .map(|_| Texture {
            image: Some(Image::default()),
            handle: None,
            materials: [None, None, None],
        })
        .collect();
    render_app(battle, particles)
}

pub(super) fn render_app(battle: Match, particles: Particles) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<SourceMaterial>()
        .init_resource::<Time<Fixed>>()
        .insert_resource(battle)
        .insert_resource(particles)
        .add_systems(Update, update);
    app.register_required_components::<Mesh3d, Visibility>();
    app.world_mut().spawn((WorldCamera, Transform::IDENTITY));
    app
}

pub(super) fn emit(app: &mut App, generation: u32) {
    app.world_mut().resource_mut::<Match>().generation = generation;
    let mut particles = app.world_mut().resource_mut::<Particles>();
    particles.begin(generation, 0);
    for index in 0..256_u16 {
        particles.create(
            usize::from(index % 4),
            Pose::at([f32::from(index), 20.0, 30.0]),
            None,
            &mut |_, high| high,
        );
    }
    particles.logic(0, &mut |_, high| high, &|_| 0.0);
}

type Handles = BTreeMap<u64, (Handle<Mesh>, Handle<SourceMaterial>)>;

fn check_meshes(app: &mut App) -> Handles {
    let mut query = app
        .world_mut()
        .query::<(&ParticleVisual, &Mesh3d, &MeshMaterial3d<SourceMaterial>)>();
    let particles = app.world().resource::<Particles>();
    let materials = app.world().resource::<Assets<SourceMaterial>>();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mut handles = BTreeMap::new();
    for (visual, drawn, material) in query.iter(app.world()) {
        assert_eq!(visual.generation, particles.generation);
        let emitter = particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == visual.system)
            .expect("visual belongs to a current emitter");
        let template = &particles.templates[emitter.template];
        let expected = mesh(
            emitter,
            template,
            bevy::math::Affine3A::IDENTITY,
            bevy::math::Affine3A::IDENTITY,
        )
        .unwrap();
        let mut before = Digest::new();
        let mut after = Digest::new();
        before.mesh(&expected, true);
        after.mesh(meshes.get(&drawn.0).unwrap(), true);
        assert_eq!(before.value(), after.value());
        let mut expected = SourceMaterial::new(
            [0.0; 3],
            [0.0; 3],
            [0.0; 3],
            1.0,
            if matches!(template.drawing.shader, crate::particles::Shader::Additive) {
                AlphaMode::Add
            } else {
                AlphaMode::Blend
            },
            true,
        );
        expected.parameters.vertex_diffuse = 3;
        expected
            .texture
            .clone_from(&particles.textures[template.texture].handle);
        assert!(*materials.get(&material.0).unwrap() == expected);
        assert!(
            handles
                .insert(visual.system, (drawn.0.clone(), material.0.clone()))
                .is_none()
        );
    }
    assert_eq!(handles.len(), particles.emitters.len());
    handles
}

#[ignore = "requires original game installation"]
#[test]
fn particle_render_preserves_meshes_material_values_and_handles_across_unchanged_frames() {
    let mut app = scene();
    emit(&mut app, 0);
    app.update();
    let before = check_meshes(&mut app);
    assert_eq!(before[&0].1, before[&1].1);
    assert_ne!(before[&0].1, before[&2].1);
    assert_ne!(before[&0].1, before[&3].1);
    assert_eq!(app.world().resource::<Assets<SourceMaterial>>().len(), 3);
    app.update();
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<SourceMaterial>>>()
        .clear();
    let started = std::time::Instant::now();
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(before, check_meshes(&mut app));
    assert!(
        !app.world_mut()
            .resource_mut::<Messages<AssetEvent<SourceMaterial>>>()
            .drain()
            .any(|event| matches!(event, AssetEvent::Modified { .. }))
    );
    println!(
        "particle render: emitters={} materials={} unchanged_8_frames_ms={:.3}",
        before.len(),
        app.world().resource::<Assets<SourceMaterial>>().len(),
        started.elapsed().as_secs_f64() * 1000.0,
    );
    emit(&mut app, 1);
    app.update();
    let restarted = check_meshes(&mut app);
    assert_eq!(restarted.len(), before.len());
    assert!(restarted.keys().all(|id| !before.contains_key(id)));
    assert_eq!(app.world().resource::<Assets<SourceMaterial>>().len(), 3);
    for (id, (_, material)) in &restarted {
        assert_eq!(*material, before[&(id % 256)].1);
    }
    app.world_mut().resource_mut::<Match>().started = false;
    app.update();
    let mut visuals = app.world_mut().query::<&ParticleVisual>();
    assert_eq!(visuals.iter(app.world()).count(), 0);
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
}
