//! Ordinary ground `AIMoveToState` / `AIInternalMoveToState`, SH AIStates.cpp1600–1944.
use crate::skirmish::{Skirmish, ai_goals::MainState, path_service};

pub(in crate::skirmish) enum MoveResult {
    Continue,
    Success,
    Failure,
}

impl Skirmish {
    pub(in crate::skirmish) fn enter_primary_ground_move(&mut self, id: usize) -> MoveResult {
        self.entities[id].ai.command.move_to.waiting =
            self.entities[id].ai.path_request.pending.is_some();
        if self.entities[id].object_status & (1 << 42) != 0 {
            return MoveResult::Failure;
        }
        let actor = &mut self.entities[id];
        actor.ai.motion.movement_active = true;
        actor.ai.motion.start_move(self.tick);
        actor.ai.blocking.start_move();
        if actor.ai.motion.ultra_accurate() || actor.flight_ultra_accurate() {
            actor.ai.command.move_to.adjusts = false;
        }
        if self.move_destination_adjusts(id) && self.entities[id].object_status & (1 << 37) == 0 {
            let goal = self.entities[id].ai.command.move_to.position;
            let goal = self
                .adjusted_destination_world(id, goal)
                .unwrap_or_else(|| self.snap_closest_goal(id, goal));
            self.entities[id].ai.command.move_to.position = goal;
            let layer = self.movement_goal_layer(id, [goal[0], goal[1]]);
            self.reserve_goal_on(id, [goal[0], goal[1]], layer);
        }
        if matches!(
            self.entities[id].ai.command.main_state,
            MainState::Repulsors
        ) {
            if self.entities[id].ai.command.repulsor.repaths == 0 {
                self.entities[id].ai.motion.end_move();
                return MoveResult::Failure;
            }
            self.entities[id].ai.command.repulsor.repaths -= 1;
        } else if self.jet_taxi_state(id) {
            self.entities[id].ai.command.move_to.waiting = true;
        } else {
            self.entities[id].ai.command.move_to.waiting = true;
            self.request_ground_path(
                id,
                path_service::Request {
                    goal: self.entities[id].ai.command.move_to.position,
                    range: 0.1,
                    kind: path_service::Kind::Move,
                },
            );
        }
        self.entities[id].ai.command.path_extra_distance = 0.0;
        self.entities[id].ai.command.desired_speed = f32::MAX;
        MoveResult::Continue
    }

    pub(in crate::skirmish) fn update_primary_ground_move(&mut self, id: usize) -> MoveResult {
        if let Some(target) = self.entities[id].ai.command.goal_object
            && self
                .entities
                .get(target)
                .is_some_and(crate::skirmish::Entity::exists)
        {
            self.entities[id].ai.command.move_to.position = self.object_position(target);
        }
        self.update_internal_move(id)
    }

    pub(in crate::skirmish) fn update_internal_move(&mut self, id: usize) -> MoveResult {
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        let movement = &mut actor.ai.command.move_to;
        if movement.waiting {
            movement.path_timestamp = now;
            if actor.ai.path_request.pending.is_some() {
                return MoveResult::Continue;
            }
            if actor.ai.ground_path.is_none() {
                return MoveResult::Failure;
            }
            movement.waiting = false;
            movement.path_goal = movement.position;
        }
        let goal = movement.position;
        let delta = [
            goal[0] - movement.path_goal[0],
            goal[1] - movement.path_goal[1],
        ];
        let target = [goal[0] - actor.position[0], goal[1] - actor.position[1]];
        let squared_delta = delta.map(|value| value * value);
        let squared_target = target.map(|value| value * value);
        let changed = squared_delta[0] + squared_delta[1]
            > (squared_target[0] + squared_target[1]) * (1.0 / (10.0 * 10.0));
        let blocked = actor.ai.blocking.stuck || actor.ai.blocking.frames > 60;
        let force = actor.ai.ground_path.is_none() || blocked;
        let recompute = force || (now.wrapping_sub(movement.path_timestamp) > 10 && changed);
        actor.drawable.model_conditions.set_flag(
            crate::skirmish::ModelConditions::MOVING,
            actor.ai.blocking.frames <= 7,
        );
        if actor.ai.ground_path.is_some() {
            actor.ai.motion.follow_path();
        }
        if recompute {
            if matches!(
                self.entities[id].ai.command.main_state,
                MainState::Repulsors
            ) {
                if self.entities[id].ai.command.repulsor.repaths == 0 {
                    return MoveResult::Failure;
                }
                self.entities[id].ai.command.repulsor.repaths -= 1;
            } else if self.jet_taxi_state(id) {
                self.entities[id].ai.command.move_to.waiting = true;
                if self.entities[id].ai.blocking.stuck {
                    self.entities[id].ai.blocking.path_through_units = true;
                }
            } else {
                self.entities[id].ai.command.move_to.waiting = true;
                self.request_ground_path(
                    id,
                    path_service::Request {
                        goal,
                        range: 0.1,
                        kind: if blocked {
                            path_service::Kind::Recovery
                        } else {
                            path_service::Kind::Move
                        },
                    },
                );
            }
            // computePath may clear the old locomotor goal. Preserve the old
            // route, when present, until this asynchronous request completes.
            if self.entities[id].ai.ground_path.is_some() {
                self.entities[id].ai.motion.follow_path();
            } else {
                return MoveResult::Continue;
            }
        }
        let actor = &self.entities[id];
        let locomotor = actor.active_locomotor(&self.catalog[actor.type_index]);
        let arrived = if locomotor.ground.is_some() {
            self.ground_path_arrived(id)
        } else if locomotor.flight.is_some() && actor.flight.is_some() {
            self.travel_flight(id, [goal[0], goal[1]], 0.1)
        } else {
            return MoveResult::Failure;
        };
        if arrived {
            if self.move_destination_adjusts(id) {
                self.entities[id].ai.motion.clear_goal();
            }
            MoveResult::Success
        } else {
            MoveResult::Continue
        }
    }
}

#[cfg(test)]
mod tests;
