//! Missile Defender command admission for native `SpecialAbilityUpdate`.
use super::{Skirmish, special_ability::PowerType};

impl Skirmish {
    #[must_use]
    pub fn can_laser_lock(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(victim)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        actor.complete()
            && !actor.paralyzed()
            && !actor.hacked()
            && actor.contained_by().is_none()
            && victim.alive()
            && !victim.masked()
            && self
                .first_special_ability(id, PowerType::SpecialMissileDefenderLaserGuidedMissiles)
                .is_some()
            && !self.catalog[victim.type_index].kind.is_building()
            && !self.allied(actor.owner, victim.owner)
            && self.maintenance_target_visible(actor.owner, target)
    }

    /// # Errors
    /// Rejects unavailable abilities and invalid building, allied or hidden targets.
    /// # Panics
    /// Requires the admitted laser ability to retain its authored power binding.
    pub fn laser_lock(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_laser_lock(id, target) {
            return Err("Invalid laser-lock target");
        }
        let ordinal = self
            .first_special_ability(id, PowerType::SpecialMissileDefenderLaserGuidedMissiles)
            .expect("admitted laser ability");
        let name = self.catalog[self.entities[id].type_index]
            .behavior
            .special_abilities[ordinal]
            .power
            .as_ref()
            .expect("laser power")
            .name
            .clone();
        if !self.special_power_at_object(id, &name, target) {
            return Err("Laser-lock power is paused or disabled");
        }
        Ok(())
    }
}
