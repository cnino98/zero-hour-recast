use super::{AiArea, OpponentRules, State};
use crate::skirmish::{Skirmish, movement::distance_squared};

pub(super) fn area<'a>(rules: &'a OpponentRules, state: &State, name: &str) -> Option<&'a AiArea> {
    // ScriptEngine::getQualifiedTriggerAreaByName uses the multiplayer start index.
    let qualified = match name {
        "[Skirmish]MyInnerPerimeter" => format!("InnerPerimeter{}", state.start + 1),
        "[Skirmish]MyOuterPerimeter" => format!("OuterPerimeter{}", state.start + 1),
        "[Skirmish]EnemyInnerPerimeter" => format!("InnerPerimeter{}", state.enemy_start + 1),
        "[Skirmish]EnemyOuterPerimeter" => format!("OuterPerimeter{}", state.enemy_start + 1),
        _ => name.to_owned(),
    };
    rules.map.areas.iter().find(|area| area.name == qualified)
}
pub(in crate::skirmish) fn inside(area: &AiArea, point: [f32; 2]) -> bool {
    // PolygonTrigger::pointInTrigger tests the object's integer XY location.
    let point = [point[0].trunc(), point[1].trunc()];
    let mut result = false;
    for (index, &left) in area.points.iter().enumerate() {
        let right = area.points[(index + 1) % area.points.len()];
        if (left[1] - right[1]).abs() < f32::EPSILON
            || (left[1] < point[1] && right[1] < point[1])
            || (left[1] >= point[1] && right[1] >= point[1])
            || (left[0] < point[0] && right[0] < point[0])
        {
            continue;
        }
        let product = (right[0] - left[0]) * (point[1] - left[1]);
        if left[0] + product / (right[1] - left[1]) >= point[0] {
            result = !result;
        }
    }
    result
}

pub(in crate::skirmish) fn bounds(area: &AiArea) -> ([f32; 2], f32) {
    let mut low = [8_388_592.0_f32; 2];
    let mut high = [-8_388_592.0_f32; 2];
    for point in &area.points {
        for axis in 0..2 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    let sum_x = low[0] + high[0];
    let sum_y = low[1] + high[1];
    let center = [sum_x / 2.0, sum_y / 2.0];
    let half_width = (high[0] - low[0]) / 2.0;
    // PolygonTrigger::updateBounds uses a sum for halfHeight, including in the
    // pinned fixed source. Preserve that arithmetic instead of fixing the name.
    let half_height = center[1];
    let height_squared = half_height * half_height;
    (center, (height_squared + half_width * half_width).sqrt())
}

struct TriggerObservation {
    position: [u32; 2],
    alive: bool,
    areas: Vec<usize>,
}

pub(in crate::skirmish) struct TriggerState {
    objects: Vec<Option<TriggerObservation>>,
    pub(in crate::skirmish) changed_at: u64,
}

impl TriggerState {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            objects: Vec::new(),
            changed_at: 0,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn observe_guard_triggers(&mut self, id: usize) {
        let Some(rules) = &self.opponent_rules else {
            return;
        };
        let actor = &self.entities[id];
        let position = [actor.position[0].trunc(), actor.position[1].trunc()];
        let alive = actor.alive();
        self.trigger_state
            .objects
            .resize_with(self.entities.len(), || None);
        let previous = &self.trigger_state.objects[id];
        if previous
            .as_ref()
            .is_some_and(|old| old.position == position.map(f32::to_bits) && old.alive == alive)
        {
            return;
        }
        let areas = rules
            .map
            .areas
            .iter()
            .enumerate()
            .filter_map(|(index, area)| (alive && inside(area, position)).then_some(index))
            .collect::<Vec<_>>();
        if previous
            .as_ref()
            .is_none_or(|old| old.alive != alive || old.areas != areas)
        {
            self.trigger_state.changed_at = self.tick;
        }
        self.trigger_state.objects[id] = Some(TriggerObservation {
            position: position.map(f32::to_bits),
            alive,
            areas,
        });
    }
}
pub(super) fn nearest(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &State,
    team: usize,
    label: &str,
) -> Option<usize> {
    let mut position = [0.0; 2];
    let mut count = 0_u16;
    for &id in &state.teams[team].members {
        if game.entities[id].alive() && game.entities[id].contained_by().is_none() {
            position[0] += game.entities[id].position[0];
            position[1] += game.entities[id].position[1];
            count = count.checked_add(1)?;
        }
    }
    if count == 0 {
        return None;
    }
    position[0] /= f32::from(count);
    position[1] /= f32::from(count);
    let label = format!("{label}{}", state.enemy_start + 1);
    rules
        .map
        .waypoints
        .iter()
        .enumerate()
        .filter(|(_, way)| {
            way.labels
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&label))
        })
        .min_by(|(_, a), (_, b)| {
            distance_squared(a.position, position)
                .total_cmp(&distance_squared(b.position, position))
        })
        .map(|(index, _)| index)
}
pub(super) fn begin(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    team: usize,
    label: &str,
    follow: bool,
) {
    // Both native approach actions return without changing orders for an empty
    // team or missing labelled waypoint. These are implemented no-ops.
    let Some(current) = nearest(game, rules, state, team, label) else {
        return;
    };
    let members = state.teams[team]
        .members
        .iter()
        .copied()
        .filter(|&id| game.entities[id].complete())
        .collect::<Vec<_>>();
    let _ = game.move_group(&members, rules.map.waypoints[current].position, false);
    state.teams[team].follow = if follow { Some((current, None)) } else { None };
}
pub(super) fn advance(game: &mut Skirmish, rules: &OpponentRules, state: &mut State, team: usize) {
    let Some((current, prior)) = state.teams[team].follow else {
        return;
    };
    if !state.teams[team]
        .members
        .iter()
        .all(|&id| !game.entities[id].alive() || game.entities[id].ai_idle())
    {
        return;
    }
    let links = &rules.map.waypoints[current].links;
    let skip = prior.and_then(|id| {
        links
            .iter()
            .position(|&link| link == rules.map.waypoints[id].id)
    });
    let count = links.len() - usize::from(skip.is_some());
    if count == 0 {
        state.teams[team].follow = None;
        return;
    }
    let Ok(maximum) = u32::try_from(count - 1) else {
        return;
    };
    let mut which = usize::try_from(game.random_between(maximum)).unwrap_or(0);
    if skip == Some(which) {
        which = links.len() - 1;
    }
    let Some(next) = rules
        .map
        .waypoints
        .iter()
        .position(|way| way.id == links[which])
    else {
        state.report("Broken map waypoint link");
        state.teams[team].follow = None;
        return;
    };
    let members = state.teams[team]
        .members
        .iter()
        .copied()
        .filter(|&id| game.entities[id].complete())
        .collect::<Vec<_>>();
    let _ = game.move_group(&members, rules.map.waypoints[next].position, false);
    state.teams[team].follow = Some((next, Some(current)));
}
