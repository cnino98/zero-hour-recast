//! Direct AI-source attack commands used by native update modules.
use crate::skirmish::{Order, Skirmish, ai_goals::COMMAND_FROM_AI, combat::attack::Goal};

impl Skirmish {
    /// `aiDoCommand` rejects effectively dead actors and sleeping AI controllers.
    pub(in crate::skirmish) fn native_ai_command_allowed(&self, id: usize) -> bool {
        self.native_ai_command_allowed_for_sleep(id, false)
    }

    pub(in crate::skirmish) fn native_ai_command_allowed_for_sleep(
        &self,
        id: usize,
        even_if_sleeping: bool,
    ) -> bool {
        let actor = &self.entities[id];
        !(actor.is_effectively_dead()
            || actor
                .delivery
                .as_ref()
                .is_some_and(|delivery| !delivery.accepting_commands)
            || !even_if_sleeping
                && self.computer_player(actor.owner)
                && actor.ai.mood.attitude == -2)
    }

    /// `privateIdle` clears the primary machine, enters Idle, then records source.
    pub(in crate::skirmish) fn ai_idle_from_ai(&mut self, id: usize) {
        self.native_idle_from(id, COMMAND_FROM_AI);
    }

    pub(in crate::skirmish) fn native_idle_from(&mut self, id: usize, source: i32) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Idle,
            },
        ) {
            return;
        }
        if self.entities[id].chinook.is_some()
            && let Some(rider) = self.entities[id].attachment_child()
            && self.entities[rider].ai.registration.is_some()
        {
            self.native_idle_from(rider, source);
        }
        self.assault_command_from(id, Order::Idle, None, source, false);
        if self.entities[id].ai.dozer.is_some() {
            self.entities[id].drawable.model_conditions.constructing = false;
        }
        if !self.native_ai_command_allowed(id) || self.attack_kind(id, "PROJECTILE") {
            return;
        }
        self.clear_ai_goal(id);
        self.set_ai_order(id, Order::Idle);
        self.set_last_command_source(id, source);
        self.entities[id].ai.command.from_ai = source == COMMAND_FROM_AI;
        // This virtual list redirects Overlord containment and Tunnel networks.
        let passengers = self.passengers(id).to_vec();
        for passenger in passengers {
            if self.entities[passenger].ai.registration.is_some() {
                self.native_idle_from(passenger, source);
            }
        }
    }

    /// `AIUpdate::privateAttackObject/privateAttackPosition` assign the shot limit
    /// after `AIAttackState::onEnter` has selected and initialized the weapon.
    pub(in crate::skirmish) fn ai_attack_goal(&mut self, id: usize, goal: Goal, max_shots: u32) {
        self.native_attack_goal_from(id, goal, max_shots, COMMAND_FROM_AI, false);
    }

    pub(in crate::skirmish) fn native_attack_goal_from(
        &mut self,
        id: usize,
        goal: Goal,
        max_shots: u32,
        source: i32,
        forced: bool,
    ) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Attack {
                    goal,
                    shots: max_shots,
                    forced,
                },
            },
        ) {
            return;
        }
        if !self.chinook_attack_passengers(id, goal, max_shots, source, forced) {
            return;
        }
        let (order, position) = match goal {
            Goal::Object(target) => (Order::Attack(target), None),
            Goal::Position(position) => {
                let actor = &self.entities[id];
                (
                    Order::FireAt {
                        position: [position[0], position[1]],
                        weapon: actor.weapon_index(actor.weapon_slot()),
                    },
                    Some(position),
                )
            }
        };
        self.assault_command_from(id, order, position, source, forced);
        if !self.native_ai_command_allowed(id) || self.entities[id].is_ai_dead() {
            return;
        }
        self.transport_attack_passengers(id, goal, max_shots, source, forced);
        self.entities[id].ai.command.from_ai = source == COMMAND_FROM_AI;
        self.entities[id].ai.attack_context.forced = forced;
        self.set_last_command_source(id, source);
        self.set_order_state(id, order, false, position);
        let slot = self.entities[id].weapon_slot();
        self.entities[id].weapon_set.weapons[slot].max_shots = Some(max_shots);
        self.worker_drop_mine_clearing_cargo(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Order, ai_goals::COMMAND_FROM_PLAYER};

    #[test]
    fn ai_idle_clears_attack_goal_and_current_victim() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let enemy = game.spawn(game.entities[actor].type_index, 1, [400.0, 150.0], true);
        game.attack(actor, enemy).unwrap();
        // AIAttackState::update assigns currentVictim after onEnter.
        game.step();
        assert_eq!(game.entities[actor].ai.current_victim, Some(enemy));
        game.ai_idle_from_ai(actor);
        assert!(game.entities[actor].ai_idle());
        assert!(game.entities[actor].ai.command.goal_object.is_none());
        assert!(game.entities[actor].ai.current_victim.is_none());
        assert!(game.entities[actor].last_command_from_ai());
    }

    #[test]
    fn ai_idle_propagates_to_actual_contained_ai() {
        let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
        let passenger = game.spawn(5, 0, [355.0, 335.0], true);
        game.add_to_contain(passenger, carrier);
        assert_eq!(game.passengers(carrier), &[passenger]);
        game.ai_busy_from_ai(carrier);
        game.ai_busy_from_ai(passenger);
        game.ai_idle_from_ai(carrier);
        for id in [carrier, passenger] {
            assert!(game.entities[id].ai_idle());
            assert!(game.entities[id].last_command_from_ai());
        }
    }

    #[test]
    fn ai_idle_rejects_projectiles_and_sleeping_computer_orders() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        game.move_to(actor, [500.0, 150.0]).unwrap();
        let Order::Move(accepted) = game.entities[actor].ai.order else {
            panic!("Player move must enter native Move");
        };
        let goal = game.entities[actor].ai_goal_position();
        let revision = game.entities[actor].ai.command_revision;
        let kind = game.entities[actor].type_index;
        game.catalog[kind].kind_of.push("PROJECTILE".into());
        game.ai_idle_from_ai(actor);
        assert!(matches!(game.entities[actor].ai.order, Order::Move(_)));
        assert_eq!(
            game.entities[actor].ai.command.last_command_source,
            COMMAND_FROM_PLAYER
        );
        game.catalog[kind]
            .kind_of
            .retain(|kind| kind != "PROJECTILE");
        game.slots[1].difficulty = Some(crate::skirmish::OpponentDifficulty::Easy);
        game.entities[actor].owner = 1;
        game.entities[actor].ai.mood.attitude = -2;
        assert!(game.computer_player(1));
        game.ai_idle_from_ai(actor);
        game.construction_move(actor, [600.0, 150.0, 40.0]);
        let Order::Move(retained) = game.entities[actor].ai.order else {
            panic!("Sleeping AI must retain the accepted player move");
        };
        assert_eq!(retained.map(f32::to_bits), accepted.map(f32::to_bits));
        assert_eq!(
            game.entities[actor].ai_goal_position().map(f32::to_bits),
            goal.map(f32::to_bits)
        );
        assert_eq!(game.entities[actor].ai.command_revision, revision);
        assert_eq!(
            game.entities[actor].ai.command.last_command_source,
            COMMAND_FROM_PLAYER
        );
    }
}
