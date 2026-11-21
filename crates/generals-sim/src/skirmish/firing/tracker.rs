//! `FiringTracker.cpp`: object callback timers, condition edges and audio order.
use crate::skirmish::{Entity, Skirmish, UnitType, WeaponFxEvent, module_updates::Helper};

#[cfg(test)]
mod tests;

pub(super) const MEAN: u32 = 1 << 2;
pub(super) const FAST: u32 = 1 << 3;

pub(in crate::skirmish) struct FiringTracker {
    pub(in crate::skirmish) veterancy_flags: u8,
    pub(in crate::skirmish) painted_target: bool,
    pub(in crate::skirmish) drone_spotting: bool,
    pub(in crate::skirmish) force_reload_at: u32,
    pub(in crate::skirmish) continuous_flags: u32,
    pub(in crate::skirmish) shots: i32,
    pub(in crate::skirmish) victim: Option<usize>,
    cooldown_at: u32,
    stop_sound_at: u32,
    pub(super) slow: bool,
}

impl FiringTracker {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            veterancy_flags: 0,
            painted_target: false,
            drone_spotting: false,
            force_reload_at: 0,
            continuous_flags: 0,
            shots: 0,
            victim: None,
            cooldown_at: 0,
            stop_sound_at: 0,
            slow: false,
        }
    }

    pub(in crate::skirmish) fn record_target(&mut self, target: Option<usize>, now: u32) {
        self.shots = if self.victim == target || now < self.cooldown_at {
            self.shots.wrapping_add(1)
        } else {
            1
        };
        self.victim = target;
    }

    fn sleep(&self, now: u32) -> u32 {
        let mut delay = crate::skirmish::module_updates::FOREVER;
        for frame in [self.stop_sound_at, self.cooldown_at, self.force_reload_at] {
            if frame != 0 {
                delay = delay.min(if frame <= now { 1 } else { frame - now });
            }
        }
        delay
    }
}

impl Entity {
    fn continuous_flag(
        &mut self,
        flag: u32,
        value: bool,
        unit: &UnitType,
        frame: u64,
        extra: u32,
        random: &mut [u32; 6],
    ) {
        let old = self.firing.continuous_flags;
        if value {
            self.firing.continuous_flags |= flag;
        } else {
            self.firing.continuous_flags &= !flag;
        }
        if self.firing.continuous_flags != old {
            self.weapon_bonus_changed(unit, frame, extra, random);
        }
    }

    fn cool_firing_tracker(
        &mut self,
        unit: &UnitType,
        frame: u64,
        extra: u32,
        random: &mut [u32; 6],
    ) {
        self.firing.slow = self.firing.continuous_flags & (FAST | MEAN) != 0;
        self.continuous_flag(FAST, false, unit, frame, extra, random);
        self.continuous_flag(MEAN, false, unit, frame, extra, random);
        if !self.firing.slow {
            self.firing.cooldown_at = 0;
        }
        self.firing.shots = 0;
        self.firing.victim = None;
    }

    fn reload_firing_tracker_ammo(&mut self, unit: &UnitType, frame: u64) {
        for slot in 0..self.weapon_set.weapons.len() {
            if let Some(weapon) = unit.weapon_template(self.weapon_index(slot)) {
                let state = &mut self.weapon_set.weapons[slot];
                if weapon.clip_size > 0
                    && state.clip_remaining == weapon.clip_size
                    && !unit.weapon_set_rules.share_reload
                {
                    continue;
                }
                state.clip_remaining = if weapon.clip_size == 0 {
                    0x7fff_ffff
                } else {
                    weapon.clip_size
                };
                state.scatter_targets = None;
                state.cooldown = 0.0;
                state.possible_next_shot =
                    u64::from(crate::skirmish::ai_dispatch::native_frame(frame));
                state.last_reload_started = frame;
                state.status = crate::skirmish::ai_dispatch::WeaponStatus::Reloading;
                if unit.weapon_set_rules.share_reload {
                    for shared in 0..self.weapon_set.weapons.len() {
                        if unit.weapon_template(self.weapon_index(shared)).is_some() {
                            let shared_state = &mut self.weapon_set.weapons[shared];
                            shared_state.possible_next_shot =
                                u64::from(crate::skirmish::ai_dispatch::native_frame(frame));
                            shared_state.status =
                                crate::skirmish::ai_dispatch::WeaponStatus::Reloading;
                        }
                    }
                }
            }
        }
    }
}

impl Skirmish {
    /// `Object::fireCurrentWeapon` invokes this after the weapon's damage/FX.
    pub(in crate::skirmish) fn notify_firing_tracker_shot(
        &mut self,
        id: usize,
        weapon_index: usize,
        target: Option<usize>,
    ) {
        let now = self.update_frame();
        let extra = self.passenger_bonus_flags(id);
        let painted = target.is_some_and(|target| self.entities[target].painted());
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        actor.update_painted_target(unit, painted, self.tick, extra, &mut self.random);
        actor.firing.record_target(target, now);
        let weapon = unit.weapon_template(weapon_index).expect("fired weapon");
        if weapon.auto_reload_idle_frames != 0 {
            actor.firing.force_reload_at = now.wrapping_add(weapon.auto_reload_idle_frames);
        }
        let coast = weapon.continuous_fire.as_ref().map_or(0, |rules| {
            crate::skirmish::progression::real_unsigned(rules.coast_frames)
        });
        actor.firing.cooldown_at = if coast == 0 {
            0
        } else {
            let next = actor.weapon_set.weapons[actor.weapon_slot()].possible_next_shot;
            u32::try_from(next & u64::from(u32::MAX))
                .expect("masked weapon frame")
                .wrapping_add(coast)
        };
        let thresholds = weapon
            .continuous_fire
            .as_ref()
            .map_or([0x7fff_ffff; 2], |rules| {
                [rules.mean_shots, rules.fast_shots]
            });
        let loop_frames = weapon.fire_sound_loop_frames;
        self.change_continuous_fire(id, thresholds, extra);
        self.record_firing_sound(id, weapon_index);
        let actor = &mut self.entities[id];
        actor.firing.stop_sound_at = if loop_frames == 0 {
            0
        } else {
            now.wrapping_add(loop_frames)
        };
        let delay = actor.firing.sleep(now);
        self.wake_helper(id, Helper::FiringTracker, delay);
    }

    fn change_continuous_fire(&mut self, id: usize, thresholds: [i32; 2], extra: u32) {
        let actor = &mut self.entities[id];
        let flags = actor.firing.continuous_flags;
        let shots = actor.firing.shots;
        let unit = &self.catalog[actor.type_index];
        if (flags & MEAN != 0 && shots < thresholds[0])
            || (flags & MEAN == 0 && flags & FAST != 0 && shots < thresholds[1])
        {
            actor.cool_firing_tracker(unit, self.tick, extra, &mut self.random);
            return;
        }
        let faster = if flags & MEAN != 0 {
            shots > thresholds[1]
        } else {
            flags & FAST == 0 && shots > thresholds[0]
        };
        if !faster || flags & FAST != 0 {
            return;
        }
        if flags & MEAN != 0 {
            let position = self.object_position(id);
            self.presentation.weapon_fx.push(WeaponFxEvent::RapidFire {
                object: id,
                position,
            });
        }
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if flags & MEAN != 0 {
            actor.continuous_flag(FAST, true, unit, self.tick, extra, &mut self.random);
            actor.continuous_flag(MEAN, false, unit, self.tick, extra, &mut self.random);
        } else {
            actor.continuous_flag(MEAN, true, unit, self.tick, extra, &mut self.random);
            actor.continuous_flag(FAST, false, unit, self.tick, extra, &mut self.random);
        }
        actor.firing.slow = false;
    }

    fn record_firing_sound(&mut self, id: usize, weapon: usize) {
        let position = self.object_position(id);
        let clear_cells = (0..self.players.len()).fold(0, |mask, owner| {
            if matches!(
                self.visibility_cell(owner, [position[0], position[1]]),
                crate::skirmish::visibility::Status::Clear
            ) {
                mask | (1 << owner)
            } else {
                mask
            }
        });
        self.presentation
            .weapon_fx
            .push(WeaponFxEvent::FiringSound {
                object: id,
                weapon,
                position,
                clear_cells,
            });
    }

    pub(in crate::skirmish) fn stop_firing_sound(&mut self, id: usize) {
        self.presentation
            .weapon_fx
            .push(WeaponFxEvent::StopFiringSound(id));
        self.entities[id].firing.stop_sound_at = 0;
    }

    pub(in crate::skirmish) fn update_firing_tracker(&mut self, id: usize) -> i32 {
        let now = self.update_frame();
        let extra = self.passenger_bonus_flags(id);
        let actor = &mut self.entities[id];
        if actor.firing.force_reload_at != 0 && now >= actor.firing.force_reload_at {
            actor.reload_firing_tracker_ammo(&self.catalog[actor.type_index], self.tick);
            actor.firing.force_reload_at = 0;
        }
        if actor.firing.stop_sound_at != 0 && now >= actor.firing.stop_sound_at {
            self.stop_firing_sound(id);
        }
        let actor = &mut self.entities[id];
        if actor.firing.cooldown_at != 0 && now > actor.firing.cooldown_at {
            actor.firing.cooldown_at = now.wrapping_add(30);
            actor.cool_firing_tracker(
                &self.catalog[actor.type_index],
                self.tick,
                extra,
                &mut self.random,
            );
            // This return takes precedence even if another timer is due sooner.
            return 30;
        }
        i32::try_from(actor.firing.sleep(now)).expect("update sleep fits signed native return")
    }
}
