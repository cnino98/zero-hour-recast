//! Draw the native sorted run's vertex interval from its current Bevy mesh allocation.
use super::{SortedBatch, SortedRange};
use bevy::{
    core_pipeline::core_3d::Transparent3d,
    ecs::{
        query::ROQueryItem,
        system::{SystemParamItem, lifetimeless::SRes},
    },
    pbr::{
        MATERIAL_BIND_GROUP_INDEX, PreprocessBindGroups, PreprocessPipelines, RenderMeshInstances,
        SetMaterialBindGroup, SetMeshBindGroup, SetMeshViewBindGroup,
        SetMeshViewBindingArrayBindGroup,
    },
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        batching::gpu_preprocessing::{GpuPreprocessingMode, GpuPreprocessingSupport},
        mesh::{RenderMesh, allocator::MeshAllocator},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
            RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
            sort_phase_system,
        },
        render_resource::{PipelineCache, PrimitiveTopology},
        sync_world::MainEntityHashMap,
    },
};
use std::ops::Range;

type DrawRun = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    SetMaterialBindGroup<MATERIAL_BIND_GROUP_INDEX>,
    DrawSortedRange,
);

#[derive(Resource, Default)]
struct Ranges(MainEntityHashMap<Option<Range<u32>>>);

pub(super) fn register(app: &mut App) {
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render
        .init_resource::<Ranges>()
        .add_render_command::<Transparent3d, DrawRun>()
        .add_systems(ExtractSchedule, extract)
        .add_systems(
            Render,
            select
                .in_set(RenderSystems::PhaseSort)
                .before(sort_phase_system::<Transparent3d>),
        );
}

type RunRanges<'w, 's> = Query<'w, 's, (Entity, Option<&'static SortedRange>), With<SortedBatch>>;

fn extract(mut runs: Extract<RunRanges>, mut ranges: ResMut<Ranges>) {
    ranges.0.clear();
    for (entity, range) in runs.iter_mut() {
        ranges
            .0
            .insert(entity.into(), range.map(|range| range.0.clone()));
    }
}

fn select(
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    functions: Res<DrawFunctions<Transparent3d>>,
    ranges: Res<Ranges>,
) {
    let command = functions.into_inner().read().id::<DrawRun>();
    let ranges = ranges.into_inner();
    for phase in phases.values_mut() {
        for item in phase.items.values_mut() {
            if ranges.0.contains_key(&item.main_entity()) {
                item.draw_function = command;
            }
        }
    }
}

// Match DrawMesh's private pipelines_are_loaded gate using Bevy's public pipeline IDs.
fn preprocess_ready(
    pipelines: &PreprocessPipelines,
    cache: &PipelineCache,
    mode: GpuPreprocessingMode,
) -> bool {
    let loaded = |id: Option<_>| id.is_some_and(|id| cache.get_compute_pipeline(id).is_some());
    if mode == GpuPreprocessingMode::None
        || !loaded(pipelines.direct_preprocess.pipeline_id)
        || !loaded(pipelines.gpu_frustum_culling_preprocess.pipeline_id)
    {
        return false;
    }
    if mode == GpuPreprocessingMode::PreprocessingOnly {
        return true;
    }
    [
        pipelines.early_gpu_occlusion_culling_preprocess.pipeline_id,
        pipelines.late_gpu_occlusion_culling_preprocess.pipeline_id,
        pipelines
            .gpu_frustum_culling_build_indexed_indirect_params
            .pipeline_id,
        pipelines
            .gpu_frustum_culling_build_non_indexed_indirect_params
            .pipeline_id,
    ]
    .into_iter()
    .all(loaded)
        && [
            &pipelines.early_phase,
            &pipelines.late_phase,
            &pipelines.main_phase,
        ]
        .into_iter()
        .all(|phase| {
            [
                phase.reset_indirect_batch_sets.pipeline_id,
                phase
                    .gpu_occlusion_culling_build_indexed_indirect_params
                    .pipeline_id,
                phase
                    .gpu_occlusion_culling_build_non_indexed_indirect_params
                    .pipeline_id,
            ]
            .into_iter()
            .all(loaded)
        })
}

fn vertex_range(
    relative: &Range<u32>,
    vertices: u32,
    allocation: &Range<u32>,
) -> Option<Range<u32>> {
    if relative.start >= relative.end
        || relative.end > vertices
        || !relative.start.is_multiple_of(3)
        || !relative.end.is_multiple_of(3)
    {
        return None;
    }
    let start = allocation.start.checked_add(relative.start)?;
    let end = allocation.start.checked_add(relative.end)?;
    (end <= allocation.end).then_some(start..end)
}

struct DrawSortedRange;

impl RenderCommand<Transparent3d> for DrawSortedRange {
    type Param = (
        SRes<Ranges>,
        SRes<RenderAssets<RenderMesh>>,
        SRes<RenderMeshInstances>,
        SRes<MeshAllocator>,
        SRes<PipelineCache>,
        Option<SRes<PreprocessPipelines>>,
        SRes<GpuPreprocessingSupport>,
    );
    type ViewQuery = Has<PreprocessBindGroups>;
    type ItemQuery = ();

    fn render<'w>(
        item: &Transparent3d,
        has_preprocess: ROQueryItem<Self::ViewQuery>,
        _: Option<()>,
        (ranges, meshes, instances, allocator, cache, pipelines, support): SystemParamItem<
            'w,
            '_,
            Self::Param,
        >,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        if let Some(pipelines) = pipelines
            && (!has_preprocess
                || !preprocess_ready(&pipelines, &cache, support.max_supported_mode))
        {
            return RenderCommandResult::Skip;
        }
        let Some(Some(relative)) = ranges.0.get(&item.main_entity()) else {
            return RenderCommandResult::Failure("Sorted draw has no triangle range");
        };
        if !matches!(
            item.extra_index(),
            PhaseItemExtraIndex::None | PhaseItemExtraIndex::DynamicOffset(_)
        ) {
            return RenderCommandResult::Failure("Sorted range requires direct drawing");
        }
        let Some(id) = instances.mesh_asset_id(item.main_entity()) else {
            return RenderCommandResult::Skip;
        };
        let Some(mesh) = meshes.into_inner().get(id) else {
            return RenderCommandResult::Skip;
        };
        if mesh.indexed() || mesh.primitive_topology() != PrimitiveTopology::TriangleList {
            return RenderCommandResult::Failure(
                "Sorted range requires nonindexed triangle-list geometry",
            );
        }
        let Some(slice) = allocator.into_inner().mesh_vertex_slice(&id) else {
            return RenderCommandResult::Skip;
        };
        let Some(vertices) = vertex_range(relative, mesh.vertex_count, &slice.range) else {
            return RenderCommandResult::Failure(
                "Sorted triangle range exceeds its mesh allocation",
            );
        };
        pass.set_vertex_buffer(0, slice.buffer.slice(..));
        pass.draw(vertices, item.batch_range().clone());
        RenderCommandResult::Success
    }
}

#[cfg(test)]
mod tests;
