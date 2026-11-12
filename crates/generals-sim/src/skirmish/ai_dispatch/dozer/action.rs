//! Persistent `PickPosition` -> `MoveToPosition` -> `DoAction` nested machines.
use super::{BUILD, BuildPhase, Feedback, REPAIR, Result, Skirmish};
use crate::skirmish::{ConstructionState, object_sounds::Action};

impl Skirmish {
    pub(super) fn pick_dozer_action_position(&mut self, id: usize) -> Result {
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        let task = state.current.expect("Dozer action task");
        let Some(target) = state.tasks[task]
            .target
            .filter(|&target| self.entities[target].exists())
        else {
            self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .goal_object = None;
            self.cancel_dozer_task(id, task, false);
            return Result::Failure;
        };
        let position =
            state.tasks[task].docks[0].unwrap_or_else(|| self.dozer_action_position(id, target));
        let state = self.entities[id]
            .ai
            .dozer
            .as_mut()
            .expect("Dozer constructor");
        state.goal_object = Some(target);
        state.goal_position = position;
        self.entities[id].ai.ignored_producer = Some(target);
        self.construction_move(id, position);
        Result::Success
    }

    pub(super) fn move_dozer_action_position(&mut self, id: usize) -> Result {
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        let task = state.current.expect("Dozer action task");
        let Some(target) = state
            .goal_object
            .filter(|&target| self.entities[target].exists())
        else {
            return Result::Failure;
        };
        if (task == REPAIR && !self.repair_benefactor(id, target))
            || (task == BUILD && self.entities[target].maintenance.builder != Some(id))
        {
            self.finish_dozer_task(id, task, true);
            self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .goal_object = None;
            return Result::Failure;
        }
        let position = self.entities[id].position;
        let delta = [
            position[0] - state.goal_position[0],
            position[1] - state.goal_position[1],
        ];
        let squared = delta.map(|value| value * value);
        let mut distance_squared = squared[0] + squared[1];
        let circle_radius = self.object_radius(id);
        if circle_radius > 0.0 {
            let distance = (distance_squared.sqrt() - circle_radius).max(0.0);
            distance_squared = distance * distance;
        }
        let radius = self.catalog[self.entities[id].type_index]
            .geometry_bounds
            .sphere_radius;
        let tolerance = (radius + 15.0).max(70.0);
        if distance_squared <= tolerance * tolerance {
            if task == BUILD {
                self.entities[target].drawable.model_conditions.construction =
                    ConstructionState::Active;
            }
            return Result::Success;
        }
        if self.entities[id].ai_idle() {
            Result::Failure
        } else {
            Result::Continue
        }
    }

    pub(super) fn perform_dozer_action(&mut self, id: usize) -> Result {
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        let task = state.current.expect("Dozer action task");
        let Some(target) = state
            .goal_object
            .filter(|&target| self.entities[target].exists())
        else {
            return Result::Failure;
        };
        if self.entities[id].unmanned() {
            return Result::Failure;
        }
        let result = match task {
            BUILD => self.perform_dozer_build(id, target),
            REPAIR => self.perform_dozer_repair(id, target),
            // Fortification is an empty native action and remains in Continue.
            _ => Result::Continue,
        };
        match result {
            Result::Success => {
                self.finish_dozer_task(id, task, true);
                self.entities[id]
                    .ai
                    .dozer
                    .as_mut()
                    .expect("Dozer constructor")
                    .goal_object = None;
                self.set_mine_clearing(id, true);
            }
            Result::Continue => self.set_mine_clearing(id, false),
            Result::Failure => {}
        }
        result
    }

    fn perform_dozer_build(&mut self, id: usize, target: usize) -> Result {
        if self.entities[id].owner != self.entities[target].owner {
            return Result::Failure;
        }
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        if matches!(state.build_phase, BuildPhase::Select) {
            if let Some(position) = state.tasks[BUILD].docks[1] {
                self.construction_move(id, position);
            }
            self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .build_phase = BuildPhase::Moving;
        }
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        if matches!(state.build_phase, BuildPhase::Moving) && self.entities[id].ai_idle() {
            self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .build_phase = BuildPhase::Building;
            let name = self.catalog[self.entities[target].type_index]
                .maintenance
                .construction_sound
                .clone();
            self.object_sound(target, name.clone(), Action::Play);
            self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .building_sound = Some((target, name));
        }
        if !matches!(
            self.entities[id]
                .ai
                .dozer
                .as_ref()
                .expect("Dozer constructor")
                .build_phase,
            BuildPhase::Building
        ) {
            return Result::Continue;
        }
        self.entities[id].drawable.model_conditions.constructing = true;
        if !self.construct_building_piece(id, target) {
            return Result::Continue;
        }
        self.presentation
            .dozer_feedback
            .push(Feedback::Construction {
                frame: self.tick,
                owner: self.entities[id].owner,
                builder: id,
                builder_type: self.entities[id].type_index,
                object_type: self.entities[target].type_index,
                position: self.object_position(target),
            });
        let position = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor")
            .tasks[BUILD]
            .docks[2]
            .unwrap_or_else(|| self.object_position(id));
        let position = self
            .adjusted_destination_world(id, position)
            .unwrap_or(position);
        self.construction_move(id, position);
        Result::Success
    }

    pub(in crate::skirmish) fn finish_dozer_building_sound(&mut self, id: usize) {
        if let Some(state) = &mut self.entities[id].ai.dozer
            && let Some((object, name)) = state.building_sound.take()
        {
            self.object_sound(object, name, Action::Stop);
        }
    }

    fn perform_dozer_repair(&mut self, id: usize, target: usize) -> Result {
        if self.entities[target]
            .health
            .partial_cmp(&self.entities[target].max_health)
            == Some(std::cmp::Ordering::Equal)
        {
            self.presentation
                .dozer_feedback
                .push(Feedback::Repair { frame: self.tick });
            return Result::Success;
        }
        self.entities[id].drawable.model_conditions.constructing = true;
        if !self.repair_benefactor(id, target) {
            self.finish_dozer_task(id, REPAIR, true);
            self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .goal_object = None;
            return Result::Failure;
        }
        let fraction = self.catalog[self.entities[id].type_index]
            .maintenance
            .repair_per_second;
        let health = self.entities[target].max_health * fraction / 30.0;
        self.entities[target].maintenance.healing_benefactor = Some(id);
        self.entities[target].maintenance.healing_benefactor_until = self.tick + 2;
        self.apply_healing(target, Some(id), health);
        Result::Continue
    }
}
