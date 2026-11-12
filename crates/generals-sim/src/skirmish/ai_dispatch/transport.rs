//! SH TransportAIUpdate.cpp private object/force/position attack overrides.
use crate::skirmish::{
    Skirmish,
    ai_goals::{COMMAND_FROM_PLAYER, COMMAND_FROM_SCRIPT},
    combat::attack::Goal,
};

impl Skirmish {
    pub(in crate::skirmish) fn transport_ai(&self, id: usize) -> bool {
        self.entities[id].ai.registration.is_some()
            && self.catalog[self.entities[id].type_index]
                .behavior_modules
                .iter()
                .any(|module| module.family == "TransportAIUpdate")
    }

    pub(super) fn transport_attack_passengers(
        &mut self,
        id: usize,
        goal: Goal,
        max_shots: u32,
        source: i32,
        forced: bool,
    ) {
        if !self.transport_ai(id)
            || !matches!(source, COMMAND_FROM_PLAYER | COMMAND_FROM_SCRIPT)
            || !self.container_fire_allowed(id, None)
        {
            return;
        }
        // The virtual list includes Overlord portable structures. Each passenger's
        // command enters its state before the parent's base privateAttack call.
        let passengers = self.passengers(id).to_vec();
        for passenger in passengers {
            if self.attack_kind(passenger, "PORTABLE_STRUCTURE")
                && self.object_disabled_mask(passenger)
                    & ((1 << 1) | (1 << 2) | (1 << 4) | (1 << 5))
                    != 0
            {
                continue;
            }
            if self.entities[passenger].ai.registration.is_some() {
                self.native_attack_goal_from(passenger, goal, max_shots, source, forced);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{
        Order,
        ai_goals::{COMMAND_FROM_AI, COMMAND_FROM_PLAYER, COMMAND_FROM_SCRIPT},
        combat::attack::Goal,
    };

    fn transport() -> (crate::skirmish::Skirmish, usize, usize, usize) {
        let (mut game, old) = crate::skirmish::containment::tests::setup(0.0, 1.0);
        let kind = game.entities[old].type_index;
        for module in &mut game.catalog[kind].behavior_modules {
            if module.family == "AIUpdateInterface" {
                module.family = "TransportAIUpdate".into();
            }
        }
        for module in &mut game.catalog[kind].update_modules {
            if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } =
                module
                && family == "AIUpdateInterface"
            {
                *family = "TransportAIUpdate".into();
            }
        }
        let carrier = game.spawn(kind, 0, [355.0, 255.0], true);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.add_to_contain(rider, carrier);
        let target = game.spawn(6, 1, [410.0, 255.0], true);
        (game, carrier, rider, target)
    }

    #[test]
    fn direct_transport_commands_keep_source_shot_limit_and_full_position_in_passenger_machine() {
        let (mut game, carrier, rider, target) = transport();
        game.native_attack_goal_from(carrier, Goal::Object(target), 7, COMMAND_FROM_SCRIPT, true);
        assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
        assert_eq!(
            game.entities[rider].ai.command.last_command_source,
            COMMAND_FROM_SCRIPT
        );
        assert!(game.entities[rider].ai.attack_context.forced);
        let slot = game.entities[rider].weapon_slot();
        assert_eq!(
            game.entities[rider].weapon_set.weapons[slot].max_shots,
            Some(7)
        );
        let revision = game.entities[rider].ai.command_revision;
        game.native_attack_goal_from(carrier, Goal::Object(target), 3, COMMAND_FROM_AI, false);
        assert_eq!(game.entities[rider].ai.command_revision, revision);
        let point = [405.0, 255.0, 25.0];
        game.native_attack_goal_from(
            carrier,
            Goal::Position(point),
            2,
            COMMAND_FROM_PLAYER,
            false,
        );
        assert!(matches!(
            game.entities[rider].ai.order,
            Order::FireAt { .. }
        ));
        assert_eq!(
            game.entities[rider].ai_goal_position().map(f32::to_bits),
            point.map(f32::to_bits)
        );
        assert_eq!(
            game.entities[rider].weapon_set.weapons[slot].max_shots,
            Some(2)
        );
        game.update_contained(rider);
        assert!(matches!(
            game.entities[rider].ai.order,
            Order::FireAt { .. }
        ));
        assert!(game.entities[rider].ai.attack.is_some());
    }

    #[test]
    fn disabled_portable_structure_is_not_ordered_but_disabled_infantry_receives_the_command() {
        let (mut game, carrier, rider, target) = transport();
        game.entities[rider].disabled_status |= 1 << 2;
        let revision = game.entities[rider].ai.command_revision;
        game.native_attack_goal_from(carrier, Goal::Object(target), 4, COMMAND_FROM_PLAYER, false);
        assert!(game.entities[rider].ai.command_revision > revision);
        let kind = game.entities[rider].type_index;
        game.catalog[kind].kind_of.push("PORTABLE_STRUCTURE".into());
        let revision = game.entities[rider].ai.command_revision;
        game.native_attack_goal_from(carrier, Goal::Object(target), 9, COMMAND_FROM_PLAYER, true);
        assert_eq!(game.entities[rider].ai.command_revision, revision);
        game.entities[rider].disabled_status &= !(1 << 2);
        game.native_attack_goal_from(carrier, Goal::Object(target), 9, COMMAND_FROM_PLAYER, true);
        assert!(game.entities[rider].ai.command_revision > revision);
    }
}
