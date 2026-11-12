//! SH WeaponSet.cpp1037–1051; Weapon.h697. Authoritative queries mutate status.
use super::WeaponStatus;
use crate::skirmish::{Entity, Skirmish, UnitType};

impl Entity {
    pub(in crate::skirmish) fn native_out_of_ammo(&mut self, unit: &UnitType, now: u32) -> bool {
        for slot in 0..self.weapon_set.weapons.len() {
            if let Some(status) = self.weapon_status(slot, unit, now)
                && !matches!(status, WeaponStatus::OutOfAmmo)
            {
                return false;
            }
        }
        true
    }

    pub(in crate::skirmish) fn native_remaining_ammo(
        &mut self,
        slot: usize,
        unit: &UnitType,
        now: u32,
    ) -> u32 {
        match self.weapon_status(slot, unit, now) {
            None | Some(WeaponStatus::Reloading) => 0,
            Some(_) => self.weapon_set.weapons[slot].clip_remaining,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn native_actor_out_of_ammo(&mut self, id: usize) -> bool {
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        actor.native_out_of_ammo(&self.catalog[actor.type_index], now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::tests::game;

    #[test]
    fn reloading_and_preattack_are_not_out_of_ammo_and_expiry_mutates_ready() {
        let mut game = game();
        let id = game.spawn(6, 0, [100.0, 100.0], true);
        let actor = &mut game.entities[id];
        let unit = &game.catalog[6];
        actor.weapon_set.weapons[0].status = WeaponStatus::Reloading;
        actor.weapon_set.weapons[0].possible_next_shot = 30;
        assert!(!actor.native_out_of_ammo(unit, 29));
        assert_eq!(actor.native_remaining_ammo(0, unit, 29), 0);
        actor.weapon_set.weapons[0].clip_remaining = 2;
        assert_eq!(actor.native_remaining_ammo(0, unit, 30), 2);
        assert!(!actor.native_out_of_ammo(unit, 30));
    }
}
