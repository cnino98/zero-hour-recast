//! `OverchargeBehavior`: source power influence, self penalty and exhaustion callback.
use crate::skirmish::{DamageType, DeathType, Entity, Skirmish, module_updates::FOREVER};

pub struct Rules {
    pub drain: f32,
    pub threshold: f32,
    pub energy_bonus: i32,
}

pub(super) struct State {
    active: bool,
}

pub struct Event {
    pub owner: usize,
    pub frame: u64,
    pub position: [f32; 3],
}

impl Entity {
    #[must_use]
    pub fn overcharge_active(&self) -> bool {
        self.maintenance
            .overcharges
            .iter()
            .any(|state| state.active)
    }

    pub(in crate::skirmish) fn first_overcharge_active(&self) -> bool {
        self.maintenance
            .overcharges
            .first()
            .is_some_and(|state| state.active)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_overcharge(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].maintenance.overcharges.len(), ordinal);
        self.entities[id]
            .maintenance
            .overcharges
            .push(State { active: false });
    }

    /// `AIGroup::groupToggleOvercharge` visits every authored interface.
    /// # Errors
    /// Requires an existing controlled Object with the actual behavior.
    pub fn toggle_overcharge(&mut self, id: usize) -> Result<(), &'static str> {
        let actor = self
            .entities
            .get(id)
            .filter(|actor| actor.exists())
            .ok_or("Missing overcharge Object")?;
        if actor.maintenance.overcharges.is_empty() {
            return Err("Object has no OverchargeBehavior");
        }
        for ordinal in 0..self.entities[id].maintenance.overcharges.len() {
            let enabled = !self.entities[id].maintenance.overcharges[ordinal].active;
            self.enable_overcharge_module(id, ordinal, enabled);
        }
        Ok(())
    }

    pub(in crate::skirmish) fn enable_overcharge_module(
        &mut self,
        id: usize,
        ordinal: usize,
        enabled: bool,
    ) {
        if self.entities[id].maintenance.overcharges[ordinal].active == enabled {
            return;
        }
        self.extend_all_power_rods(id, enabled);
        let actor = &self.entities[id];
        let owner = actor.owner;
        let bonus = self.catalog[actor.type_index].maintenance.overcharges[ordinal].energy_bonus;
        if enabled {
            self.add_native_power_bonus(id, owner, bonus);
        } else {
            self.remove_native_power_bonus(id, owner, bonus);
        }
        self.entities[id].maintenance.overcharges[ordinal].active = enabled;
        self.wake_family_update(
            id,
            "OverchargeBehavior",
            ordinal,
            if enabled { 1 } else { FOREVER },
        );
    }

    pub(in crate::skirmish) fn update_overcharge(&mut self, id: usize, ordinal: usize) -> i32 {
        if self.entities[id].maintenance.overcharges[ordinal].active {
            let actor = &self.entities[id];
            let rules = &self.catalog[actor.type_index].maintenance.overcharges[ordinal];
            let amount = (actor.max_health * rules.drain) / 30.0;
            let threshold = rules.threshold;
            self.apply_weapon_hit(id, id, amount, DamageType::Penalty, DeathType::Normal);
            let actor = &self.entities[id];
            if actor.health < actor.max_health * threshold {
                self.enable_overcharge_module(id, ordinal, false);
                self.presentation.overcharge_events.push(Event {
                    owner: self.entities[id].owner,
                    frame: self.tick,
                    position: self.object_position(id),
                });
            }
        }
        1
    }

    pub(in crate::skirmish) fn on_capture_overcharge(
        &mut self,
        id: usize,
        ordinal: usize,
        old: usize,
        new: usize,
    ) {
        if !self.entities[id].maintenance.overcharges[ordinal].active || self.object_disabled(id) {
            return;
        }
        let actor = &self.entities[id];
        let bonus = self.catalog[actor.type_index].maintenance.overcharges[ordinal].energy_bonus;
        self.remove_native_power_bonus(id, old, bonus);
        self.add_native_power_bonus(id, new, bonus);
    }

    pub(in crate::skirmish) fn on_delete_overcharge(&mut self, id: usize, ordinal: usize) {
        if !self.entities[id].maintenance.overcharges[ordinal].active {
            return;
        }
        let actor = &self.entities[id];
        let owner = actor.owner;
        let bonus = self.catalog[actor.type_index].maintenance.overcharges[ordinal].energy_bonus;
        self.remove_native_power_bonus(id, owner, bonus);
        self.entities[id].maintenance.overcharges[ordinal].active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Faction, Kind, tests};

    #[test]
    fn actual_toggle_drains_penalty_then_strict_threshold_disables_and_retracts() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::China, Kind::Defense);
        unit.max_health = 1000.0;
        unit.initial_health = 1000.0;
        unit.energy = 10;
        unit.maintenance.power_plants = vec![1];
        unit.maintenance.overcharges.push(Rules {
            drain: 3.0,
            threshold: 0.9,
            energy_bonus: 10,
        });
        tests::register_update(&mut unit, "PowerPlantUpdate");
        tests::register_update(&mut unit, "OverchargeBehavior");
        unit.behavior_modules.last_mut().unwrap().interfaces = 5;
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [300.0, 200.0], true);
        game.update_power_excluding(None, None);
        let power = game.players[0].power;
        game.toggle_overcharge(id).unwrap();
        assert_eq!(game.players[0].power, power + 10);
        assert!(game.entities[id].overcharge_active());
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(game.entities[id].health.to_bits(), 900.0_f32.to_bits());
        assert!(game.entities[id].overcharge_active());
        assert!(game.presentation.overcharge_events.is_empty());
        game.tick = 2;
        game.run_object_updates();
        assert_eq!(game.entities[id].health.to_bits(), 800.0_f32.to_bits());
        assert!(!game.entities[id].overcharge_active());
        assert_eq!(game.players[0].power, power);
        assert!(game.entities[id].power_plant_conditions().next().is_none());
        assert_eq!(game.presentation.overcharge_events.len(), 1);
        assert_eq!(game.presentation.overcharge_events[0].owner, 0);
        game.tick = 3;
        game.run_object_updates();
        assert_eq!(game.entities[id].health.to_bits(), 800.0_f32.to_bits());
    }

    #[test]
    fn real_subdual_capture_and_disabled_delete_preserve_power_influence() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::China, Kind::Defense);
        unit.max_health = 1000.0;
        unit.initial_health = 1000.0;
        unit.energy = 10;
        unit.maintenance.subdual = Some(crate::skirmish::microwave::SubdualRules {
            cap: 1000.0,
            heal_rate: 2,
            heal_amount: 1000.0,
        });
        unit.maintenance.overcharges.push(Rules {
            drain: 0.03,
            threshold: 0.5,
            energy_bonus: 10,
        });
        tests::register_update(&mut unit, "OverchargeBehavior");
        unit.behavior_modules.last_mut().unwrap().interfaces = 5;
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [300.0, 200.0], true);
        let enemy = game.spawn(6, 1, [350.0, 200.0], true);
        game.update_power_excluding(None, None);
        let base0 = game.players[0].power - 10;
        let base1 = game.players[1].power;
        game.toggle_overcharge(id).unwrap();
        assert_eq!(game.players[0].power, base0 + 20);
        game.apply_weapon_hit(
            id,
            enemy,
            1000.0,
            DamageType::SubdualBuilding,
            DeathType::Normal,
        );
        assert!(game.object_disabled(id));
        assert_eq!(game.players[0].power, base0);
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(game.entities[id].health.to_bits(), 1000.0_f32.to_bits());
        game.tick = 2;
        game.run_object_updates();
        assert!(!game.object_disabled(id));
        assert_eq!(game.players[0].power, base0 + 20);
        game.tick = 3;
        game.run_object_updates();
        assert!(game.entities[id].health < 1000.0);
        game.transfer_maintenance_owner(id, 1);
        assert_eq!(game.players[0].power, base0);
        assert_eq!(game.players[1].power, base1 + 20);
        game.apply_weapon_hit(
            id,
            enemy,
            1000.0,
            DamageType::SubdualBuilding,
            DeathType::Normal,
        );
        assert!(game.object_disabled(id));
        assert_eq!(game.players[1].power, base1);
        game.destroy_maintenance_object(id);
        game.process_update_destruction();
        assert_eq!(game.players[1].power, base1);
        assert!(!game.entities[id].exists());
    }
}
