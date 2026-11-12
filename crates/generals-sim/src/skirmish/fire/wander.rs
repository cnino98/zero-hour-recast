//! Native `WanderAIUpdate` requests an ordinary AI move only while idle.
use crate::skirmish::{Skirmish, UnitAiUpdate};
use num_traits::ToPrimitive;

impl Skirmish {
    pub(in crate::skirmish) fn update_wander_ai_module(&mut self, id: usize) -> i32 {
        self.update_wander(id);
        self.update_ai_module(id);
        1
    }

    pub(in crate::skirmish) fn update_wander(&mut self, id: usize) {
        if !matches!(
            self.catalog[self.entities[id].type_index].ai.update,
            UnitAiUpdate::Wander
        ) || !self.entities[id].ai_idle()
        {
            return;
        }
        let mut destination = self.object_position(id);
        for coordinate in &mut destination[..2] {
            *coordinate += (self.random_between(45) + 5)
                .to_f32()
                .expect("wander offset");
        }
        self.construction_move(id, destination);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::Order;

    #[test]
    fn idle_wander_consumes_x_then_y_integer_draws_and_keeps_active_move() {
        let mut game = crate::skirmish::tests::game();
        game.catalog[5].ai.update = UnitAiUpdate::Wander;
        let module = game.catalog[5]
            .behavior_modules
            .iter_mut()
            .find(|module| module.family == "AIUpdateInterface")
            .unwrap();
        module.family = "WanderAIUpdate".into();
        let module = game.catalog[5]
            .update_modules
            .iter_mut()
            .find_map(|module| {
                if let crate::skirmish::module_updates::ModuleRegistration::Behavior {
                    family,
                    ..
                } = module
                {
                    (family == "AIUpdateInterface").then_some(family)
                } else {
                    None
                }
            })
            .unwrap();
        *module = "WanderAIUpdate".into();
        let id = game.spawn(5, 0, [200.0, 200.0], true);
        let mut expected = game.random;
        let x = crate::skirmish::startup::random_between(&mut expected, 45) + 5;
        let y = crate::skirmish::startup::random_between(&mut expected, 45) + 5;
        let requested = [200.0 + x.to_f32().unwrap(), 200.0 + y.to_f32().unwrap()];
        game.refresh_occupancy();
        let admitted = game.adjusted_destination(id, requested).unwrap();
        game.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
        game.update_wander(id);
        assert!(game.entities[id].last_command_from_ai());
        assert_eq!(game.random, expected);
        let Order::Move(destination) = game.entities[id].ai.order else {
            panic!("native idle wander must install a move");
        };
        assert_eq!(destination.map(f32::to_bits), admitted.map(f32::to_bits));
        game.update_wander(id);
        assert!(game.entities[id].last_command_from_ai());
        assert_eq!(game.random, expected);
        assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
        game.set_ai_order(id, Order::Idle);
        crate::skirmish::startup::random_between(&mut expected, 60);
        assert_eq!(
            game.random, expected,
            "native Idle onEnter consumes its own sleep draw"
        );
        game.ai_busy_from_ai(id);
        game.update_wander(id);
        assert_eq!(game.random, expected);
        assert!(game.entities[id].ai_busy());
    }
}

#[cfg(test)]
mod callback_tests {
    use crate::skirmish::module_updates::ModuleRegistration;
    use crate::skirmish::{Order, UnitAiUpdate, tests};

    #[test]
    fn authored_wander_callback_precedes_base_and_stays_awake_while_moving() {
        let mut game = tests::game();
        let unit = &mut game.catalog[5];
        unit.ai.update = UnitAiUpdate::Wander;
        unit.behavior_modules
            .iter_mut()
            .find(|module| module.family == "AIUpdateInterface")
            .unwrap()
            .family = "WanderAIUpdate".into();
        for module in &mut unit.update_modules {
            if let ModuleRegistration::Behavior { family, .. } = module
                && family == "AIUpdateInterface"
            {
                *family = "WanderAIUpdate".into();
            }
        }
        let id = game.spawn(5, 0, [200.0, 200.0], true);
        game.tick = 1;
        game.run_object_updates();
        assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
        assert!(game.entities[id].last_command_from_ai());
        let registration = game.entities[id].ai.registration.unwrap();
        assert_eq!(game.update_remaining_sleep(id, registration), 1);
    }
}
