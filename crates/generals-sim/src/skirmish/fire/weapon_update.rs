//! SH FireWeaponUpdate.cpp: each authored module allocates its own PRIMARY Weapon.
use crate::skirmish::{
    Skirmish, WeaponRules,
    ai_dispatch::{WeaponStatus, native_frame},
    firing::WeaponState,
};

pub struct Rules {
    pub weapon: Option<WeaponRules>,
    pub initial_delay: u32,
    pub exclusive_delay: u32,
}

pub(in crate::skirmish) struct State {
    weapon: Option<WeaponState>,
    initial_frame: u32,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_fire_weapon_module(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.fire_weapons.len(), ordinal);
        let now = self.update_frame();
        let type_index = self.entities[id].type_index;
        let rules = &self.catalog[type_index].behavior.fire_weapons[ordinal];
        let initial_frame = now.wrapping_add(rules.initial_delay);
        let mut state = State {
            weapon: None,
            initial_frame,
        };
        let mut shared_reload = None;
        if let Some(weapon) = &rules.weapon {
            let mut instance = WeaponState::new(weapon.clip_size);
            instance.clip_remaining = 0;
            instance.max_shots = Some(0x7fff_ffff);
            let rate = self.entities[id].weapon_rate_for(weapon, self.passenger_bonus_flags(id));
            let shared = self.catalog[type_index].weapon_set_rules.share_reload;
            instance.reload_owned(weapon, now, rate, true, shared);
            shared_reload = shared.then_some((WeaponStatus::Reloading, now));
            state.weapon = Some(instance);
        }
        self.entities[id].behaviors.fire_weapons.push(state);
        if let Some((status, deadline)) = shared_reload {
            self.share_owned_weapon_deadline(id, status, deadline);
        }
    }

    pub(in crate::skirmish) fn share_owned_weapon_deadline(
        &mut self,
        id: usize,
        status: WeaponStatus,
        deadline: u32,
    ) {
        let unit = &self.catalog[self.entities[id].type_index];
        let actor = &mut self.entities[id];
        for slot in 0..3 {
            if unit.weapon_template(actor.weapon_index(slot)).is_some() {
                actor.weapon_set.weapons[slot].status = status;
                actor.weapon_set.weapons[slot].possible_next_shot = u64::from(deadline);
            }
        }
    }

    pub(in crate::skirmish) fn update_fire_weapon_module(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        let now = self.update_frame();
        if now < self.entities[id].behaviors.fire_weapons[ordinal].initial_frame {
            return 1;
        }
        let Some(instance) = &mut self.entities[id].behaviors.fire_weapons[ordinal].weapon else {
            return 1;
        };
        if !matches!(instance.owned_status(now), WeaponStatus::Ready) {
            return 1;
        }
        if self.entities[id].build_progress < 1.0 {
            return 1;
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = &unit.behavior.fire_weapons[ordinal];
        let recent = (0..3)
            .filter(|&slot| unit.weapon_template(actor.weapon_index(slot)).is_some())
            .filter_map(|slot| actor.weapon_set.weapons[slot].last_shot)
            .map(native_frame)
            .max()
            .unwrap_or(0);
        if rules.exclusive_delay > 0 && now < recent.wrapping_add(rules.exclusive_delay) {
            return 1;
        }
        let index = unit.fire_weapon_start() + ordinal;
        let type_index = self.entities[id].type_index;
        let binding = crate::skirmish::WeaponBinding {
            type_index,
            weapon_index: index,
            weapon_slot: 0,
        };
        self.fire_update_weapon(id, ordinal, now, binding);
        1
    }

    fn fire_update_weapon(
        &mut self,
        id: usize,
        ordinal: usize,
        now: u32,
        binding: crate::skirmish::WeaponBinding,
    ) {
        let type_index = binding.type_index;
        let index = binding.weapon_index;
        self.request_owned_weapon_assistance(id, binding, None);
        let leech = self.catalog[type_index]
            .weapon_template(index)
            .expect("allocated Weapon")
            .leech_range;
        self.entities[id].behaviors.fire_weapons[ordinal]
            .weapon
            .as_mut()
            .expect("allocated Weapon")
            .leech_range_active |= leech;
        let template_branch = self.fire_owned_weapon_prelude(id, binding, None);
        let unit = &self.catalog[type_index];
        let position = self.object_position(id);
        let shot_rate = self.entities[id].weapon_rate_for(
            unit.behavior.fire_weapons[ordinal]
                .weapon
                .as_ref()
                .expect("allocated Weapon"),
            self.passenger_bonus_flags(id),
        );
        let actor = &mut self.entities[id];
        let weapon = unit.behavior.fire_weapons[ordinal]
            .weapon
            .as_ref()
            .expect("allocated Weapon");
        let instance = actor.behaviors.fire_weapons[ordinal]
            .weapon
            .as_mut()
            .expect("allocated Weapon");
        let barrel = if template_branch {
            instance.owned_barrel(weapon, actor.health, actor.max_health)
        } else {
            0
        };
        let pattern = if template_branch {
            instance.owned_scatter(weapon, &mut self.random)
        } else {
            None
        };
        let position = pattern.map_or(position, |offset| {
            let point = [position[0] + offset[0], position[1] + offset[1]];
            [point[0], point[1], self.ground_height(point)]
        });
        let target = crate::skirmish::damage::WeaponTarget {
            object: None,
            position,
            projectile_position: position,
            scatter_radius: 0.0,
        };
        if template_branch {
            self.fire_owned_weapon_payload(id, binding, barrel, target, true);
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let weapon = unit.behavior.fire_weapons[ordinal]
            .weapon
            .as_ref()
            .expect("allocated module Weapon");
        let reload_rate = actor.weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        let shared = unit.weapon_set_rules.share_reload;
        let propagation = self.entities[id].behaviors.fire_weapons[ordinal]
            .weapon
            .as_mut()
            .expect("allocated module Weapon")
            .finish_owned_shot(
                weapon,
                now,
                shot_rate,
                reload_rate,
                shared,
                &mut self.random,
            );
        if let Some((status, deadline)) = propagation.shared {
            self.share_owned_weapon_deadline(id, status, deadline);
        }
    }
}
