//! Native `FireWeaponUpdate`, `ActiveBody` subdual and `SubdualDamageHelper`.
use super::{Entity, Order, Skirmish};

pub struct SubdualRules {
    pub cap: f32,
    pub heal_rate: u32,
    pub heal_amount: f32,
}

pub(super) struct State {
    amount: f32,
    subdued: bool,
    pub(in crate::skirmish) heal_countdown: u32,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            amount: 0.0,
            subdued: false,
            heal_countdown: 0,
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn subdued(&self) -> bool {
        self.maintenance.microwave.subdued
    }
    #[must_use]
    pub const fn subdual_damage(&self) -> f32 {
        self.maintenance.microwave.amount
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn apply_subdual_damage(&mut self, id: usize, amount: f32) {
        let unit = &self.catalog[self.entities[id].type_index];
        if !unit.body_kind.is_active()
            || self.entities[id].indestructible()
            || self.entities[id].is_effectively_dead()
        {
            return;
        }
        let Some(rules) = &self.catalog[self.entities[id].type_index]
            .maintenance
            .subdual
        else {
            return;
        };
        let projectile = unit.kind_of.iter().any(|kind| kind == "PROJECTILE");
        let actor = &mut self.entities[id];
        // ActiveBody::isSubdued uses retained DISABLED_SUBDUED except for projectiles.
        let old = if projectile {
            actor.max_health <= actor.maintenance.microwave.amount
        } else {
            actor.maintenance.microwave.subdued
        };
        actor.maintenance.microwave.amount =
            (actor.maintenance.microwave.amount + amount).clamp(0.0, rules.cap);
        let subdued = actor.max_health <= actor.maintenance.microwave.amount;
        let rate = rules.heal_rate;
        let changed = old != subdued;
        let internet = self.catalog[actor.type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "FS_INTERNET_CENTER");
        if changed && projectile {
            if subdued {
                self.jam_projectile_object(id);
            }
        } else if changed {
            self.entities[id].maintenance.microwave.subdued = subdued;
            self.observe_spy_vision_disabled(id);
            for rider in self.passengers(id).to_vec() {
                if subdued {
                    self.set_order(rider, Order::Idle);
                } else if internet {
                    self.start_contained_hacking(rider);
                }
            }
            self.update_power();
            self.observe_spy_vision_disabled(id);
        }
        if amount > 0.0 && self.has_builtin_helper(id, super::module_updates::Helper::SubdualDamage)
        {
            self.entities[id].maintenance.microwave.heal_countdown = rate;
            self.wake_helper(id, super::module_updates::Helper::SubdualDamage, 1);
        }
        if self.entities[id].has_drawable() {
            self.entities[id].helpers.gaining_subdual = amount > 0.0;
        }
    }

    pub(in crate::skirmish) fn update_subdual_damage_helper(&mut self, id: usize) -> i32 {
        let state = &mut self.entities[id].maintenance.microwave;
        state.heal_countdown = state.heal_countdown.wrapping_sub(1);
        if state.heal_countdown > 0 {
            return 1;
        }
        let rules = self.catalog[self.entities[id].type_index]
            .maintenance
            .subdual
            .as_ref();
        // ActiveBodyModuleData's actual default heal rate/amount are both zero.
        let (rate, amount) = rules.map_or((0, 0.0), |rules| (rules.heal_rate, rules.heal_amount));
        self.entities[id].maintenance.microwave.heal_countdown = rate;
        self.apply_subdual_damage(id, -amount);
        if self.entities[id].maintenance.microwave.amount > 0.0 {
            1
        } else {
            0x3fff_ffff
        }
    }
}

pub(super) fn validate(rules: Option<&SubdualRules>) -> Result<(), &'static str> {
    if rules.is_some_and(|rules| {
        !rules.cap.is_finite()
            || rules.cap <= 0.0
            || !rules.heal_amount.is_finite()
            || rules.heal_amount < 0.0
    }) {
        return Err("Invalid subdual body rules");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
