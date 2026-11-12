//! SH `AIIdleState` -> `AIMoveAwayFromRepulsorsState` -> `AIWanderInPlaceState`.
use crate::skirmish::{
    CELL_SIZE, Skirmish, ai_goals::MainState, ground_movement::MoveResult, locomotor,
};
use num_traits::ToPrimitive;

pub(in crate::skirmish) struct State {
    pub(in crate::skirmish) repaths: u8,
    check_path: bool,
    goal: [f32; 3],
    origin: [f32; 3],
    wait_frames: i32,
    timer: i32,
}

impl State {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            repaths: 0,
            check_path: false,
            goal: [0.0; 3],
            origin: [0.0; 3],
            wait_frames: 0,
            timer: 0,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn closest_repulsor(&self, id: usize) -> Option<usize> {
        if !self.ai_rules.repulsors_enabled {
            return None;
        }
        let source = &self.entities[id];
        let range = source.vision_range(&self.catalog[source.type_index]);
        let mut nearest_squared = range * range;
        let mut nearest = None;
        let mut found_ring = None;
        for (ring, other) in self.partition.outward_range_rings(source.position, range) {
            if found_ring.is_some_and(|found| ring > found) {
                break;
            }
            if id == other {
                continue;
            }
            let victim = &self.entities[other];
            if !victim.exists()
                || victim.stealthed() && !victim.detected() && !victim.has_native_disguise()
            {
                continue;
            }
            let squared = crate::skirmish::combat::circle_distance_squared(
                source.position,
                victim.position,
                self.object_radius(id) + self.object_radius(other),
            );
            if squared >= nearest_squared {
                continue;
            }
            if victim.object_status & (1 << 9) == 0
                && (victim.is_effectively_dead()
                    || self.object_relationship(id, other) != 0
                    || !self.attack_kind(other, "STRUCTURE") && self.attack_kind(other, "INERT")
                    || !self.object_can_attack(other))
            {
                continue;
            }
            nearest = Some(other);
            nearest_squared = squared;
            found_ring = Some(ring);
        }
        nearest
    }

    pub(in crate::skirmish) fn enter_repulsor_state(&mut self, id: usize) {
        self.exit_waypoint_state(id);
        self.enter_repulsor_transition(id, true);
    }

    fn enter_repulsor_transition(&mut self, id: usize, mut repulsor: bool) {
        // State::friend_checkForTransitions returns failure at depth twenty,
        // retaining the last entered state even when its entry failed.
        for _ in 0..20 {
            let result = if repulsor {
                self.enter_move_away_from_repulsors(id)
            } else {
                self.enter_wander_in_place(id)
            };
            if matches!(result, MoveResult::Continue) {
                return;
            }
            repulsor = !repulsor;
        }
    }

    fn enter_move_away_from_repulsors(&mut self, id: usize) -> MoveResult {
        self.exit_repulsor_state(id);
        self.entities[id].ai.command.main_state = MainState::Repulsors;
        self.entities[id].ai.primary.sleep_until = 0;
        self.ai_state_changed(id);
        let Some(enemy) = self.closest_repulsor(id) else {
            return MoveResult::Failure;
        };
        self.choose_locomotor_set(id, locomotor::set::PANIC);
        self.entities[id]
            .drawable
            .model_conditions
            .set_flag(crate::skirmish::ModelConditions::PANICKING, true);
        self.entities[id].ai.command.repulsor.repaths = 1;
        self.entities[id].ai.command.repulsor.check_path = true;
        self.entities[id].ai.command.move_to.position = self.entities[id].ai.command.repulsor.goal;
        self.entities[id].ai.command.move_to.adjusts = false;
        self.remove_goal(id);
        self.request_safe_path(id, enemy);
        self.enter_primary_ground_move(id)
    }

    fn enter_wander_in_place(&mut self, id: usize) -> MoveResult {
        self.exit_repulsor_state(id);
        self.entities[id].ai.command.main_state = MainState::WanderInPlace;
        self.entities[id].ai.primary.sleep_until = 0;
        self.entities[id].ai.command.repulsor.origin = self.object_position(id);
        self.choose_locomotor_set(id, locomotor::set::WANDER);
        self.entities[id].ai.command.repulsor.timer = 0;
        self.entities[id].ai.command.repulsor.wait_frames =
            10 + i32::try_from(id & 7).expect("bounded native object id bits");
        let result = self.start_wander_in_place_move(id);
        self.ai_state_changed(id);
        result
    }

    fn start_wander_in_place_move(&mut self, id: usize) -> MoveResult {
        let actor = &self.entities[id];
        let loco = actor.active_locomotor(&self.catalog[actor.type_index]);
        let delta = if loco.wander_length.is_none() {
            3
        } else {
            let radius = loco.wander_about_point_radius;
            (radius / CELL_SIZE + 0.5)
                .floor()
                .to_i32()
                .expect("native wander radius")
        };
        let span = u32::try_from(i64::from(delta) * 2).expect("nonnegative native wander radius");
        let mut goal = self.entities[id].ai.command.repulsor.origin;
        for coordinate in &mut goal[..2] {
            let offset = i64::from(self.random_between(span)) - i64::from(delta);
            let displacement = offset.to_f32().expect("native wander offset") * CELL_SIZE;
            *coordinate += displacement;
        }
        self.entities[id].ai.command.move_to.adjusts =
            !self.entities[id].ai.motion.ultra_accurate();
        self.entities[id].ai.command.move_to.position = goal;
        self.enter_primary_ground_move(id)
    }

    pub(in crate::skirmish) fn update_repulsor_state(&mut self, id: usize) {
        if matches!(
            self.entities[id].ai.command.main_state,
            MainState::Repulsors
        ) {
            if self.entities[id].ai.command.repulsor.check_path
                && self.entities[id].ai.path_request.pending.is_none()
                && let Some(goal) = self.entities[id]
                    .ai
                    .ground_path
                    .as_ref()
                    .and_then(crate::skirmish::path_following::Path::last_position)
            {
                self.entities[id].ai.command.repulsor.goal = goal;
                self.entities[id].ai.command.move_to.position = goal;
                self.entities[id].ai.command.move_to.adjusts = false;
                self.entities[id].ai.command.repulsor.check_path = false;
            }
            if !matches!(self.update_internal_move(id), MoveResult::Continue) {
                self.enter_repulsor_transition(id, false);
            }
        } else {
            let result = self.update_internal_move(id);
            if self.attack_kind(id, "CAN_BE_REPULSED") {
                self.entities[id].ai.command.repulsor.timer -= 1;
                if self.entities[id].ai.command.repulsor.timer < 0 {
                    self.entities[id].ai.command.repulsor.timer =
                        self.entities[id].ai.command.repulsor.wait_frames;
                    if self.closest_repulsor(id).is_some() {
                        self.enter_repulsor_state(id);
                        return;
                    }
                }
            }
            if !matches!(result, MoveResult::Continue) {
                self.start_wander_in_place_move(id);
            }
        }
    }

    pub(in crate::skirmish) fn exit_repulsor_state(&mut self, id: usize) {
        if matches!(
            self.entities[id].ai.command.main_state,
            MainState::Repulsors | MainState::WanderInPlace
        ) {
            let actor = &mut self.entities[id];
            actor.ai.motion.end_move();
            actor.ai.motion.movement_active = false;
            actor
                .drawable
                .model_conditions
                .set_flag(crate::skirmish::ModelConditions::PANICKING, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_repulsion_waits_for_the_fifo_and_stop_clears_the_panicking_condition() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let kind = game.entities[actor].type_index;
        game.catalog[kind].kind_of.push("CAN_BE_REPULSED".into());
        game.catalog[kind].ai.vision_range = 80.0;
        game.ai_rules.repulsors_enabled = true;
        game.ai_rules.repulsed_distance = 20.0;
        let repulsor = game.spawn(kind, 0, [340.0, 150.0], true);
        game.entities[repulsor].object_status |= 1 << 9;
        game.update_ground_contacts();
        game.tick = 10;
        game.update_ai_module(actor);
        assert!(matches!(
            game.entities[actor].ai.command.main_state,
            MainState::Repulsors
        ));
        assert!(!game.entities[actor].ai_idle());
        assert!(game.entities[actor].ai.path_request.pending.is_some());
        assert!(game.entities[actor].ai.ground_path.is_none());
        assert!(
            game.model_flags(&game.entities[actor], [0.5, 0.1])
                .contains("PANICKING")
        );
        game.process_path_service();
        let end = game.entities[actor]
            .ai
            .ground_path
            .as_ref()
            .unwrap()
            .last_position()
            .unwrap();
        let delta = [end[0] - 340.0, end[1] - 150.0];
        let squared = delta.map(|component| component * component);
        assert!(squared[0] + squared[1] > 100.0 * 100.0);
        game.stop(actor).unwrap();
        assert!(
            !game
                .model_flags(&game.entities[actor], [0.5, 0.1])
                .contains("PANICKING")
        );
        game.finish_completed_movement(actor);
        assert!(game.entities[actor].ai.ground_path.is_none());
    }

    #[test]
    fn explicit_repulsor_status_precedes_dead_relationship_and_attack_filters() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        game.ai_rules.repulsors_enabled = true;
        let target = game.spawn(game.entities[actor].type_index, 0, [320.0, 150.0], true);
        game.entities[target].set_effectively_dead(true);
        game.entities[target].object_status |= (1 << 9) | (1 << 6);
        game.update_ground_contacts();
        assert_eq!(game.closest_repulsor(actor), Some(target));
        game.entities[target].object_status &= !(1 << 9);
        assert_eq!(game.closest_repulsor(actor), None);
        game.entities[target].object_status |= 1 << 9;
        game.ai_rules.repulsors_enabled = false;
        assert_eq!(game.closest_repulsor(actor), None);
    }

    #[test]
    fn immobile_entry_keeps_safe_request_but_never_starts_move_or_consumes_repath() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let kind = game.entities[actor].type_index;
        game.ai_rules.repulsors_enabled = true;
        game.entities[actor].object_status |= 1 << 42;
        let other = game.spawn(kind, 0, [320.0, 150.0], true);
        game.entities[other].object_status |= 1 << 9;
        game.update_ground_contacts();
        game.tick = 10;
        game.enter_repulsor_state(actor);
        assert!(game.entities[actor].ai.path_request.pending.is_some());
        assert!(!game.entities[actor].ai.motion.is_moving());
        assert!(!game.entities[actor].ai.motion.movement_active);
        assert_eq!(game.entities[actor].ai.command.repulsor.repaths, 1);
        assert!(matches!(
            game.entities[actor].ai.command.main_state,
            MainState::WanderInPlace
        ));
        assert!(
            !game.entities[actor]
                .drawable
                .model_conditions
                .has_flag(crate::skirmish::ModelConditions::PANICKING)
        );
        game.entities[actor].object_status &= !(1 << 42);
        game.enter_repulsor_state(actor);
        assert!(game.entities[actor].ai.motion.is_moving());
        assert_eq!(game.entities[actor].ai.command.repulsor.repaths, 0);
        assert!(
            game.entities[actor]
                .drawable
                .model_conditions
                .has_flag(crate::skirmish::ModelConditions::PANICKING)
        );
    }
}
