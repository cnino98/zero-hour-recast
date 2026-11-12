//! `SpectreGunshipUpdate` CRC=0: client-only constructor RNG then impact jitter.
use super::{Particles, Pose, random};
use crate::{model_animation::Animations, playtest::Match};

impl Particles {
    pub fn spectre_strafe(&mut self, battle: &Match, index: usize, animations: &mut Animations) {
        let game = &battle.simulation;
        let event = &game.presentation.spectre_strafes[index];
        let viewer = battle.observer_player.unwrap_or(0);
        if event.visible_players & (1 << viewer) == 0 {
            return;
        }
        let Some(template) = self
            .templates
            .iter()
            .position(|entry| entry.name == event.particle)
        else {
            return;
        };
        self.begin(battle.generation, game.tick);
        let mut random = random(animations, battle.generation);
        let emitter = self.create(template, Pose::at([0.0; 3]), None, &mut random);
        let x = event.position[0] + random(-5.0, 5.0);
        let y = event.position[1] + random(-5.0, 5.0);
        let position = [x, y, game.ground_height([x, y])];
        self.emitters
            .iter_mut()
            .find(|entry| entry.id == emitter)
            .expect("created Spectre strafe system")
            .transform = Pose::at(position);
    }
}
