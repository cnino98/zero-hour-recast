//! SH `HeightMap::Render`: scorches, bridges, tracks, then equal-depth terrain shroud.
use super::SourceMaterial;
use bevy::{
    core_pipeline::core_3d::Transparent3d,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_phase::{SortedRenderPhase, ViewSortedRenderPhases, sort_phase_system},
        render_resource::CompareFunction,
        sync_world::MainEntity,
    },
};

pub const SCORCH: usize = 60;
pub const BRIDGE: usize = 61;
pub const TRACK: usize = 62;
pub const SHROUD: usize = 63;
pub const BRIDGE_SHROUD: usize = 64;
pub const PASSES: [usize; 5] = [SCORCH, BRIDGE, BRIDGE_SHROUD, TRACK, SHROUD];

pub fn shroud_material() -> SourceMaterial {
    let mut material = SourceMaterial::new(
        [0.0; 3],
        [0.0; 3],
        [0.0; 3],
        1.0,
        AlphaMode::Multiply,
        false,
    );
    material.parameters.shroud_mode = 2;
    material.parameters.vertex_diffuse = 4;
    material.depth_compare = Some(CompareFunction::Equal);
    material
}

#[derive(Component)]
pub struct TrackOrder(pub usize);

#[derive(Resource)]
struct Tracks(Vec<(usize, Entity)>);

pub(super) fn register(app: &mut App) {
    app.sub_app_mut(RenderApp)
        .insert_resource(Tracks(Vec::new()))
        .add_systems(ExtractSchedule, extract)
        .add_systems(
            Render,
            order
                .in_set(RenderSystems::PhaseSort)
                .after(sort_phase_system::<Transparent3d>),
        );
}

#[derive(bevy::ecs::system::SystemParam)]
struct ExtractedTracks<'w, 's> {
    query: Extract<'w, 's, Query<'static, 'static, (Entity, &'static TrackOrder)>>,
}

fn extract(mut tracks: ResMut<Tracks>, sources: ExtractedTracks) {
    let ExtractedTracks { query } = sources;
    tracks.0.clear();
    tracks
        .0
        .extend(query.iter().map(|(entity, order)| (order.0, entity)));
    tracks.0.sort_unstable_by_key(|&(order, _)| order);
}

fn order(mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>, tracks: Res<Tracks>) {
    let tracks = tracks.into_inner();
    for phase in phases.values_mut() {
        order_tracks(phase, &tracks.0);
    }
}

fn order_tracks(phase: &mut SortedRenderPhase<Transparent3d>, tracks: &[(usize, Entity)]) {
    // W3DTerrainTracks::flush binds each used-list entry's texture and submits
    // that entire strip. A texture batch or camera-depth sort changes overlaps.
    let key = |entity| (Entity::PLACEHOLDER, MainEntity::from(entity));
    let Some(mut destination) = tracks
        .iter()
        .filter_map(|&(_, entity)| phase.items.get_index_of(&key(entity)))
        .min()
    else {
        return;
    };
    for &(_, entity) in tracks {
        if let Some(index) = phase.items.get_index_of(&key(entity)) {
            phase.items.move_index(index, destination);
            destination += 1;
        }
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
    fn track_submission_preserves_used_list_across_textures_and_camera_depth() {
        let mut world = World::new();
        let ids: Vec<_> = (0..3).map(|_| world.spawn_empty().id()).collect();
        let mut phase = SortedRenderPhase::<Transparent3d>::default();
        for (index, &id) in ids.iter().enumerate() {
            phase.add_retained(Transparent3d {
                sorting_info: TransparentSortingInfo3d::Sorted {
                    mesh_center: Vec3::ZERO,
                    depth_bias: 0.0,
                },
                distance: [100.0, 50.0, 5.0][index],
                pipeline: CachedRenderPipelineId::INVALID,
                entity: (Entity::PLACEHOLDER, id.into()),
                draw_function: DrawFunctionId(0),
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: true,
            });
        }
        order_tracks(&mut phase, &[(0, ids[2]), (1, ids[0]), (2, ids[1])]);
        assert_eq!(
            phase.items.keys().map(|key| key.1.id()).collect::<Vec<_>>(),
            [ids[2], ids[0], ids[1]]
        );
        let shroud = shroud_material();
        assert_eq!(shroud.depth_compare, Some(CompareFunction::Equal));
        assert!(!shroud.depth_write);
        assert_eq!(shroud.alpha_mode, AlphaMode::Multiply);
        assert_eq!(PASSES, [SCORCH, BRIDGE, BRIDGE_SHROUD, TRACK, SHROUD]);
    }
}
