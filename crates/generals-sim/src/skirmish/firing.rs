//! `FiringTracker::shotFired/update` and `WeaponTemplate::getDelayBetweenShots`.
//! The pinned source's strict thresholds and post-shot bonus changes matter here.

use super::ai_dispatch::{WeaponStatus, native_frame};
use num_traits::ToPrimitive;

pub struct ContinuousFire {
    pub mean_shots: i32,
    pub fast_shots: i32,
    pub coast_frames: f32,
}

pub(super) struct PreAttack {
    pub start: u64,
    pub frames: u32,
}

pub(super) struct WeaponState {
    pub status: WeaponStatus,
    pub leech_range_active: bool,
    pub pre_attack: Option<PreAttack>,
    pub scatter_targets: Option<Vec<usize>>,
    pub cooldown: f32,
    pub possible_next_shot: u64,
    pub last_reload_started: u64,
    pub max_shots: Option<u32>,
    pub clip_remaining: u32,
    pub last_shot: Option<u64>,
    pub aiming: bool,
    pub barrel: usize,
    pub barrel_shots: u32,
}

impl WeaponState {
    pub(super) const fn new(clip: u32) -> Self {
        Self {
            status: WeaponStatus::Reloading,
            leech_range_active: false,
            pre_attack: None,
            scatter_targets: None,
            cooldown: 0.0,
            possible_next_shot: 0,
            last_reload_started: 0,
            max_shots: None,
            clip_remaining: if clip == 0 { 0x7fff_ffff } else { clip },
            last_shot: None,
            aiming: false,
            barrel: 0,
            barrel_shots: 0,
        }
    }
}

impl super::Entity {
    pub(super) fn weapon_bonus_changed(
        &mut self,
        unit: &super::UnitType,
        frame: u64,
        extra: u32,
        random: &mut [u32; 6],
    ) {
        // Object's actual condition-bit edges call every allocated weapon in
        // slot order. Weapon::onWeaponBonusChange restarts the full active delay.
        for slot in 0..self.weapon_set.weapons.len() {
            let Some(weapon) = unit.weapon_template(self.weapon_index(slot)) else {
                continue;
            };
            let rate = self.weapon_rate_for(weapon, extra);
            let frames = match self.weapon_status(slot, unit, native_frame(frame)) {
                Some(WeaponStatus::Reloading) => super::progression::real_unsigned(
                    ((weapon.reload_seconds * 30.0).ceil() / rate).floor(),
                ),
                Some(WeaponStatus::BetweenShots) => weapon.sample_shot_delay(random, rate),
                _ => continue,
            };
            let delay = super::progression::integer_real(frames);
            let deadline = u64::from(native_frame(frame).wrapping_add(frames));
            self.weapon_set.weapons[slot].cooldown = delay;
            self.weapon_set.weapons[slot].possible_next_shot = deadline;
            self.weapon_set.weapons[slot].last_reload_started = frame;
            if unit.weapon_set_rules.share_reload {
                // Native shared reload changes every slot to RELOADING_CLIP,
                // including later slots visited by this callback's outer loop.
                for shared in 0..self.weapon_set.weapons.len() {
                    if unit.weapon_template(self.weapon_index(shared)).is_some() {
                        let state = &mut self.weapon_set.weapons[shared];
                        state.cooldown = delay;
                        state.possible_next_shot = deadline;
                        state.status = WeaponStatus::Reloading;
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn spend_weapon_round(&mut self, unit: &super::UnitType, frame: u64, extra: u32) {
        let slot = self.weapon_slot();
        let weapon = self.active_weapon(unit);
        let rate = self.weapon_rate_for(weapon, extra);
        let delay = ((weapon.delay * 30.0).ceil() / rate).floor();
        self.weapon_set.weapons[slot].cooldown = delay.max(1.0);
        self.weapon_set.weapons[slot].last_reload_started = frame;
        self.weapon_set.weapons[slot].possible_next_shot =
            u64::from(native_frame(frame).wrapping_add(super::progression::real_unsigned(delay)));
        self.weapon_set.weapons[slot].status = WeaponStatus::BetweenShots;
        self.weapon_set.weapons[self.weapon_set.current].clip_remaining -= 1;
        if self.weapon_set.weapons[self.weapon_set.current].clip_remaining == 0
            && matches!(weapon.reload, super::ClipReload::Automatic)
        {
            self.weapon_set.weapons[self.weapon_set.current].cooldown =
                ((weapon.reload_seconds * 30.0).ceil() / rate)
                    .floor()
                    .max(1.0);
            let delay = ((weapon.reload_seconds * 30.0).ceil() / rate).floor();
            self.weapon_set.weapons[slot].possible_next_shot = u64::from(
                native_frame(frame).wrapping_add(super::progression::real_unsigned(delay)),
            );
            self.weapon_set.weapons[self.weapon_set.current].clip_remaining =
                if weapon.clip_size == 0 {
                    0x7fff_ffff
                } else {
                    weapon.clip_size
                };
            self.weapon_set.weapons[slot].status = WeaponStatus::Reloading;
            self.weapon_set.weapons[self.weapon_set.current].scatter_targets = None;
        }
        if self.weapon_set.weapons[slot].clip_remaining == 0 {
            self.weapon_set.weapons[slot].possible_next_shot = 0x7fff_ffff;
            self.weapon_set.weapons[slot].status = WeaponStatus::OutOfAmmo;
        }
        if let Some(count) = &mut self.weapon_set.weapons[slot].max_shots {
            *count = count.saturating_sub(1);
        }
        if unit.weapon_set_rules.share_reload
            && !matches!(
                self.weapon_set.weapons[slot].status,
                WeaponStatus::OutOfAmmo
            )
        {
            let status = self.weapon_set.weapons[slot].status;
            let cooldown = self.weapon_set.weapons[slot].cooldown;
            let deadline = self.weapon_set.weapons[slot].possible_next_shot;
            for shared in 0..self.weapon_set.weapons.len() {
                if unit.weapon_template(self.weapon_index(shared)).is_some() {
                    let state = &mut self.weapon_set.weapons[shared];
                    state.cooldown = cooldown;
                    state.possible_next_shot = deadline;
                    state.status = status;
                }
            }
        }
        if matches!(
            self.weapon_set.weapons[slot].status,
            WeaponStatus::Reloading
        ) && matches!(self.weapon_set.lock, super::weapons::Lock::Temporary)
        {
            self.weapon_set.lock = super::weapons::Lock::None;
        }
    }

    #[must_use]
    pub const fn attacking(&self) -> bool {
        self.weapon_set.status.attacking
    }

    #[must_use]
    pub const fn continuous_fire_condition(&self) -> Option<&'static str> {
        match self.firing.continuous_flags {
            tracker::MEAN => Some("CONTINUOUS_FIRE_MEAN"),
            tracker::FAST => Some("CONTINUOUS_FIRE_FAST"),
            _ if self.firing.slow => Some("CONTINUOUS_FIRE_SLOW"),
            _ => None,
        }
    }

    /// The original drawable retains its condition until a source callback changes it.
    #[must_use]
    pub const fn weapon_condition(&self) -> Option<&'static str> {
        self.weapon_set.status.condition[self.weapon_set.current]
    }

    pub(super) const fn is_firing_weapon(&self) -> bool {
        self.weapon_set.status.firing
    }
}

mod binding;
mod owned;
mod payload;
pub use binding::WeaponBinding;
pub(super) mod status;
mod tracker;
pub(super) use tracker::FiringTracker;

#[cfg(test)]
mod bonus_tests {
    use crate::skirmish::{Faction, Kind, WeaponRules, ai_dispatch::WeaponStatus, tests};

    #[test]
    fn shared_reload_bonus_callback_uses_native_slot_order_and_clip_status() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::Usa, Kind::Tank);
        unit.weapon.delay = 2.0;
        unit.weapon.reload_seconds = 3.0;
        let mut secondary = WeaponRules::unarmed();
        secondary.delay = 4.0;
        secondary.reload_seconds = 5.0;
        unit.secondary_weapon = Some(secondary);
        unit.weapon_set_rules.share_reload = true;
        let index = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(index, 0, [300.0, 300.0], true);
        game.entities[id].spend_weapon_round(&game.catalog[index], game.tick, 0);
        let actor = &mut game.entities[id];
        actor.weapon_bonus_changed(&game.catalog[index], game.tick, 0, &mut game.random);
        // PRIMARY's shot delay marks both as reloading; the subsequent SECONDARY
        // callback then sets both to its full clip reload duration.
        for slot in [0, 1] {
            assert!(matches!(
                actor.weapon_set.weapons[slot].status,
                WeaponStatus::Reloading
            ));
            assert_eq!(
                actor.weapon_set.weapons[slot].cooldown.to_bits(),
                150.0_f32.to_bits()
            );
        }
        assert_eq!(
            actor.weapon_set.weapons[2].cooldown.to_bits(),
            0.0_f32.to_bits()
        );
    }
}

impl super::Skirmish {
    pub(in crate::skirmish) fn weapon_clip_reload_frames(
        &self,
        id: usize,
        slot: usize,
    ) -> Option<f32> {
        let actor = &self.entities[id];
        let weapon = self.allocated_weapon(id, slot)?;
        let rate = actor.weapon_rate_for(weapon, self.passenger_bonus_flags(id));
        Some(((weapon.reload_seconds * 30.0).ceil() / rate).floor())
    }

    // ControlBarCommand GUI_COMMAND_FIRE_WEAPON uses the actual lazy status
    // query, with a zero effective clip-reload exception and two flicker frames.
    pub(in crate::skirmish) fn weapon_button_ready(&mut self, id: usize, slot: usize) -> bool {
        let Some(reload) = self.weapon_clip_reload_frames(id, slot) else {
            return false;
        };
        if reload == 0.0 {
            return true;
        }
        let actor = &self.entities[id];
        let next = native_frame(actor.weapon_set.weapons[slot].possible_next_shot);
        let now = self.update_frame();
        let type_index = actor.type_index;
        matches!(
            self.entities[id].weapon_status(slot, &self.catalog[type_index], now),
            Some(WeaponStatus::Ready)
        ) && next != now
            && next != now.wrapping_sub(1)
    }

    pub(super) fn update_weapon_clocks(&mut self, id: usize) {
        // Native Weapon::getStatus resolves its frame deadline when queried.
        for weapon in &mut self.entities[id].weapon_set.weapons {
            weapon.cooldown = weapon
                .possible_next_shot
                .saturating_sub(self.tick)
                .to_f32()
                .expect("weapon frame interval converts to Real");
        }
    }
}

impl super::Entity {
    pub(super) fn pre_attack_animation(&self) -> Option<(u64, u32, bool)> {
        self.weapon_condition()
            .filter(|condition| condition.starts_with("PREATTACK_"))
            .and(self.weapon_set.status.pre_attack_animation)
    }
}

#[cfg(test)]
mod artillery_tests {
    use crate::skirmish::tests::game;

    #[test]
    fn preattack_waits_exact_frames_and_new_fire_entry_restarts_the_original_delay() {
        let mut game = game();
        game.catalog[6].weapon.pre_attack_frames = 8;
        let id = game.spawn(6, 0, [100.0, 100.0], true);
        game.tick = 10;
        game.prefire_native_weapon(id, Some(0));
        assert_eq!(
            game.entities[id].weapon_set.weapons[0]
                .pre_attack
                .as_ref()
                .map(|attack| (attack.start, attack.frames)),
            Some((10, 8))
        );
        assert!(matches!(
            game.entities[id].weapon_status(0, &game.catalog[6], 17),
            Some(crate::skirmish::ai_dispatch::WeaponStatus::PreAttack)
        ));
        assert!(matches!(
            game.entities[id].weapon_status(0, &game.catalog[6], 18),
            Some(crate::skirmish::ai_dispatch::WeaponStatus::Ready)
        ));
        game.exit_attack_fire(id);
        game.tick = 25;
        game.prefire_native_weapon(id, Some(1));
        assert_eq!(
            game.entities[id].weapon_set.weapons[0]
                .pre_attack
                .as_ref()
                .map(|attack| (attack.start, attack.frames)),
            Some((25, 8))
        );
        game.exit_attack_fire(id);
        assert!(game.entities[id].weapon_set.weapons[0].pre_attack.is_none());
        game.tick = 27;
        game.prefire_native_weapon(id, Some(1));
        assert_eq!(
            game.entities[id].weapon_set.weapons[0]
                .pre_attack
                .as_ref()
                .map(|attack| (attack.start, attack.frames)),
            Some((27, 8))
        );
    }

    #[test]
    fn idle_reload_fills_partial_clip_at_deadline_without_restarting_full_clip() {
        let mut game = game();
        game.catalog[6].weapon.clip_size = 6;
        let id = game.spawn(6, 0, [100.0, 100.0], true);
        game.entities[id].weapon_set.weapons[0].clip_remaining = 2;
        game.entities[id].firing.force_reload_at = 183;
        game.tick = 182;
        game.update_firing_tracker(id);
        game.update_weapon_clocks(id);
        assert_eq!(game.entities[id].weapon_set.weapons[0].clip_remaining, 2);
        game.tick = 183;
        game.update_firing_tracker(id);
        game.update_weapon_clocks(id);
        assert_eq!(game.entities[id].weapon_set.weapons[0].clip_remaining, 6);
        assert_eq!(game.entities[id].firing.force_reload_at, 0);
        game.entities[id].weapon_set.weapons[0].cooldown = 80.0;
        game.entities[id].weapon_set.weapons[0].possible_next_shot = 263;
        game.entities[id].firing.force_reload_at = 184;
        game.tick = 184;
        game.update_firing_tracker(id);
        game.update_weapon_clocks(id);
        assert_eq!(
            game.entities[id].weapon_set.weapons[0].cooldown.to_bits(),
            79.0_f32.to_bits()
        );
    }
}

impl super::Skirmish {
    /// `Object::preFireCurrentWeapon` is a FIRE entry action, never a readiness poll.
    pub(in crate::skirmish) fn prefire_native_weapon(&mut self, id: usize, target: Option<usize>) {
        let now = self.update_frame();
        let actor = &self.entities[id];
        let slot = actor.weapon_slot();
        if now.wrapping_add(1) < native_frame(actor.weapon_set.weapons[slot].possible_next_shot) {
            return;
        }
        let extra = self.passenger_bonus_flags(id);
        let actor = &mut self.entities[id];
        let weapon = actor.active_weapon(&self.catalog[actor.type_index]);
        let frames = match weapon.pre_attack_type {
            super::Prefire::Attack if actor.firing.victim == target && actor.firing.shots > 0 => 0,
            super::Prefire::Clip
                if weapon.clip_size > 0
                    && actor.weapon_set.weapons[slot].clip_remaining < weapon.clip_size =>
            {
                0
            }
            _ => super::progression::real_unsigned(
                super::progression::integer_real(weapon.pre_attack_frames)
                    * actor.weapon_factor(weapon, extra, super::containment::bonuses::PRE_ATTACK),
            ),
        };
        if frames > 0 {
            let state = &mut actor.weapon_set.weapons[slot];
            state.status = WeaponStatus::PreAttack;
            state.pre_attack = Some(PreAttack {
                start: u64::from(now),
                frames,
            });
            if weapon.leech_range {
                state.leech_range_active = true;
            }
        }
        actor.helpers.defection.undetected = false;
    }
}
