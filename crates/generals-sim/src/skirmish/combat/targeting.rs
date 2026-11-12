//! SH `Object::isAbleToAttack` and `WeaponSet` target admission, 2bfaea60 / CRC=0.
use crate::skirmish::{
    DamageType, DeathType, Skirmish, UnitAiUpdate,
    ai_dispatch::{WeaponStatus, native_frame},
    ai_goals::COMMAND_FROM_PLAYER,
    weapons::{ClipReload, Lock},
};

#[cfg(test)]
mod tests;

pub enum Result {
    NotPossible,
    InvalidShot,
    Possible,
    PossibleAfterMoving,
}

impl Skirmish {
    pub(in crate::skirmish) fn object_can_attack(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let kind = |name: &str| unit.kind_of.iter().any(|kind| kind == name);
        if actor.object_status & (1 << 6) != 0
            || actor.contained_by().is_some() && !self.passenger_can_fire(id)
            || actor.under_construction()
            || actor.selling()
            || actor.subdued()
        {
            return false;
        }
        if kind("PORTABLE_STRUCTURE") || kind("SPAWNS_ARE_THE_WEAPONS") {
            if actor.hacked() || self.object_disabled_mask(id) & (1 << 2) != 0 {
                return false;
            }
            if kind("INFANTRY")
                && self.slaved_parent(id).is_some_and(|parent| {
                    self.entities[parent].exists() && self.entities[parent].subdued()
                })
            {
                return false;
            }
        }
        let ai = !matches!(unit.ai.update, UnitAiUpdate::Absent);
        let mut any_weapon = false;
        let mut enabled = false;
        for slot in 0..3 {
            if self.allocated_weapon(id, slot).is_none() {
                continue;
            }
            any_weapon = true;
            let turret_enabled = if unit
                .turret
                .as_ref()
                .is_some_and(|rules| rules.controlled_slots & (1 << slot) != 0)
            {
                actor.ai.turret.enabled
            } else if unit
                .secondary_turret
                .as_ref()
                .is_some_and(|rules| rules.controlled_slots & (1 << slot) != 0)
            {
                actor
                    .ai
                    .secondary_turret
                    .as_ref()
                    .is_some_and(|turret| turret.enabled)
            } else {
                true
            };
            enabled |= turret_enabled;
        }
        if ai && !kind("CAN_ATTACK") && any_weapon && !enabled {
            return false;
        }
        kind("CAN_ATTACK")
            || actor.object_status & (1 << 2) != 0
            || self.container_fire_allowed(id, Some(id)) && !self.passengers(id).is_empty()
            || ai && any_weapon
            || self
                .spawned_children(id)
                .iter()
                .any(|&child| self.entities[child].exists() && self.object_can_attack(child))
            || unit.enter_guard
    }

    pub(in crate::skirmish) fn attack_target_result(
        &mut self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
        forced: bool,
        _continued: bool,
        specific_slot: Option<usize>,
    ) -> Result {
        if let Some(target) = target {
            if !self.native_target_legal(id, target, forced) {
                return Result::NotPossible;
            }
            self.weapon_target_result(
                id,
                Some(target),
                self.object_position(target),
                specific_slot,
            )
        } else {
            self.weapon_target_result(id, None, position, specific_slot)
        }
    }

    pub(in crate::skirmish) fn player_weapon_attack_result(
        &mut self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
        forced: bool,
    ) -> Result {
        self.native_weapon_attack_result(id, target, position, forced, COMMAND_FROM_PLAYER)
    }

    pub(in crate::skirmish) fn native_weapon_attack_result(
        &mut self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
        forced: bool,
        source: i32,
    ) -> Result {
        if target.is_some_and(|target| !self.native_target_legal_from(id, target, forced, source)) {
            return Result::NotPossible;
        }
        self.weapon_target_result(id, target, position, None)
    }

    fn native_target_legal(&self, id: usize, target: usize, forced: bool) -> bool {
        self.native_target_legal_from(
            id,
            target,
            forced,
            self.entities[id].ai.command.last_command_source,
        )
    }

    fn native_target_legal_from(
        &self,
        id: usize,
        target: usize,
        forced: bool,
        source: i32,
    ) -> bool {
        let (Some(actor), Some(victim)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        if id == target
            || actor.is_effectively_dead()
            || victim.is_effectively_dead()
            || !actor.exists()
            || !victim.exists()
        {
            return false;
        }
        let unit = &self.catalog[victim.type_index];
        let kind = |name: &str| unit.kind_of.iter().any(|kind| kind == name);
        if victim.masked()
            || kind("UNATTACKABLE")
            || victim.object_status & (1 << 26) != 0
                && source == crate::skirmish::ai_goals::COMMAND_FROM_AI
        {
            return false;
        }
        let ignore_stealth = actor.ai.attack_context.ignoring_stealth
            || actor.object_status & (1 << 27) != 0
            || forced
                && (actor.owner == victim.owner
                    || kind("DISGUISER") && victim.has_native_disguise());
        if !ignore_stealth && victim.stealthed() && !victim.detected() {
            if !kind("DISGUISER") {
                return false;
            }
            if let Some(owner) = victim.disguised_player()
                && !self.enemies(actor.owner, owner)
            {
                return false;
            }
        }
        let relation = self.object_relationship(id, target);
        if source == COMMAND_FROM_PLAYER && !forced {
            if relation == 2 {
                return false;
            }
            if relation != 0 && !kind("MINE") && !victim.script_targetable() {
                return false;
            }
        }
        if victim.contained_by().is_some()
            && !victim.stationed()
            && victim.attachment_parent().is_none()
            && !victim.parachute_rider()
        {
            return false;
        }
        if !forced
            && let Some(owner) = self.container_apparent_owner(target, actor.owner)
            && self.object_player_relationship(id, owner) != 0
            && source == COMMAND_FROM_PLAYER
            && (!victim.script_targetable() || relation == 2)
        {
            return false;
        }
        true
    }

    fn weapon_target_result(
        &mut self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
        specific_slot: Option<usize>,
    ) -> Result {
        let mask = target.map_or(0x02, |target| self.victim_anti_mask(target));
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let combined = (0..3)
            .filter_map(|slot| self.allocated_weapon(id, slot))
            .fold(0, |mask, weapon| mask | weapon.anti_mask);
        let current = self.allocated_weapon(id, actor.weapon_slot());
        let in_range =
            self.current_weapon_target_in_range(id, target, position, combined & mask != 0);
        let immobile = unit.kind_of.iter().any(|kind| kind == "IMMOBILE");
        let spawn_weapons = unit
            .kind_of
            .iter()
            .any(|kind| kind == "SPAWNS_ARE_THE_WEAPONS");
        if (immobile || spawn_weapons || actor.contained_by().is_some())
            && current.is_some()
            && !in_range
        {
            return Result::InvalidShot;
        }
        let has_damage = (0..3)
            .filter_map(|slot| self.allocated_weapon(id, slot))
            .any(|weapon| {
                matches!(weapon.damage_type, crate::skirmish::DamageType::Deploy)
                    || matches!(weapon.damage_type, DamageType::Disarm)
                    || !matches!(weapon.damage_type, DamageType::Hack)
                        && (weapon.damage > 0.0
                            || weapon
                                .area
                                .as_ref()
                                .is_some_and(|area| area.secondary_damage > 0.0))
            });
        if has_damage {
            if combined & mask == 0 {
                return Result::InvalidShot;
            }
            let Some(target) = target else {
                return if in_range {
                    Result::Possible
                } else {
                    Result::PossibleAfterMoving
                };
            };
            if !self.any_weapon_within_target_pitch(id, target) {
                return Result::InvalidShot;
            }
            let selected = if matches!(actor.weapon_set.lock, Lock::None) {
                specific_slot
            } else {
                Some(actor.weapon_slot())
            };
            let hero = unit.kind_of.iter().any(|kind| kind == "HERO");
            let primary = actor.weapon_slot() == 0;
            for slot in (0..3).rev() {
                if selected.is_some_and(|selected| slot != selected) {
                    continue;
                }
                let pilot = self
                    .allocated_weapon(id, slot)
                    .is_some_and(|weapon| matches!(weapon.damage_type, DamageType::KillPilot));
                if self.estimate_native_weapon_damage(id, slot, target) != 0.0
                    && !(pilot && hero && primary && specific_slot.is_none())
                {
                    return if in_range {
                        Result::Possible
                    } else {
                        Result::PossibleAfterMoving
                    };
                }
            }
        }
        for rider in self.passengers(id).to_vec() {
            if self.container_fire_allowed(id, None) && self.object_can_attack(rider) {
                let result = self.weapon_target_result(rider, target, position, None);
                if matches!(result, Result::Possible | Result::PossibleAfterMoving) {
                    return result;
                }
            }
        }
        for child in self.spawned_children(id).to_vec() {
            if !self.entities[child].exists() {
                continue;
            }
            match self.weapon_target_result(child, target, position, None) {
                Result::Possible => {
                    return if in_range || immobile && spawn_weapons {
                        Result::Possible
                    } else {
                        Result::PossibleAfterMoving
                    };
                }
                // SpawnBehavior returns the first possible result. The enclosing
                // WeaponSet only accepts POSSIBLE, even when later children could fire.
                Result::PossibleAfterMoving => break,
                Result::InvalidShot | Result::NotPossible => {}
            }
        }
        Result::InvalidShot
    }

    // The native loop checks the current slot twice, not each allocated slot.
    fn current_weapon_target_in_range(
        &self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
        targets_kind: bool,
    ) -> bool {
        let actor = &self.entities[id];
        let current = self.allocated_weapon(id, actor.weapon_slot());
        current.is_some_and(|weapon| {
            targets_kind
                && target.map_or_else(
                    || {
                        let origin = if actor.garrisoned() && !actor.stationed() {
                            let Some(origin) = self.garrison_attack_position_at(id, position)
                            else {
                                return false;
                            };
                            [origin[0], origin[1]]
                        } else {
                            actor.position
                        };
                        let separation = super::circle_distance_squared(
                            origin,
                            [position[0], position[1]],
                            self.object_radius(id),
                        );
                        let range = actor.weapon_range_for(weapon, self.passenger_bonus_flags(id));
                        separation >= weapon.min_range * weapon.min_range
                            && separation <= range * range
                    },
                    |target| {
                        if actor.garrisoned() && !actor.stationed() {
                            self.garrison_attack_position(id, target)
                                .is_some_and(|point| {
                                    self.attack_range_for(id, target, [point[0], point[1]], weapon)
                                })
                        } else {
                            self.attack_range_for(id, target, actor.position, weapon)
                        }
                    },
                )
        })
    }

    pub(in crate::skirmish) fn estimate_native_weapon_damage(
        &mut self,
        id: usize,
        slot: usize,
        target: usize,
    ) -> f32 {
        if self.allocated_weapon(id, slot).is_none() {
            return 0.0;
        }
        let now = native_frame(self.tick);
        let type_index = self.entities[id].type_index;
        let status = self.entities[id].weapon_status(slot, &self.catalog[type_index], now);
        let actor = &self.entities[id];
        let weapon = self.allocated_weapon(id, slot).expect("allocated weapon");
        if matches!(status, Some(WeaponStatus::OutOfAmmo))
            && !matches!(weapon.reload, ClipReload::Automatic)
        {
            return 0.0;
        }
        let victim = &self.entities[target];
        let unit = &self.catalog[victim.type_index];
        let kind = |name: &str| unit.kind_of.iter().any(|kind| kind == name);
        let amount = weapon.damage * actor.weapon_damage_bonus(weapon, 0);
        if kind("SHRUBBERY") {
            return f32::from(u8::from(matches!(weapon.death_type, DeathType::Burned)));
        }
        if kind("STRUCTURE")
            && matches!(weapon.damage_type, DamageType::Sniper)
            && self.passengers(target).is_empty()
            && self.spawned_children(target).is_empty()
        {
            return 0.0;
        }
        if (matches!(weapon.damage_type, DamageType::Surrender) || weapon.allow_attack_garrisoned)
            && self.clearable_garrison(target)
        {
            return 1.0;
        }
        if matches!(weapon.damage_type, DamageType::Disarm) {
            return f32::from(u8::from(
                kind("MINE") || kind("BOOBY_TRAP") || kind("DEMOTRAP"),
            ));
        }
        if matches!(weapon.damage_type, crate::skirmish::DamageType::Deploy)
            && !victim.is_airborne_target()
        {
            return 1.0;
        }
        if !unit.body_kind.is_active() {
            return 0.0;
        }
        if matches!(
            weapon.damage_type,
            DamageType::SubdualBuilding
                | DamageType::SubdualVehicle
                | DamageType::SubdualMissile
                | DamageType::SubdualUnresistable
        ) && unit.maintenance.subdual.is_none()
        {
            return 0.0;
        }
        if matches!(weapon.damage_type, DamageType::KillGarrisoned) {
            return f32::from(u8::from(self.clearable_garrison(target)));
        }
        if matches!(weapon.damage_type, DamageType::Sniper)
            && kind("STRUCTURE")
            && victim.under_construction()
        {
            return 0.0;
        }
        if matches!(
            weapon.damage_type,
            DamageType::Unresistable | DamageType::SubdualUnresistable
        ) {
            return amount;
        }
        // estimateDamage does not apply ActiveBody's damage scalar.
        (amount * victim.active_armor(unit)[weapon.damage_type as usize]).max(0.0)
    }
}
