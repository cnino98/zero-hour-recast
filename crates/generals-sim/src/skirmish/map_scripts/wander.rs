//! Native script waypoint selection; movement belongs to the primary AI state.
use crate::skirmish::{AiWaypoint, Skirmish, locomotor};

pub(super) fn start(
    game: &mut Skirmish,
    id: usize,
    label: &str,
    graph: &[AiWaypoint],
    panic: bool,
) -> bool {
    let actor = &game.entities[id];
    if actor.ai.registration.is_none() {
        return true;
    }
    let mut nearest = None;
    let mut distance = f32::MAX;
    // TerrainLogic prepends waypoints; strict ties retain the newest record.
    for (index, waypoint) in graph.iter().enumerate().rev() {
        if !waypoint
            .labels
            .iter()
            .any(|value| value.eq_ignore_ascii_case(label))
        {
            continue;
        }
        let squared = [
            waypoint.position[0] - actor.position[0],
            waypoint.position[1] - actor.position[1],
        ]
        .map(|value| value * value);
        let squared = squared[0] + squared[1];
        if nearest.is_none() || squared < distance {
            nearest = Some(index);
            distance = squared;
        }
    }
    let Some(current) = nearest else {
        return false;
    };
    game.choose_locomotor_set(
        id,
        if panic {
            locomotor::set::PANIC
        } else {
            locomotor::set::WANDER
        },
    );
    game.start_waypoint_wander(id, graph, current, panic);
    true
}
