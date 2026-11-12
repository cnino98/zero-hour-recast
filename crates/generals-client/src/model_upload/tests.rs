use super::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{BaseMeshPipelineKey, Indices},
    render::{mesh::RenderMeshBufferInfo, render_resource::PrimitiveTopology},
};

mod gpu;

#[test]
fn extraction_tracks_sorted_run_lifetime_without_admitting_ordinary_sources() {
    use crate::{
        model_animation::AnimatedPart, model_render::PartBinding, sorted_view::SortedBatch,
    };
    use bevy::render::MainWorld;
    use std::sync::Arc;

    let mut assets = Assets::<Mesh>::default();
    let handles: Vec<_> = (0..3).map(|_| assets.add(triangle())).collect();
    let mut world = World::new();
    world.init_resource::<MainWorld>();
    world.insert_resource(ModelMeshes(BTreeSet::new()));
    let mut schedule = Schedule::new(ExtractSchedule);
    schedule.add_systems(extract_models);
    world.add_schedule(schedule);
    let (model, run) = {
        let mut main = world.resource_mut::<MainWorld>();
        let model = main
            .spawn((
                Mesh3d(handles[0].clone()),
                AnimatedPart {
                    model: 0,
                    binding: Arc::new(PartBinding {
                        mesh: 0,
                        pass: 0,
                        vertices: vec![0, 1, 2],
                    }),
                },
            ))
            .id();
        let run = main
            .spawn((Mesh3d(handles[1].clone()), SortedBatch, Visibility::Visible))
            .id();
        main.spawn(Mesh3d(handles[2].clone()));
        (model, run)
    };
    world.run_schedule(ExtractSchedule);
    let expected = BTreeSet::from([handles[0].id(), handles[1].id()]);
    assert_eq!(world.resource::<ModelMeshes>().0, expected);
    // An inactive high-water run still owns its reusable allocation. Hidden
    // output is not submitted, but showing it again must not lose eligibility.
    world
        .resource_mut::<MainWorld>()
        .entity_mut(run)
        .insert(Visibility::Hidden);
    world.run_schedule(ExtractSchedule);
    assert_eq!(world.resource::<ModelMeshes>().0, expected);
    world.resource_mut::<MainWorld>().despawn(run);
    world.run_schedule(ExtractSchedule);
    assert_eq!(
        world.resource::<ModelMeshes>().0,
        BTreeSet::from([handles[0].id()])
    );
    world.resource_mut::<MainWorld>().despawn(model);
    world.run_schedule(ExtractSchedule);
    assert!(world.resource::<ModelMeshes>().0.is_empty());
}

fn triangle() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 0.0, 1.0]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.5_f32, 0.25]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.25_f32, 0.5, 0.75, 1.0]; 3])
}

fn prepared(mesh: &Mesh, layouts: &mut MeshVertexBufferLayouts) -> RenderMesh {
    RenderMesh {
        vertex_count: u32::try_from(mesh.count_vertices()).unwrap(),
        aabb_center: Vec3::ZERO,
        buffer_info: mesh
            .indices()
            .map_or(RenderMeshBufferInfo::NonIndexed, |indices| {
                RenderMeshBufferInfo::Indexed {
                    count: u32::try_from(indices.len()).unwrap(),
                    index_format: indices.into(),
                }
            }),
        key_bits: BaseMeshPipelineKey::from_primitive_topology_and_strip_index(
            mesh.primitive_topology(),
            mesh.indices().map(Into::into),
        ),
        layout: mesh.get_mesh_vertex_buffer_layout(layouts),
    }
}

#[test]
fn reuse_requires_exact_layout_count_topology_and_no_index_or_morph_buffer() {
    let mut layouts = MeshVertexBufferLayouts::default();
    let mesh = triangle();
    let mut previous = prepared(&mesh, &mut layouts);
    assert!(compatible(&mesh, &previous, &mut layouts));
    // Different final positions are precisely the intended upload, not a
    // reason to discard an allocation of the same layout and capacity.
    let moved =
        triangle().with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[4.0_f32, -0.0, 7.0]; 3]);
    assert!(compatible(&moved, &previous, &mut layouts));
    previous.vertex_count = 4;
    assert!(!compatible(&mesh, &previous, &mut layouts));
    previous.vertex_count = 3;
    let fewer = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 2]);
    assert!(!compatible(&fewer, &previous, &mut layouts));
    let different_layout =
        triangle().with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0_f32; 2]; 3]);
    assert!(!compatible(&different_layout, &previous, &mut layouts));
    let indexed = triangle().with_inserted_indices(Indices::U32(vec![0, 1, 2]));
    assert!(!compatible(&indexed, &previous, &mut layouts));
    previous.key_bits = BaseMeshPipelineKey::from_primitive_topology_and_strip_index(
        PrimitiveTopology::LineList,
        None,
    );
    assert!(!compatible(&mesh, &previous, &mut layouts));
    previous = prepared(&mesh, &mut layouts);
    previous.key_bits |= BaseMeshPipelineKey::MORPH_TARGETS;
    assert!(!compatible(&mesh, &previous, &mut layouts));
    previous = prepared(&mesh, &mut layouts);
    previous.buffer_info = RenderMeshBufferInfo::Indexed {
        count: 3,
        index_format: bevy::render::render_resource::IndexFormat::Uint32,
    };
    assert!(!compatible(&mesh, &previous, &mut layouts));
}

#[test]
fn allocator_filter_restores_added_modified_removed_order_and_every_vertex_byte() {
    let mut assets = Assets::<Mesh>::default();
    let handles: Vec<_> = (0..5).map(|_| assets.add(triangle())).collect();
    let ids: Vec<_> = handles.iter().map(Handle::id).collect();
    let mut extracted = ExtractedAssets::<RenderMesh>::default();
    for (index, &id) in (0_u16..).zip(&ids) {
        let changed = triangle().with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[f32::from(index), -0.0_f32, 7.5]; 3],
        );
        extracted.extracted.push((id, changed));
        extracted.added.insert(id);
    }
    extracted.modified.extend([ids[0], ids[2], ids[4]]);
    // Unused/removal wins over reuse even if the same ID is re-extracted.
    extracted.removed.insert(ids[2]);
    let expected: Vec<_> = extracted
        .extracted
        .iter()
        .map(|(id, mesh)| (*id, mesh.create_packed_vertex_buffer_data()))
        .collect();
    let mut pending = Uploads(Vec::new());
    pending.hold(
        &mut extracted,
        &BTreeSet::from([ids[0], ids[1], ids[2], ids[4]]),
    );
    assert_eq!(
        pending.0.iter().map(|upload| upload.id).collect::<Vec<_>>(),
        vec![ids[0], ids[1], ids[4]]
    );
    assert_eq!(
        extracted
            .extracted
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        vec![ids[2], ids[3]]
    );
    assert_eq!(extracted.modified.len(), 1);
    assert!(extracted.modified.contains(&ids[2]));
    assert!(extracted.removed.contains(&ids[2]));
    pending.restore(&mut extracted);
    assert!(pending.0.is_empty());
    assert_eq!(
        extracted
            .extracted
            .iter()
            .map(|(id, mesh)| (*id, mesh.create_packed_vertex_buffer_data()))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(extracted.added.len(), 5);
    assert_eq!(extracted.modified.len(), 3);
    assert_eq!(extracted.removed.len(), 1);
    assert!(extracted.modified.contains(&ids[0]));
    assert!(extracted.modified.contains(&ids[4]));
    assert!(extracted.removed.contains(&ids[2]));
}

#[test]
fn indexed_reuse_checks_count_format_alignment_and_uploads_current_order() {
    let mut layouts = MeshVertexBufferLayouts::default();
    let mesh = triangle().with_inserted_indices(Indices::U32(vec![0, 1, 2, 2, 1, 0]));
    let previous = prepared(&mesh, &mut layouts);
    let reordered = triangle().with_inserted_indices(Indices::U32(vec![2, 0, 1, 0, 1, 2]));
    assert!(compatible(&reordered, &previous, &mut layouts));
    assert_ne!(
        mesh.get_index_buffer_bytes(),
        reordered.get_index_buffer_bytes()
    );
    assert_eq!(
        upload_size(&reordered),
        reordered.get_vertex_buffer_size() + 24
    );
    let fewer = triangle().with_inserted_indices(Indices::U32(vec![0, 1, 2]));
    assert!(!compatible(&fewer, &previous, &mut layouts));
    let narrow = triangle().with_inserted_indices(Indices::U16(vec![0, 1, 2, 2, 1, 0]));
    assert!(!compatible(&narrow, &previous, &mut layouts));
    let narrow_previous = prepared(&narrow, &mut layouts);
    assert!(compatible(&narrow, &narrow_previous, &mut layouts));
    let odd = triangle().with_inserted_indices(Indices::U16(vec![0, 1, 2]));
    let odd_previous = prepared(&odd, &mut layouts);
    assert!(!compatible(&odd, &odd_previous, &mut layouts));
    let empty = triangle().with_inserted_indices(Indices::U32(vec![]));
    let empty_previous = prepared(&empty, &mut layouts);
    assert!(!compatible(&empty, &empty_previous, &mut layouts));
    assert!(!compatible(&triangle(), &previous, &mut layouts));
}

#[test]
fn extraction_tracks_surface_ownership_but_not_unmarked_indexed_meshes() {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::MainWorld;
    let mut assets = Assets::<Mesh>::default();
    let marked = assets.add(triangle().with_inserted_indices(Indices::U32(vec![0, 1, 2])));
    let ordinary = assets.add(triangle().with_inserted_indices(Indices::U32(vec![0, 1, 2])));
    let mut world = World::new();
    world.init_resource::<MainWorld>();
    world.insert_resource(ModelMeshes(BTreeSet::new()));
    let surface = world
        .resource_mut::<MainWorld>()
        .spawn((Mesh3d(marked.clone()), crate::terrain::changes::Surface(7)))
        .id();
    world.resource_mut::<MainWorld>().spawn(Mesh3d(ordinary));
    world.run_system_once(extract_models).unwrap();
    assert_eq!(
        world.resource::<ModelMeshes>().0,
        BTreeSet::from([marked.id()])
    );
    world.resource_mut::<MainWorld>().despawn(surface);
    world.run_system_once(extract_models).unwrap();
    assert!(world.resource::<ModelMeshes>().0.is_empty());
}
