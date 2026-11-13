//! Burton's object weapon and charge commands use the native attack and ability states.
use super::{PackingState, PowerType, Skirmish};
use crate::skirmish::{
    CanAttackResult, DamageType, ai_goals::COMMAND_FROM_PLAYER, combat::attack::Goal, weapons::Lock,
};

const KNIFE: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BurtonCharge {
    Timed,
    Remote,
}

impl BurtonCharge {
    const fn kind(self) -> PowerType {
        match self {
            Self::Timed => PowerType::SpecialTimedCharges,
            Self::Remote => PowerType::SpecialRemoteCharges,
        }
    }

    #[must_use]
    pub const fn power(self) -> &'static str {
        match self {
            Self::Timed => "SpecialAbilityColonelBurtonTimedCharges",
            Self::Remote => "SpecialAbilityColonelBurtonRemoteCharges",
        }
    }
}

impl Skirmish {
    fn burton_actor(&self, id: usize) -> bool {
        self.ready_actor(id).is_ok_and(|actor| {
            self.catalog[actor.type_index].name == "AmericaInfantryColonelBurton"
                && !actor.masked()
                && !self.object_disabled(id)
        })
    }

    pub fn burton_knife_ready(&mut self, id: usize) -> bool {
        self.burton_actor(id)
            && self
                .allocated_weapon(id, KNIFE)
                .is_some_and(|weapon| matches!(weapon.damage_type, DamageType::Melee))
            && self.weapon_button_ready(id, KNIFE)
    }

    pub fn burton_knife_target(&mut self, id: usize, target: usize) -> bool {
        self.burton_knife_ready(id)
            && self
                .entities
                .get(target)
                .is_some_and(crate::skirmish::Entity::alive)
            && !self.action_target_shrouded(id, target, COMMAND_FROM_PLAYER)
            && matches!(
                self.player_weapon_attack_result(
                    id,
                    Some(target),
                    self.object_position(target),
                    false
                ),
                CanAttackResult::Possible | CanAttackResult::PossibleAfterMoving
            )
            && self.estimate_native_weapon_damage(id, KNIFE, target) != 0.0
    }

    /// # Errors
    /// Rejects an unavailable knife or a target the selected weapon cannot attack.
    pub fn burton_order_knife(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        if !self.burton_knife_target(id, target) {
            return Err("Choose a visible target Burton can attack with his knife");
        }
        if !matches!(self.entities[id].weapon_set.lock, Lock::Permanent) {
            self.entities[id].weapon_set.current = KNIFE;
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

    pub fn burton_charge_ready(&mut self, id: usize, charge: BurtonCharge) -> bool {
        if !self.burton_actor(id) {
            return false;
        }
        let Some(ordinal) = self.first_special_ability(id, charge.kind()) else {
            return false;
        };
        let state = &self.entities[id].behaviors.special_abilities[ordinal];
        // SpecialAbilityUpdate::isPowerCurrentlyInUse allows zero-reload commands
        // while packed or packing. These powers both have an authored zero reload.
        let in_use = state.preparing.within_start_range
            && matches!(
                state.packing,
                PackingState::Unpacking | PackingState::Unpacked
            );
        !in_use && self.special_power_ready(id, charge.power())
    }

    pub fn burton_charge_target(&mut self, id: usize, charge: BurtonCharge, target: usize) -> bool {
        self.burton_charge_ready(id, charge)
            && self.entities.get(target).is_some_and(|victim| {
                victim.alive() && !victim.masked() && victim.contained_by().is_none()
            })
            && self.object_relationship(id, target) != 2
            && self.can_special_power_at_object_from(
                id,
                charge.power(),
                charge.kind(),
                target,
                COMMAND_FROM_PLAYER,
            )
    }

    /// # Errors
    /// Rejects unavailable charges, invalid targets and targets Burton has already rigged.
    pub fn burton_order_charge(
        &mut self,
        id: usize,
        charge: BurtonCharge,
        target: usize,
    ) -> Result<(), &'static str> {
        if self.burton_charge_target(id, charge, target)
            && self.special_power_at_object(id, charge.power(), target)
        {
            Ok(())
        } else {
            Err("Choose a visible vehicle or structure Burton can rig with a charge")
        }
    }

    pub fn burton_can_detonate(&mut self, id: usize) -> bool {
        self.burton_actor(id)
            && self
                .first_special_ability(id, BurtonCharge::Remote.kind())
                .is_some_and(|ordinal| {
                    self.entities[id].behaviors.special_abilities[ordinal].object_entries != 0
                })
            && self.special_power_ready(id, BurtonCharge::Remote.power())
    }

    /// # Errors
    /// Rejects detonation when Burton cannot act or has no remote charges.
    pub fn burton_detonate(&mut self, id: usize) -> Result<(), &'static str> {
        if self.burton_can_detonate(id)
            && self.native_special_power(id, BurtonCharge::Remote.power(), None, None, false)
        {
            Ok(())
        } else {
            Err("Burton has no remote charges ready to detonate")
        }
    }
}

#[cfg(test)]
mod tests;
