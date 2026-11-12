use super::{ParticleVisual, equivalence, geometry, mesh};
use crate::{particles::Particles, playtest_view::WorldCamera, source_material::SourceMaterial};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use std::collections::{BTreeMap, BTreeSet};

type Drawn = BTreeMap<u64, (Entity, Handle<Mesh>, Handle<SourceMaterial>)>;

#[test]
fn alpha_test_sprite_has_original_depth_cutout_and_texture_only_gradient() {
    use crate::particles::{Pose, Shader, Texture, tests};
    let mut templates: Vec<_> = ["alpha", "additive", "alpha-test"]
        .map(tests::template)
        .into_iter()
        .collect();
    templates[1].drawing.shader = Shader::Additive;
    templates[2].drawing.shader = Shader::AlphaTest;
    let mut particles = tests::system(templates);
    let mut images = Assets::<Image>::default();
    let texture = images.add(Image::default());
    particles.textures.push(Texture {
        image: None,
        handle: Some(texture.clone()),
        materials: [None, None, None],
    });
    for index in 0..3 {
        particles.create(index, Pose::at([0.0; 3]), None, &mut |low, _| low);
    }
    particles.logic(0, &mut |low, _| low, &|_| 0.0);
    let mut materials = Assets::<SourceMaterial>::default();
    super::upload_materials(&mut particles, &mut materials);
    let handles: Vec<_> = particles.textures[0]
        .materials
        .iter()
        .map(|material| material.clone().unwrap())
        .collect();
    assert_ne!(handles[0], handles[1]);
    assert_ne!(handles[1], handles[2]);
    for (index, handle) in handles.iter().enumerate() {
        let material = materials.get(handle).unwrap();
        assert_eq!(material.texture, Some(texture.clone()));
        assert_eq!(material.parameters.vertex_diffuse, 3);
        if index < 2 {
            assert!(material.double_sided && !material.depth_write);
            assert_eq!(material.parameters.primary_gradient, 1);
        }
    }
    let tested = materials.get(&handles[2]).unwrap();
    assert!(matches!(
        materials.get(&handles[0]).unwrap().alpha_mode,
        AlphaMode::Blend
    ));
    assert!(matches!(
        materials.get(&handles[1]).unwrap().alpha_mode,
        AlphaMode::Add
    ));
    assert!(matches!(tested.alpha_mode, AlphaMode::Mask(_)));
    assert!(tested.depth_write && !tested.double_sided);
    assert_eq!(tested.parameters.primary_gradient, 0);
    assert_eq!(
        tested.parameters.alpha_test.to_bits(),
        (96.0_f32 / 255.0).to_bits()
    );
    super::upload_materials(&mut particles, &mut materials);
    assert_eq!(materials.len(), 3);
}

fn drawn(app: &mut App) -> Drawn {
    let mut query = app.world_mut().query::<(
        Entity,
        &ParticleVisual,
        &Mesh3d,
        &MeshMaterial3d<SourceMaterial>,
    )>();
    query
        .iter(app.world())
        .map(|(entity, visual, mesh, material)| {
            (visual.system, (entity, mesh.0.clone(), material.0.clone()))
        })
        .collect()
}

fn modified(app: &mut App) -> BTreeSet<AssetId<Mesh>> {
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<Mesh>>>()
        .drain()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } => Some(id),
            _ => None,
        })
        .collect()
}

fn check_geometry(app: &mut App, visuals: &Drawn) {
    let mut query = app
        .world_mut()
        .query_filtered::<&Transform, With<WorldCamera>>();
    let camera = query.single(app.world()).unwrap().compute_affine();
    let particles = app.world().resource::<Particles>();
    let meshes = app.world().resource::<Assets<Mesh>>();
    for emitter in &particles.emitters {
        if emitter.particles.is_empty() {
            continue;
        }
        let expected = mesh(
            emitter,
            &particles.templates[emitter.template],
            camera.inverse(),
            camera,
        )
        .unwrap();
        assert!(geometry::matches(
            meshes.get(&visuals[&emitter.id].1).unwrap(),
            &expected
        ));
    }
}

#[ignore = "requires original game installation"]
#[test]
fn particle_uploads_follow_geometry_changes_and_preserve_visibility_and_materials() {
    let mut app = equivalence::scene();
    equivalence::emit(&mut app, 0);
    app.update();
    app.update();
    let visuals = drawn(&mut app);
    modified(&mut app);
    for _ in 0..8 {
        app.update();
        assert!(modified(&mut app).is_empty());
    }
    app.world_mut()
        .entity_mut(visuals[&0].0)
        .insert(Visibility::Hidden);
    app.update();
    assert!(modified(&mut app).is_empty());
    assert!(matches!(
        app.world().get::<Visibility>(visuals[&0].0),
        Some(Visibility::Hidden)
    ));
    {
        let mut particles = app.world_mut().resource_mut::<Particles>();
        particles.emitters[0].particles[0].size += 3.0;
        particles.emitters[1].particles[0].color = [0.25, 0.5, 0.75];
    }
    app.update();
    assert_eq!(
        modified(&mut app),
        BTreeSet::from([visuals[&0].1.id(), visuals[&1].1.id()])
    );
    check_geometry(&mut app, &visuals);
    app.world_mut()
        .entity_mut(visuals[&0].0)
        .insert(Visibility::Inherited);
    app.update();
    assert!(modified(&mut app).is_empty());
    assert!(matches!(
        app.world().get::<Visibility>(visuals[&0].0),
        Some(Visibility::Inherited)
    ));
    let mut camera = app
        .world_mut()
        .query_filtered::<&mut Transform, With<WorldCamera>>();
    camera.single_mut(app.world_mut()).unwrap().rotation = Quat::from_rotation_y(0.5);
    app.update();
    assert_eq!(
        modified(&mut app),
        visuals.values().map(|(_, mesh, _)| mesh.id()).collect()
    );
    check_geometry(&mut app, &visuals);
    assert_eq!(drawn(&mut app), visuals);
}

#[test]
fn comparison_keeps_every_attribute_bit_and_index_representation() {
    use bevy::{
        asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology,
    };
    let original = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32, 1.0, 2.0]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 1.0, 0.0]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32, 1.0]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32, 1.0, 0.0, 1.0]; 3])
    .with_inserted_indices(Indices::U32(vec![0, 1, 2]));
    assert!(geometry::matches(&original, &original));
    for attribute in [
        Mesh::ATTRIBUTE_POSITION,
        Mesh::ATTRIBUTE_NORMAL,
        Mesh::ATTRIBUTE_UV_0,
        Mesh::ATTRIBUTE_COLOR,
    ] {
        let mut changed = original.clone();
        match changed.attribute_mut(attribute).unwrap() {
            VertexAttributeValues::Float32x2(values) => values[0][0] = -0.0,
            VertexAttributeValues::Float32x3(values) => values[0][0] = -0.0,
            VertexAttributeValues::Float32x4(values) => values[0][0] = -0.0,
            _ => panic!("particle attribute format"),
        }
        assert!(!geometry::matches(&original, &changed));
    }
    let mut changed = original.clone();
    changed.insert_indices(Indices::U32(vec![0, 2, 1]));
    assert!(!geometry::matches(&original, &changed));
    changed.insert_indices(Indices::U16(vec![0, 1, 2]));
    assert!(!geometry::matches(&original, &changed));
    changed = original.clone();
    changed.remove_attribute(Mesh::ATTRIBUTE_NORMAL);
    assert!(!geometry::matches(&original, &changed));
}

#[ignore = "requires original game installation"]
#[test]
fn original_firebase_missile_and_volume_emitters_skip_unchanged_uploads() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let mut particles = crate::particles::load(&assets, &battle.simulation.catalog).unwrap();
    particles.begin(battle.generation, 0);
    for name in [
        "FireBaseHowitzerPillarArms",
        "FireBaseShellExplosionCloud",
        "MissileDefenderMissileExhaust",
        "BombTruckDefaultLenzFlare",
    ] {
        let index = particles
            .templates
            .iter()
            .position(|template| template.name == name)
            .unwrap();
        particles.create(
            index,
            crate::particles::Pose::at([100.0, 200.0, 30.0]),
            None,
            &mut |_, high| high,
        );
    }
    for frame in 0..3 {
        particles.logic(frame, &mut |_, high| high, &|_| 0.0);
    }
    let mut app = equivalence::render_app(battle, particles);
    app.update();
    app.update();
    let visuals = drawn(&mut app);
    assert!(
        visuals.len() >= 4,
        "original roots and attached systems must render"
    );
    check_geometry(&mut app, &visuals);
    modified(&mut app);
    for _ in 0..8 {
        app.update();
        assert!(modified(&mut app).is_empty());
    }
    let changed = {
        let mut particles = app.world_mut().resource_mut::<Particles>();
        let emitter = particles
            .emitters
            .iter_mut()
            .find(|emitter| !emitter.particles.is_empty())
            .unwrap();
        emitter.particles[0].position[0] += 1.0;
        emitter.id
    };
    app.update();
    assert_eq!(
        modified(&mut app),
        BTreeSet::from([visuals[&changed].1.id()])
    );
    check_geometry(&mut app, &visuals);
    assert_eq!(drawn(&mut app), visuals);
}
