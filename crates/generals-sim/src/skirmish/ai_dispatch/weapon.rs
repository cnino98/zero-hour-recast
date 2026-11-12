//! SH Object.cpp1496–1561; Weapon.cpp2795–2806,2854–2869,2938–2956.
use super::native_frame;
use crate::skirmish::{Entity, UnitType};

#[derive(Clone, Copy)]
pub(in crate::skirmish) enum Status {
    PreAttack,
    Ready,
    OutOfAmmo,
    Reloading,
    BetweenShots,
}

impl Entity {
    /// Missing allocated weapon is distinct from any `WeaponStatus`.
    pub(in crate::skirmish) fn weapon_status(
        &mut self,
        slot: usize,
        unit: &UnitType,
        now: u32,
    ) -> Option<Status> {
        let weapon = unit.weapon_template(self.weapon_index(slot))?;
        let state = &mut self.weapon_set.weapons[slot];
        if state
            .pre_attack
            .as_ref()
            .is_some_and(|attack| now < native_frame(attack.start).wrapping_add(attack.frames))
        {
            return Some(Status::PreAttack);
        }
        if now >= native_frame(state.possible_next_shot) {
            state.status = if weapon.clip_size == 0 || state.clip_remaining > 0 {
                Status::Ready
            } else {
                Status::OutOfAmmo
            };
        }
        Some(state.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_status_cannot_resurrect_pre_attack_after_a_ready_query() {
        let mut game = crate::skirmish::tests::game();
        let id = game.spawn(6, 0, [100.0, 100.0], true);
        let actor = &mut game.entities[id];
        let state = &mut actor.weapon_set.weapons[0];
        state.possible_next_shot = 30;
        state.status = Status::PreAttack;
        state.pre_attack = Some(crate::skirmish::firing::PreAttack {
            start: 10,
            frames: 8,
        });
        assert!(matches!(
            actor.weapon_status(0, &game.catalog[6], 20),
            Some(Status::PreAttack)
        ));
        assert!(matches!(
            actor.weapon_status(0, &game.catalog[6], 30),
            Some(Status::Ready)
        ));
        actor.weapon_set.weapons[0].possible_next_shot = 50;
        assert!(matches!(
            actor.weapon_status(0, &game.catalog[6], 31),
            Some(Status::Ready)
        ));
    }
}
