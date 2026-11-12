//! Indexed neighbors, changed triangle order and allocator fallbacks on a real device.
use super::*;

fn indexed(indices: Vec<u32>) -> Mesh {
    triangle().with_inserted_indices(Indices::U32(indices))
}

fn allocate(world: &mut World, meshes: Vec<(AssetId<Mesh>, Mesh)>) {
    for (id, mesh) in &meshes {
        let previous = prepared(mesh, &mut world.resource_mut::<MeshVertexBufferLayouts>());
        world
            .resource_mut::<RenderAssets<RenderMesh>>()
            .insert(*id, previous);
    }
    let added = meshes.iter().map(|(id, _)| *id).collect();
    world.insert_resource(ExtractedAssets::<RenderMesh> {
        extracted: meshes,
        added,
        ..default()
    });
    world.run_system_once(allocate_and_free_meshes).unwrap();
}

fn read_mesh(world: &World, id: AssetId<Mesh>, mesh: &Mesh) {
    read_vertices(world, id, &mesh.create_packed_vertex_buffer_data());
    if let Some(bytes) = mesh.get_index_buffer_bytes() {
        let width = match mesh.indices().unwrap() {
            Indices::U16(_) => 2,
            Indices::U32(_) => 4,
        };
        let slice = world
            .resource::<MeshAllocator>()
            .mesh_index_slice(&id)
            .unwrap();
        assert_eq!(
            usize::try_from(slice.range.end - slice.range.start).unwrap(),
            mesh.indices().unwrap().len()
        );
        read_buffer(
            world.resource::<RenderDevice>(),
            world.resource::<RenderQueue>(),
            slice.buffer,
            u64::from(slice.range.start) * width,
            bytes,
        );
    } else {
        assert!(
            world
                .resource::<MeshAllocator>()
                .mesh_index_slice(&id)
                .is_none()
        );
    }
}

fn assert_shared_slabs(allocator: &MeshAllocator, ids: &[AssetId<Mesh>]) {
    let first = allocator.mesh_slabs(&ids[0]).unwrap();
    for id in &ids[1..] {
        let slabs = allocator.mesh_slabs(id).unwrap();
        assert_eq!(slabs.vertex_slab_id, first.vertex_slab_id);
        assert_eq!(slabs.index_slab_id, first.index_slab_id);
    }
}

#[ignore = "requires GPU device"]
#[test]
fn indexed_updates_copy_current_indices_and_vertices_after_both_slabs_grow() {
    let mut world = gpu_world();
    let mut assets = Assets::<Mesh>::default();
    let original = indexed(vec![0, 1, 2, 2, 1, 0]);
    let handles: Vec<_> = (0..4).map(|_| assets.add(original.clone())).collect();
    let ids: Vec<_> = handles.iter().map(Handle::id).collect();
    allocate(
        &mut world,
        ids[..3].iter().map(|id| (*id, original.clone())).collect(),
    );
    let allocator = world.resource::<MeshAllocator>();
    let old_vertex = allocator.mesh_vertex_slice(&ids[0]).unwrap().buffer.id();
    let old_index = allocator.mesh_index_slice(&ids[0]).unwrap().buffer.id();
    let old_vertex_range = allocator.mesh_vertex_slice(&ids[0]).unwrap().range;
    let old_index_range = allocator.mesh_index_slice(&ids[0]).unwrap().range;
    let slabs = allocator.mesh_slabs(&ids[0]).unwrap();
    assert_shared_slabs(allocator, &ids[..3]);
    let changed = indexed(vec![2, 0, 1, 0, 1, 2])
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[9.0, -0.0, 7.0]; 3]);
    let other = indexed(vec![1, 2, 0, 1, 0, 2]);
    let mut large = original.clone();
    for _ in 0..11 {
        large.merge(&original).unwrap();
    }
    large.insert_indices(Indices::U32([0, 1, 2].repeat(200)));
    let mut extracted = ExtractedAssets::<RenderMesh> {
        extracted: vec![
            (ids[2], other.clone()),
            (ids[3], large.clone()),
            (ids[0], changed.clone()),
        ],
        ..default()
    };
    extracted.added.extend([ids[2], ids[3], ids[0]]);
    extracted.modified.extend([ids[2], ids[0]]);
    world.insert_resource(extracted);
    world
        .resource_mut::<ModelMeshes>()
        .0
        .extend(ids.iter().copied());
    world.run_system_once(retain_allocations).unwrap();
    assert_eq!(world.resource::<Uploads>().0.len(), 2);
    world.run_system_once(allocate_and_free_meshes).unwrap();
    let allocator = world.resource::<MeshAllocator>();
    assert_eq!(
        allocator.mesh_slabs(&ids[3]).unwrap().vertex_slab_id,
        slabs.vertex_slab_id
    );
    assert_eq!(
        allocator.mesh_slabs(&ids[3]).unwrap().index_slab_id,
        slabs.index_slab_id
    );
    assert_ne!(
        allocator.mesh_vertex_slice(&ids[0]).unwrap().buffer.id(),
        old_vertex
    );
    assert_ne!(
        allocator.mesh_index_slice(&ids[0]).unwrap().buffer.id(),
        old_index
    );
    assert_eq!(
        allocator.mesh_vertex_slice(&ids[0]).unwrap().range,
        old_vertex_range
    );
    assert_eq!(
        allocator.mesh_index_slice(&ids[0]).unwrap().range,
        old_index_range
    );
    world.run_system_once(upload_vertices).unwrap();
    assert!(world.resource::<Uploads>().0.is_empty());
    assert_eq!(
        world
            .resource::<ExtractedAssets<RenderMesh>>()
            .extracted
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [ids[2], ids[3], ids[0]]
    );
    for (id, mesh) in ids.iter().zip([changed, original, other, large]) {
        read_mesh(&world, *id, &mesh);
    }
}

#[ignore = "requires GPU device"]
#[test]
fn index_count_format_removal_and_retirement_use_ordinary_allocator() {
    let mut world = gpu_world();
    let mut assets = Assets::<Mesh>::default();
    let original = indexed(vec![0, 1, 2, 2, 1, 0]);
    let handle = assets.add(original.clone());
    let neighbor = assets.add(original.clone());
    let id = handle.id();
    allocate(
        &mut world,
        vec![(id, original.clone()), (neighbor.id(), original.clone())],
    );
    world.resource_mut::<ModelMeshes>().0.insert(id);
    let cases = [
        indexed(vec![2, 1, 0]),
        indexed(vec![0, 1, 2, 2, 0, 1]),
        triangle().with_inserted_indices(Indices::U16(vec![0, 1, 2, 2, 1, 0])),
        triangle().with_inserted_indices(Indices::U16(vec![2, 1, 0, 1, 0, 2])),
        triangle(),
        original.clone(),
        original.clone(),
    ];
    for (index, mesh) in cases.into_iter().enumerate() {
        let mut extracted = ExtractedAssets::<RenderMesh> {
            extracted: vec![(id, mesh.clone())],
            ..default()
        };
        extracted.added.insert(id);
        extracted.modified.insert(id);
        if index == 6 {
            extracted.removed.insert(id);
        }
        world.insert_resource(extracted);
        world.run_system_once(retain_allocations).unwrap();
        assert_eq!(world.resource::<Uploads>().0.len(), usize::from(index == 3));
        world.run_system_once(allocate_and_free_meshes).unwrap();
        world.run_system_once(upload_vertices).unwrap();
        read_mesh(&world, id, &mesh);
        read_mesh(&world, neighbor.id(), &original);
        let previous = prepared(&mesh, &mut world.resource_mut::<MeshVertexBufferLayouts>());
        world
            .resource_mut::<RenderAssets<RenderMesh>>()
            .insert(id, previous);
    }
    let mut extracted = ExtractedAssets::<RenderMesh>::default();
    extracted.removed.insert(id);
    world.insert_resource(extracted);
    world.run_system_once(retain_allocations).unwrap();
    world.run_system_once(allocate_and_free_meshes).unwrap();
    assert!(
        world
            .resource::<MeshAllocator>()
            .mesh_vertex_slice(&id)
            .is_none()
    );
    assert!(
        world
            .resource::<MeshAllocator>()
            .mesh_index_slice(&id)
            .is_none()
    );
    assert_eq!(
        world.resource::<MeshAllocator>().index_allocation_count(),
        1
    );
    read_mesh(&world, neighbor.id(), &original);
}
