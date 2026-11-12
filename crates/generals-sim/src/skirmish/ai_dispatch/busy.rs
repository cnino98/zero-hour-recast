//! `AIUpdate::privateBusy` and `AIBusyState`, SH AIUpdate.cpp4054 / AIStateMachine.h338.
use crate::skirmish::{
    Entity, Skirmish,
    ai_goals::{COMMAND_FROM_AI, MainState},
};

impl Entity {
    pub(in crate::skirmish) const fn ai_busy(&self) -> bool {
        matches!(self.ai.command.main_state, MainState::Busy)
    }

    pub(in crate::skirmish) const fn last_command_from_ai(&self) -> bool {
        self.ai.command.last_command_source == COMMAND_FROM_AI
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn set_last_command_source(&mut self, id: usize, source: i32) {
        self.entities[id].ai.command.last_command_source = source;
        self.entities[id].ai.attack_context.source = match source {
            0 => crate::skirmish::builder_disarm::Source::Player,
            1 => crate::skirmish::builder_disarm::Source::Script,
            2 => crate::skirmish::builder_disarm::Source::Ai,
            3 => crate::skirmish::builder_disarm::Source::Dozer,
            _ => unreachable!("native CommandSourceType"),
        };
    }

    /// Concrete `CleanupHazard` caller invokes this only after `isIdle()`. Idle has
    /// no onExit action; clearing its machine therefore has no movement exit.
    pub(in crate::skirmish) fn ai_busy_from_ai(&mut self, id: usize) {
        self.native_busy_from(id, COMMAND_FROM_AI);
    }

    pub(in crate::skirmish) fn native_busy_from(&mut self, id: usize, source: i32) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Busy,
            },
        ) || !self.native_ai_command_allowed(id)
        {
            return;
        }
        if self
            .prepare_order_state(id, crate::skirmish::Order::Idle, false, None, None)
            .is_none()
        {
            return;
        }
        self.clear_ai_goal(id);
        self.entities[id].ai.waypoints.points.clear();
        self.clear_temporary(id);
        self.ai_state_changed(id);
        self.set_last_command_source(id, source);
        self.entities[id].ai.command.main_state = MainState::Busy;
        self.entities[id].ai.primary.sleep_until = 0;
        self.ai_state_changed(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Order, ai_goals::COMMAND_FROM_PLAYER};

    #[test]
    fn cleanup_busy_has_shared_primary_identity_and_does_not_run_idle_targeting() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let target = game.spawn(game.entities[actor].type_index, 1, [350.0, 150.0], true);
        game.set_ai_order(actor, Order::Idle);
        game.ai_busy_from_ai(actor);
        assert!(game.entities[actor].ai_busy());
        assert!(!game.entities[actor].ai_idle());
        assert!(game.entities[actor].last_command_from_ai());
        game.update_order(actor);
        assert!(game.entities[actor].ai_busy());
        assert!(game.entities[actor].ai.ai_target.is_none());
        game.attack(actor, target).unwrap();
        assert!(!game.entities[actor].ai_busy());
        assert!(!game.entities[actor].last_command_from_ai());
    }

    #[test]
    fn internal_completion_retains_last_command_source_and_busy_preserves_locogoal() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        game.set_last_command_source(actor, COMMAND_FROM_PLAYER);
        game.set_ai_order(actor, Order::Idle);
        assert!(!game.entities[actor].last_command_from_ai());
        let goal = game.entities[actor].ai.motion.has_goal();
        game.ai_busy_from_ai(actor);
        assert_eq!(game.entities[actor].ai.motion.has_goal(), goal);
        assert!(game.entities[actor].last_command_from_ai());
    }

    #[test]
    fn temporary_updates_preserve_the_primary_states_idle_and_busy_queries() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        game.ai_idle_from_ai(actor);
        game.set_temporary_busy(actor);
        assert!(matches!(game.entities[actor].ai.order, Order::Idle));
        assert!(game.entities[actor].ai_idle());
        assert!(!game.entities[actor].ai_busy());
        game.construction_move(actor, [400.0, 150.0, 0.0]);
        assert!(matches!(game.entities[actor].ai.order, Order::Move(_)));
        assert!(!game.entities[actor].ai.blocking.temporary_move_to());
        assert!(!game.entities[actor].ai_idle());
        assert!(!game.entities[actor].ai_busy());
        game.ai_busy_from_ai(actor);
        game.construction_move(actor, [450.0, 150.0, 0.0]);
        assert!(game.entities[actor].ai.blocking.temporary_move_to());
        assert!(game.entities[actor].ai_busy());
        game.clear_temporary(actor);
        assert!(game.entities[actor].ai_busy());
    }
}
