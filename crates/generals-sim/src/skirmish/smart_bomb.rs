//! Native `SmartBombTargetHomingUpdate` adjusts XY directly, in the normal phase.
use super::Skirmish;

pub struct Rules {
    pub course_scalar: f32,
}

pub(in crate::skirmish) struct State {
    target: Option<[f32; 3]>,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_smart_bomb(&mut self, id: usize) {
        self.entities[id]
            .behaviors
            .smart_bombs
            .push(State { target: None });
    }

    pub(in crate::skirmish) fn set_smart_bomb_target(&mut self, id: usize, target: [f32; 3]) {
        if target.iter().any(|coordinate| *coordinate != 0.0)
            && let Some(state) = self.entities[id].behaviors.smart_bombs.first_mut()
        {
            state.target = Some(target);
        }
    }

    pub(in crate::skirmish) fn update_smart_bomb(&mut self, id: usize, ordinal: usize) -> i32 {
        let actor = &self.entities[id];
        let Some(target) = actor.behaviors.smart_bombs[ordinal].target else {
            return 1;
        };
        let unit = &self.catalog[actor.type_index];
        if self.height_above_ground(id) > -9.0 * self.gravity {
            let current = self.object_position(id);
            let status = unit.behavior.smart_bombs[ordinal]
                .course_scalar
                .clamp(0.0, 1.0);
            let correction = 1.0 - status;
            let homing = target.map(|component| component * correction);
            let carried = current.map(|component| component * status);
            self.set_object_position(
                id,
                [homing[0] + carried[0], homing[1] + carried[1], current[2]],
            );
        }
        1
    }
}
