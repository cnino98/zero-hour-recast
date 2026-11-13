//! `DemoTrapUpdate`, SH 2bfaea60: authored mode selection and proximity scans.
use super::{Skirmish, WeaponBinding, WeaponRules, movement::distance_squared, weapons};

#[cfg(test)]
mod tests;

pub struct Rules {
    /// Detonation, proximity and manual slots, in native field order.
    pub slots: [usize; 3],
    pub proximity_default: bool,
    pub friendly_detonation: bool,
    pub detonate_when_killed: bool,
    pub range: f32,
    pub scan_frames: u32,
    pub ignore_kind: Vec<String>,
    pub detonation_weapon: Option<WeaponRules>,
}

pub(super) struct State {
    detonated: bool,
    scan_frames: i32,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_demo_trap(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.demo_trap.len(), ordinal);
        self.entities[id].behaviors.demo_trap.push(State {
            detonated: false,
            scan_frames: 0,
        });
    }

    pub(in crate::skirmish) fn created_demo_trap(&mut self, id: usize, ordinal: usize) {
        self.entities[id].weapon_set.forced_veteran = true;
        self.replace_weapon_set(id);
        let rules = &self.catalog[self.entities[id].type_index].demo_trap[ordinal];
        let slot = rules.slots[if rules.proximity_default { 1 } else { 2 }];
        if self.allocated_weapon(id, slot).is_some()
            && !matches!(self.entities[id].weapon_set.lock, weapons::Lock::Permanent)
        {
            self.entities[id].weapon_set.current = slot;
            self.entities[id].weapon_set.lock = weapons::Lock::Temporary;
        }
    }

    pub(in crate::skirmish) fn update_demo_trap(&mut self, id: usize, ordinal: usize) -> i32 {
        let actor = &self.entities[id];
        let state = &actor.behaviors.demo_trap[ordinal];
        if state.detonated || actor.under_construction() || actor.selling() {
            return 1;
        }
        let rules = &self.catalog[actor.type_index].demo_trap[ordinal];
        if !actor.alive() {
            if rules.detonate_when_killed {
                self.detonate_trap(id, ordinal);
            }
            return 1;
        }
        let slot = actor.weapon_slot();
        if slot == rules.slots[0] {
            self.detonate_trap(id, ordinal);
            return 1;
        }
        let state = &mut self.entities[id].behaviors.demo_trap[ordinal];
        if state.scan_frames > 0 {
            state.scan_frames -= 1;
            return 1;
        }
        if slot == rules.slots[2] {
            return 1;
        }
        state.scan_frames = i32::from_ne_bytes(rules.scan_frames.to_ne_bytes());
        if self.trap_enemy_in_range(id, ordinal) {
            self.detonate_trap(id, ordinal);
        }
        1
    }

    fn trap_enemy_in_range(&self, id: usize, ordinal: usize) -> bool {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].demo_trap[ordinal];
        let mut detonate = false;
        // iterateObjectsInRange(pos) admits strict center distance, prepends each
        // accepted object and retains that order for ITER_FASTEST.
        for other in self
            .partition
            .fastest_range_objects(actor.position, rules.range)
        {
            let target = &self.entities[other];
            if distance_squared(actor.position, target.position)
                .partial_cmp(&(rules.range * rules.range))
                != Some(std::cmp::Ordering::Less)
            {
                continue;
            }
            let unit = &self.catalog[target.type_index];
            if rules
                .ignore_kind
                .iter()
                .any(|kind| unit.kind_of.contains(kind))
                || !target.alive()
            {
                continue;
            }
            if unit.kind_of.iter().any(|kind| kind == "DOZER")
                && self
                    .allocated_weapon(other, target.weapon_slot())
                    .is_some_and(|weapon| matches!(weapon.damage_type, super::DamageType::Disarm))
                && target.attacking()
            {
                continue;
            }
            if !self.enemies(actor.owner, target.owner) {
                if !rules.friendly_detonation {
                    return false;
                }
                continue;
            }
            if self.height_above_ground(other) > 0.0 {
                continue;
            }
            if distance_squared(actor.position, target.position) <= rules.range * rules.range {
                detonate = true;
                if rules.friendly_detonation {
                    break;
                }
            }
        }
        detonate
    }

    fn detonate_trap(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if !actor.under_construction()
            && !actor.selling()
            && unit.demo_trap[ordinal].detonation_weapon.is_some()
        {
            let binding = WeaponBinding {
                type_index: actor.type_index,
                weapon_index: unit.demo_weapon_start() + ordinal,
                weapon_slot: 0,
            };
            self.fire_temporary_weapon_binding_at(id, binding, self.object_position(id));
        }
        self.kill_object(id, super::DeathType::Normal);
        self.entities[id].behaviors.demo_trap[ordinal].detonated = true;
    }
}
