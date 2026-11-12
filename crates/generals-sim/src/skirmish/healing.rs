//! Ordered concrete `AutoHealBehavior` instances (SH 2bfaea60).
use super::{Skirmish, module_updates::FOREVER, movement::distance_squared};

#[cfg(test)]
mod tests;

pub struct AutoHeal {
    pub amount: f32,
    pub delay: u32,
    pub damage_delay: u32,
    pub starts_active: bool,
    pub triggers: Vec<String>,
    pub conflicts: Vec<String>,
    pub requires_all: bool,
    pub targets: AutoHealTargets,
    pub single_burst: bool,
}

pub struct AutoHealTargets {
    pub radius: f32,
    pub affects_whole_player: bool,
    pub kinds: Option<Vec<String>>,
    pub forbidden: Vec<String>,
    pub skip_self: bool,
}

pub(super) struct State {
    pub(super) active: bool,
    pub(super) soonest: u32,
    stopped: bool,
}

pub(super) fn validate(unit: &super::UnitType) -> Result<(), &'static str> {
    if unit.maintenance.auto_heal.iter().any(|heal| {
        !heal.amount.is_finite() || !heal.targets.radius.is_finite() || heal.targets.radius < 0.0
    }) {
        return Err("Invalid AutoHealBehavior source values");
    }
    Ok(())
}

impl Skirmish {
    pub(in crate::skirmish) fn stop_first_auto_heal(&mut self, id: usize) {
        if let Some(state) = self.entities[id].maintenance.auto_heal.first_mut() {
            state.stopped = true;
            state.soonest = FOREVER;
            self.wake_family_update(id, "AutoHealBehavior", 0, FOREVER);
        }
    }

    pub(super) fn construct_auto_heal(&mut self, id: usize, registration: usize) {
        let index = self.entities[id].maintenance.auto_heal.len();
        let rules = &self.catalog[self.entities[id].type_index]
            .maintenance
            .auto_heal[index];
        let active = rules.starts_active;
        let delay = rules.delay;
        self.entities[id].maintenance.auto_heal.push(State {
            active: false,
            soonest: 0,
            stopped: false,
        });
        if active {
            self.give_self_upgrade_instance(id, "AutoHealBehavior", index);
            let sleep = self.random_update_phase(delay);
            self.wake_update(id, registration, sleep);
        } else {
            self.wake_update(id, registration, FOREVER);
        }
    }

    pub(in crate::skirmish) fn apply_auto_heal_upgrade(&mut self, id: usize, ordinal: usize) {
        self.wake_family_update(id, "AutoHealBehavior", ordinal, 1);
        self.entities[id].maintenance.auto_heal[ordinal].active = true;
    }

    pub(super) fn auto_heal_module_damaged(&mut self, id: usize, index: usize) {
        let now = self.update_frame();
        let type_index = self.entities[id].type_index;
        let state = &self.entities[id].maintenance.auto_heal[index];
        let rules = &self.catalog[type_index].maintenance.auto_heal[index];
        if state.stopped || !state.active || rules.targets.radius != 0.0 {
            return;
        }
        if rules.damage_delay > 0 {
            self.wake_family_update(id, "AutoHealBehavior", index, rules.damage_delay);
        } else if now > state.soonest {
            self.wake_family_update(id, "AutoHealBehavior", index, 1);
        }
    }

    pub(super) fn update_auto_heal_module(&mut self, source: usize, index: usize) -> i32 {
        let actor = &self.entities[source];
        let state = &actor.maintenance.auto_heal[index];
        if state.stopped || !state.active || actor.is_effectively_dead() {
            return i32::from_ne_bytes(FOREVER.to_ne_bytes());
        }
        let rules = &self.catalog[actor.type_index].maintenance.auto_heal[index];
        let delay = rules.delay;
        let whole_player = rules.targets.affects_whole_player;
        let radius = rules.targets.radius;
        let single_burst = rules.single_burst;
        if !whole_player && radius == 0.0 {
            if actor.health >= actor.max_health {
                return i32::from_ne_bytes(FOREVER.to_ne_bytes());
            }
            self.pulse_auto_heal(source, source, index);
        } else {
            let candidates = if whole_player {
                self.team_membership.player_objects(actor.owner)
            } else {
                self.partition.fastest_range_objects(actor.position, radius)
            };
            for target in candidates {
                if self.auto_heal_target(source, target, index) {
                    self.pulse_auto_heal(source, target, index);
                }
            }
        }
        i32::from_ne_bytes(
            if !whole_player && radius != 0.0 && single_burst {
                FOREVER
            } else {
                delay
            }
            .to_ne_bytes(),
        )
    }

    fn auto_heal_target(&self, source: usize, target: usize, index: usize) -> bool {
        let healer = &self.entities[source];
        let actor = &self.entities[target];
        let rules = &self.catalog[healer.type_index].maintenance.auto_heal[index];
        let kinds = &self.catalog[actor.type_index].kind_of;
        if actor.is_effectively_dead()
            || actor.health >= actor.max_health
            || (rules.targets.skip_self && source == target)
            || rules
                .targets
                .kinds
                .as_ref()
                .map_or(kinds.is_empty(), |wanted| {
                    !wanted.iter().any(|kind| kinds.contains(kind))
                })
            || rules
                .targets
                .forbidden
                .iter()
                .any(|kind| kinds.contains(kind))
        {
            return false;
        }
        if rules.targets.affects_whole_player {
            actor.owner == healer.owner && self.cell_at(actor.position).is_some()
        } else {
            self.allied(actor.owner, healer.owner)
                && self.cell_at(actor.position).is_some() == self.cell_at(healer.position).is_some()
                && distance_squared(healer.position, actor.position)
                    < rules.targets.radius * rules.targets.radius
        }
    }

    fn pulse_auto_heal(&mut self, source: usize, target: usize, index: usize) {
        let now = self.update_frame();
        let rules = &self.catalog[self.entities[source].type_index]
            .maintenance
            .auto_heal[index];
        let deadline = now.wrapping_add(rules.delay);
        let amount = rules.amount;
        let actor = &mut self.entities[target];
        if rules.targets.radius == 0.0
            || u64::from(now) > actor.maintenance.healing_benefactor_until
            || actor.maintenance.healing_benefactor == Some(source)
        {
            if rules.targets.radius != 0.0 {
                actor.maintenance.healing_benefactor = Some(source);
                actor.maintenance.healing_benefactor_until = u64::from(deadline);
            }
            self.apply_healing(target, Some(source), amount);
        }
        // This records an attempted pulse even when another benefactor rejects its healing.
        self.entities[source].maintenance.auto_heal[index].soonest = deadline;
    }
}
