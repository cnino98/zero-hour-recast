//! `Object::adjustModelConditionForWeaponStatus`, invoked by its FINAL helper.
use crate::skirmish::{Skirmish, ai_dispatch::WeaponStatus};

#[cfg(test)]
mod tests;

pub(in crate::skirmish) struct State {
    pub attacking: bool,
    pub aiming: bool,
    pub firing: bool,
    pub last_condition: [Option<&'static str>; 3],
    pub condition_initialized: [bool; 3],
    pub condition: [Option<&'static str>; 3],
    pub clip_status: [(u32, u32); 3],
    pub pre_attack_animation: Option<(u64, u32, bool)>,
}

impl State {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            attacking: false,
            aiming: false,
            firing: false,
            last_condition: [None; 3],
            condition_initialized: [false; 3],
            condition: [None; 3],
            clip_status: [(0, 0); 3],
            pre_attack_animation: None,
        }
    }
}

const FIRING: [&str; 3] = ["FIRING_A", "FIRING_B", "FIRING_C"];
const BETWEEN: [&str; 3] = [
    "BETWEEN_FIRING_SHOTS_A",
    "BETWEEN_FIRING_SHOTS_B",
    "BETWEEN_FIRING_SHOTS_C",
];
const RELOADING: [&str; 3] = ["RELOADING_A", "RELOADING_B", "RELOADING_C"];
const PREATTACK: [&str; 3] = ["PREATTACK_A", "PREATTACK_B", "PREATTACK_C"];

impl Skirmish {
    pub(in crate::skirmish) fn update_weapon_status(&mut self, id: usize) {
        let now = crate::skirmish::ai_dispatch::native_frame(self.tick);
        let unit = &self.catalog[self.entities[id].type_index];
        for slot in 0..3 {
            let actor = &mut self.entities[id];
            let Some(weapon) = unit.weapon_template(actor.weapon_index(slot)) else {
                actor.weapon_set.status.last_condition[slot] = None;
                actor.weapon_set.status.condition_initialized[slot] = true;
                continue;
            };
            let state = &actor.weapon_set.weapons[slot];
            let condition = if slot != actor.weapon_set.current {
                None
            } else if state
                .last_shot
                .is_some_and(|shot| crate::skirmish::ai_dispatch::native_frame(shot) == now)
            {
                Some(FIRING[slot])
            } else if actor.weapon_set.status.attacking {
                let status = actor
                    .weapon_status(slot, unit, now)
                    .expect("allocated weapon");
                let aimed = actor.weapon_set.status.aiming || actor.weapon_set.status.firing;
                match status {
                    WeaponStatus::PreAttack => Some(PREATTACK[slot]),
                    WeaponStatus::Reloading => Some(RELOADING[slot]),
                    WeaponStatus::BetweenShots => Some(BETWEEN[slot]),
                    WeaponStatus::Ready if aimed => Some(BETWEEN[slot]),
                    WeaponStatus::Ready | WeaponStatus::OutOfAmmo => None,
                }
            } else {
                None
            };
            if !actor.has_drawable() {
                continue;
            }
            let ammo = actor.native_remaining_ammo(slot, unit, now);
            actor.weapon_set.status.clip_status[slot] = (ammo, weapon.clip_size);
            if actor.weapon_set.status.condition_initialized[slot]
                && actor.weapon_set.status.last_condition[slot] == condition
            {
                continue;
            }
            actor.weapon_set.status.condition_initialized[slot] = true;
            actor.weapon_set.status.last_condition[slot] = condition;
            if matches!(
                (slot, actor.drawable.slave_condition),
                (1, Some("FIRING_B")) | (2, Some("FIRING_C"))
            ) {
                actor.drawable.slave_condition = None;
            }
            actor.weapon_set.status.condition[slot] = condition;
            if condition == Some(PREATTACK[slot]) {
                let state = &actor.weapon_set.weapons[slot];
                if let Some(attack) = &state.pre_attack {
                    let done = crate::skirmish::ai_dispatch::native_frame(attack.start)
                        .wrapping_add(attack.frames);
                    if done > now {
                        actor.weapon_set.status.pre_attack_animation =
                            Some((self.tick, done - now, false));
                    }
                }
            }
        }
    }
}
