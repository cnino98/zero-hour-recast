//! `AIAttackApproachTargetState::computePath` moving-victim replan admission.
use crate::skirmish::Skirmish;

pub(in crate::skirmish) struct Approach {
    target: usize,
    previous: [f32; 2],
    checked: u64,
}

impl Skirmish {
    pub(in crate::skirmish) fn reuse_attack_path(&mut self, id: usize, target: usize) -> bool {
        let position = self.entities[target].position;
        let actor = &mut self.entities[id];
        if actor
            .ai
            .command
            .attack_approach
            .as_ref()
            .is_none_or(|state| state.target != target)
        {
            actor.ai.command.attack_approach = Some(Approach {
                target,
                previous: [0.0; 2],
                checked: self.tick.wrapping_sub(10),
            });
        }
        let state = actor
            .ai
            .command
            .attack_approach
            .as_mut()
            .expect("attack approach state");
        let forced = actor.ai.blocking.stuck
            || (actor.ai.ground_path.is_none() && actor.ai.path.optimized.is_empty());
        if !forced && self.tick.wrapping_sub(state.checked) < 10 {
            return true;
        }
        // Native updates this timestamp before testing whether the target
        // moved enough; a suppressed request still starts the next interval.
        state.checked = self.tick;
        if !forced && same_position(actor.position, state.previous, position) {
            return true;
        }
        state.previous = position;
        false
    }
}

fn same_position(source: [f32; 2], previous: [f32; 2], current: [f32; 2]) -> bool {
    let displacement = [current[0] - previous[0], current[1] - previous[1]];
    let target = [current[0] - source[0], current[1] - source[1]];
    let moved = displacement.map(|component| component * component);
    let distance = target.map(|component| component * component);
    let tolerance = (distance[0] + distance[1]) * (1.0 / (10.0 * 10.0));
    moved[0] + moved[1] <= tolerance
}

#[cfg(test)]
mod tests;
