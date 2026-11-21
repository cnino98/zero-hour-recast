use super::*;
use crate::sorted_view::{Run, submission};
use bevy::{
    ecs::{schedule::Schedule, system::RunSystemOnce},
    render::{
        MainWorld,
        render_phase::{DrawFunctionId, PhaseItemExtraIndex, SortedRenderPhase},
        render_resource::{CachedRenderPipelineId, TextureFormat},
        view::{ExtractedView, RetainedViewEntity},
    },
};

fn phase(ids: &[Entity; 6], old_material_bias: bool) -> SortedRenderPhase<Transparent3d> {
    let mut phase = SortedRenderPhase::default();
    for index in [1, 3, 2, 0, 5, 4] {
        let sorting_info = match index {
            5 => TransparentSortingInfo3d::AlwaysOnTop,
            4 => TransparentSortingInfo3d::Sorted {
                mesh_center: Vec3::Z,
                depth_bias: 0.5,
            },
            // A nonparticipant ties the ordinal-one run; stable insertion order matters.
            3 => TransparentSortingInfo3d::Sorted {
                mesh_center: Vec3::Z,
                depth_bias: 0.0,
            },
            index => TransparentSortingInfo3d::Sorted {
                mesh_center: Vec3::ZERO,
                depth_bias: if old_material_bias {
                    index.to_f32().unwrap()
                } else {
                    0.0
                },
            },
        };
        phase.add_retained(Transparent3d {
            sorting_info,
            distance: 0.0,
            pipeline: CachedRenderPipelineId::INVALID,
            entity: (Entity::PLACEHOLDER, ids[index].into()),
            draw_function: DrawFunctionId(0),
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            indexed: false,
        });
    }
    phase
}

#[test]
fn phase_draws_match_per_material_bias_including_ties_and_unrelated_items() {
    let mut world = World::new();
    let ids = std::array::from_fn(|_| world.spawn_empty().id());
    let view_id = world.spawn_empty().id();
    let view = ExtractedView {
        retained_view_entity: RetainedViewEntity::new(view_id.into(), None, 0),
        clip_from_view: Mat4::IDENTITY,
        world_from_view: GlobalTransform::from_translation(Vec3::new(0.0, 0.0, 13.0)),
        clip_from_world: None,
        target_format: TextureFormat::Bgra8UnormSrgb,
        viewport: UVec4::new(0, 0, 1280, 800),
        color_grading: default(),
        invert_culling: false,
    };
    let mut expected = phase(&ids, true);
    expected.recalculate_sort_keys(&view);
    expected.sort();
    let keys = |phase: &SortedRenderPhase<Transparent3d>| {
        phase
            .items
            .values()
            .map(|item| (item.entity.1, item.distance.to_bits()))
            .collect::<Vec<_>>()
    };
    let mut phases = ViewSortedRenderPhases::default();
    phases.insert(view.retained_view_entity, phase(&ids, false));
    let key = view.retained_view_entity;
    world.entity_mut(view_id).insert(view);
    world.insert_resource(phases);
    world.init_resource::<MainWorld>();
    world.resource_mut::<MainWorld>().insert_resource(Pool {
        vertices: Vec::new(),
        triangles: Vec::new(),
        runs: ids[..3]
            .iter()
            .map(|&entity| Run {
                entity,
                range: 0..3,
                material: Handle::default(),
                clear: false,
            })
            .collect(),
        mesh: None,
        variants: default(),
        registry: submission::Registry::new(),
        sequence: 0,
        instances: default(),
    });
    world.insert_resource(Ordinals(default()));
    let mut schedule = Schedule::new(ExtractSchedule);
    schedule.add_systems(extract);
    world.add_schedule(schedule);
    world.run_schedule(ExtractSchedule);
    world.run_system_once(apply).unwrap();
    world
        .run_system_once(sort_phase_system::<Transparent3d>)
        .unwrap();
    let actual = &world.resource::<ViewSortedRenderPhases<Transparent3d>>()[&key];
    assert_eq!(keys(actual), keys(&expected));
    assert_eq!(
        actual
            .items
            .values()
            .map(|item| item.entity.1.id())
            .collect::<Vec<_>>(),
        [ids[5], ids[0], ids[1], ids[3], ids[4], ids[2]]
    );
}
