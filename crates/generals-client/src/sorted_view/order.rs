//! Preserve the triangle pool's ordinal as the depth bias of each Bevy phase item.
use super::Pool;
use bevy::{
    core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d},
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems,
        render_phase::{ViewSortedRenderPhases, sort_phase_system},
        sync_world::{MainEntity, MainEntityHashMap},
    },
};
use num_traits::ToPrimitive;

#[derive(Resource)]
struct Ordinals(MainEntityHashMap<f32>);

pub(super) fn register(app: &mut App) {
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render
        .insert_resource(Ordinals(MainEntityHashMap::default()))
        .add_systems(ExtractSchedule, extract)
        .add_systems(
            Render,
            apply
                .in_set(RenderSystems::PhaseSort)
                .before(sort_phase_system::<Transparent3d>),
        );
}

fn extract(main: Res<MainWorld>, mut ordinals: ResMut<Ordinals>) {
    let pool = main.into_inner().resource::<Pool>();
    // A pool appends ordinal slots until map retirement replaces the whole set.
    if pool
        .runs
        .first()
        .is_none_or(|run| !ordinals.0.contains_key(&MainEntity::from(run.entity)))
    {
        ordinals.0.clear();
    }
    for (index, run) in pool.runs.iter().enumerate().skip(ordinals.0.len()) {
        ordinals.0.insert(
            run.entity.into(),
            index.to_f32().expect("bounded sorted draw count"),
        );
    }
}

fn apply(mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>, ordinals: Res<Ordinals>) {
    let ordinals = ordinals.into_inner();
    for phase in phases.values_mut() {
        for item in phase.items.values_mut() {
            if let Some(ordinal) = ordinals.0.get(&item.entity.1)
                && let TransparentSortingInfo3d::Sorted { depth_bias, .. } = &mut item.sorting_info
            {
                *depth_bias = *ordinal;
            }
        }
    }
}

#[cfg(test)]
mod tests;
