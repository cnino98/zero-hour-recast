//! Real-device readback through the actual allocator; no window is required.
use super::*;
use bevy::{
    ecs::system::RunSystemOnce,
    render::{
        mesh::allocator::MeshAllocatorSettings,
        render_resource::{DownlevelFlags, MapMode, PollType},
        renderer::{RenderAdapter, initialize_renderer},
        settings::{Backends, WgpuSettings},
        slab_allocator::SlabAllocatorSettings,
    },
};
use std::time::Duration;

mod indexed;

#[test]
#[ignore = "Explicit real-device vertex-packing measurement; run without competing work"]
fn mapped_vertex_packing_workload() {
    let world = gpu_world();
    let device = world.resource::<RenderDevice>();
    let queue = world.resource::<RenderQueue>();
    for (vertices, copies) in [(6, 1024), (300, 128), (3000, 64)] {
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[7.0_f32, 8.0, 9.0]; vertices],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 0.0, 1.0]; vertices])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.5_f32, 0.25]; vertices])
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            vec![[0.25_f32, 0.5, 0.75, 1.0]; vertices],
        );
        let expected = mesh.create_packed_vertex_buffer_data().repeat(copies);
        for scratch in [false, true] {
            let staging = device.create_buffer(&BufferDescriptor {
                label: Some("vertex packing measurement"),
                size: u64::try_from(expected.len()).unwrap(),
                usage: BufferUsages::COPY_SRC,
                mapped_at_creation: true,
            });
            let mut samples = Vec::new();
            for frame in 0..35 {
                let mut mapped = staging.slice(..).get_mapped_range_mut();
                let start = std::time::Instant::now();
                let size = mesh.get_vertex_buffer_size();
                for index in 0..copies {
                    let mut destination = mapped.slice(index * size..(index + 1) * size);
                    if scratch {
                        destination.copy_from_slice(&mesh.create_packed_vertex_buffer_data());
                    } else {
                        mesh.write_packed_vertex_buffer_data(destination);
                    }
                }
                if frame >= 5 {
                    samples.push(start.elapsed());
                }
            }
            staging.unmap();
            check_staging_bytes(device, queue, &staging, &expected);
            samples.sort_unstable();
            eprintln!(
                "VERTEX_PACKING vertices={vertices} copies={copies} scratch={scratch} bytes={} p50_us={} max_us={} exact_bytes=true",
                expected.len(),
                samples[15].as_micros(),
                samples[29].as_micros()
            );
        }
    }
}

fn check_staging_bytes(
    device: &RenderDevice,
    queue: &RenderQueue,
    staging: &bevy::render::render_resource::Buffer,
    expected: &[u8],
) {
    let size = u64::try_from(expected.len()).unwrap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("vertex packing measurement readback"),
        size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(staging, 0, &readback, 0, size);
    queue.submit([encoder.finish()]);
    let (sent, received) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sent.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    received
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    assert_eq!(&*readback.slice(..).get_mapped_range(), expected);
    readback.unmap();
}

fn gpu_world() -> World {
    let resources = bevy::tasks::block_on(initialize_renderer(
        Backends::PRIMARY,
        None,
        &WgpuSettings::default(),
    ));
    let mut world = World::new();
    world.insert_resource(resources.0);
    world.insert_resource(resources.1);
    world.insert_resource(resources.3);
    world.insert_resource(resources.4);
    assert!(
        world
            .resource::<RenderAdapter>()
            .get_downlevel_capabilities()
            .flags
            .contains(DownlevelFlags::BASE_VERTEX),
        "test requires shared vertex slabs"
    );
    world.insert_resource(MeshAllocatorSettings {
        slab_allocator_settings: SlabAllocatorSettings {
            min_slab_size: 512,
            max_slab_size: 8192,
            large_threshold: 8192,
            growth_factor: 2.0,
        },
        extra_buffer_usages: BufferUsages::COPY_SRC,
    });
    world.init_resource::<MeshAllocator>();
    world.init_resource::<MeshVertexBufferLayouts>();
    world.init_resource::<RenderAssets<RenderMesh>>();
    world.init_resource::<ExtractedAssets<RenderMesh>>();
    world.insert_resource(ModelMeshes(BTreeSet::new()));
    world.insert_resource(Uploads(Vec::new()));
    world
}

fn read_vertices(world: &World, id: AssetId<Mesh>, expected: &[u8]) {
    let device = world.resource::<RenderDevice>();
    let queue = world.resource::<RenderQueue>();
    let allocator = world.resource::<MeshAllocator>();
    let slice = allocator.mesh_vertex_slice(&id).unwrap();
    let size = u64::try_from(expected.len()).unwrap();
    // Position + normal + UV0 + color use the original 48-byte packed layout.
    let stride = 48;
    assert_eq!(
        size,
        u64::from(slice.range.end - slice.range.start) * stride
    );
    read_buffer(
        device,
        queue,
        slice.buffer,
        u64::from(slice.range.start) * stride,
        expected,
    );
}

fn read_buffer(
    device: &RenderDevice,
    queue: &RenderQueue,
    buffer: &bevy::render::render_resource::Buffer,
    offset: u64,
    expected: &[u8],
) {
    let size = u64::try_from(expected.len()).unwrap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("retained model readback"),
        size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("retained model readback copy"),
    });
    encoder.copy_buffer_to_buffer(buffer, offset, &readback, 0, size);
    queue.submit([encoder.finish()]);
    let (sent, received) = std::sync::mpsc::channel();
    readback.slice(..).map_async(MapMode::Read, move |result| {
        sent.send(result).unwrap();
    });
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    received
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    assert_eq!(&*readback.slice(..).get_mapped_range(), expected);
    readback.unmap();
}

fn allocate_initial_meshes(world: &mut World, ids: &[AssetId<Mesh>]) {
    for &id in ids {
        let previous = prepared(
            &triangle(),
            &mut world.resource_mut::<MeshVertexBufferLayouts>(),
        );
        world
            .resource_mut::<RenderAssets<RenderMesh>>()
            .insert(id, previous);
        let mut extracted = world.resource_mut::<ExtractedAssets<RenderMesh>>();
        extracted.extracted.push((id, triangle()));
        extracted.added.insert(id);
    }
    world.run_system_once(allocate_and_free_meshes).unwrap();
}

#[ignore = "requires GPU device"]
#[test]
fn retained_gpu_uploads_preserve_neighbor_bytes_through_real_slab_growth() {
    let mut world = gpu_world();
    let mut assets = Assets::<Mesh>::default();
    let handles: Vec<_> = (0..4).map(|_| assets.add(triangle())).collect();
    let ids: Vec<_> = handles.iter().map(Handle::id).collect();
    let original = triangle().create_packed_vertex_buffer_data();
    allocate_initial_meshes(&mut world, &ids[..3]);
    let allocator = world.resource::<MeshAllocator>();
    let old_buffer = allocator.mesh_vertex_slice(&ids[0]).unwrap().buffer.id();
    let old_range = allocator.mesh_vertex_slice(&ids[0]).unwrap().range;
    let old_slab = allocator.mesh_slabs(&ids[0]).unwrap().vertex_slab_id;
    assert_eq!(
        old_slab,
        allocator.mesh_slabs(&ids[1]).unwrap().vertex_slab_id
    );
    assert_eq!(
        old_slab,
        allocator.mesh_slabs(&ids[2]).unwrap().vertex_slab_id
    );
    let changed = triangle()
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[9.0_f32, -0.0, 13.5]; 3]);
    let other =
        triangle().with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[-1.0_f32, 0.0, 0.0]; 3]);
    let large = triangle()
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[3.0_f32, 4.0, 5.0]; 36])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 1.0, 0.0]; 36])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.75_f32, 0.25]; 36])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0_f32; 4]; 36]);
    let expected = [
        (ids[0], changed.create_packed_vertex_buffer_data()),
        (ids[1], original),
        (ids[2], other.create_packed_vertex_buffer_data()),
        (ids[3], large.create_packed_vertex_buffer_data()),
    ];
    let mut extracted = ExtractedAssets::<RenderMesh> {
        extracted: vec![(ids[2], other), (ids[3], large), (ids[0], changed)],
        ..default()
    };
    extracted.added.extend([ids[0], ids[2], ids[3]]);
    extracted.modified.extend([ids[0], ids[2]]);
    world.insert_resource(extracted);
    world
        .resource_mut::<ModelMeshes>()
        .0
        .extend(ids.iter().copied());
    world.run_system_once(retain_allocations).unwrap();
    assert_eq!(world.resource::<Uploads>().0.len(), 2);
    assert_eq!(
        world
            .resource::<ExtractedAssets<RenderMesh>>()
            .extracted
            .len(),
        1
    );
    world.run_system_once(allocate_and_free_meshes).unwrap();
    let allocator = world.resource::<MeshAllocator>();
    assert_eq!(
        old_slab,
        allocator.mesh_slabs(&ids[0]).unwrap().vertex_slab_id
    );
    assert_eq!(
        old_slab,
        allocator.mesh_slabs(&ids[3]).unwrap().vertex_slab_id
    );
    assert_ne!(
        old_buffer,
        allocator.mesh_vertex_slice(&ids[0]).unwrap().buffer.id()
    );
    assert_eq!(
        old_range,
        allocator.mesh_vertex_slice(&ids[0]).unwrap().range
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
        vec![ids[2], ids[3], ids[0]]
    );
    for (id, bytes) in expected {
        read_vertices(&world, id, &bytes);
    }
}

#[ignore = "requires GPU device"]
#[test]
fn sorted_runs_reuse_exact_bytes_resize_through_allocator_and_release_on_retirement() {
    use crate::sorted_view::SortedBatch;
    use bevy::render::MainWorld;

    let mut world = gpu_world();
    let mut assets = Assets::<Mesh>::default();
    let handles: Vec<_> = (0..3).map(|_| assets.add(triangle())).collect();
    let ids: Vec<_> = handles.iter().map(Handle::id).collect();
    allocate_initial_meshes(&mut world, &ids);
    world.init_resource::<MainWorld>();
    let first = world
        .resource_mut::<MainWorld>()
        .spawn((Mesh3d(handles[0].clone()), SortedBatch))
        .id();
    let second = world
        .resource_mut::<MainWorld>()
        .spawn((Mesh3d(handles[1].clone()), SortedBatch))
        .id();
    world.run_system_once(extract_models).unwrap();
    assert_eq!(
        world.resource::<ModelMeshes>().0,
        BTreeSet::from([ids[0], ids[1]])
    );

    let changed =
        triangle().with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[8.0_f32, -0.0, 2.0]; 3]);
    let mut resized = triangle();
    resized.merge(&triangle()).unwrap();
    let expected = changed.create_packed_vertex_buffer_data();
    let resized_bytes = resized.create_packed_vertex_buffer_data();
    let resized_prepared = prepared(
        &resized,
        &mut world.resource_mut::<MeshVertexBufferLayouts>(),
    );
    let old_range = world
        .resource::<MeshAllocator>()
        .mesh_vertex_slice(&ids[0])
        .unwrap()
        .range;
    let mut extracted = ExtractedAssets::<RenderMesh> {
        extracted: vec![(ids[0], changed), (ids[1], resized)],
        ..default()
    };
    extracted.modified.extend([ids[0], ids[1]]);
    world.insert_resource(extracted);
    world.run_system_once(retain_allocations).unwrap();
    assert_eq!(
        world
            .resource::<Uploads>()
            .0
            .iter()
            .map(|upload| upload.id)
            .collect::<Vec<_>>(),
        [ids[0]]
    );
    assert_eq!(
        world.resource::<ExtractedAssets<RenderMesh>>().extracted[0].0,
        ids[1]
    );
    world.run_system_once(allocate_and_free_meshes).unwrap();
    world.run_system_once(upload_vertices).unwrap();
    assert_eq!(
        world
            .resource::<MeshAllocator>()
            .mesh_vertex_slice(&ids[0])
            .unwrap()
            .range,
        old_range
    );
    read_vertices(&world, ids[0], &expected);
    read_vertices(&world, ids[1], &resized_bytes);
    read_vertices(
        &world,
        ids[2],
        &triangle().create_packed_vertex_buffer_data(),
    );

    retire_and_shrink_sorted_runs(&mut world, &ids, [first, second], resized_prepared);
}

fn retire_and_shrink_sorted_runs(
    world: &mut World,
    ids: &[AssetId<Mesh>],
    [first, second]: [Entity; 2],
    resized_prepared: RenderMesh,
) {
    use bevy::render::MainWorld;

    // Mirror the prepared descriptor after ordinary RenderMesh preparation;
    // shrinking the same run must still take the ordinary reallocation path.
    world
        .resource_mut::<RenderAssets<RenderMesh>>()
        .insert(ids[1], resized_prepared);
    world.resource_mut::<MainWorld>().despawn(first);
    world.run_system_once(extract_models).unwrap();
    assert_eq!(world.resource::<ModelMeshes>().0, BTreeSet::from([ids[1]]));
    let mut extracted = ExtractedAssets::<RenderMesh> {
        extracted: vec![(ids[1], triangle())],
        ..default()
    };
    extracted.removed.insert(ids[0]);
    extracted.modified.insert(ids[1]);
    world.insert_resource(extracted);
    world.run_system_once(retain_allocations).unwrap();
    assert!(world.resource::<Uploads>().0.is_empty());
    world.run_system_once(allocate_and_free_meshes).unwrap();
    assert!(
        world
            .resource::<MeshAllocator>()
            .mesh_vertex_slice(&ids[0])
            .is_none()
    );
    read_vertices(
        world,
        ids[1],
        &triangle().create_packed_vertex_buffer_data(),
    );
    read_vertices(
        world,
        ids[2],
        &triangle().create_packed_vertex_buffer_data(),
    );

    world.resource_mut::<MainWorld>().despawn(second);
    world.run_system_once(extract_models).unwrap();
    assert!(world.resource::<ModelMeshes>().0.is_empty());
    let mut extracted = ExtractedAssets::<RenderMesh>::default();
    extracted.removed.insert(ids[1]);
    world.insert_resource(extracted);
    world.run_system_once(retain_allocations).unwrap();
    assert!(world.resource::<Uploads>().0.is_empty());
    world.run_system_once(allocate_and_free_meshes).unwrap();
    assert!(
        world
            .resource::<MeshAllocator>()
            .mesh_vertex_slice(&ids[1])
            .is_none()
    );
    read_vertices(
        world,
        ids[2],
        &triangle().create_packed_vertex_buffer_data(),
    );
}
