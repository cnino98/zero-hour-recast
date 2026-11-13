//! `ActiveBody` pilot damage and the ignored-obstacle `PhysicsBehavior` takeover.
use crate::skirmish::{
    DamageType, Skirmish, ai_goals::COMMAND_FROM_PLAYER, combat::attack::Goal, weapons::Lock,
};

impl Skirmish {
    /// Original `Weapon::getPercentReadyToFire`, only while the pilot clip reloads.
    #[must_use]
    pub fn pilot_reload_progress(&mut self, id: usize, slot: usize) -> Option<f32> {
        let weapon = self.allocated_weapon(id, slot)?;
        if !matches!(weapon.damage_type, DamageType::KillPilot) {
            return None;
        }
        self.weapon_reload_progress(id, slot)
    }

    /// `ControlBar` skips the clock and status query for zero effective clip reload.
    pub fn weapon_button_reload_progress(&mut self, id: usize, slot: usize) -> Option<f32> {
        if self.weapon_clip_reload_frames(id, slot)? == 0.0 {
            return None;
        }
        self.weapon_reload_progress(id, slot)
    }

    /// Original `Weapon::getPercentReadyToFire`, restricted to active clip reload.
    #[must_use]
    pub fn weapon_reload_progress(&mut self, id: usize, slot: usize) -> Option<f32> {
        self.allocated_weapon(id, slot)?;
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if !matches!(
            actor.weapon_status(slot, unit, now),
            Some(super::super::ai_dispatch::WeaponStatus::Reloading)
        ) {
            return None;
        }
        let state = &actor.weapon_set.weapons[slot];
        let next = super::super::ai_dispatch::native_frame(state.possible_next_shot);
        let start = super::super::ai_dispatch::native_frame(state.last_reload_started);
        let total = next.wrapping_sub(start);
        if total == 0 {
            return Some(1.0);
        }
        let time_left = next.wrapping_sub(now);
        let elapsed = total.wrapping_sub(time_left);
        if elapsed >= total {
            return Some(1.0);
        }
        Some(
            super::super::progression::integer_real(elapsed)
                / super::super::progression::integer_real(total),
        )
    }

    pub(in crate::skirmish) fn hero_pilot_targetable(&self, id: usize, target: usize) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if actor.weapon_slot() != 0
            || !unit.kind_of.iter().any(|kind| kind == "HERO")
            || !(0..3).any(|slot| {
                self.allocated_weapon(id, slot)
                    .is_some_and(|weapon| matches!(weapon.damage_type, DamageType::KillPilot))
            })
        {
            return true;
        }
        // WeaponSet's original Jarmen cursor fix excludes KILL_PILOT while
        // PRIMARY is selected and no specific slot was requested.
        let victim = &self.entities[target];
        (0..3).any(|slot| {
            self.allocated_weapon(id, slot).is_some_and(|weapon| {
                !matches!(weapon.damage_type, DamageType::KillPilot)
                    && victim.adjust_damage(
                        &self.catalog[victim.type_index],
                        weapon.damage_type,
                        weapon.damage,
                    ) > 0.0
            })
        })
    }

    /// Original control-bar clip status and one-frame flicker guard.
    #[must_use]
    pub fn pilot_weapon_ready(&mut self, id: usize, slot: usize) -> bool {
        let Some(weapon) = self.allocated_weapon(id, slot) else {
            return false;
        };
        let actor = &self.entities[id];
        if !actor.complete()
            || actor.hacked()
            || actor.paralyzed()
            || actor.unmanned()
            || !matches!(weapon.damage_type, DamageType::KillPilot)
        {
            return false;
        }
        self.weapon_button_ready(id, slot)
    }

    /// Original action admission; a landed aircraft vehicle can be sniped.
    #[must_use]
    pub fn can_snipe_vehicle(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(victim)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        let unit = &self.catalog[victim.type_index];
        victim.alive()
            && (unit.combat_bike.is_none() || victim.loaded())
            && !victim.unmanned()
            && !victim.masked()
            && self.enemies(actor.owner, victim.owner)
            && self.maintenance_target_visible(actor.owner, target)
            && unit.kind_of.iter().any(|kind| kind == "VEHICLE")
            && !unit.kind_of.iter().any(|kind| kind == "DRONE")
            && self.victim_anti_mask(target) & 0x02 != 0
    }

    #[must_use]
    pub fn can_fire_pilot_weapon(&self, id: usize, target: usize, slot: usize) -> bool {
        let Some(weapon) = self.allocated_weapon(id, slot) else {
            return false;
        };
        self.can_snipe_vehicle(id, target)
            && matches!(weapon.damage_type, DamageType::KillPilot)
            && self.within_target_pitch(id, target, weapon)
            && weapon.anti_mask & self.victim_anti_mask(target) != 0
            && self.entities[target].adjust_damage(
                &self.catalog[self.entities[target].type_index],
                weapon.damage_type,
                weapon.damage,
            ) > 0.0
    }

    /// Applies the original temporary slot lock before attacking the chosen vehicle.
    /// # Errors
    /// Rejects an unavailable pilot weapon, disabled sniper or invalid visible target.
    pub fn snipe_vehicle(
        &mut self,
        id: usize,
        target: usize,
        slot: usize,
    ) -> Result<(), &'static str> {
        let actor = self.ready_actor(id)?;
        if actor.hacked() || actor.paralyzed() || !self.can_fire_pilot_weapon(id, target, slot) {
            return Err("Choose a visible enemy vehicle with a living pilot");
        }
        // GameLogic::onDoWeaponAtObject locks the requested slot before the
        // attack state's onEnter chooses its weapon and captures its allocation.
        if !matches!(self.entities[id].weapon_set.lock, Lock::Permanent) {
            self.entities[id].weapon_set.current = slot;
            self.entities[id].weapon_set.lock = Lock::Temporary;
        }
        self.native_attack_goal_from(
            id,
            Goal::Object(target),
            i32::MAX.unsigned_abs(),
            COMMAND_FROM_PLAYER,
            false,
        );
        Ok(())
    }

    pub(in crate::skirmish) fn kill_vehicle_pilot(
        &mut self,
        target: usize,
        source: usize,
        amount: f32,
    ) {
        if !self.entities[target].alive() {
            return;
        }
        let unit = &self.catalog[self.entities[target].type_index];
        if unit.combat_bike.is_some() {
            self.bike_kill_pilot(target, source);
        } else if unit.kind_of.iter().any(|kind| kind == "VEHICLE") {
            self.set_unmanned(target);
            self.deselect_dead_object(target);
            if self.entities[target].ai.registration.is_some() {
                self.ai_idle_from_ai(target);
            }
            // ActiveBody changes ownership after setDisabled and its callbacks.
            self.transfer_maintenance_owner(target, self.neutral_owner());
        }
        // Handled pilot damage changes no health and does not invoke OnDamage.
        if amount > 0.0 {
            self.entities[target].damage_state.last_attacker = Some(source);
        }
    }

    pub(in crate::skirmish) fn unmanned_admits(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(host)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        let unit = &self.catalog[actor.type_index];
        id != target
            && actor.complete()
            && !actor.masked()
            && !actor.unmanned()
            && host.complete()
            && host.unmanned()
            && !host.masked()
            && actor.contained_by().is_none()
            && unit.kind_of.iter().any(|kind| kind == "INFANTRY")
            && !unit.kind_of.iter().any(|kind| {
                matches!(
                    kind.as_str(),
                    "REJECT_UNMANNED" | "STRUCTURE" | "IMMOBILE" | "IGNORED_IN_GUI" | "MOB_NEXUS"
                )
            })
            && !self.catalog[host.type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "IGNORED_IN_GUI")
            && self.maintenance_target_visible(actor.owner, target)
    }

    pub(in crate::skirmish) fn unmanned_contact(&mut self, id: usize, target: usize) {
        // The source special case is inside PhysicsBehavior's own ignored-ID branch,
        // before its reciprocal-ignored check. Incidental contacts never re-crew.
        if self.entities[id].ai.ignored_producer != Some(target)
            || !self.entities[target].unmanned()
            || !self.catalog[self.entities[id].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "INFANTRY")
        {
            return;
        }
        self.entities[target].crew.unmanned = false;
        self.observe_spy_vision_disabled(target);
        self.transfer_maintenance_owner(target, self.entities[id].owner);
        self.transfer_crew_name(id, target);
        self.destroy_maintenance_object(id);
    }
}
