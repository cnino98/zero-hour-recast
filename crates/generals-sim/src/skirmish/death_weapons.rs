//! `FireWeaponWhenDeadBehavior::onDie` and the temporary PRIMARY weapon callback.
use super::{Skirmish, WeaponBinding, WeaponRules, damage::WeaponTarget, firing::WeaponState};
#[cfg(test)]
mod projectile_tests;

#[cfg(test)]
mod tests;

pub struct Rules {
    pub weapon: Option<WeaponRules>,
    pub veterancy_mask: u8,
    pub exempt_status: u64,
    pub required_status: u64,
    pub removals: Vec<String>,
    pub upgrade_fx: String,
    pub death_mask: u32,
    pub starts_active: bool,
    pub triggers: Vec<String>,
    pub conflicts: Vec<String>,
    pub requires_all: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_death_weapon(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.death_weapons.len(), ordinal);
        self.entities[id].behaviors.death_weapons.push(false);
        if self.catalog[self.entities[id].type_index]
            .behavior
            .death_weapons[ordinal]
            .starts_active
        {
            self.activate_death_weapon(id, ordinal);
        }
    }

    fn activate_death_weapon(&mut self, id: usize, ordinal: usize) {
        self.give_self_upgrade_instance(id, "FireWeaponWhenDeadBehavior", ordinal);
    }

    pub(in crate::skirmish) fn apply_death_weapon_upgrade(&mut self, id: usize, ordinal: usize) {
        self.entities[id].behaviors.death_weapons[ordinal] = true;
    }

    pub(in crate::skirmish) fn death_weapon_on_die(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = &unit.behavior.death_weapons[ordinal];
        if !actor.behaviors.death_weapons[ordinal]
            || !self.death_conditions_match(
                id,
                rules.death_mask,
                rules.veterancy_mask,
                rules.exempt_status,
                rules.required_status,
            )
            || actor.under_construction()
            || rules.conflicts.iter().any(|name| {
                self.object_has_upgrade(id, name) || self.has_upgrade(actor.owner, name)
            })
            || rules.weapon.is_none()
        {
            return;
        }
        self.fire_temporary_weapon(id, unit.death_weapon_start() + ordinal);
    }

    pub(super) fn fire_temporary_weapon(&mut self, id: usize, index: usize) {
        self.fire_temporary_weapon_at(id, index, self.object_position(id));
    }

    pub(super) fn fire_temporary_weapon_at(&mut self, id: usize, index: usize, position: [f32; 3]) {
        let target = WeaponTarget {
            object: None,
            position,
            projectile_position: position,
            scatter_radius: 0.0,
        };
        let binding = WeaponBinding {
            type_index: self.entities[id].type_index,
            weapon_index: index,
            weapon_slot: 0,
        };
        self.fire_temporary_weapon_payload(id, binding, target);
    }

    pub(in crate::skirmish) fn fire_temporary_weapon_binding_at(
        &mut self,
        id: usize,
        binding: WeaponBinding,
        position: [f32; 3],
    ) {
        let target = WeaponTarget {
            object: None,
            position,
            projectile_position: position,
            scatter_radius: 0.0,
        };
        self.fire_temporary_weapon_payload(id, binding, target);
    }

    pub(in crate::skirmish) fn fire_temporary_weapon_target(
        &mut self,
        id: usize,
        binding: WeaponBinding,
        target: usize,
    ) {
        let position = self.object_position(target);
        let target = WeaponTarget {
            object: Some(target),
            position,
            projectile_position: position,
            scatter_radius: 0.0,
        };
        self.fire_temporary_weapon_payload(id, binding, target);
    }

    fn fire_temporary_weapon_payload(
        &mut self,
        id: usize,
        binding: WeaponBinding,
        mut target: WeaponTarget,
    ) {
        let unit = &self.catalog[binding.type_index];
        let weapon = unit
            .weapon_template(binding.weapon_index)
            .expect("temporary Weapon template");
        let now = self.update_frame();
        let mut instance = WeaponState::new(weapon.clip_size);
        instance.clip_remaining = 0;
        instance.max_shots = Some(0x7fff_ffff);
        let shot_rate = self.entities[id].weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        let shared = self.catalog[self.entities[id].type_index]
            .weapon_set_rules
            .share_reload;
        instance.reload_owned(weapon, now, shot_rate, true, shared);
        let propagation = shared.then_some((instance.status, now));
        if let Some((status, deadline)) = propagation {
            self.share_owned_weapon_deadline(id, status, deadline);
        }
        self.request_owned_weapon_assistance(id, binding, target.object);
        instance.leech_range_active |= self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("temporary Weapon template")
            .leech_range;
        let template_branch = self.fire_owned_weapon_prelude(id, binding, target.object);
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("temporary Weapon template");
        let shot_rate = self.entities[id].weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        let barrel = if template_branch {
            instance.owned_barrel(
                weapon,
                self.entities[id].health,
                self.entities[id].max_health,
            )
        } else {
            0
        };
        let pattern = if template_branch {
            instance.owned_scatter(weapon, &mut self.random)
        } else {
            None
        };
        if let Some(offset) = pattern {
            target.position[0] += offset[0];
            target.position[1] += offset[1];
            target.position[2] = self.ground_height([target.position[0], target.position[1]]);
            target.projectile_position = target.position;
            target.object = None;
        }
        if template_branch {
            self.fire_owned_weapon_payload(id, binding, barrel, target, false);
        }
        let unit = &self.catalog[binding.type_index];
        let weapon = unit
            .weapon_template(binding.weapon_index)
            .expect("temporary Weapon template");
        let reload_rate = self.entities[id].weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        if let Some((status, deadline)) = instance
            .finish_owned_shot(
                weapon,
                now,
                shot_rate,
                reload_rate,
                shared,
                &mut self.random,
            )
            .shared
        {
            self.share_owned_weapon_deadline(id, status, deadline);
        }
    }
}
