//! `BaseRegenerateUpdate`: damage restarts its delay; healing uses three-frame pulses.
use crate::skirmish::{Skirmish, UnitType};

pub struct Regeneration {
    pub percent_per_second: f32,
    pub delay: u32,
}

pub(super) fn validate(unit: &UnitType) -> Result<(), &'static str> {
    if unit.maintenance.regeneration.as_ref().is_some_and(|rules| {
        !rules.percent_per_second.is_finite() || rules.percent_per_second < 0.0
    }) {
        return Err("Invalid base regeneration percentage");
    }
    Ok(())
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_base_regeneration(
        &mut self,
        id: usize,
        registration: usize,
    ) {
        let rules = self.catalog[self.entities[id].type_index]
            .maintenance
            .regeneration
            .as_ref()
            .expect("bound BaseRegenerateUpdate data");
        let delay = if rules.percent_per_second == 0.0 {
            crate::skirmish::module_updates::FOREVER
        } else {
            1
        };
        self.wake_update(id, registration, delay);
    }

    pub(in crate::skirmish) fn regeneration_damaged(&mut self, id: usize) {
        let Some(rules) = &self.catalog[self.entities[id].type_index]
            .maintenance
            .regeneration
        else {
            return;
        };
        let delay = if rules.percent_per_second > 0.0 {
            rules.delay
        } else {
            crate::skirmish::module_updates::FOREVER
        };
        let count = self.catalog[self.entities[id].type_index].update_modules.iter().filter(|entry| {
            matches!(entry, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. }
                if family == "BaseRegenerateUpdate")
        }).count();
        for ordinal in 0..count {
            self.wake_family_update(id, "BaseRegenerateUpdate", ordinal, delay);
        }
    }

    pub(in crate::skirmish) fn update_base_regeneration(&mut self, id: usize) -> i32 {
        let actor = &self.entities[id];
        if actor.under_construction() {
            return 1;
        }
        if actor.selling()
            || actor.health.partial_cmp(&actor.max_health) == Some(std::cmp::Ordering::Equal)
        {
            return 0x3fff_ffff;
        }
        let rules = self.catalog[actor.type_index]
            .maintenance
            .regeneration
            .as_ref()
            .expect("registered BaseRegenerateUpdate data");
        let amount = 3.0 * (actor.max_health * rules.percent_per_second) / 30.0;
        self.apply_healing(id, Some(id), amount);
        3
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn absorbed_damage_does_not_restart_regeneration_or_change_retained_damage() {
        let mut game = crate::skirmish::tests::game();
        game.catalog[6].maintenance.regeneration = Some(crate::skirmish::Regeneration {
            percent_per_second: 0.1,
            delay: 9,
        });
        crate::skirmish::tests::register_update(&mut game.catalog[6], "BaseRegenerateUpdate");
        game.catalog[6].armor[crate::skirmish::DamageType::Explosion as usize] = 0.0;
        let target = game.spawn(6, 0, [300.0, 200.0], true);
        game.tick = 12;
        let registration = game.catalog[6].update_modules.len() - 1;
        game.wake_update(target, registration, 7);
        let health = game.entities[target].health;
        game.apply_weapon_hit(
            target,
            2,
            100.0,
            crate::skirmish::DamageType::Explosion,
            crate::skirmish::DeathType::Exploded,
        );
        assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
        assert_eq!(game.update_remaining_sleep(target, registration), 7);
        assert_eq!(game.entities[target].damage_state.retained_source, None);
        game.catalog[6].armor[crate::skirmish::DamageType::Explosion as usize] = 1.0;
        game.apply_weapon_hit(
            target,
            2,
            1.0,
            crate::skirmish::DamageType::Explosion,
            crate::skirmish::DeathType::Exploded,
        );
        assert_eq!(game.update_remaining_sleep(target, registration), 9);
        assert_eq!(game.entities[target].damage_state.retained_source, Some(2));
    }
}
