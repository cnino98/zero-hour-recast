//! Keep each native mesh's additional heat pass after its authored material passes.
use super::HeatPart;
use crate::model_animation::AnimatedPart;
use bevy::{
    core_pipeline::core_3d::Transparent3d,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_phase::{SortedRenderPhase, ViewSortedRenderPhases, sort_phase_system},
        sync_world::MainEntity,
    },
};
use std::collections::BTreeMap;

struct MeshPasses {
    base: Vec<Entity>,
    heat: Vec<Entity>,
}

#[derive(Resource)]
struct Passes(Vec<MeshPasses>);

pub fn register(app: &mut App) {
    app.sub_app_mut(RenderApp)
        .insert_resource(Passes(Vec::new()))
        .add_systems(ExtractSchedule, extract)
        .add_systems(
            Render,
            order
                .in_set(RenderSystems::PhaseSort)
                .after(sort_phase_system::<Transparent3d>),
        );
}

type SourcePart = (Entity, &'static ChildOf, &'static AnimatedPart);

#[derive(bevy::ecs::system::SystemParam)]
struct ExtractedParts<'w, 's> {
    overlays: Extract<'w, 's, Query<'static, 'static, (Entity, &'static HeatPart)>>,
    sources: Extract<'w, 's, Query<'static, 'static, SourcePart, Without<HeatPart>>>,
}

fn extract(mut passes: ResMut<Passes>, parts: ExtractedParts) {
    let ExtractedParts { overlays, sources } = parts;
    let mut groups = BTreeMap::new();
    for (entity, overlay) in overlays.iter() {
        let Ok((_, parent, part)) = sources.get(overlay.source) else {
            continue;
        };
        groups
            .entry((parent.parent(), part.model, part.binding.mesh))
            .or_insert_with(|| MeshPasses {
                base: Vec::new(),
                heat: Vec::new(),
            })
            .heat
            .push(entity);
    }
    if !groups.is_empty() {
        for (entity, parent, part) in sources.iter() {
            if let Some(group) = groups.get_mut(&(parent.parent(), part.model, part.binding.mesh)) {
                group.base.push(entity);
            }
        }
    }
    passes.0 = groups.into_values().collect();
}

fn order(mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>, passes: Res<Passes>) {
    let passes = passes.into_inner();
    for phase in phases.values_mut() {
        for group in &passes.0 {
            order_mesh(phase, group);
        }
    }
}

fn order_mesh(phase: &mut SortedRenderPhase<Transparent3d>, group: &MeshPasses) {
    // Bevy's mesh material queue uses MainEntity with a placeholder render entity.
    // Opaque base passes already precede this phase; only present transparent
    // base passes need ordering. Leave every unrelated mesh's order intact.
    let key = |entity| (Entity::PLACEHOLDER, MainEntity::from(entity));
    let Some(mut last) = group
        .base
        .iter()
        .filter_map(|&entity| phase.items.get_index_of(&key(entity)))
        .max()
    else {
        return;
    };
    for &entity in &group.heat {
        let Some(index) = phase.items.get_index_of(&key(entity)) else {
            continue;
        };
        let destination = if index <= last { last } else { last + 1 };
        phase.items.move_index(index, destination);
        last = destination;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        core_pipeline::core_3d::TransparentSortingInfo3d,
        material::labels::DrawFunctionId,
        render::{render_phase::PhaseItemExtraIndex, render_resource::CachedRenderPipelineId},
    };

    #[test]
    fn heat_follows_all_authored_mesh_passes_without_biasing_depth_or_other_meshes() {
        let mut world = World::new();
        let ids: Vec<_> = (0..6).map(|_| world.spawn_empty().id()).collect();
        let mut phase = SortedRenderPhase::<Transparent3d>::default();
        for &id in &ids {
            phase.add_retained(Transparent3d {
                sorting_info: TransparentSortingInfo3d::Sorted {
                    mesh_center: Vec3::ZERO,
                    depth_bias: 0.0,
                },
                distance: 500.0,
                pipeline: CachedRenderPipelineId::INVALID,
                entity: (Entity::PLACEHOLDER, id.into()),
                draw_function: DrawFunctionId(0),
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: true,
            });
        }
        let group = MeshPasses {
            base: vec![ids[2], ids[4]],
            heat: vec![ids[0], ids[3]],
        };
        order_mesh(&mut phase, &group);
        let actual: Vec<_> = phase.items.keys().map(|key| key.1.id()).collect();
        assert_eq!(actual, [ids[1], ids[2], ids[4], ids[0], ids[3], ids[5]]);
        assert!(
            phase
                .items
                .values()
                .all(|item| item.distance.to_bits() == 500.0_f32.to_bits())
        );
        order_mesh(&mut phase, &group);
        let repeated: Vec<_> = phase.items.keys().map(|key| key.1.id()).collect();
        assert_eq!(repeated, actual);
    }
}
