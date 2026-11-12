//! `AIUpdate::privateFaceObject/Position` and `AIFaceState`, SH 2bfaea60.
use crate::skirmish::{
    Order, Skirmish,
    ai_goals::{COMMAND_FROM_AI, MainState},
    combat::{attack::Goal, relative_angle},
    locomotor,
};

impl Skirmish {
    /// The real AI-source producer used by `SpecialAbilityUpdate::startFacing`.
    pub(in crate::skirmish) fn ai_face_from_ai(&mut self, id: usize, goal: Goal) {
        self.native_face_from(id, goal, COMMAND_FROM_AI);
    }

    pub(in crate::skirmish) fn native_face_from(&mut self, id: usize, goal: Goal, source: i32) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Face(goal),
            },
        ) {
            return;
        }
        if self.entities[id].ai.registration.is_none() {
            return;
        }
        if self.entities[id].ai.dozer.is_some() {
            self.entities[id].drawable.model_conditions.constructing = false;
        }
        if !self.native_ai_command_allowed(id)
            || self.attack_kind(id, "IMMOBILE")
            || self.object_disabled(id)
        {
            return;
        }
        let accepted = self
            .prepare_order_state(id, Order::Idle, false, None, None)
            .is_some();
        self.clear_ai_goal(id);
        match goal {
            Goal::Object(target) => {
                if self.entities[target].exists() {
                    self.set_ai_goal_object(id, target);
                }
            }
            Goal::Position(position) => self.set_ai_goal_position(id, position),
        }
        self.entities[id].ai.blocking.frames = 0;
        self.entities[id].ai.blocking.blocked = false;
        self.entities[id].ai.blocking.stuck = false;
        self.set_last_command_source(id, source);
        self.entities[id].ai.command.from_ai = source == COMMAND_FROM_AI;
        if !accepted {
            return;
        }
        let actor = &self.entities[id];
        let rules = actor.active_locomotor(&self.catalog[actor.type_index]);
        let can_turn_in_place = rules.ground.as_ref().map_or_else(
            || {
                rules
                    .flight
                    .as_ref()
                    .is_some_and(|rules| rules.min_speed == 0.0)
            },
            |rules| rules.min_speed == 0.0,
        );
        if matches!(goal, Goal::Object(_)) && self.entities[id].ai.command.goal_object.is_none() {
            self.set_ai_order(id, Order::Idle);
            return;
        }
        self.entities[id].ai.command.main_state = match goal {
            Goal::Object(_) => MainState::FaceObject { can_turn_in_place },
            Goal::Position(_) => MainState::FacePosition { can_turn_in_place },
        };
        self.ai_state_changed(id);
    }

    pub(super) fn update_native_face(&mut self, id: usize) -> u32 {
        let actor = &self.entities[id];
        let (position, can_turn) = match actor.ai.command.main_state {
            MainState::FaceObject { can_turn_in_place } => {
                let Some(target) = actor
                    .ai
                    .command
                    .goal_object
                    .filter(|&id| self.entities[id].exists())
                else {
                    self.set_ai_order(id, Order::Idle);
                    return 1;
                };
                (self.object_position(target), can_turn_in_place)
            }
            MainState::FacePosition { can_turn_in_place } => {
                (actor.ai.command.goal_position, can_turn_in_place)
            }
            _ => unreachable!("actual Face primary state"),
        };
        let angle = relative_angle(actor.position, actor.facing, [position[0], position[1]]);
        if angle.abs() < 0.035 {
            self.set_ai_order(id, Order::Idle);
        } else if can_turn {
            let desired = actor.facing + angle;
            self.entities[id].ai.motion.request_angle(desired);
        } else if actor
            .active_locomotor(&self.catalog[actor.type_index])
            .flight
            .is_some()
        {
            self.aim_moving_flight(id, position);
        } else {
            let actor = &mut self.entities[id];
            actor.ai.motion.request_path(locomotor::GroundGoal {
                height: position[2],
                position: [position[0], position[1]],
                distance: 0.0,
                desired_speed: actor.ai.command.desired_speed,
                no_slow_down: actor.ai.motion.no_slow_down(),
                ultra_accurate: actor.ai.motion.ultra_accurate(),
            });
        }
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::ground_movement::tests::moving_tank;

    #[test]
    fn real_face_command_uses_primary_clock_and_locomotor_then_enters_idle_at_strict_tolerance() {
        let (mut game, actor) = moving_tank();
        let position = [300.0, 400.0, 20.0];
        game.ai_face_from_ai(actor, Goal::Position(position));
        assert!(matches!(
            game.entities[actor].ai.command.main_state,
            MainState::FacePosition {
                can_turn_in_place: true
            }
        ));
        assert!(game.entities[actor].last_command_from_ai());
        assert_eq!(
            game.entities[actor].ai_goal_position().map(f32::to_bits),
            position.map(f32::to_bits)
        );
        for _ in 0..100 {
            game.step();
            if game.entities[actor].ai_idle() {
                break;
            }
        }
        assert!(game.entities[actor].ai_idle());
        assert!(
            relative_angle(
                game.entities[actor].position,
                game.entities[actor].facing,
                [position[0], position[1]]
            )
            .abs()
                < 0.035
        );
        assert_eq!(
            game.entities[actor].ai_goal_position().map(f32::to_bits),
            position.map(f32::to_bits)
        );
    }

    #[test]
    fn face_object_tracks_real_target_while_retaining_entry_primary_position_and_turn_capability() {
        let (mut game, actor) = moving_tank();
        let kind = game.entities[actor].type_index;
        let target = game.spawn(kind, 0, [350.0, 150.0], true);
        let snapshot = game.object_position(target);
        game.ai_face_from_ai(actor, Goal::Object(target));
        game.set_object_position(target, [300.0, 300.0, 0.0]);
        game.update_native_face(actor);
        assert!(matches!(
            game.entities[actor].ai.command.main_state,
            MainState::FaceObject {
                can_turn_in_place: true
            }
        ));
        assert_eq!(
            game.entities[actor].ai_goal_position().map(f32::to_bits),
            snapshot.map(f32::to_bits)
        );
        game.destroy_maintenance_object(target);
        game.process_update_destruction();
        game.update_native_face(actor);
        assert!(game.entities[actor].ai_idle());
    }

    #[test]
    fn nonzero_minimum_speed_uses_explicit_goal_with_native_zero_remaining_distance() {
        let (mut game, actor) = moving_tank();
        let kind = game.entities[actor].type_index;
        game.catalog[kind]
            .locomotor
            .ground
            .as_mut()
            .unwrap()
            .min_speed = 1.0;
        game.ai_face_from_ai(actor, Goal::Position([300.0, 400.0, 20.0]));
        game.update_native_face(actor);
        let Some(locomotor::GroundRequest::Path(goal)) =
            game.entities[actor].ai.motion.take_request()
        else {
            panic!("source POSITION_EXPLICIT locomotor request");
        };
        assert_eq!(goal.distance.to_bits(), 0.0_f32.to_bits());
        assert_eq!(goal.height.to_bits(), 20.0_f32.to_bits());
        assert_eq!(
            goal.position.map(f32::to_bits),
            [300.0_f32, 400.0].map(f32::to_bits)
        );
    }
}
