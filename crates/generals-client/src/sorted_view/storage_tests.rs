//! Exercise actual `upload_runs` shared pool regions and committed ECS bindings.
use super::{
    Node, Pool, Resources, SortedRange, SourceMaterial, WorldCamera, geometry, submission, update,
    upload_runs,
};
use bevy::{
    camera::{primitives::Frustum, visibility::RenderLayers},
    ecs::system::SystemState,
    math::Affine3A,
    prelude::*,
};
use num_traits::ToPrimitive;
use std::collections::HashMap;

type Identity = (Entity, AssetId<Mesh>, AssetId<SourceMaterial>);

fn scene() -> (App, [Handle<SourceMaterial>; 2]) {
    let mut app = App::new();
    app.insert_resource(Assets::<Mesh>::default());
    let mut assets = Assets::<SourceMaterial>::default();
    let materials = std::array::from_fn(|index| {
        assets.add(SourceMaterial::new(
            [index.to_f32().unwrap(), 0.25, 0.5],
            [1.0; 3],
            [0.0; 3],
            0.5,
            AlphaMode::Blend,
            false,
        ))
    });
    app.insert_resource(assets);
    app.insert_resource(Pool {
        vertices: Vec::new(),
        triangles: Vec::new(),
        runs: Vec::new(),
        mesh: None,
        variants: HashMap::new(),
        registry: submission::Registry::new(),
        sequence: 0,
        instances: HashMap::new(),
    });
    app.world_mut().spawn((
        WorldCamera,
        GlobalTransform::IDENTITY,
        Frustum(bevy::math::primitives::ViewFrustum::from_clip_from_world(
            &Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0),
        )),
    ));
    app.add_systems(Update, update);
    (app, materials)
}

fn mesh(count: usize, ordinal: usize, phase: u16) -> Mesh {
    assert!(count > 0 && count.is_multiple_of(3));
    let marker = ordinal.to_f32().unwrap().mul_add(10.0, f32::from(phase));
    let mut mesh = geometry::empty_mesh();
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        (0..count)
            .map(|vertex| [marker, vertex.to_f32().unwrap() * 0.25, -1.0])
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.25, 0.5]; count]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.2, 0.4, 0.6, 0.5]; count]);
    mesh
}

fn identity(app: &App) -> Vec<Identity> {
    app.world()
        .resource::<Pool>()
        .runs
        .iter()
        .map(|run| {
            (
                run.entity,
                app.world().get::<Mesh3d>(run.entity).unwrap().id(),
                run.material.id(),
            )
        })
        .collect()
}

fn frame(app: &mut App, inputs: &[Handle<SourceMaterial>; 2], counts: &[usize], phase: u16) {
    let world = app.world_mut();
    let mut pool = world.remove_resource::<Pool>().unwrap();
    pool.vertices.clear();
    pool.triangles.clear();
    let mut nodes = Vec::new();
    let mut expected = Vec::new();
    for (index, &count) in counts.iter().enumerate() {
        let mesh = mesh(count, index, phase);
        expected.push(mesh.create_packed_vertex_buffer_data());
        geometry::append(
            &mesh,
            Affine3A::IDENTITY,
            Affine3A::IDENTITY,
            index,
            &mut pool.vertices,
            &mut pool.triangles,
        );
        nodes.push(Node {
            mesh: world.resource_mut::<Assets<Mesh>>().add(mesh),
            material: inputs[index % inputs.len()].clone(),
            world: Affine3A::IDENTITY,
            scale: 1.0,
            depth: -1.0,
            sequence: u64::try_from(index).unwrap(),
            submission: None,
            particle: false,
            unbounded: false,
        });
    }
    let mut system = SystemState::<(Commands, Resources)>::new(world);
    {
        let (mut commands, mut resources) = system.get_mut(world).unwrap();
        upload_runs(&mut pool, &nodes, &mut commands, &mut resources);
    }
    world.insert_resource(pool);
    system.apply(world);
    for node in nodes {
        world.resource_mut::<Assets<Mesh>>().remove(&node.mesh);
    }
    assert_outputs(app, inputs, &expected);
}

fn assert_outputs(app: &App, inputs: &[Handle<SourceMaterial>; 2], expected: &[Vec<u8>]) {
    let world = app.world();
    let pool = world.resource::<Pool>();
    let meshes = world.resource::<Assets<Mesh>>();
    let materials = world.resource::<Assets<SourceMaterial>>();
    let Some(handle) = &pool.mesh else {
        assert!(expected.is_empty() && pool.runs.is_empty() && meshes.is_empty());
        return;
    };
    let mesh = meshes.get(handle).unwrap();
    assert!(mesh.indices().is_none());
    assert_eq!(mesh.final_aabb, geometry::empty_mesh().final_aabb);
    let stride = mesh.get_vertex_buffer_size() / mesh.count_vertices();
    let packed = mesh.create_packed_vertex_buffer_data();
    let mut next_vertex = 0;
    for (index, run) in pool.runs.iter().enumerate() {
        assert_eq!(world.get::<Mesh3d>(run.entity).unwrap().id(), handle.id());
        assert!(
            world
                .get::<bevy::render::batching::NoAutomaticBatching>(run.entity)
                .is_some()
        );
        assert_eq!(
            world
                .get::<MeshMaterial3d<SourceMaterial>>(run.entity)
                .unwrap()
                .id(),
            run.material.id()
        );
        let visible = index < expected.len();
        assert_eq!(
            world.get::<InheritedVisibility>(run.entity).unwrap().get(),
            visible
        );
        assert_eq!(
            world.get::<RenderLayers>(run.entity).unwrap(),
            &if visible {
                RenderLayers::layer(3)
            } else {
                RenderLayers::none()
            }
        );
        let region = &world.get::<SortedRange>(run.entity).unwrap().0;
        if let Some(expected) = expected.get(index) {
            assert_eq!(expected.len() % stride, 0);
            let end = next_vertex + expected.len() / stride;
            assert_eq!(
                region,
                &(u32::try_from(next_vertex).unwrap()..u32::try_from(end).unwrap())
            );
            assert_eq!(&packed[next_vertex * stride..end * stride], expected);
            next_vertex = end;
            let original = materials.get(&inputs[index % inputs.len()]).unwrap();
            let output = materials.get(&run.material).unwrap();
            assert!(super::interchangeable_materials(original, output));
            assert!(output.force_sorted_phase);
            assert_eq!(output.depth_bias.to_bits(), 0.0_f32.to_bits());
        } else {
            assert_eq!(
                region,
                &(0..0),
                "hidden ordinal cannot address stale triangles"
            );
        }
    }
    if !expected.is_empty() {
        assert!(
            packed[next_vertex * stride..].iter().all(|&byte| byte == 0),
            "nondrawn high-water tail is cleared"
        );
    }
    assert_eq!(
        meshes.len(),
        1,
        "every visible and hidden ordinal shares one pool asset"
    );
    assert_eq!(materials.len(), pool.variants.len() + inputs.len());
    assert!(pool.variants.len() <= inputs.len());
}

#[test]
fn changing_run_sizes_keep_one_pool_and_exact_contiguous_source_bytes() {
    for (previous, requested) in [
        (vec![3, 6], vec![9, 3]),
        (vec![6, 3], vec![9, 6]),
        (vec![3, 3, 6], vec![3, 6, 3]),
        (vec![3, 6, 3, 9], vec![12, 3, 6, 3, 9]),
        (vec![3, 6, 3, 9, 12], vec![9, 3]),
        (vec![3, 3, 6, 6, 9], vec![6, 3, 12, 9, 3]),
        (vec![3, 3, 6], vec![6, 6, 6, 3]),
        (vec![], vec![3, 6]),
        (vec![3, 6], vec![]),
    ] {
        let (mut app, inputs) = scene();
        frame(&mut app, &inputs, &previous, 0);
        let before = app.world().resource::<Pool>().mesh.clone();
        frame(&mut app, &inputs, &requested, 1);
        let pool = app.world().resource::<Pool>();
        if let Some(before) = before {
            assert_eq!(pool.mesh.as_ref().unwrap(), &before);
        }
        let mesh = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(pool.mesh.as_ref().unwrap())
            .unwrap();
        assert_eq!(
            mesh.count_vertices(),
            previous.iter().sum::<usize>().max(requested.iter().sum())
        );
        assert_eq!(pool.runs.len(), previous.len().max(requested.len()));
    }
}

#[test]
fn hidden_slots_reenter_with_steady_entities_materials_and_pool_handle() {
    let (mut app, inputs) = scene();
    frame(&mut app, &inputs, &[3, 6, 9], 0);
    let before = identity(&app);
    frame(&mut app, &inputs, &[9], 1);
    assert_eq!(identity(&app), before);
    frame(&mut app, &inputs, &[6, 3, 9], 2);
    assert_eq!(identity(&app), before);
    frame(&mut app, &inputs, &[], 3);
    assert_eq!(identity(&app), before);
    frame(&mut app, &inputs, &[3, 6, 9], 4);
    assert_eq!(identity(&app), before);
}

#[test]
fn growth_and_map_reload_retire_the_shared_pool_and_every_draw_binding() {
    let (mut app, inputs) = scene();
    frame(&mut app, &inputs, &[3, 6], 0);
    let before = identity(&app);
    frame(&mut app, &inputs, &[6, 3, 9, 12], 1);
    let grown = identity(&app);
    assert_eq!(grown.len(), 4);
    assert_eq!(&grown[..before.len()], &before);
    frame(&mut app, &inputs, &[12], 2);
    frame(&mut app, &inputs, &[12, 9, 6, 3], 3);
    assert_eq!(identity(&app), grown);
    app.world_mut().despawn(grown[1].0);
    app.update();
    let pool = app.world().resource::<Pool>();
    assert!(pool.runs.is_empty() && pool.mesh.is_none() && pool.variants.is_empty());
    assert!(app.world().resource::<Assets<Mesh>>().is_empty());
    assert_eq!(
        app.world().resource::<Assets<SourceMaterial>>().len(),
        inputs.len()
    );
    for &(entity, mesh, material) in &grown {
        assert!(app.world().get_entity(entity).is_err());
        assert!(!app.world().resource::<Assets<Mesh>>().contains(mesh));
        assert!(
            !app.world()
                .resource::<Assets<SourceMaterial>>()
                .contains(material)
        );
    }
    frame(&mut app, &inputs, &[3, 6], 4);
    let reloaded = identity(&app);
    assert_ne!(reloaded[0].1, grown[0].1);
    assert!(
        reloaded
            .iter()
            .all(|new| grown.iter().all(|old| old.0 != new.0))
    );
}

#[test]
fn shared_source_variants_rebind_runs_and_update_their_exact_render_state() {
    let (mut app, inputs) = scene();
    frame(&mut app, &inputs, &[3, 6, 9], 0);
    let before = identity(&app);
    assert_eq!(before[0].2, before[2].2);
    assert_ne!(before[0].2, before[1].2);
    let reversed = [inputs[1].clone(), inputs[0].clone()];
    app.world_mut()
        .resource_mut::<Assets<SourceMaterial>>()
        .get_mut(&inputs[0])
        .unwrap()
        .parameters
        .point_ambient[1]
        .w = -0.0;
    frame(&mut app, &reversed, &[3, 6, 9], 1);
    let after = identity(&app);
    assert_eq!(
        [after[0].2, after[1].2, after[2].2],
        [before[1].2, before[0].2, before[1].2]
    );
    assert!(
        before
            .iter()
            .zip(&after)
            .all(|(old, new)| (old.0, old.1) == (new.0, new.1))
    );
    let materials = app.world().resource::<Assets<SourceMaterial>>();
    assert_eq!(
        materials.get(after[1].2).unwrap().parameters.point_ambient[1]
            .w
            .to_bits(),
        (-0.0_f32).to_bits()
    );
    assert_eq!(
        materials.get(after[0].2).unwrap().parameters.point_ambient[1]
            .w
            .to_bits(),
        0.0_f32.to_bits()
    );
}

fn component_ticks(app: &App) -> Vec<[u32; 5]> {
    app.world()
        .resource::<Pool>()
        .runs
        .iter()
        .map(|run| {
            let entity = app.world().entity(run.entity);
            [
                entity.get_change_ticks::<Visibility>(),
                entity.get_change_ticks::<InheritedVisibility>(),
                entity.get_change_ticks::<RenderLayers>(),
                entity.get_change_ticks::<SortedRange>(),
                entity.get_change_ticks::<MeshMaterial3d<SourceMaterial>>(),
            ]
            .map(|ticks| ticks.unwrap().changed.get())
        })
        .collect()
}

#[test]
fn retained_runs_write_only_changed_ranges_materials_and_visibility_transitions() {
    let (mut app, inputs) = scene();
    frame(&mut app, &inputs, &[3, 3, 3], 0);
    let initial = identity(&app);
    let visible = component_ticks(&app);
    frame(&mut app, &inputs, &[3, 3, 3], 1);
    assert_eq!(component_ticks(&app), visible);

    frame(&mut app, &inputs, &[6], 2);
    let shrunk = component_ticks(&app);
    assert_eq!(shrunk[0][..3], visible[0][..3]);
    assert_ne!(shrunk[0][3], visible[0][3]);
    for i in [1, 2] {
        for field in 0..4 {
            assert_ne!(shrunk[i][field], visible[i][field]);
        }
        assert_eq!(shrunk[i][4], visible[i][4]);
    }
    frame(&mut app, &inputs, &[6], 3);
    assert_eq!(component_ticks(&app), shrunk);
    frame(&mut app, &inputs, &[], 4);
    let idle = component_ticks(&app);
    assert_eq!(idle[1..], shrunk[1..]);
    frame(&mut app, &inputs, &[], 5);
    assert_eq!(component_ticks(&app), idle);

    frame(&mut app, &inputs, &[3, 3, 3], 6);
    assert_eq!(identity(&app), initial);
    let restored = component_ticks(&app);
    for (before, after) in idle.iter().zip(&restored) {
        for field in 0..4 {
            assert_ne!(before[field], after[field]);
        }
        assert_eq!(before[4], after[4]);
    }
    let swapped = [inputs[1].clone(), inputs[0].clone()];
    frame(&mut app, &swapped, &[3, 3, 3], 7);
    for (before, after) in restored.iter().zip(component_ticks(&app)) {
        assert_eq!(before[..4], after[..4]);
        assert_ne!(before[4], after[4]);
    }
}
