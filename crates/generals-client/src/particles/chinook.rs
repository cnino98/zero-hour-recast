//! Chinook rotor-wash particle creation after its ordered client probability draw.
use super::{Particles, Pose, random};
use crate::{model_animation::Animations, playtest::Match};

impl Particles {
    pub fn chinook_rotor_wash(
        &mut self,
        battle: &Match,
        name: &str,
        position: [f32; 3],
        animations: &mut Animations,
    ) {
        let Some(template) = self
            .templates
            .iter()
            .position(|template| template.name == name)
        else {
            return;
        };
        self.begin(battle.generation, battle.simulation.tick);
        let emitter = self.create(
            template,
            Pose::at([0.0; 3]),
            None,
            &mut random(animations, battle.generation),
        );
        self.emitters
            .iter_mut()
            .find(|entry| entry.id == emitter)
            .expect("created rotor wash")
            .transform = Pose::at(position);
    }
}
