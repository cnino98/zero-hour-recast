//! `W3DWaypointBuffer`'s selected Listening Outpost branch (242–325).
use super::{Match, Route, Vec3};

pub(super) fn selected_outpost(battle: &Match, id: usize) -> bool {
    let actor = &battle.simulation.entities[id];
    actor.owner == battle.resource_owner().unwrap_or(0)
        && battle.simulation.catalog[actor.type_index]
            .kind_of
            .iter()
            .any(|flag| flag == "REVEALS_ENEMY_PATHS")
}

pub(super) fn points(battle: &Match, outpost: usize, hovered: Option<usize>) -> Option<Route> {
    let game = &battle.simulation;
    let enemy = hovered?;
    let target = game.entities.get(enemy)?;
    let detector = &game.entities[outpost];
    if !game.enemies(target.owner, detector.owner) {
        return None;
    }
    let origin = game.object_position(enemy);
    let position = game.object_position(outpost);
    let distance = length([
        position[0] - origin[0],
        position[1] - origin[1],
        position[2] - origin[2],
    ]);
    if distance > detector.vision_range(&game.catalog[detector.type_index]) {
        return None;
    }
    let render_point =
        |position: [f32; 3]| Vec3::new(position[0] * 0.1, position[2] * 0.1, -position[1] * 0.1);
    let marker_points = if let Some((path, index)) = target.waypoint_path()
        && index < path.len()
    {
        path[index..]
            .iter()
            .copied()
            .map(render_point)
            .collect::<Vec<_>>()
    } else {
        let goal = target.ai_goal_position();
        if length(goal) <= 1.0 {
            return None;
        }
        vec![render_point(goal)]
    };
    let points = std::iter::once(render_point(origin))
        .chain(marker_points.iter().copied())
        .take(513)
        .collect();
    Some(Route {
        points,
        marker_points,
        revealed: true,
    })
}

fn length(position: [f32; 3]) -> f32 {
    let squared = position.map(|value| value * value);
    (squared[0] + squared[1] + squared[2]).sqrt()
}

#[cfg(test)]
mod tests;
