//! `TransportContain::letRidersUpgradeWeaponSet`, SH 2bfaea60.
use crate::skirmish::{Skirmish, WeaponRules};

impl Skirmish {
    pub(in crate::skirmish::containment) fn update_armed_rider_weapons(&mut self, carrier: usize) {
        if self
            .container_rules(carrier)
            .is_none_or(|rules| !rules.weapons.upgrade_transport)
        {
            return;
        }
        let armed = self.passengers(carrier).iter().any(|&id| {
            let actor = &self.entities[id];
            self.catalog[actor.type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "INFANTRY")
                && (0..3).any(|slot| self.allocated_weapon(id, slot).is_some_and(viable_weapon))
        });
        // set/clearWeaponSetFlag does nothing when the flag is unchanged. In
        // particular, boarding a second armed rider must not refill the clip.
        if self.entities[carrier].weapon_set.upgraded != armed {
            self.entities[carrier].weapon_set.upgraded = armed;
            self.replace_weapon_set(carrier);
        }
    }
}

fn viable_weapon(weapon: &WeaponRules) -> bool {
    // The represented DEPLOY type has a separate flag. Weapon::isDamageWeapon
    // also recognizes positive secondary damage, independent of shot readiness.
    !weapon.contact()
        && (matches!(weapon.damage_type, crate::skirmish::DamageType::Deploy)
            || matches!(weapon.damage_type, crate::skirmish::DamageType::Disarm)
            || weapon.damage > 0.0
            || weapon
                .area
                .as_ref()
                .is_some_and(|area| area.secondary_damage > 0.0))
}

#[cfg(test)]
mod tests;
