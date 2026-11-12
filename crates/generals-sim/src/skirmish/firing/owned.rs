//! SH Weapon.cpp: separately allocated module weapons retain real ammo and deadlines.
use super::WeaponState;
use crate::skirmish::{
    ClipReload, WeaponRules,
    ai_dispatch::{WeaponStatus, native_frame},
};

pub(in crate::skirmish) struct ShotCompletion {
    pub reloaded: bool,
    pub shared: Option<(WeaponStatus, u32)>,
}

impl WeaponState {
    pub(in crate::skirmish) fn owned_status(&mut self, now: u32) -> WeaponStatus {
        if self
            .pre_attack
            .as_ref()
            .is_some_and(|attack| now < native_frame(attack.start).wrapping_add(attack.frames))
        {
            return WeaponStatus::PreAttack;
        }
        if now >= native_frame(self.possible_next_shot) {
            self.status = if self.clip_remaining > 0 {
                WeaponStatus::Ready
            } else {
                WeaponStatus::OutOfAmmo
            };
        }
        self.status
    }

    /// Returns whether reloadWithBonus ran, including its shared reload side effect.
    pub(in crate::skirmish) fn reload_owned(
        &mut self,
        rules: &WeaponRules,
        now: u32,
        rate: f32,
        instant: bool,
        shared: bool,
    ) -> bool {
        if rules.clip_size > 0 && self.clip_remaining == rules.clip_size && !shared {
            return false;
        }
        self.clip_remaining = if rules.clip_size == 0 {
            0x7fff_ffff
        } else {
            rules.clip_size
        };
        self.status = WeaponStatus::Reloading;
        let delay = if instant {
            0
        } else {
            crate::skirmish::progression::real_unsigned(
                ((rules.reload_seconds * 30.0).ceil() / rate).floor(),
            )
        };
        self.last_reload_started = u64::from(now);
        self.possible_next_shot = u64::from(now.wrapping_add(delay));
        self.scatter_targets = Some((0..rules.scatter_targets.len()).collect());
        true
    }

    pub(in crate::skirmish) fn owned_barrel(
        &mut self,
        rules: &WeaponRules,
        health: f32,
        max_health: f32,
    ) -> usize {
        let count = rules
            .motion
            .as_ref()
            .map_or(0, |motion| motion.launch_bones(health, max_health).len());
        if self.barrel >= count {
            self.barrel = 0;
            self.barrel_shots = 0;
        }
        self.barrel
    }

    pub(in crate::skirmish) fn owned_scatter(
        &mut self,
        rules: &WeaponRules,
        random: &mut [u32; 6],
    ) -> Option<[f32; 2]> {
        let unused = self.scatter_targets.as_mut()?;
        if unused.is_empty() {
            return None;
        }
        let maximum = u32::try_from(unused.len() - 1).expect("native scatter count");
        let pick = usize::try_from(crate::skirmish::startup::random_between(random, maximum))
            .expect("native scatter choice");
        Some(
            rules.scatter_targets[unused.swap_remove(pick)]
                .map(|axis| axis * rules.scatter_target_scalar),
        )
    }

    /// Native calls this after `WeaponTemplate` payload; reentrant damage sees old ammo/status.
    /// Returns the shared status/deadline only on native propagation branches.
    pub(in crate::skirmish) fn finish_owned_shot(
        &mut self,
        rules: &WeaponRules,
        now: u32,
        shot_rate: f32,
        reload_rate: f32,
        shared: bool,
    ) -> ShotCompletion {
        if matches!(rules.damage_type, crate::skirmish::DamageType::Disarm) {
            self.clip_remaining = self.clip_remaining.wrapping_sub(1);
            if let Some(count) = &mut self.max_shots {
                *count = count.wrapping_sub(1);
            }
            if self.clip_remaining == 0 && matches!(rules.reload, ClipReload::Automatic) {
                self.reload_owned(rules, now, reload_rate, false, shared);
                return ShotCompletion {
                    reloaded: true,
                    shared: shared.then_some((self.status, native_frame(self.possible_next_shot))),
                };
            }
            return ShotCompletion {
                reloaded: false,
                shared: None,
            };
        }
        self.last_shot = Some(u64::from(now));
        self.clip_remaining = self.clip_remaining.wrapping_sub(1);
        if let Some(count) = &mut self.max_shots {
            *count = count.wrapping_sub(1);
        }
        let shots = rules
            .motion
            .as_ref()
            .map_or(1, |motion| motion.shots_per_barrel);
        self.barrel_shots = self.barrel_shots.wrapping_add(1);
        if self.barrel_shots >= shots {
            self.barrel = self.barrel.wrapping_add(1);
            self.barrel_shots = 0;
        }
        if self.clip_remaining == 0 {
            if matches!(rules.reload, ClipReload::Automatic) {
                self.reload_owned(rules, now, reload_rate, false, shared);
                return ShotCompletion {
                    reloaded: true,
                    shared: shared.then_some((self.status, native_frame(self.possible_next_shot))),
                };
            }
            self.status = WeaponStatus::OutOfAmmo;
            self.possible_next_shot = 0x7fff_ffff;
            return ShotCompletion {
                reloaded: false,
                shared: None,
            };
        }
        let delay = crate::skirmish::progression::real_unsigned(
            ((rules.delay * 30.0).ceil() / shot_rate).floor(),
        );
        self.status = WeaponStatus::BetweenShots;
        self.last_reload_started = u64::from(now);
        self.possible_next_shot = u64::from(now.wrapping_add(delay));
        ShotCompletion {
            reloaded: false,
            shared: shared.then_some((self.status, native_frame(self.possible_next_shot))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_finite_clip_exhaustion_uses_reload_deadline_and_infinite_clip_still_decrements() {
        let mut rules = WeaponRules::unarmed();
        rules.clip_size = 2;
        rules.delay = 0.1;
        rules.reload_seconds = 0.5;
        rules.reload = ClipReload::Automatic;
        let mut state = WeaponState::new(0);
        state.clip_remaining = 0;
        assert!(state.reload_owned(&rules, 10, 1.0, true, false));
        assert!(matches!(state.owned_status(10), WeaponStatus::Ready));
        assert!(
            state
                .finish_owned_shot(&rules, 10, 1.0, 1.0, false)
                .shared
                .is_none()
        );
        assert!(matches!(state.owned_status(12), WeaponStatus::BetweenShots));
        assert!(matches!(state.owned_status(13), WeaponStatus::Ready));
        assert_eq!(
            state
                .finish_owned_shot(&rules, 13, 1.0, 1.0, true)
                .shared
                .map(|(_, deadline)| deadline),
            Some(28)
        );
        assert_eq!(state.clip_remaining, 2);
        assert!(matches!(state.owned_status(27), WeaponStatus::Reloading));
        assert!(matches!(state.owned_status(28), WeaponStatus::Ready));
        rules.clip_size = 0;
        state.reload_owned(&rules, 28, 1.0, true, false);
        state.finish_owned_shot(&rules, 28, 1.0, 1.0, false);
        assert_eq!(state.clip_remaining, 0x7fff_fffe);
    }

    #[test]
    fn zero_delay_and_u32_wrapping_deadlines_are_not_clamped_to_one() {
        let mut rules = WeaponRules::unarmed();
        rules.delay = 0.0;
        let mut state = WeaponState::new(0);
        state.finish_owned_shot(&rules, u32::MAX, 1.0, 1.0, false);
        assert_eq!(state.possible_next_shot, u64::from(u32::MAX));
        assert!(matches!(state.owned_status(u32::MAX), WeaponStatus::Ready));
    }
}

#[cfg(test)]
mod payload_tests {
    use super::*;

    #[test]
    fn disarm_null_target_spends_ammo_without_changing_lastfire_barrel_or_delay() {
        let mut rules = WeaponRules::unarmed();
        rules.damage_type = crate::skirmish::DamageType::Disarm;
        rules.clip_size = 2;
        rules.delay = 1.0;
        let mut state = WeaponState::new(0);
        state.clip_remaining = 0;
        state.reload_owned(&rules, 40, 1.0, true, false);
        state.last_shot = Some(30);
        state.barrel = 2;
        state.finish_owned_shot(&rules, 40, 1.0, 1.0, false);
        assert_eq!(state.clip_remaining, 1);
        assert_eq!(state.last_shot, Some(30));
        assert_eq!(state.barrel, 2);
        assert_eq!(state.possible_next_shot, 40);
        assert!(matches!(state.owned_status(40), WeaponStatus::Ready));
    }
}

#[cfg(test)]
mod captured_bonus_tests {
    use super::*;

    #[test]
    fn between_shots_uses_pre_payload_bonus_but_reload_recomputes_after_payload() {
        let mut rules = WeaponRules::unarmed();
        rules.clip_size = 2;
        rules.delay = 1.0;
        rules.reload_seconds = 2.0;
        let mut state = WeaponState::new(0);
        state.clip_remaining = 0;
        state.reload_owned(&rules, 100, 1.0, true, false);
        state.finish_owned_shot(&rules, 100, 1.0, 2.0, false);
        assert_eq!(state.possible_next_shot, 130);
        state.finish_owned_shot(&rules, 130, 1.0, 2.0, false);
        assert_eq!(state.possible_next_shot, 160);
        assert_eq!(state.clip_remaining, 2);
        assert!(matches!(state.status, WeaponStatus::Reloading));
    }
}

#[cfg(test)]
mod runtime_scalar_tests {
    use super::*;

    #[test]
    fn authored_scalar_is_applied_at_fire_time_in_each_real_instance() {
        let mut rules = WeaponRules::unarmed();
        rules.scatter_targets = vec![[0.25, -0.5]];
        rules.scatter_target_scalar = 40.0;
        let mut state = WeaponState::new(0);
        state.reload_owned(&rules, 1, 1.0, true, false);
        let mut random = [7; 6];
        assert_eq!(
            state.owned_scatter(&rules, &mut random),
            Some([10.0, -20.0])
        );
        assert_eq!(random, [7; 6]);
        assert_eq!(state.owned_scatter(&rules, &mut random), None);
        state.reload_owned(&rules, 2, 1.0, true, false);
        rules.scatter_target_scalar = 0.0;
        assert_eq!(state.owned_scatter(&rules, &mut random), Some([0.0, -0.0]));
    }
}
