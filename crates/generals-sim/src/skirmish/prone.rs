//! SH 2bfaea60 ProneUpdate.cpp: signed damage accumulation and NORMAL countdown.
use super::{ModelConditions, Skirmish};
use num_traits::ToPrimitive;

impl Skirmish {
    pub(super) fn construct_prone(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.prone.len(), ordinal);
        self.entities[id].behaviors.prone.push(0);
    }

    /// `SlavedUpdate::onSlaverDamage` → `AIUpdate::privateGoProne`, first Prone module.
    pub(super) fn slaved_on_master_damage(&mut self, id: usize, actual_damage: f32) {
        if self.entities[id].ai.registration.is_none()
            || !self.native_ai_command_allowed(id)
            || self.entities[id].behaviors.prone.is_empty()
        {
            return;
        }
        let ratio = self.catalog[self.entities[id].type_index].behavior.prone[0];
        let frames = &mut self.entities[id].behaviors.prone[0];
        let was_prone = *frames > 0;
        // Native Int damageTaken truncates before multiplication; compound +=
        // promotes the old Int to Real and truncates the sum back to Int.
        let damage = actual_damage.to_i32().expect("native damage Int range");
        let extra = damage.to_f32().expect("native damage Real") * ratio;
        *frames = (frames.to_f32().expect("native prone Real") + extra)
            .to_i32()
            .expect("native prone Int range");
        if !was_prone && *frames > 0 {
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(ModelConditions::PRONE, true);
            self.entities[id].object_status |= 1 << 6;
        }
    }

    pub(super) fn update_prone(&mut self, id: usize, ordinal: usize) -> i32 {
        let frames = &mut self.entities[id].behaviors.prone[ordinal];
        if *frames > 0 {
            *frames -= 1;
            if *frames == 0 {
                self.entities[id]
                    .drawable
                    .model_conditions
                    .set_flag(ModelConditions::PRONE, false);
                self.entities[id].object_status &= !(1 << 6);
            }
        }
        1
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{ModelConditions, Skirmish, tests};

    fn soldier(ratios: &[f32]) -> (Skirmish, usize) {
        let mut game = tests::game();
        let unit = &mut game.catalog[5];
        for &ratio in ratios {
            unit.behavior.prone.push(ratio);
            tests::register_update(unit, "ProneUpdate");
        }
        let id = game.spawn(5, 0, [300.0, 200.0], true);
        (game, id)
    }

    #[test]
    fn damage_truncates_before_scaling_and_stacks_on_the_first_authored_instance() {
        let (mut game, id) = soldier(&[1.75, 20.0]);
        game.slaved_on_master_damage(id, 1.99);
        assert_eq!(game.entities[id].behaviors.prone, [1, 0]);
        assert!(!game.object_can_attack(id));
        game.slaved_on_master_damage(id, 2.99);
        assert_eq!(game.entities[id].behaviors.prone, [4, 0]);
        assert!(
            game.model_flags(&game.entities[id], [0.5, 0.1])
                .contains("PRONE")
        );
        for frame in 1..=4 {
            game.tick = frame;
            game.run_object_updates();
            assert_eq!(game.object_can_attack(id), frame == 4);
        }
        assert_eq!(game.entities[id].behaviors.prone, [0, 0]);
        assert!(
            !game
                .model_flags(&game.entities[id], [0.5, 0.1])
                .contains("PRONE")
        );
    }

    #[test]
    fn held_freezes_normal_prone_countdown_until_the_real_disable_edge_clears() {
        let (mut game, id) = soldier(&[1.0]);
        game.slaved_on_master_damage(id, 2.0);
        game.set_disabled_held(id);
        for frame in 1..=3 {
            game.tick = frame;
            game.run_object_updates();
        }
        assert_eq!(game.entities[id].behaviors.prone, [2]);
        game.clear_disabled_held(id);
        for frame in 4..=5 {
            game.tick = frame;
            game.run_object_updates();
        }
        assert_eq!(game.entities[id].behaviors.prone, [0]);
        assert!(game.object_can_attack(id));
    }

    #[test]
    fn nonpositive_damage_does_not_invent_an_effect_stop_transition() {
        let (mut game, id) = soldier(&[1.0]);
        game.slaved_on_master_damage(id, 0.9);
        assert!(game.object_can_attack(id));
        game.slaved_on_master_damage(id, 2.0);
        game.slaved_on_master_damage(id, -3.0);
        game.update_prone(id, 0);
        assert_eq!(game.entities[id].behaviors.prone, [-1]);
        assert!(
            game.entities[id]
                .drawable
                .model_conditions
                .has_flag(ModelConditions::PRONE)
        );
        assert!(!game.object_can_attack(id));
    }
}
