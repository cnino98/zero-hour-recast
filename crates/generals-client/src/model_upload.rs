//! Retain compatible dynamic mesh allocations for exact vertex and index uploads.
//!
//! W3D's `DX8SkinFVFCategoryContainer` fills shared dynamic vertex buffers without
//! changing posed vertices. Bevy 0.19.1 instead frees every modified allocation
//! in `mesh/allocator.rs::free_meshes`, and wgpu's `write_buffer_with` allocates one
//! staging buffer per write. Keep Bevy's extraction and `RenderMesh` preparation;
//! replace only allocation/upload for compatible model, sorted-run, terrain and water
//! buffers. Changed counts, layouts or index formats use the ordinary allocator.
use bevy::{
    mesh::MeshVertexBufferLayouts,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        mesh::{
            RenderMesh, RenderMeshBufferInfo,
            allocator::{MeshAllocator, allocate_and_free_meshes},
        },
        render_asset::{ExtractedAssets, RenderAssets, prepare_assets},
        render_resource::{Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor},
        renderer::{RenderDevice, RenderQueue},
    },
};
use std::{collections::BTreeSet, time::Instant};

pub mod diagnostics;

#[cfg(test)]
mod tests;

pub struct Plugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stage {
    Retain,
    Upload,
}

#[derive(Resource)]
struct ModelMeshes(BTreeSet<AssetId<Mesh>>);

struct Upload {
    index: usize,
    id: AssetId<Mesh>,
    mesh: Mesh,
    added: bool,
    modified: bool,
}

#[derive(Resource)]
struct Uploads(Vec<Upload>);

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render
            .insert_resource(ModelMeshes(BTreeSet::new()))
            .insert_resource(Uploads(Vec::new()))
            .add_systems(ExtractSchedule, extract_models)
            .add_systems(
                Render,
                (
                    retain_allocations
                        .in_set(Stage::Retain)
                        .before(allocate_and_free_meshes),
                    upload_vertices
                        .in_set(Stage::Upload)
                        .after(allocate_and_free_meshes)
                        .before(prepare_assets::<RenderMesh>),
                )
                    .in_set(RenderSystems::PrepareAssets),
            );
    }
}

type Sources = Or<(
    With<crate::model_animation::AnimatedPart>,
    With<crate::sorted_view::SortedBatch>,
    With<crate::terrain::changes::Surface>,
    With<crate::water_view::WaterVisual>,
)>;

fn extract_models(mut models: Extract<Query<&Mesh3d, Sources>>, mut meshes: ResMut<ModelMeshes>) {
    meshes.0.clear();
    meshes.0.extend(models.iter_mut().map(|mesh| mesh.id()));
}

fn retain_allocations(
    (models, diagnostics): (Res<ModelMeshes>, Option<Res<diagnostics::State>>),
    mut pending: ResMut<Uploads>,
    mut extracted: ResMut<ExtractedAssets<RenderMesh>>,
    prepared: Res<RenderAssets<RenderMesh>>,
    allocator: Res<MeshAllocator>,
    mut layouts: ResMut<MeshVertexBufferLayouts>,
    device: Res<RenderDevice>,
) {
    let models = models.into_inner();
    let prepared = prepared.into_inner();
    let allocator = allocator.into_inner();
    let device = device.into_inner();
    debug_assert!(pending.0.is_empty());
    let mut bytes = 0_u64;
    let mut eligible = BTreeSet::new();
    let mut frame = diagnostics.as_ref().and_then(|state| state.begin());
    let mut count = |id, mesh: &Mesh, outcome| {
        if let Some(state) = &diagnostics {
            state.count(&mut frame, id, mesh, outcome);
        }
    };
    for (id, mesh) in &extracted.extracted {
        if !models.0.contains(id) {
            count(*id, mesh, "outside-sources");
            continue;
        }
        if extracted.removed.contains(id) {
            count(*id, mesh, "removed");
            continue;
        }
        let Some(previous) = prepared.get(*id) else {
            count(*id, mesh, "no-prepared-mesh");
            continue;
        };
        if !compatible(mesh, previous, &mut layouts) {
            if diagnostics.is_some() {
                let reason = diagnostics::incompatible_reason(mesh, previous, &mut layouts);
                count(*id, mesh, reason);
            }
            continue;
        }
        let Some(slice) = allocator.mesh_vertex_slice(id) else {
            count(*id, mesh, "no-allocation");
            continue;
        };
        if slice.range.end - slice.range.start != previous.vertex_count {
            count(*id, mesh, "allocation-range-count");
            continue;
        }
        if !(u64::from(slice.range.start) * previous.layout.0.layout().array_stride)
            .is_multiple_of(4)
        {
            count(*id, mesh, "unaligned-vertex-allocation");
            continue;
        }
        if let RenderMeshBufferInfo::Indexed {
            count: index_count,
            index_format,
        } = previous.buffer_info
        {
            let Some(indices) = allocator.mesh_index_slice(id) else {
                count(*id, mesh, "no-index-allocation");
                continue;
            };
            if indices.range.end - indices.range.start != index_count {
                count(*id, mesh, "index-allocation-range-count");
                continue;
            }
            let width = match index_format {
                bevy::render::render_resource::IndexFormat::Uint16 => 2,
                bevy::render::render_resource::IndexFormat::Uint32 => 4,
            };
            if !(u64::from(indices.range.start) * width).is_multiple_of(4) {
                count(*id, mesh, "unaligned-index-allocation");
                continue;
            }
        }
        let size = u64::try_from(upload_size(mesh)).expect("mesh upload size");
        // Fall back to Bevy when one batch would exceed the actual device limit.
        if size > device.limits().max_buffer_size - bytes {
            count(*id, mesh, "batch-buffer-limit");
            continue;
        }
        bytes += size;
        eligible.insert(*id);
        count(*id, mesh, "retained");
    }
    pending.hold(&mut extracted, &eligible);
    if let Some(state) = &diagnostics {
        state.commit(frame);
    }
}

fn compatible(mesh: &Mesh, previous: &RenderMesh, layouts: &mut MeshVertexBufferLayouts) -> bool {
    compatible_indices(mesh, previous)
        && !mesh.has_morph_targets()
        && !previous.has_morph_targets()
        && mesh.primitive_topology() == previous.primitive_topology()
        && mesh.count_vertices() == usize::try_from(previous.vertex_count).expect("vertex count")
        && mesh.get_vertex_buffer_size() > 0
        && mesh.get_vertex_buffer_size().is_multiple_of(4)
        && mesh.get_mesh_vertex_buffer_layout(layouts) == previous.layout
}

fn compatible_indices(mesh: &Mesh, previous: &RenderMesh) -> bool {
    match (mesh.indices(), &previous.buffer_info) {
        (None, RenderMeshBufferInfo::NonIndexed) => true,
        (
            Some(indices),
            RenderMeshBufferInfo::Indexed {
                count,
                index_format,
            },
        ) => {
            let bytes = mesh.get_index_buffer_bytes().expect("indexed mesh bytes");
            indices.len() == usize::try_from(*count).expect("index count")
                && bevy::render::render_resource::IndexFormat::from(indices) == *index_format
                && !bytes.is_empty()
                && bytes.len().is_multiple_of(4)
        }
        _ => false,
    }
}

fn upload_size(mesh: &Mesh) -> usize {
    mesh.get_vertex_buffer_size()
        .checked_add(mesh.get_index_buffer_bytes().map_or(0, <[u8]>::len))
        .expect("mesh upload size")
}

impl Uploads {
    fn hold(
        &mut self,
        extracted: &mut ExtractedAssets<RenderMesh>,
        eligible: &BTreeSet<AssetId<Mesh>>,
    ) {
        if eligible.is_empty() {
            return;
        }
        let meshes = std::mem::take(&mut extracted.extracted);
        for (index, (id, mesh)) in meshes.into_iter().enumerate() {
            if eligible.contains(&id) && !extracted.removed.contains(&id) {
                self.0.push(Upload {
                    index,
                    id,
                    mesh,
                    added: extracted.added.remove(&id),
                    modified: extracted.modified.remove(&id),
                });
            } else {
                extracted.extracted.push((id, mesh));
            }
        }
    }

    fn restore(&mut self, extracted: &mut ExtractedAssets<RenderMesh>) {
        if self.0.is_empty() {
            return;
        }
        let original_count = extracted.extracted.len() + self.0.len();
        let mut ordinary = std::mem::take(&mut extracted.extracted).into_iter();
        let mut held = self.0.drain(..).peekable();
        extracted.extracted.reserve(original_count);
        for index in 0..original_count {
            if held.peek().is_some_and(|upload| upload.index == index) {
                let upload = held.next().expect("retained mesh");
                if upload.added {
                    extracted.added.insert(upload.id);
                }
                if upload.modified {
                    extracted.modified.insert(upload.id);
                }
                extracted.extracted.push((upload.id, upload.mesh));
            } else {
                extracted
                    .extracted
                    .push(ordinary.next().expect("ordinary extracted mesh"));
            }
        }
    }
}

fn upload_vertices(
    mut pending: ResMut<Uploads>,
    mut extracted: ResMut<ExtractedAssets<RenderMesh>>,
    allocator: Res<MeshAllocator>,
    prepared: Res<RenderAssets<RenderMesh>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    diagnostics: Option<Res<diagnostics::State>>,
) {
    let allocator = allocator.into_inner();
    let prepared = prepared.into_inner();
    let device = device.into_inner();
    let queue = queue.into_inner();
    let diagnostics = diagnostics.map(Res::into_inner);
    if pending.0.is_empty() {
        return;
    }
    let staging = stage_uploads(&pending.0, device, diagnostics);
    let encoding_start = diagnostics.as_ref().map(|_| Instant::now());
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("retained mesh vertex and index copies"),
    });
    let mut offset = 0;
    for upload in &pending.0 {
        // Query after Bevy allocation: another mesh may have grown this slab
        // and copied resident data into a replacement buffer this frame.
        let slice = allocator
            .mesh_vertex_slice(&upload.id)
            .expect("retained vertex allocation");
        let previous = prepared.get(upload.id).expect("retained prepared model");
        let stride = previous.layout.0.layout().array_stride;
        let size = u64::try_from(upload.mesh.get_vertex_buffer_size()).expect("vertex buffer size");
        encoder.copy_buffer_to_buffer(
            &staging,
            offset,
            slice.buffer,
            u64::from(slice.range.start) * stride,
            size,
        );
        offset += size;
        if let Some(index_bytes) = upload.mesh.get_index_buffer_bytes() {
            let indices = allocator
                .mesh_index_slice(&upload.id)
                .expect("retained index allocation");
            let width = match previous.index_format().expect("retained index format") {
                bevy::render::render_resource::IndexFormat::Uint16 => 2,
                bevy::render::render_resource::IndexFormat::Uint32 => 4,
            };
            let size = u64::try_from(index_bytes.len()).expect("index buffer size");
            encoder.copy_buffer_to_buffer(
                &staging,
                offset,
                indices.buffer,
                u64::from(indices.range.start) * width,
                size,
            );
            offset += size;
        }
    }
    // Queue submission owns the temporary buffer until all copies complete.
    // Subsequent draws use the same mesh/slab IDs and exact byte ranges.
    let commands = encoder.finish();
    if let (Some(state), Some(start)) = (&diagnostics, encoding_start) {
        state.record("mesh-retained-copy-encode", start);
    }
    let submission_start = diagnostics.as_ref().map(|_| Instant::now());
    queue.submit([commands]);
    if let (Some(state), Some(start)) = (&diagnostics, submission_start) {
        state.record("mesh-retained-queue-submit", start);
    }
    pending.restore(&mut extracted);
}

fn stage_uploads(
    uploads: &[Upload],
    device: &RenderDevice,
    diagnostics: Option<&diagnostics::State>,
) -> Buffer {
    let size = uploads
        .iter()
        .map(|upload| upload_size(&upload.mesh))
        .sum::<usize>();
    let packing_start = diagnostics.map(|_| Instant::now());
    let staging = device.create_buffer(&BufferDescriptor {
        label: Some("retained mesh uploads"),
        size: u64::try_from(size).expect("mesh batch size"),
        usage: BufferUsages::COPY_SRC,
        mapped_at_creation: true,
    });
    {
        let mut mapped = staging.slice(..).get_mapped_range_mut();
        let mut offset = 0;
        let vertex_pack_start = diagnostics.map(|_| Instant::now());
        for upload in uploads {
            let end = offset + upload.mesh.get_vertex_buffer_size();
            upload
                .mesh
                .write_packed_vertex_buffer_data(mapped.slice(offset..end));
            offset = end + upload.mesh.get_index_buffer_bytes().map_or(0, <[u8]>::len);
        }
        if let (Some(state), Some(start)) = (diagnostics, vertex_pack_start) {
            state.record("mesh-retained-vertex-pack", start);
        }
        offset = 0;
        for upload in uploads {
            offset += upload.mesh.get_vertex_buffer_size();
            if let Some(indices) = upload.mesh.get_index_buffer_bytes() {
                let end = offset + indices.len();
                mapped.slice(offset..end).copy_from_slice(indices);
                offset = end;
            }
        }
    }
    staging.unmap();
    if let (Some(state), Some(start)) = (diagnostics, packing_start) {
        state.record("mesh-retained-staging-pack", start);
    }
    staging
}
