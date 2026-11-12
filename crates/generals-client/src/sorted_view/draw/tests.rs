use super::{Ranges, extract, vertex_range};
use crate::sorted_view::{SortedBatch, SortedRange};
use bevy::{
    ecs::schedule::Schedule,
    prelude::*,
    render::{ExtractSchedule, MainWorld, sync_world::MainEntity},
};

#[test]
fn ranges_use_the_current_slab_base_and_reject_invalid_allocations() {
    assert_eq!(vertex_range(&(3..6), 9, &(17..27)), Some(20..23));
    assert_eq!(vertex_range(&(3..6), 12, &(43..56)), Some(46..49));
    for relative in [0..0, 1..3, 3..5, 3..12] {
        assert!(vertex_range(&relative, 9, &(17..27)).is_none());
    }
    assert!(vertex_range(&(3..6), 9, &(17..22)).is_none());
    assert!(vertex_range(&(0..3), 3, &(u32::MAX - 1..u32::MAX)).is_none());
}

#[test]
fn extraction_tracks_missing_ranges_and_only_tagged_entities() {
    let mut world = World::new();
    world.init_resource::<MainWorld>();
    world.init_resource::<Ranges>();
    let mut schedule = Schedule::new(ExtractSchedule);
    schedule.add_systems(extract);
    world.add_schedule(schedule);
    let (draw, missing, unrelated) = {
        let mut main = world.resource_mut::<MainWorld>();
        (
            main.spawn((SortedBatch, SortedRange(3..6))).id(),
            main.spawn(SortedBatch).id(),
            main.spawn(SortedRange(3..6)).id(),
        )
    };
    world.run_schedule(ExtractSchedule);
    let ranges = world.resource::<Ranges>();
    assert_eq!(ranges.0.get(&MainEntity::from(draw)), Some(&Some(3..6)));
    assert_eq!(ranges.0.get(&MainEntity::from(missing)), Some(&None));
    assert!(!ranges.0.contains_key(&MainEntity::from(unrelated)));
    world.resource_mut::<MainWorld>().despawn(draw);
    world.run_schedule(ExtractSchedule);
    assert!(
        !world
            .resource::<Ranges>()
            .0
            .contains_key(&MainEntity::from(draw))
    );
}

mod gpu;
