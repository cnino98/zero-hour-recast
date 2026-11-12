//! AIStates.cpp2356–3140: retained Pursue/Approach path requests and exits.
use super::{Goal, Phase, Return, Skirmish, State};
use crate::skirmish::{CELL_SIZE, STEP_SECONDS, locomotor, path_service};

impl Skirmish {
    pub(super) fn attack_can_pursue(&self, id: usize, target: usize) -> bool {
        let actor = &self.entities[id];
        let victim = &self.entities[target];
        let unit = &self.catalog[actor.type_index];
        if self.catalog[victim.type_index]
            .crush
            .physics_module
            .is_none()
            || self.current_weapon_turret(id).is_none()
        {
            return false;
        }
        if self.ai_rules.crushes_infantry
            && self.computer_player(actor.owner)
            && self.can_crush(id, target)
        {
            return true;
        }
        if self.range_distance(id, target) < actor.active_weapon(unit).min_range {
            return false;
        }
        let maximum = locomotor::speed(actor, unit) * STEP_SECONDS;
        let speed = victim.physics_forward_speed();
        if speed >= maximum || speed < maximum / 10.0 {
            return false;
        }
        let dx = victim.position[0] - actor.position[0];
        let dy = victim.position[1] - actor.position[1];
        let forward_x = dx * victim.facing.cos();
        let forward_y = dy * victim.facing.sin();
        forward_x + forward_y >= 0.0
    }

    pub(super) fn enter_attack_movement(&mut self, id: usize, state: &mut State) -> Return {
        let pursue = matches!(state.phase, Phase::Pursue);
        if !pursue
            && self.attack_kind(id, "PROJECTILE")
            && let Some(flight) = &mut self.entities[id].flight
        {
            flight.set_precise_z(true);
        }
        if let Goal::Object(target) = state.goal {
            if self.entities[target].is_effectively_dead() {
                return Return::Success;
            }
            if pursue && self.attack_kind(id, "PROJECTILE") {
                return Return::Success;
            }
            if !pursue && self.attack_movement_in_range(id, state.goal) {
                return Return::Success;
            }
            let retaliating =
                matches!(self.entities[id].ai.order, crate::skirmish::Order::Guard(_))
                    && self.entities[id].ai.guard.retaliating;
            let human_ai = !retaliating
                && !self.computer_player(self.entities[id].owner)
                && self.entities[id].last_command_from_ai();
            if human_ai {
                if pursue {
                    return Return::Success;
                }
                let actor = &self.entities[id];
                if !actor.ai.command.allowed_to_chase
                    && !actor
                        .active_weapon(&self.catalog[actor.type_index])
                        .contact()
                {
                    return Return::Failure;
                }
            }
            if !pursue && !retaliating && self.attack_computer_avoids_aircraft(id, target) {
                return Return::Failure;
            }
            let can_pursue = self.attack_can_pursue(id, target);
            if pursue && !can_pursue || !pursue && can_pursue {
                return Return::Success;
            }
        } else if pursue {
            return Return::Success;
        } else {
            let actor = &mut self.entities[id];
            actor.ai.path.clear();
            actor.ai.path_request.attack_path = false;
            actor.ai.ground_path = None;
            actor.ai.path_endpoint = None;
            actor.ai.motion.clear_goal();
        }
        state.movement.previous_position = [0.0; 3];
        state.movement.approach_frame = 0_u32.wrapping_sub(10);
        state.movement.waiting = false;
        if let Some(which) = self.current_weapon_turret(id) {
            self.set_native_turret_target(id, which, Some(state.goal), state.forced, false);
        }
        if !self.compute_attack_state_path(id, state, pursue) {
            return if pursue {
                Return::Success
            } else {
                Return::Failure
            };
        }
        self.begin_ground_move(id);
        Return::Continue
    }

    fn attack_computer_avoids_aircraft(&self, id: usize, target: usize) -> bool {
        self.computer_player(self.entities[id].owner)
            && !matches!(
                self.entities[id].ai.command.main_state,
                crate::skirmish::ai_goals::MainState::Hunt
            )
            && self.attack_kind(target, "AIRCRAFT")
            && self.entities[target].is_airborne_target()
    }

    fn compute_attack_state_path(&mut self, id: usize, state: &mut State, pursue: bool) -> bool {
        if self.attack_kind(id, "IMMOBILE") || self.object_disabled(id) {
            return false;
        }
        let actor = &self.entities[id];
        if pursue && actor.ai.blocking.stuck {
            return false;
        }
        if state.movement.waiting {
            return true;
        }
        let forced = actor.ai.blocking.stuck
            || actor.ai.ground_path.is_none() && actor.ai.path_request.pending.is_none();
        let now = self.update_frame();
        if !forced && now.wrapping_sub(state.movement.approach_frame) < 10 {
            return true;
        }
        state.movement.approach_frame = now;
        let goal = self.attack_goal_position(state.goal);
        if let Goal::Object(target) = state.goal {
            if !forced
                && same_position(
                    self.object_position(id),
                    state.movement.previous_position,
                    goal,
                )
            {
                return true;
            }
            state.movement.previous_position = goal;
            let can_pursue = self.attack_can_pursue(id, target);
            if pursue != can_pursue {
                return false;
            }
            let actor = &self.entities[id];
            if !pursue
                && actor
                    .active_weapon(&self.catalog[actor.type_index])
                    .contact()
            {
                self.entities[id].ai.ignored_producer = Some(target);
                self.entities[id].ai.command.path_extra_distance = 10.0 * CELL_SIZE;
            }
        } else if !forced {
            return true;
        }
        state.movement.goal = goal;
        let target = match state.goal {
            Goal::Object(target) => target,
            Goal::Position(_) => usize::MAX,
        };
        state.movement.waiting = true;
        let requested = match state.goal {
            Goal::Object(target) if !pursue => self.target_center(target),
            _ => goal,
        };
        self.request_ground_path(
            id,
            path_service::Request {
                goal: requested,
                range: 0.1,
                kind: if pursue {
                    path_service::Kind::Move
                } else {
                    path_service::Kind::Attack(target)
                },
            },
        );
        true
    }

    pub(super) fn update_attack_movement(
        &mut self,
        id: usize,
        state: &mut State,
        pursue: bool,
    ) -> Return {
        let mut result = self.update_attack_movement_internal(id, state, pursue);
        let initial = if pursue {
            state.movement.initial_pursuit
        } else {
            state.movement.initial_approach
        };
        if initial
            && let Some(which) = self.current_weapon_turret(id)
            && let Some(target) = self.mood_target(id, true, false)
        {
            self.set_native_turret_target(
                id,
                which,
                Some(Goal::Object(target)),
                state.forced,
                false,
            );
        }
        // A failed internal object movement returns through AIM so range and
        // legality are reevaluated. Position failure exits the outer attack.
        if matches!(result, Return::Failure)
            && !pursue
            && matches!(state.goal, Goal::Object(_))
            && self.entities[id].ai.path_request.failed
        {
            result = Return::Success;
        }
        result
    }

    fn update_attack_movement_internal(
        &mut self,
        id: usize,
        state: &mut State,
        pursue: bool,
    ) -> Return {
        if let Goal::Object(target) = state.goal {
            let victim = &self.entities[target];
            if !victim.exists() || victim.is_effectively_dead() {
                self.notify_attack_victim_dead(id);
                self.set_attack_current_victim(id, None);
                return Return::Failure;
            }
            if !pursue && self.attack_computer_avoids_aircraft(id, target) {
                return Return::Failure;
            }
            if victim.stealthed() && !victim.detected() && !victim.has_native_disguise() {
                let actor = &self.entities[id];
                let disarm = matches!(
                    actor
                        .active_weapon(&self.catalog[actor.type_index])
                        .damage_type,
                    crate::skirmish::DamageType::Disarm
                ) && ["MINE", "BOOBY_TRAP", "DEMOTRAP"]
                    .iter()
                    .any(|kind| self.attack_kind(target, kind));
                if pursue || !disarm {
                    return Return::Failure;
                }
            }
            self.set_attack_current_victim(id, Some(target));
            if !pursue {
                let actor = &self.entities[id];
                if actor
                    .active_weapon(&self.catalog[actor.type_index])
                    .contact()
                    && self.attack_goal_in_range(id, state.goal)
                {
                    return Return::Success;
                }
                if !actor.ai.path_request.attack_path
                    && self.attack_movement_in_range(id, state.goal)
                {
                    return Return::Success;
                }
            }
        } else if !self.entities[id].ai.path_request.attack_path
            && self.attack_movement_in_range(id, state.goal)
        {
            return Return::Success;
        }
        if state.movement.waiting && self.entities[id].ai.path_request.pending.is_none() {
            state.movement.waiting = false;
            if self.entities[id].ai.path_request.failed {
                return Return::Failure;
            }
            if self.entities[id].ai.ground_path.is_none() {
                return Return::Success;
            }
        }
        if !self.compute_attack_state_path(id, state, pursue) {
            return if pursue || matches!(state.goal, Goal::Position(_)) {
                Return::Failure
            } else {
                Return::Success
            };
        }
        if self.entities[id].ai.path_request.pending.is_some() {
            // AIInternalMoveToState waits before restoring the locomotor goal.
            // A throttled attack request deliberately cleared that goal.
            return Return::Continue;
        }
        if self.entities[id]
            .active_locomotor(&self.catalog[self.entities[id].type_index])
            .ground
            .is_some()
        {
            if self.ground_path_arrived(id) {
                return Return::Success;
            }
            let moving = self.entities[id].ai.blocking.frames <= 7;
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(crate::skirmish::ModelConditions::MOVING, moving);
            self.entities[id].ai.motion.follow_path();
        } else {
            let point = [state.movement.goal[0], state.movement.goal[1]];
            if self.entities[id].flight.is_some()
                && self.entities[id]
                    .active_locomotor(&self.catalog[self.entities[id].type_index])
                    .flight
                    .is_some()
            {
                if self.travel_flight(id, point, 0.1) {
                    return Return::Success;
                }
            } else {
                // A missing locomotor cannot advance the retained native path.
                return Return::Failure;
            }
        }
        if pursue {
            return self.update_pursuit_speed(id, state);
        }
        Return::Continue
    }

    fn update_pursuit_speed(&mut self, id: usize, state: &mut State) -> Return {
        if let Goal::Object(target) = state.goal {
            if self.current_weapon_turret(id).is_none() {
                return Return::Success;
            }
            if self.attack_movement_in_range(id, state.goal) {
                let which = self.current_weapon_turret(id).expect("pursuit turret");
                self.set_native_turret_target(id, which, Some(state.goal), state.forced, false);
                state.movement.initial_pursuit = false;
                let speed = self.entities[target].physics_forward_speed();
                let close = self.weapon_goal_in_range(
                    id,
                    self.object_position(id),
                    Some(target),
                    self.object_position(target),
                );
                self.entities[id].ai.command.desired_speed = if self.can_crush(id, target) {
                    f32::MAX
                } else if close {
                    speed * 0.95
                } else {
                    speed
                };
            } else {
                self.entities[id].ai.command.desired_speed = f32::MAX;
            }
        }
        Return::Continue
    }

    pub(super) fn exit_attack_movement(&mut self, id: usize, state: &mut State) {
        let ground = self.attack_ground_movement(id);
        let projectile = self.attack_kind(id, "PROJECTILE");
        let actor = &mut self.entities[id];
        actor.ai.motion.end_move();
        actor.ai.motion.movement_active = false;
        if matches!(state.phase, Phase::Approach) {
            actor.ai.ignored_producer = None;
            if projectile && let Some(flight) = &mut actor.flight {
                flight.set_precise_z(false);
            }
            let delta = [
                state.movement.goal[0] - actor.position[0],
                state.movement.goal[1] - actor.position[1],
            ];
            let squared = delta.map(|component| component * component);
            if ground && squared[0] + squared[1] < CELL_SIZE * CELL_SIZE * 0.125 {
                actor.position = [state.movement.goal[0], state.movement.goal[1]];
                actor.set_physical_position(state.movement.goal);
            }
        }
        if matches!(state.phase, Phase::Approach) {
            state.movement.initial_approach = false;
        } else {
            state.movement.initial_pursuit = false;
        }
    }

    fn attack_movement_in_range(&self, id: usize, goal: Goal) -> bool {
        if !self.attack_goal_in_range(id, goal) {
            return false;
        }
        let target = match goal {
            Goal::Object(target) => Some(target),
            Goal::Position(_) => None,
        };
        !(self.attack_ground_movement(id)
            && target.is_none_or(|target| self.height_above_ground(target) <= -9.0 * self.gravity)
            && self.attack_view_blocked(
                id,
                target,
                self.object_position(id),
                self.attack_goal_position(goal),
            ))
    }
}

fn same_position(source: [f32; 3], previous: [f32; 3], current: [f32; 3]) -> bool {
    let moved = [
        (current[0] - previous[0]).powi(2),
        (current[1] - previous[1]).powi(2),
    ];
    let distance = [
        (current[0] - source[0]).powi(2),
        (current[1] - source[1]).powi(2),
    ];
    moved[0] + moved[1] <= (distance[0] + distance[1]) * (1.0 / (10.0 * 10.0))
}

#[cfg(test)]
mod tests {
    use super::{Goal, Phase, Return};
    use crate::skirmish::{Order, ai_goals::COMMAND_FROM_PLAYER};

    #[test]
    fn approach_uses_current_range_even_after_the_weapon_has_leeched_its_target() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let kind = game.entities[actor].type_index;
        game.catalog[kind].weapon.leech_range = true;
        let target = game.spawn(kind, 1, [350.0, 150.0], true);
        game.set_last_command_source(actor, COMMAND_FROM_PLAYER);
        game.set_order(actor, Order::Attack(target));
        for _ in 0..2 {
            game.tick += 1;
            game.update_native_attack(actor, Goal::Object(target));
        }
        assert!(!game.presentation.shots.is_empty());
        assert!(game.entities[actor].weapon_set.weapons[0].leech_range_active);
        game.entities[target].position = [800.0, 150.0];
        assert!(!game.attack_out_of_range(actor, Goal::Object(target)));
        let mut state = game.entities[actor].ai.attack.take().unwrap();
        state.phase = Phase::Approach;
        assert!(matches!(
            game.enter_attack_movement(actor, &mut state),
            Return::Continue
        ));
        assert!(game.entities[actor].ai.path_request.pending.is_some());
    }
}

#[cfg(test)]
mod approach_tests;
