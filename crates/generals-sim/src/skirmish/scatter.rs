//! `Weapon::rebuildScatterTargets` and the native random swap/pop shot sequence.
use super::{Skirmish, startup};

impl Skirmish {
    pub(in crate::skirmish) fn scatter_weapon_target(
        &mut self,
        id: usize,
        slot: usize,
        mut position: [f32; 3],
    ) -> Option<[f32; 3]> {
        let actor = &mut self.entities[id];
        let weapon = self.catalog[actor.type_index].weapon_template(actor.weapon_index(slot))?;
        if weapon.scatter_targets.is_empty() {
            return None;
        }
        let unused = actor.weapon_set.weapons[slot]
            .scatter_targets
            .get_or_insert_with(|| (0..weapon.scatter_targets.len()).collect());
        if unused.is_empty() {
            return None;
        }
        let maximum = u32::try_from(unused.len() - 1).expect("native scatter target count");
        let pick = usize::try_from(startup::random_between(&mut self.random, maximum))
            .expect("scatter target index");
        let index = unused.swap_remove(pick);
        let offset = weapon.scatter_targets[index].map(|axis| axis * weapon.scatter_target_scalar);
        position[0] += offset[0];
        position[1] += offset[1];
        position[2] = self.ground_height([position[0], position[1]]);
        Some(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{ClipReload, Kind, tests::game};

    #[test]
    fn native_pattern_uses_each_entry_once_then_center_and_rebuilds_only_on_reload() {
        let mut game = game();
        let index = game.type_for(0, Kind::Tank).unwrap();
        let weapon = &mut game.catalog[index].weapon;
        weapon.scatter_targets = vec![[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]];
        weapon.scatter_target_scalar = 1.0;
        weapon.clip_size = 4;
        weapon.reload = ClipReload::Automatic;
        let id = game.spawn(index, 0, [300.0; 2], true);
        let mut seed = game.random;
        let mut unused = vec![0, 1, 2];
        for _ in 0..3 {
            let pick = usize::try_from(startup::random_between(
                &mut seed,
                u32::try_from(unused.len() - 1).unwrap(),
            ))
            .unwrap();
            let expected = game.catalog[index].weapon.scatter_targets[unused.swap_remove(pick)];
            let actual = game
                .scatter_weapon_target(id, 0, [300.0, 300.0, 90.0])
                .unwrap();
            assert_eq!(
                actual.map(f32::to_bits),
                [300.0 + expected[0], 300.0 + expected[1], 0.0].map(f32::to_bits)
            );
            game.entities[id].spend_weapon_round(&game.catalog[index], game.tick, 0);
        }
        assert!(game.scatter_weapon_target(id, 0, [300.0; 3]).is_none());
        assert_eq!(game.random, seed);
        game.entities[id].spend_weapon_round(&game.catalog[index], game.tick, 0);
        assert!(game.scatter_weapon_target(id, 0, [300.0; 3]).is_some());
    }
}
