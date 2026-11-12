//! `JetOrHeliReloadAmmoState` and `Weapon::setClipPercentFull`.
use super::super::{Entity, Skirmish, UnitType, progression};

pub(super) struct Reload {
    frames: u32,
    done: u32,
}

impl Reload {
    pub(super) const fn new() -> Self {
        Self { frames: 1, done: 0 }
    }

    pub(super) fn begin(actor: &Entity, unit: &UnitType, now: u64, extra: u32) -> Self {
        let frames = (0..actor.weapon_set.weapons.len())
            .filter_map(|slot| {
                let weapon = unit.weapon_template(actor.weapon_index(slot))?;
                let reload = ((weapon.reload_seconds * 30.0).ceil()
                    / actor.weapon_rate_for(weapon, extra))
                .floor();
                let mut reload = progression::real_unsigned(reload);
                if weapon.clip_size > 0 {
                    let needed = weapon.clip_size - actor.weapon_set.weapons[slot].clip_remaining;
                    reload = (reload * needed) / weapon.clip_size;
                }
                Some(reload)
            })
            .max()
            .unwrap_or(0)
            .max(1);
        Self {
            frames,
            done: crate::skirmish::ai_dispatch::native_frame(now).wrapping_add(frames),
        }
    }

    fn percent(&self, now: u64) -> f32 {
        let now = crate::skirmish::ai_dispatch::native_frame(now);
        if now >= self.done {
            return 1.0;
        }
        let remaining = self.done.wrapping_sub(now);
        progression::integer_real(self.frames.wrapping_sub(remaining))
            / progression::integer_real(self.frames)
    }
}

impl Skirmish {
    pub(in crate::skirmish::aircraft) fn reload_aircraft(&mut self, id: usize) -> bool {
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let percent = actor
            .aircraft
            .as_ref()
            .expect("aircraft state")
            .rearm
            .percent(self.tick);
        let mut done = true;
        for slot in 0..actor.weapon_set.weapons.len() {
            let Some(weapon) = unit.weapon_template(actor.weapon_index(slot)) else {
                continue;
            };
            let state = &mut actor.weapon_set.weapons[slot];
            if weapon.clip_size > 0 {
                let ammo = progression::real_unsigned(
                    (progression::integer_real(weapon.clip_size) * percent).floor(),
                );
                if ammo > state.clip_remaining {
                    state.clip_remaining = ammo;
                    state.scatter_targets = None;
                    state.cooldown = 0.0;
                    state.possible_next_shot = self.tick;
                    state.last_reload_started = self.tick;
                    state.status = if ammo > 0 {
                        crate::skirmish::ai_dispatch::WeaponStatus::OutOfAmmo
                    } else {
                        crate::skirmish::ai_dispatch::WeaponStatus::Ready
                    };
                }
            }
            if state.clip_remaining != weapon.clip_size {
                done = false;
            }
        }
        done
    }
}
