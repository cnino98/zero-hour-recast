use super::geometry::{self, Triangle};
use bevy::{math::Affine3A, prelude::*};

fn triangle(depth: f32, node: usize) -> Triangle {
    Triangle {
        vertices: [0, 1, 2],
        node,
        depth,
    }
}

#[test]
fn overlapping_effects_interleave_triangles_instead_of_sorting_whole_emitters() {
    // The first emitter surrounds the second in depth. Either possible whole
    // emitter order produces an incorrect alpha result for one of its faces.
    let mut triangles = vec![triangle(-9.0, 0), triangle(-1.0, 0), triangle(-5.0, 1)];
    geometry::sort(&mut triangles);
    assert_eq!(
        triangles.iter().map(|item| item.node).collect::<Vec<_>>(),
        [0, 1, 0]
    );
    let rgb = triangles.iter().fold([0.0; 3], |background, triangle| {
        let source = if triangle.node == 0 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        std::array::from_fn(|channel| {
            let foreground = source[channel] * 0.5;
            let background = background[channel] * 0.5;
            foreground + background
        })
    });
    assert_eq!(
        rgb.map(f32::to_bits),
        [0.625_f32, 0.25, 0.0].map(f32::to_bits)
    );
}

#[test]
fn original_sort_retains_small_equal_depth_order_and_large_partition_permutation() {
    let mut small: Vec<_> = (0..16).map(|node| triangle(-1.0, node)).collect();
    geometry::sort(&mut small);
    assert_eq!(
        small.iter().map(|item| item.node).collect::<Vec<_>>(),
        (0..16).collect::<Vec<_>>()
    );
    let mut large: Vec<_> = (0..17).map(|node| triangle(-1.0, node)).collect();
    geometry::sort(&mut large);
    // Direct source median-of-three partition: pivot0 meets right8; the
    // remaining two subranges are stable insertion sorts at equal depths.
    assert_eq!(
        large.iter().map(|item| item.node).collect::<Vec<_>>(),
        [9, 8, 15, 14, 13, 12, 11, 10, 0, 1, 7, 6, 5, 4, 3, 2, 16]
    );
}

#[test]
fn material_runs_keep_source_uv_color_and_world_transform() {
    let mut source = geometry::empty_mesh();
    source.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0.0, 0.0, -1.0], [1.0, 0.0, -1.0], [0.0, 1.0, -1.0]],
    );
    source.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3]);
    source.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
    );
    source.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.1, 0.2, 0.3, 0.4]; 3]);
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    geometry::append(
        &source,
        Affine3A::from_translation(Vec3::new(10.0, 20.0, -30.0)),
        Affine3A::IDENTITY,
        4,
        &mut vertices,
        &mut triangles,
    );
    assert_eq!(triangles[0].depth.to_bits(), (-31.0_f32).to_bits());
    assert_eq!(triangles[0].node, 4);
    let mut output = geometry::empty_mesh();
    geometry::fill(&mut output, &vertices, &triangles);
    assert_eq!(
        output.attribute(Mesh::ATTRIBUTE_UV_0),
        source.attribute(Mesh::ATTRIBUTE_UV_0)
    );
    assert_eq!(
        output.attribute(Mesh::ATTRIBUTE_COLOR),
        source.attribute(Mesh::ATTRIBUTE_COLOR)
    );
    assert_eq!(
        vertices[0].position.map(f32::to_bits),
        [10.0_f32, 20.0, -31.0].map(f32::to_bits)
    );
    assert!(output.final_aabb.is_some());
}

fn run_bytes(world: &World, entity: Entity) -> Vec<u8> {
    let handle = &world.get::<Mesh3d>(entity).unwrap().0;
    let mesh = world.resource::<Assets<Mesh>>().get(handle).unwrap();
    assert!(
        mesh.asset_usage
            .contains(bevy::asset::RenderAssetUsages::RENDER_WORLD)
    );
    let stride = mesh.get_vertex_buffer_size() / mesh.count_vertices();
    let region = &world.get::<super::SortedRange>(entity).unwrap().0;
    let start = usize::try_from(region.start).unwrap() * stride;
    let end = usize::try_from(region.end).unwrap() * stride;
    mesh.create_packed_vertex_buffer_data()[start..end].to_vec()
}

fn scene() -> (App, Entity, Entity) {
    use super::{Pool, update};
    use crate::{playtest_view::WorldCamera, source_material::SourceMaterial};
    use bevy::camera::visibility::RenderLayers;
    let mut app = App::new();
    app.insert_resource(Assets::<Mesh>::default());
    app.insert_resource(Assets::<SourceMaterial>::default());
    app.insert_resource(Pool {
        vertices: Vec::new(),
        triangles: Vec::new(),
        runs: Vec::new(),
        mesh: None,
        variants: std::collections::HashMap::new(),
        registry: super::submission::Registry::new(),
        sequence: 0,
        instances: std::collections::HashMap::new(),
    });
    super::observe_sources(&mut app);
    app.add_systems(Update, update);
    app.world_mut().spawn((
        WorldCamera,
        GlobalTransform::IDENTITY,
        bevy::camera::primitives::Frustum(
            bevy::math::primitives::ViewFrustum::from_clip_from_world(&Mat4::perspective_rh(
                std::f32::consts::FRAC_PI_2,
                1.0,
                0.1,
                100.0,
            )),
        ),
    ));
    let mut mesh = geometry::empty_mesh();
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0.0, 0.0, -1.0], [1.0, 0.0, -1.0], [0.0, 1.0, -1.0]],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0; 2]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]; 3]);
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
    let material = app
        .world_mut()
        .resource_mut::<Assets<SourceMaterial>>()
        .add(SourceMaterial::new(
            [0.0; 3],
            [1.0; 3],
            [0.0; 3],
            0.5,
            AlphaMode::Blend,
            false,
        ));
    let first = app
        .world_mut()
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            GlobalTransform::from_translation(Vec3::new(0.0, 0.0, -10.0)),
            InheritedVisibility::VISIBLE,
            RenderLayers::layer(3),
        ))
        .id();
    // These tests exercise distinct draw state as well as shared geometry.
    let mut second_material = app
        .world()
        .resource::<Assets<SourceMaterial>>()
        .get(&material)
        .unwrap()
        .clone();
    second_material.parameters.opacity = 0.75;
    let second_material = app
        .world_mut()
        .resource_mut::<Assets<SourceMaterial>>()
        .add(second_material);
    let second = app
        .world_mut()
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(second_material),
            GlobalTransform::from_translation(Vec3::new(0.0, 0.0, -20.0)),
            InheritedVisibility::VISIBLE,
            RenderLayers::layer(3),
        ))
        .id();
    (app, first, second)
}

#[test]
fn shared_mesh_instances_keep_independent_poses_and_reuse_batches_on_steady_frames() {
    use super::Pool;
    let (mut app, first, _) = scene();
    app.update();
    let before: Vec<_> = app
        .world()
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
        .collect();
    assert_eq!(before.len(), 2);
    let depths: Vec<_> = app
        .world()
        .resource::<Pool>()
        .triangles
        .iter()
        .map(|triangle| triangle.depth.to_bits())
        .collect();
    assert_eq!(depths, [-21.0_f32, -11.0].map(f32::to_bits));
    app.update();
    let after: Vec<_> = app
        .world()
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
        .collect();
    assert_eq!(before, after);
    *app.world_mut().get_mut::<GlobalTransform>(first).unwrap() =
        GlobalTransform::from_translation(Vec3::new(0.0, 0.0, -30.0));
    app.update();
    let depths: Vec<_> = app
        .world()
        .resource::<Pool>()
        .triangles
        .iter()
        .map(|triangle| triangle.depth.to_bits())
        .collect();
    assert_eq!(depths, [-31.0_f32, -21.0].map(f32::to_bits));
}

#[test]
fn hidden_sources_leave_pool_and_map_reload_retires_stale_batch_handles() {
    use super::Pool;
    let (mut app, first, second) = scene();
    app.update();
    *app.world_mut()
        .get_mut::<InheritedVisibility>(first)
        .unwrap() = InheritedVisibility::HIDDEN;
    app.update();
    assert_eq!(app.world().resource::<Pool>().triangles.len(), 1);
    let old: Vec<_> = app
        .world()
        .resource::<Pool>()
        .runs
        .iter()
        .map(|run| {
            (
                run.entity,
                app.world().get::<Mesh3d>(run.entity).unwrap().0.clone(),
                run.material.clone(),
            )
        })
        .collect();
    for (entity, mesh, material) in &old {
        app.world_mut().despawn(*entity);
        app.world_mut().resource_mut::<Assets<Mesh>>().remove(mesh);
        app.world_mut()
            .resource_mut::<Assets<crate::source_material::SourceMaterial>>()
            .remove(material);
    }
    app.update();
    assert_eq!(app.world().resource::<Pool>().triangles.len(), 1);
    assert_eq!(app.world().resource::<Pool>().runs.len(), 1);
    assert!(
        !old.iter()
            .any(|(entity, _, _)| *entity == app.world().resource::<Pool>().runs[0].entity)
    );
    assert!(app.world().get_entity(second).is_ok());
}

#[test]
fn offscreen_sources_do_not_enter_or_expand_the_shared_triangle_pool() {
    use super::Pool;
    use crate::source_material::SourceMaterial;
    use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
    let (mut app, first, _) = scene();
    let mesh = app.world().get::<Mesh3d>(first).unwrap().0.clone();
    let material = app
        .world()
        .get::<MeshMaterial3d<SourceMaterial>>(first)
        .unwrap()
        .0
        .clone();
    for _ in 0..1000 {
        app.world_mut().spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            GlobalTransform::from_translation(Vec3::new(1000.0, 0.0, -10.0)),
            InheritedVisibility::VISIBLE,
            RenderLayers::layer(3),
        ));
    }
    app.update();
    assert_eq!(app.world().resource::<Pool>().triangles.len(), 2);
    assert_eq!(app.world().resource::<Pool>().vertices.len(), 6);
    assert_eq!(app.world().resource::<Pool>().runs.len(), 2);
    *app.world_mut().get_mut::<GlobalTransform>(first).unwrap() =
        GlobalTransform::from_translation(Vec3::new(1000.0, 0.0, -10.0));
    app.update();
    assert_eq!(app.world().resource::<Pool>().triangles.len(), 1);
    app.world_mut().entity_mut(first).insert(NoFrustumCulling);
    app.update();
    assert_eq!(app.world().resource::<Pool>().triangles.len(), 2);
}

#[test]
fn model_replacement_joins_scene_head_but_hiding_and_late_parts_keep_membership() {
    use super::{RenderInstance, SubmissionSequence};
    use crate::source_material::SourceMaterial;
    let (mut app, first, _) = scene();
    let material = app
        .world()
        .get::<MeshMaterial3d<SourceMaterial>>(first)
        .unwrap()
        .0
        .clone();
    let instance = RenderInstance::new();
    let old_part = app
        .world_mut()
        .spawn((MeshMaterial3d(material.clone()), instance.clone()))
        .id();
    let old_sequence = app.world().get::<SubmissionSequence>(old_part).unwrap().0;
    let other = app
        .world_mut()
        .spawn((MeshMaterial3d(material.clone()), RenderInstance::new()))
        .id();
    let other_sequence = app.world().get::<SubmissionSequence>(other).unwrap().0;
    app.world_mut()
        .entity_mut(old_part)
        .insert(Visibility::Hidden);
    let late_part = app
        .world_mut()
        .spawn((MeshMaterial3d(material.clone()), instance))
        .id();
    assert_eq!(
        app.world().get::<SubmissionSequence>(late_part).unwrap().0,
        old_sequence
    );
    app.world_mut().despawn(old_part);
    let replacement = app
        .world_mut()
        .spawn((MeshMaterial3d(material), RenderInstance::new()))
        .id();
    assert!(
        app.world()
            .get::<SubmissionSequence>(replacement)
            .unwrap()
            .0
            > other_sequence
    );
    assert!(old_sequence < other_sequence);
}

#[test]
fn cpu_only_particle_sources_keep_exact_sorted_runs_and_direct_transitions() {
    use super::{Pool, SortedSource};
    use crate::particles::ParticleVisual;
    use bevy::{asset::RenderAssetUsages, camera::visibility::RenderLayers};
    let (mut app, first, second) = scene();
    for (system, entity) in [(1, first), (2, second)] {
        app.world_mut().entity_mut(entity).insert(ParticleVisual {
            system,
            generation: 0,
            bounded: false,
        });
    }
    let source_handle = app.world().get::<Mesh3d>(first).unwrap().0.clone();
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&source_handle)
        .unwrap()
        .insert_indices(bevy::mesh::Indices::U32(vec![2, 1, 0]));
    app.update();
    let output = |app: &App| {
        app.world()
            .resource::<Pool>()
            .runs
            .iter()
            .map(|run| (run.material.id(), run_bytes(app.world(), run.entity)))
            .collect::<Vec<_>>()
    };
    let original = output(&app);
    assert_eq!(original.len(), 2);
    let handle = app.world().get::<Mesh3d>(first).unwrap().0.clone();
    let before = app
        .world()
        .resource::<Assets<Mesh>>()
        .get(&handle)
        .unwrap()
        .create_packed_vertex_buffer_data();
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&handle)
        .unwrap()
        .asset_usage = RenderAssetUsages::MAIN_WORLD;
    app.update();
    assert_eq!(output(&app), original);
    assert_eq!(
        app.world()
            .resource::<Assets<Mesh>>()
            .get(&handle)
            .unwrap()
            .create_packed_vertex_buffer_data(),
        before
    );
    for entity in [first, second] {
        assert!(app.world().get::<SortedSource>(entity).is_some());
        assert_eq!(
            app.world().get::<RenderLayers>(entity),
            Some(&RenderLayers::none())
        );
    }
    // A ground streak returning to its single point restores direct membership.
    for entity in [first, second] {
        app.world_mut()
            .entity_mut(entity)
            .insert(RenderLayers::layer(7));
    }
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&handle)
        .unwrap()
        .asset_usage |= RenderAssetUsages::RENDER_WORLD;
    app.update();
    for entity in [first, second] {
        assert!(app.world().get::<SortedSource>(entity).is_none());
        assert_eq!(
            app.world().get::<RenderLayers>(entity),
            Some(&RenderLayers::layer(7))
        );
    }
    for run in &app.world().resource::<Pool>().runs {
        assert!(matches!(
            app.world().get::<Visibility>(run.entity),
            Some(Visibility::Hidden)
        ));
    }
    for entity in [first, second] {
        app.world_mut()
            .entity_mut(entity)
            .insert(RenderLayers::layer(3));
    }
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&handle)
        .unwrap()
        .asset_usage = RenderAssetUsages::MAIN_WORLD;
    app.update();
    assert_eq!(output(&app), original);
    assert_eq!(app.world().get::<Mesh3d>(first).unwrap().0, handle);
}

#[test]
fn equivalent_adjacent_materials_share_a_draw_without_changing_sorted_vertex_bytes() {
    use super::Pool;
    use crate::source_material::SourceMaterial;
    let (mut app, first, second) = scene();
    let first_material = app
        .world()
        .get::<MeshMaterial3d<SourceMaterial>>(first)
        .unwrap()
        .0
        .clone();
    let mut same = app
        .world()
        .resource::<Assets<SourceMaterial>>()
        .get(&first_material)
        .unwrap()
        .clone();
    // Both fields are replaced on output; neither may prevent coalescing.
    same.depth_bias = f32::NAN;
    same.force_sorted_phase = true;
    let same = app
        .world_mut()
        .resource_mut::<Assets<SourceMaterial>>()
        .add(same);
    app.world_mut()
        .entity_mut(second)
        .insert(MeshMaterial3d(same.clone()));
    app.update();
    let world = app.world();
    let pool = world.resource::<Pool>();
    assert_eq!(
        pool.runs.len(),
        1,
        "two distinct material handles with equal draw state need one draw"
    );
    assert_eq!(
        pool.triangles
            .iter()
            .map(|triangle| triangle.depth.to_bits())
            .collect::<Vec<_>>(),
        [-21.0_f32, -11.0].map(f32::to_bits)
    );
    let mut expected = Vec::new();
    for triangle in &pool.triangles {
        // Old node-run output, concatenated in the original global triangle order.
        let mut old = geometry::empty_mesh();
        geometry::fill(&mut old, &pool.vertices, std::slice::from_ref(triangle));
        expected.extend(old.create_packed_vertex_buffer_data());
    }
    assert_eq!(
        world
            .resource::<Assets<Mesh>>()
            .get(pool.mesh.as_ref().unwrap())
            .unwrap()
            .create_packed_vertex_buffer_data(),
        expected
    );
    assert_eq!(
        world
            .resource::<Assets<SourceMaterial>>()
            .get(&pool.runs[0].material)
            .unwrap()
            .depth_bias
            .to_bits(),
        0.0_f32.to_bits()
    );
    // Re-evaluate effective materials every frame: a dynamic change must split.
    app.world_mut()
        .resource_mut::<Assets<SourceMaterial>>()
        .get_mut(&same)
        .unwrap()
        .parameters
        .point_ambient[2]
        .w = 0.25;
    app.update();
    assert_eq!(app.world().resource::<Pool>().runs.len(), 2);
    let world = app.world();
    let pool = world.resource::<Pool>();
    let materials = world.resource::<Assets<SourceMaterial>>();
    assert_eq!(
        materials
            .get(&pool.runs[0].material)
            .unwrap()
            .parameters
            .point_ambient[2]
            .w
            .to_bits(),
        0.25_f32.to_bits()
    );
    assert_eq!(
        materials
            .get(&pool.runs[1].material)
            .unwrap()
            .parameters
            .point_ambient[2]
            .w
            .to_bits(),
        0.0_f32.to_bits()
    );
}

#[test]
fn sorted_draw_equivalence_retains_uniform_float_bits_and_pipeline_bindings() {
    use super::interchangeable_materials;
    use crate::source_material::SourceMaterial;
    let base = SourceMaterial::new([0.0; 3], [1.0; 3], [0.0; 3], 0.5, AlphaMode::Blend, false);
    let mutations: [fn(&mut SourceMaterial); 21] = [
        |m| m.parameters.ambient.x = -0.0,
        |m| m.parameters.diffuse.x = 0.5,
        |m| m.parameters.emissive.x = 0.5,
        |m| m.parameters.scene_ambient.x = 0.5,
        |m| m.parameters.light_color[3].w = 0.5,
        |m| m.parameters.light_direction[2].w = 0.5,
        |m| m.parameters.point_position[1].w = 0.5,
        |m| m.parameters.point_ambient[0].w = 0.5,
        |m| m.parameters.terrain_lights = 1,
        |m| m.parameters.alpha_test = 0.5,
        |m| m.parameters.vertex_diffuse = 3,
        |m| m.parameters.opacity = 0.5,
        |m| m.parameters.primary_gradient = 0,
        |m| m.parameters.shroud_extent.z = -0.0,
        |m| m.alpha_mode = AlphaMode::Add,
        |m| m.double_sided = true,
        |m| m.depth_write = true,
        |m| m.depth_compare = Some(bevy::render::render_resource::CompareFunction::Always),
        |m| m.texture = Some(Handle::default()),
        |m| m.shroud = Some(Handle::default()),
        |m| m.alpha_mode = AlphaMode::Mask(0.25),
    ];
    assert!(interchangeable_materials(&base, &base));
    for (index, change) in mutations.into_iter().enumerate() {
        let mut changed = base.clone();
        change(&mut changed);
        assert!(
            !interchangeable_materials(&base, &changed),
            "draw state {index} cannot merge"
        );
    }
    let mut nan = base.clone();
    nan.parameters.alpha_test = f32::from_bits(0x7fc0_0001);
    assert!(interchangeable_materials(&nan, &nan));
    let mut changed = nan.clone();
    changed.parameters.alpha_test = f32::from_bits(0x7fc0_0002);
    assert!(!interchangeable_materials(&nan, &changed));
    let mut mask = base;
    mask.alpha_mode = AlphaMode::Mask(0.0);
    let mut changed = mask.clone();
    changed.alpha_mode = AlphaMode::Mask(-0.0);
    assert!(!interchangeable_materials(&mask, &changed));
}

#[test]
fn equal_materials_never_merge_across_a_distinct_intervening_sorted_draw() {
    use super::Pool;
    use crate::source_material::SourceMaterial;
    let (mut app, first, _) = scene();
    let mesh = app.world().get::<Mesh3d>(first).unwrap().0.clone();
    let material = app
        .world()
        .get::<MeshMaterial3d<SourceMaterial>>(first)
        .unwrap()
        .0
        .clone();
    app.world_mut().spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        GlobalTransform::from_translation(Vec3::new(0.25, 0.0, -30.0)),
        InheritedVisibility::VISIBLE,
        bevy::camera::visibility::RenderLayers::layer(3),
    ));
    app.update();
    let world = app.world();
    let pool = world.resource::<Pool>();
    assert_eq!(pool.runs.len(), 3, "A/B/A must retain all three draws");
    assert_eq!(
        pool.triangles
            .iter()
            .map(|triangle| triangle.depth.to_bits())
            .collect::<Vec<_>>(),
        [-31.0_f32, -21.0, -11.0].map(f32::to_bits)
    );
    let mut expected = Vec::new();
    for triangle in &pool.triangles {
        let mut old = geometry::empty_mesh();
        geometry::fill(&mut old, &pool.vertices, std::slice::from_ref(triangle));
        expected.extend(old.create_packed_vertex_buffer_data());
    }
    let actual: Vec<_> = pool
        .runs
        .iter()
        .flat_map(|run| run_bytes(world, run.entity))
        .collect();
    assert_eq!(actual, expected);
}
