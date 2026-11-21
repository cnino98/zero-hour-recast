//! SH 2bfaea60 Object helpers: callback state and actual object effects.
use super::{
    Entity, Skirmish, TerrainDecal,
    module_updates::{FOREVER, Helper, ModuleRegistration},
};
use num_traits::ToPrimitive;

pub(super) struct State {
    pub(super) gaining_subdual: bool,
    pub(super) special: SpecialState,
    pub(super) status_damage: StatusDamage,
    pub(super) temporary: TemporaryBonus,
    pub(super) defection: Defection,
}

pub(super) struct SpecialState {
    pub(super) cheering: bool,
    pub(super) until: u32,
}

pub(super) struct StatusDamage {
    pub(super) painted: bool,
    to_heal: bool,
    pub(super) until: u32,
}

pub(super) struct TemporaryBonus {
    pub(super) bonus: Option<u8>,
    pub(super) until: u32,
    pub(super) flags: u32,
    pub(super) frenzy_tint: bool,
}

pub(super) struct Defection {
    pub(super) undetected: bool,
    start: u32,
    end: u32,
    flash_phase: f32,
    fx: bool,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            gaining_subdual: false,
            special: SpecialState {
                cheering: false,
                until: 0,
            },
            status_damage: StatusDamage {
                painted: false,
                to_heal: false,
                until: 0,
            },
            temporary: TemporaryBonus {
                bonus: None,
                until: 0,
                flags: 0,
                frenzy_tint: false,
            },
            defection: Defection {
                undetected: false,
                start: 0,
                end: 0,
                flash_phase: 0.0,
                fx: false,
            },
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn shadows_enabled(&self) -> bool {
        self.drawable.status & 2 != 0
    }

    #[must_use]
    pub const fn terrain_decal_none(&self) -> bool {
        matches!(self.drawable.terrain_decal, TerrainDecal::None)
    }

    #[must_use]
    pub const fn terrain_decal(&self) -> &TerrainDecal {
        &self.drawable.terrain_decal
    }

    #[must_use]
    pub const fn terrain_decal_fade(&self) -> Option<[f32; 2]> {
        self.drawable.decal_fade_request
    }

    #[must_use]
    pub const fn gaining_subdual_damage(&self) -> bool {
        self.helpers.gaining_subdual
    }

    #[must_use]
    pub const fn has_drawable(&self) -> bool {
        self.drawable.present
    }

    /// Creation order includes replacement drawables on an existing object.
    #[must_use]
    pub const fn drawable_birth(&self) -> u64 {
        self.drawable.birth
    }
    #[must_use]
    pub const fn is_effectively_dead(&self) -> bool {
        self.private_status & 1 != 0
    }

    pub(in crate::skirmish) const fn set_effectively_dead(&mut self, dead: bool) {
        if dead {
            self.private_status |= 1;
        } else {
            self.private_status &= !1;
        }
    }

    pub(in crate::skirmish) const fn is_off_map(&self) -> bool {
        self.private_status & (1 << 3) != 0
    }

    pub(in crate::skirmish) const fn set_off_map(&mut self, off_map: bool) {
        if off_map {
            self.private_status |= 1 << 3;
        } else {
            self.private_status &= !(1 << 3);
        }
    }

    pub(in crate::skirmish) const fn is_airborne_target(&self) -> bool {
        self.object_status & (1 << 7) != 0
    }

    pub(in crate::skirmish) const fn set_airborne_target(&mut self, airborne: bool) {
        if airborne {
            self.object_status |= 1 << 7;
        } else {
            self.object_status &= !(1 << 7);
        }
    }

    pub(in crate::skirmish) const fn has_native_disguise(&self) -> bool {
        self.object_status & (1 << 43) != 0
    }

    #[must_use]
    pub const fn frenzy_tint(&self) -> bool {
        self.helpers.temporary.frenzy_tint
    }
}

impl Skirmish {
    // SlowDeathBehavior.cpp214–230; only infantry with a nonzero sink rate call this.
    pub(in crate::skirmish) fn set_slow_death_shadow(&mut self, id: usize) {
        let drawable = &mut self.entities[id].drawable;
        if drawable.present {
            drawable.status &= !2;
            drawable.decal_fade_request = Some([0.0, -0.2]);
        }
    }

    // ActiveBody::attemptDamage after FX, followed by Object::setStatus.
    pub(in crate::skirmish) fn repulse_after_damage(&mut self, id: usize) {
        if self.ai_rules.repulsors_enabled
            && self.attack_kind(id, "CAN_BE_REPULSED")
            && self.entities[id].object_status & (1 << 9) == 0
        {
            self.entities[id].object_status |= 1 << 9;
            self.wake_helper(id, Helper::Repulsor, 60);
        }
    }

    pub(in crate::skirmish) fn has_builtin_helper(&self, id: usize, helper: Helper) -> bool {
        let actor = &self.entities[id];
        self.catalog[actor.type_index]
            .update_modules
            .iter()
            .enumerate()
            .any(|(index, entry)| {
                index < self.update_objects[actor.update_owner].handles.len()
                    && matches!(entry, ModuleRegistration::Helper(found) if *found == helper)
            })
    }

    pub(in crate::skirmish) fn construct_builtin_helper(&mut self, id: usize, helper: Helper) {
        let state = &mut self.entities[id].helpers;
        match helper {
            Helper::StatusDamage => {
                state.status_damage.to_heal = false;
                state.status_damage.until = 0;
            }
            Helper::SubdualDamage => self.entities[id].maintenance.microwave.heal_countdown = 0,
            Helper::Defection => {
                state.defection.start = 0;
                state.defection.end = 0;
                state.defection.flash_phase = 0.0;
                state.defection.fx = false;
            }
            Helper::TemporaryWeaponBonus => {
                state.temporary.bonus = None;
                state.temporary.until = 0;
            }
            // These helpers have no additional native constructor state. Object
            // fields and root's firing state are initialized in Entity::new.
            Helper::StateMachine
            | Helper::Repulsor
            | Helper::WeaponStatus
            | Helper::FiringTracker => {}
        }
    }

    pub(in crate::skirmish) fn update_builtin_helper(&mut self, id: usize, helper: Helper) -> i32 {
        match helper {
            Helper::StateMachine => {
                self.entities[id].helpers.special.cheering = false;
                self.entities[id].helpers.special.until = 0;
                0x3fff_ffff
            }
            Helper::Repulsor => {
                self.entities[id].object_status &= !(1 << 9);
                0x3fff_ffff
            }
            Helper::StatusDamage => {
                // Native debug assertion; readiness otherwise comes from sleeping UpdateModule.
                debug_assert!(self.entities[id].helpers.status_damage.until <= self.update_frame());
                let state = &mut self.entities[id].helpers;
                if state.status_damage.to_heal {
                    state.status_damage.painted = false;
                    state.status_damage.to_heal = false;
                    state.status_damage.until = 0;
                }
                0x3fff_ffff
            }
            Helper::SubdualDamage => self.update_subdual_damage_helper(id),
            Helper::Defection => self.update_defection_helper(id),
            Helper::TemporaryWeaponBonus => {
                debug_assert!(self.entities[id].helpers.temporary.until <= self.update_frame());
                self.clear_temporary_weapon_bonus(id);
                0x3fff_ffff
            }
            Helper::WeaponStatus => {
                self.update_weapon_status(id);
                1
            }
            Helper::FiringTracker => self.update_firing_tracker(id),
        }
    }

    pub(in crate::skirmish) fn destroy_builtin_helper(&mut self, id: usize, helper: Helper) {
        if helper == Helper::FiringTracker {
            self.stop_firing_sound(id);
        }
        // The other seven original destructors have no side effects.
    }

    pub(in crate::skirmish) fn do_painted_status_damage(&mut self, id: usize, duration: f32) {
        if !self.has_builtin_helper(id, Helper::StatusDamage) {
            return;
        }
        let signed = duration
            .floor()
            .to_i32()
            .expect("represented native signed status duration");
        let delay = u32::from_ne_bytes(signed.to_ne_bytes());
        let deadline = self.update_frame().wrapping_add(delay);
        let state = &mut self.entities[id].helpers;
        state.status_damage.painted = true;
        state.status_damage.to_heal = true;
        state.status_damage.until = deadline;
        self.wake_helper(id, Helper::StatusDamage, delay);
    }

    pub(in crate::skirmish) fn do_temp_weapon_bonus(
        &mut self,
        id: usize,
        bonus: u8,
        duration: u32,
    ) {
        if !self.has_builtin_helper(id, Helper::TemporaryWeaponBonus) {
            return;
        }
        if self.entities[id].helpers.temporary.bonus != Some(bonus) {
            self.clear_temporary_weapon_bonus(id);
        }
        let flag = 1_u32 << bonus;
        if self.entities[id].helpers.temporary.flags & flag == 0 {
            let extra = self.passenger_bonus_flags(id);
            let actor = &mut self.entities[id];
            actor.helpers.temporary.flags |= flag;
            actor.weapon_bonus_changed(
                &self.catalog[actor.type_index],
                self.tick,
                extra,
                &mut self.random,
            );
        }
        let deadline = self.update_frame().wrapping_add(duration);
        let actor = &mut self.entities[id];
        actor.helpers.temporary.bonus = Some(bonus);
        actor.helpers.temporary.until = deadline;
        if actor.drawable.present {
            actor.helpers.temporary.frenzy_tint = true;
        }
        self.wake_helper(id, Helper::TemporaryWeaponBonus, duration);
    }

    fn clear_temporary_weapon_bonus(&mut self, id: usize) {
        let Some(bonus) = self.entities[id].helpers.temporary.bonus else {
            return;
        };
        let old = self.entities[id].helpers.temporary.flags;
        self.entities[id].helpers.temporary.flags &= !(1_u32 << bonus);
        if old != self.entities[id].helpers.temporary.flags {
            let extra = self.passenger_bonus_flags(id);
            let actor = &mut self.entities[id];
            actor.weapon_bonus_changed(
                &self.catalog[actor.type_index],
                self.tick,
                extra,
                &mut self.random,
            );
        }
        let actor = &mut self.entities[id];
        let state = &mut actor.helpers;
        state.temporary.bonus = None;
        state.temporary.until = 0;
        if actor.drawable.present {
            state.temporary.frenzy_tint = false;
        }
    }

    pub(in crate::skirmish) fn start_defection_timer(
        &mut self,
        id: usize,
        duration: u32,
        with_fx: bool,
    ) {
        if !self.has_builtin_helper(id, Helper::Defection) {
            return;
        }
        if !self.entities[id].helpers.defection.undetected {
            self.wake_helper(id, Helper::Defection, FOREVER);
            return;
        }
        let now = self.update_frame();
        let state = &mut self.entities[id].helpers;
        state.defection.start = now;
        state.defection.end = now.wrapping_add(duration);
        state.defection.flash_phase = 0.0;
        state.defection.fx = with_fx;
        self.wake_helper(id, Helper::Defection, 1);
    }

    fn update_defection_helper(&mut self, id: usize) -> i32 {
        let now = self.update_frame();
        let state = &self.entities[id].helpers;
        if !state.defection.undetected {
            return 0x3fff_ffff;
        }
        // Native expiry precedes effectively-dead / IS_FIRING_WEAPON exposure.
        if now >= state.defection.end {
            let fx = self.entities[id].drawable.present && state.defection.fx;
            self.entities[id].helpers.defection.undetected = false;
            if fx {
                self.capture_event(id, id, super::CaptureEffect::Detected);
            }
            return 0x3fff_ffff;
        }
        if self.entities[id].is_effectively_dead() || self.entities[id].is_firing_weapon() {
            self.entities[id].helpers.defection.undetected = false;
            return 0x3fff_ffff;
        }
        let actor = &mut self.entities[id];
        let state = &mut actor.helpers;
        if actor.drawable.present && state.defection.fx {
            let last = state
                .defection
                .flash_phase
                .to_i32()
                .expect("native defection flash phase")
                & 1
                != 0;
            let left = state
                .defection
                .end
                .wrapping_sub(now)
                .to_f32()
                .expect("native time left");
            let phase_change = 0.5 * (1.0 - left / 300.0);
            state.defection.flash_phase += phase_change;
            let next = state
                .defection
                .flash_phase
                .to_i32()
                .expect("native defection flash phase")
                & 1
                != 0;
            if last && !next {
                self.capture_event(id, id, super::CaptureEffect::DefectorTick);
            }
        }
        1
    }
}

#[cfg(test)]
mod tests;
