//! SH `FireWeaponWhenDamagedBehavior`: eight separately allocated PRIMARY weapons.
use crate::skirmish::{
    DamageType, Skirmish, WeaponRules, ai_dispatch::WeaponStatus, firing::WeaponState,
    module_updates::FOREVER,
};

pub struct Rules {
    pub reaction: [Option<WeaponRules>; 4],
    pub continuous: [Option<WeaponRules>; 4],
    pub starts_active: bool,
    pub damage_types: u64,
    pub amount: f32,
    pub triggers: Vec<String>,
    pub conflicts: Vec<String>,
    pub requires_all: bool,
    pub removals: Vec<String>,
    pub upgrade_fx: String,
}

pub(in crate::skirmish) struct State {
    active: bool,
    reaction: [Option<WeaponState>; 4],
    continuous: [Option<WeaponState>; 4],
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_damage_weapon_module(
        &mut self,
        id: usize,
        ordinal: usize,
    ) {
        assert_eq!(self.entities[id].behaviors.damage_weapons.len(), ordinal);
        let now = self.update_frame();
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = &unit.behavior.damage_weapons[ordinal];
        let shared = unit.weapon_set_rules.share_reload;
        let extra = self.passenger_bonus_flags(id);
        let mut state = State {
            active: false,
            reaction: std::array::from_fn(|_| None),
            continuous: std::array::from_fn(|_| None),
        };
        let mut propagation = Vec::new();
        for (templates, instances) in [
            (&rules.reaction, &mut state.reaction),
            (&rules.continuous, &mut state.continuous),
        ] {
            for (template, instance) in templates.iter().zip(instances) {
                if let Some(template) = template {
                    let mut weapon = WeaponState::new(template.clip_size);
                    weapon.clip_remaining = 0;
                    weapon.max_shots = Some(0x7fff_ffff);
                    let rate = actor.weapon_rate_for(template, extra);
                    weapon.reload_owned(template, now, rate, false, shared);
                    if shared {
                        propagation.push((
                            WeaponStatus::Reloading,
                            crate::skirmish::ai_dispatch::native_frame(weapon.possible_next_shot),
                        ));
                    }
                    *instance = Some(weapon);
                }
            }
        }
        let starts_active = rules.starts_active;
        let awake = starts_active && rules.continuous.iter().any(Option::is_some);
        self.entities[id].behaviors.damage_weapons.push(state);
        for (status, deadline) in propagation {
            self.share_owned_weapon_deadline(id, status, deadline);
        }
        if starts_active {
            self.activate_damage_weapon_module(id, ordinal);
        }
        self.wake_family_update(
            id,
            "FireWeaponWhenDamagedBehavior",
            ordinal,
            if awake { 1 } else { FOREVER },
        );
    }

    /// `UpgradeMux::upgradeImplementation` wakes even modules with only reaction weapons.
    pub(in crate::skirmish) fn activate_damage_weapon_module(&mut self, id: usize, ordinal: usize) {
        self.give_self_upgrade_instance(id, "FireWeaponWhenDamagedBehavior", ordinal);
    }

    pub(in crate::skirmish) fn apply_damage_weapon_upgrade(&mut self, id: usize, ordinal: usize) {
        self.wake_family_update(id, "FireWeaponWhenDamagedBehavior", ordinal, 1);
        self.entities[id].behaviors.damage_weapons[ordinal].active = true;
    }

    pub(in crate::skirmish) fn reset_damage_weapon_upgrade(&mut self, id: usize, ordinal: usize) {
        self.entities[id].behaviors.damage_weapons[ordinal].active = false;
    }

    pub(in crate::skirmish) fn damage_weapon_on_damage(
        &mut self,
        id: usize,
        ordinal: usize,
        damage_type: DamageType,
        actual: f32,
    ) {
        if !self.entities[id].behaviors.damage_weapons[ordinal].active {
            return;
        }
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .damage_weapons[ordinal];
        if rules.damage_types & (1_u64 << damage_type as u32) == 0 || actual < rules.amount {
            return;
        }
        self.fire_damage_weapon_instance(id, ordinal, false);
    }

    pub(in crate::skirmish) fn update_damage_weapon_module(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        if !self.entities[id].behaviors.damage_weapons[ordinal].active {
            return i32::try_from(FOREVER).expect("native update sleep");
        }
        self.fire_damage_weapon_instance(id, ordinal, true);
        1
    }

    fn fire_damage_weapon_instance(&mut self, id: usize, ordinal: usize, continuous: bool) {
        let body = self.body_damage_state(id);
        let now = self.update_frame();
        let instances = &mut self.entities[id].behaviors.damage_weapons[ordinal];
        let instance = if continuous {
            &mut instances.continuous[body]
        } else {
            &mut instances.reaction[body]
        };
        let Some(instance) = instance else {
            return;
        };
        if !matches!(instance.owned_status(now), WeaponStatus::Ready) {
            return;
        }
        let index = self.catalog[self.entities[id].type_index].damage_weapon_start()
            + ordinal * 8
            + usize::from(continuous) * 4
            + body;
        let binding = crate::skirmish::WeaponBinding {
            type_index: self.entities[id].type_index,
            weapon_index: index,
            weapon_slot: 0,
        };
        self.request_owned_weapon_assistance(id, binding, None);
        let leech = self.catalog[binding.type_index]
            .weapon_template(index)
            .expect("allocated Weapon")
            .leech_range;
        let instances = &mut self.entities[id].behaviors.damage_weapons[ordinal];
        let instance = if continuous {
            instances.continuous[body].as_mut()
        } else {
            instances.reaction[body].as_mut()
        }
        .expect("allocated Weapon");
        instance.leech_range_active |= leech;
        let template_branch = self.fire_owned_weapon_prelude(id, binding, None);
        let unit = &self.catalog[self.entities[id].type_index];
        let rules = &unit.behavior.damage_weapons[ordinal];
        let weapon = if continuous {
            rules.continuous[body].as_ref()
        } else {
            rules.reaction[body].as_ref()
        }
        .expect("allocated Weapon");
        let shot_rate = self.entities[id].weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        let actor = &mut self.entities[id];
        let instances = &mut actor.behaviors.damage_weapons[ordinal];
        let instance = if continuous {
            instances.continuous[body].as_mut()
        } else {
            instances.reaction[body].as_mut()
        }
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
        let mut position = self.object_position(id);
        if let Some(offset) = pattern {
            position[0] += offset[0];
            position[1] += offset[1];
            position[2] = self.ground_height([position[0], position[1]]);
        }
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
        let template = unit
            .weapon_template(index)
            .expect("allocated damage weapon");
        let reload_rate = actor.weapon_rate_for(template, self.passenger_bonus_flags(id));
        let shared = unit.weapon_set_rules.share_reload;
        let instances = &mut self.entities[id].behaviors.damage_weapons[ordinal];
        let instance = if continuous {
            &mut instances.continuous[body]
        } else {
            &mut instances.reaction[body]
        };
        let propagation = instance
            .as_mut()
            .expect("allocated damage weapon")
            .finish_owned_shot(template, now, shot_rate, reload_rate, shared);
        if let Some((status, deadline)) = propagation.shared {
            self.share_owned_weapon_deadline(id, status, deadline);
        }
    }
}
