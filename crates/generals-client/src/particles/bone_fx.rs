//! Ordered native `BoneFXUpdate` particle creation and per-instance destruction.
use super::{Particles, Pose};
use generals_sim::skirmish::bone_fx::{Action, Event};

impl Particles {
    pub fn bone_event(
        &mut self,
        battle: &crate::playtest::Match,
        index: usize,
        animations: &mut crate::model_animation::Animations,
    ) {
        let event = &battle.simulation.presentation.bone_fx[index];
        self.begin(battle.generation, event.frame);
        self.record_bone_event(event, &mut super::random(animations, battle.generation));
    }

    fn record_bone_event(&mut self, event: &Event, random: &mut impl FnMut(f32, f32) -> f32) {
        match &event.action {
            Action::Clear => {
                if let Some(systems) = self.bone_systems.remove(&(event.object, event.module)) {
                    for system in systems {
                        self.destroy(system);
                    }
                }
            }
            Action::Particle {
                template,
                local,
                position,
                basis,
                hidden,
                ..
            } => {
                let index = self
                    .templates
                    .iter()
                    .position(|entry| entry.name == *template)
                    .expect("validated original BoneFX particle template");
                // Native create precedes setPosition/attachToObject/stop.
                let system = self.create(index, Pose::at([0.0; 3]), None, random);
                let local = Pose::at(*local);
                let parent = Pose {
                    position: *position,
                    axes: std::array::from_fn(|column| {
                        std::array::from_fn(|row| basis[row][column])
                    }),
                };
                let emitter = self
                    .emitters
                    .iter_mut()
                    .find(|emitter| emitter.id == system)
                    .expect("created BoneFX system");
                emitter.transform = super::model_states::compose(&parent, &local);
                emitter.object_attachment = Some((event.object, local));
                // stop() is persistent and independent of later partition visibility.
                emitter.lifecycle.stopped = *hidden;
                self.bone_systems
                    .entry((event.object, event.module))
                    .or_default()
                    .push(system);
            }
        }
    }
}

#[cfg(test)]
mod tests;
