//! SH `SupplyWarehouseCripplingBehavior.cpp`: per-instance suppression and healing.
use super::{Skirmish, module_updates::FOREVER};

pub struct Rules {
    pub suppression: u32,
    pub delay: u32,
    pub amount: f32,
}

pub(super) struct State {
    suppressed_until: u32,
    next_healing: u32,
}

impl Skirmish {
    pub(super) fn construct_supply_crippling(&mut self, id: usize, ordinal: usize) {
        let instances = &mut self.entities[id].behaviors.supply_crippling;
        assert_eq!(instances.len(), ordinal);
        instances.push(State {
            suppressed_until: 0,
            next_healing: 0,
        });
    }

    pub(super) fn supply_crippling_damaged(&mut self, id: usize, ordinal: usize) {
        let now = self.update_frame();
        let suppression = self.catalog[self.entities[id].type_index]
            .behavior
            .supply_crippling[ordinal]
            .suppression;
        let state = &mut self.entities[id].behaviors.supply_crippling[ordinal];
        state.suppressed_until = now.wrapping_add(suppression);
        state.next_healing = state.suppressed_until;
        let delay = state.suppressed_until.wrapping_sub(now);
        self.wake_family_update(id, "SupplyWarehouseCripplingBehavior", ordinal, delay);
    }

    pub(super) fn supply_crippling_body_state(&mut self, id: usize, old: usize, new: usize) {
        if new == 2 {
            self.native_dock_set_crippled(id, true);
        } else if old == 2 {
            self.native_dock_set_crippled(id, false);
        }
    }

    pub(super) fn update_supply_crippling(&mut self, id: usize, ordinal: usize) -> i32 {
        let now = self.update_frame();
        let rule = &self.catalog[self.entities[id].type_index]
            .behavior
            .supply_crippling[ordinal];
        let amount = rule.amount;
        self.entities[id].behaviors.supply_crippling[ordinal].next_healing =
            now.wrapping_add(rule.delay);
        self.apply_healing(id, None, amount);
        let actor = &self.entities[id];
        if actor.health.partial_cmp(&actor.max_health) == Some(std::cmp::Ordering::Equal) {
            i32::try_from(FOREVER).expect("native forever sleep")
        } else {
            // UPDATE_SLEEP casts the unsigned difference to its signed sleep type.
            i32::from_ne_bytes(
                actor.behaviors.supply_crippling[ordinal]
                    .next_healing
                    .wrapping_sub(now)
                    .to_ne_bytes(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rules;
    use crate::skirmish::{
        DeathType, Faction, Kind,
        module_updates::{FOREVER, ModuleRegistration},
        tests,
    };

    #[test]
    fn constructor_sleep_repeated_damage_suppression_and_healing_follow_heap_deadlines() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::Gla, Kind::TechBuilding);
        unit.initial_health = 100.0;
        unit.max_health = 100.0;
        unit.behavior.supply_crippling.push(Rules {
            suppression: 4,
            delay: 2,
            amount: 10.0,
        });
        tests::register_update(&mut unit, "SupplyWarehouseCripplingBehavior");
        let registration = unit.update_modules.iter().position(|module| matches!(module,
            ModuleRegistration::Behavior { family, .. } if family == "SupplyWarehouseCripplingBehavior")).unwrap();
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [300.0, 300.0], true);
        assert_eq!(
            game.update_remaining_sleep(id, registration) + game.update_frame(),
            FOREVER
        );
        game.apply_typed_hit(id, 0, 20.0, DeathType::Normal);
        game.tick = 2;
        game.apply_typed_hit(id, 0, 10.0, DeathType::Normal);
        assert_eq!(
            game.entities[id].behaviors.supply_crippling[0].suppressed_until,
            6
        );
        assert_eq!(
            game.entities[id].behaviors.supply_crippling[0].next_healing,
            6
        );
        for frame in 3..=10 {
            game.tick = frame;
            game.run_object_updates();
            let expected: f32 = match frame {
                3..=5 => 70.0,
                6..=7 => 80.0,
                8..=9 => 90.0,
                _ => 100.0,
            };
            assert_eq!(game.entities[id].health.to_bits(), expected.to_bits());
        }
        assert_eq!(
            game.update_remaining_sleep(id, registration) + game.update_frame(),
            FOREVER
        );
        assert_eq!(
            game.entities[id].behaviors.supply_crippling[0].next_healing,
            12
        );
    }

    #[test]
    fn disabled_held_defers_healing_and_zero_delay_still_returns_native_none() {
        let mut game = tests::game();
        let unit = &mut game.catalog[5];
        unit.behavior.supply_crippling.push(Rules {
            suppression: 0,
            delay: 0,
            amount: 1.0,
        });
        tests::register_update(unit, "SupplyWarehouseCripplingBehavior");
        let id = game.spawn(5, 0, [300.0, 300.0], true);
        game.apply_typed_hit(id, 0, 10.0, DeathType::Normal);
        game.set_disabled_held(id);
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(game.entities[id].health.to_bits(), 90.0_f32.to_bits());
        game.clear_disabled_held(id);
        game.tick = 2;
        game.run_object_updates();
        assert_eq!(game.entities[id].health.to_bits(), 91.0_f32.to_bits());
        game.run_object_updates();
        assert_eq!(game.entities[id].health.to_bits(), 91.0_f32.to_bits());
        game.tick = 3;
        game.run_object_updates();
        assert_eq!(game.entities[id].health.to_bits(), 92.0_f32.to_bits());
    }
}
