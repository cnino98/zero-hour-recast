//! `ScriptEngine` attack sets and `AI::findClosestEnemy` priority/distance ordering.
use super::{AiCall, State};
use crate::skirmish::{Skirmish, movement::distance_squared};
use std::{cmp::Ordering, collections::BTreeMap};

pub(in crate::skirmish) struct AttackPriority {
    name: String,
    default: i32,
    types: BTreeMap<String, i32>,
}

pub(super) fn action(game: &mut Skirmish, state: &State, call: &AiCall) -> Option<()> {
    let parameters = &call.parameters;
    let name = &parameters.first()?.text;
    match call.name.as_str() {
        "SET_DEFAULT_ATTACK_PRIORITY" => {
            let value = parameters.get(1)?.integer;
            let index = definition(game, name);
            game.attack_priorities[index].default = value;
        }
        "SET_ATTACK_PRIORITY_THING" => {
            let target = &parameters.get(1)?.text;
            let priority = parameters.get(2)?.integer;
            let names = state
                .object_lists
                .get(target)
                .map_or_else(|| vec![target.clone()], Clone::clone);
            for target in names {
                if game.catalog.iter().any(|unit| unit.name == target) {
                    let index = definition(game, name);
                    game.attack_priorities[index].types.insert(target, priority);
                }
            }
        }
        _ => return None,
    }
    Some(())
}

fn definition(game: &mut Skirmish, name: &str) -> usize {
    if let Some(index) = find(game, name) {
        return index;
    }
    let index = game.attack_priorities.len();
    game.attack_priorities.push(AttackPriority {
        name: name.to_owned(),
        default: 1,
        types: BTreeMap::new(),
    });
    index
}

pub(super) fn find(game: &Skirmish, name: &str) -> Option<usize> {
    game.attack_priorities
        .iter()
        .position(|set| set.name == name)
}

impl Skirmish {
    pub(in crate::skirmish) fn prioritized_target(
        &self,
        id: usize,
        candidates: impl Iterator<Item = usize>,
    ) -> Option<usize> {
        let distance = |target: usize| {
            let center =
                distance_squared(self.entities[id].position, self.entities[target].position).sqrt();
            (center - (self.object_radius(id) + self.object_radius(target))).max(0.0)
        };
        let mut candidates = candidates
            .map(|target| (target, distance(target)))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
        let Some(info) = self.entities[id]
            .ai
            .attack_priority
            .and_then(|index| self.attack_priorities.get(index))
        else {
            return candidates.first().map(|&(target, _)| target);
        };
        if info.name.is_empty() {
            return candidates.first().map(|&(target, _)| target);
        }
        let modifier = self.opponent_rules.as_ref()?.attack_priority_distance;
        let mut best = None;
        let mut effective = 0.0_f64;
        let mut actual = 0;
        for (target, distance) in candidates {
            let priority = info
                .types
                .get(&self.catalog[self.entities[target].type_index].name)
                .copied()
                .unwrap_or(info.default);
            if priority == 0 {
                continue;
            }
            // The source truncates the positive distance ratio to Int. f64
            // retains both that integer and the i32 subtraction exactly here.
            let adjusted =
                (f64::from(priority) - f64::from((distance / modifier).trunc())).max(1.0);
            if adjusted.total_cmp(&effective) == Ordering::Greater
                || (adjusted.total_cmp(&effective) == Ordering::Equal && priority > actual)
            {
                best = Some(target);
                effective = adjusted;
                actual = priority;
            }
        }
        best
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn native_default_attack_priority(&self, info: Option<usize>) -> bool {
        info.is_none_or(|index| self.attack_priorities[index].name.is_empty())
    }

    pub(in crate::skirmish) fn native_attack_priority(
        &self,
        info: Option<usize>,
        target: usize,
    ) -> i32 {
        let info = &self.attack_priorities[info.expect("native attack priority pointer")];
        info.types
            .get(&self.catalog[self.entities[target].type_index].name)
            .copied()
            .unwrap_or(info.default)
    }
}
