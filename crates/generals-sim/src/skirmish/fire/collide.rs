//! `FireWeaponCollide.cpp`: one persistent PRIMARY weapon per authored COLLIDE interface.
use crate::skirmish::{
    Skirmish, WeaponBinding, WeaponRules, ai_dispatch::WeaponStatus, firing::WeaponState,
};

#[cfg(test)]
mod tests;

pub struct Rules {
    pub weapon: WeaponRules,
    pub required: u64,
    pub forbidden: u64,
    pub fire_once: bool,
}

pub(in crate::skirmish) struct State {
    weapon: WeaponState,
    ever_fired: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_collision_weapon(&mut self, id: usize, ordinal: usize) {
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .collision_weapons[ordinal];
        // allocateNewWeapon only constructs; onCollide performs the first reload.
        let mut weapon = WeaponState::new(0);
        weapon.status = WeaponStatus::OutOfAmmo;
        weapon.clip_remaining = 0;
        weapon.max_shots = Some(0x7fff_ffff);
        weapon.barrel_shots = rules
            .weapon
            .motion
            .as_ref()
            .map_or(1, |motion| motion.shots_per_barrel);
        assert_eq!(self.entities[id].behaviors.collision_weapons.len(), ordinal);
        self.entities[id].behaviors.collision_weapons.push(State {
            weapon,
            ever_fired: false,
        });
    }

    pub(in crate::skirmish) fn fire_weapon_contact(
        &mut self,
        id: usize,
        ordinal: usize,
        other: usize,
    ) {
        let status = self.object_status_bits(id);
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = &unit.behavior.collision_weapons[ordinal];
        if status & rules.required != rules.required
            || status & rules.forbidden != 0
            || actor.behaviors.collision_weapons[ordinal].ever_fired && rules.fire_once
        {
            return;
        }
        // The pinned source never assigns m_everFired after construction.
        let binding = WeaponBinding {
            type_index: actor.type_index,
            weapon_index: unit.collision_weapon_start() + ordinal,
            weapon_slot: 0,
        };
        self.reload_collision_weapon(id, ordinal);
        self.fire_collision_weapon(id, ordinal, binding, other);
    }

    fn reload_collision_weapon(&mut self, id: usize, ordinal: usize) {
        let now = self.update_frame();
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let weapon = &unit.behavior.collision_weapons[ordinal].weapon;
        let rate = actor.weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        let shared = unit.weapon_set_rules.share_reload;
        let instance = &mut self.entities[id].behaviors.collision_weapons[ordinal].weapon;
        if instance.reload_owned(weapon, now, rate, true, shared) && shared {
            self.share_owned_weapon_deadline(id, WeaponStatus::Reloading, now);
        }
    }

    fn fire_collision_weapon(
        &mut self,
        id: usize,
        ordinal: usize,
        binding: WeaponBinding,
        other: usize,
    ) {
        self.request_owned_weapon_assistance(id, binding, Some(other));
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("collision weapon template");
        self.entities[id].behaviors.collision_weapons[ordinal]
            .weapon
            .leech_range_active |= weapon.leech_range;
        let template_branch = self.fire_owned_weapon_prelude(id, binding, Some(other));
        let now = self.update_frame();
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("collision weapon template");
        let actor = &self.entities[id];
        let shot_rate = actor.weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        let actor = &mut self.entities[id];
        let instance = &mut actor.behaviors.collision_weapons[ordinal].weapon;
        if template_branch {
            if !matches!(instance.owned_status(now), WeaponStatus::Ready)
                || instance.clip_remaining == 0
            {
                return;
            }
            let barrel = instance.owned_barrel(weapon, actor.health, actor.max_health);
            let pattern = instance.owned_scatter(weapon, &mut self.random);
            let pattern = pattern.map(|offset| {
                let position = self.object_position(other);
                let point = [position[0] + offset[0], position[1] + offset[1]];
                [point[0], point[1], self.ground_height(point)]
            });
            let target = self.weapon_target(other, pattern);
            self.fire_owned_weapon_payload(id, binding, barrel, target, false);
        }
        self.finish_collision_shot(id, ordinal, binding, now, shot_rate);
    }

    fn finish_collision_shot(
        &mut self,
        id: usize,
        ordinal: usize,
        binding: WeaponBinding,
        now: u32,
        shot_rate: f32,
    ) {
        let unit = &self.catalog[binding.type_index];
        let weapon = unit
            .weapon_template(binding.weapon_index)
            .expect("retained collision weapon");
        let rate = self.entities[id].weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        let shared = unit.weapon_set_rules.share_reload;
        let completion = self.entities[id].behaviors.collision_weapons[ordinal]
            .weapon
            .finish_owned_shot(weapon, now, shot_rate, rate, shared, &mut self.random);
        if let Some((status, deadline)) = completion.shared {
            self.share_owned_weapon_deadline(id, status, deadline);
        }
    }
}
