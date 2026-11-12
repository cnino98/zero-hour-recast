//! AIUpdate.cpp2927 privateMoveToObject and `AIMoveToState`'s tracked object goal.
use crate::skirmish::{Order, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn native_move_to_object_from(
        &mut self,
        id: usize,
        target: usize,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::MoveObject(target),
            },
        ) {
            return;
        }
        if !self.native_ai_command_allowed(id)
            || self.entities[id].is_ai_dead()
            || self.catalog[self.entities[id].type_index].immobile()
            || self.object_disabled(id)
        {
            return;
        }
        let position = self.object_position(target);
        let order = Order::Move([position[0], position[1]]);
        let Some(ground_move) = self.prepare_order_state(id, order, false, None, Some(source))
        else {
            return;
        };
        self.clear_ai_goal(id);
        self.set_ai_goal_object(id, target);
        let actor = &mut self.entities[id];
        actor.ai.command.move_to.position = position;
        actor.ai.command.move_to.adjusts = actor.ai.ignored_producer != Some(target);
        actor.ai.blocking.frames = 0;
        actor.ai.blocking.blocked = false;
        actor.ai.blocking.stuck = false;
        self.enter_order_state(id, order, ground_move);
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Order, ai_goals::COMMAND_FROM_AI, ground_movement::tests::moving_tank};

    #[test]
    fn move_to_object_tracks_new_xyz_and_preserves_ignored_obstacle() {
        let (mut game, actor) = moving_tank();
        let target = game.spawn(game.entities[actor].type_index, 0, [430.0, 150.0], true);
        game.ai_busy_from_ai(target);
        game.entities[actor].ai.ignored_producer = Some(target);
        game.native_move_to_object_from(actor, target, COMMAND_FROM_AI);
        assert!(matches!(game.entities[actor].ai.order, Order::Move(_)));
        assert_eq!(game.entities[actor].ai.command.goal_object, Some(target));
        assert_eq!(
            game.entities[actor].ai_goal_position().map(f32::to_bits),
            game.object_position(target).map(f32::to_bits)
        );
        assert_eq!(game.entities[actor].ai.ignored_producer, Some(target));
        assert!(!game.entities[actor].ai.command.move_to.adjusts);
        assert!(game.entities[actor].ai.path_request.pending.is_some());
        assert!(game.entities[actor].ai.blocking.temporary_goal().is_none());
        game.set_object_position(target, [440.0, 180.0, 25.0]);
        game.update_primary_ground_move(actor);
        assert_eq!(
            game.entities[actor]
                .ai
                .command
                .move_to
                .position
                .map(f32::to_bits),
            [440.0_f32, 180.0, 25.0].map(f32::to_bits)
        );
        assert_eq!(
            game.entities[actor].ai.command.last_command_source,
            COMMAND_FROM_AI
        );
    }

    #[test]
    fn move_to_object_replaces_primary_attack_and_cleans_up_its_victim() {
        let (mut game, actor) = moving_tank();
        let target = game.spawn(game.entities[actor].type_index, 1, [350.0, 150.0], true);
        game.attack(actor, target).unwrap();
        game.step();
        assert_eq!(game.entities[actor].ai.current_victim, Some(target));
        game.native_move_to_object_from(actor, target, COMMAND_FROM_AI);
        assert!(game.entities[actor].ai.attack.is_none());
        assert!(game.entities[actor].ai.current_victim.is_none());
        assert_eq!(game.entities[actor].ai.command.goal_object, Some(target));
        assert!(game.entities[actor].ai.blocking.temporary_goal().is_none());
    }
}
