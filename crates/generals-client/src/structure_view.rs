//! Ordered `GameClientRandom` callbacks of the civilian structure update modules.
use crate::{model_animation::Animations, particles::Particles, playtest::Match};
use bevy::prelude::*;
use generals_sim::skirmish::structure_death::{Event, VisualAction};
use std::collections::BTreeMap;

#[derive(Resource, Default)]
pub struct Structures {
    generation: Option<u32>,
    offsets: BTreeMap<usize, [f32; 3]>,
    bursts: BTreeMap<usize, u64>,
    transitions: BTreeMap<(usize, usize), Vec<u64>>,
}

impl Structures {
    pub fn offset(&self, generation: u32, object: usize) -> [f32; 3] {
        if self.generation == Some(generation) {
            self.offsets.get(&object).copied().unwrap_or([0.0; 3])
        } else {
            [0.0; 3]
        }
    }

    pub fn record(
        &mut self,
        battle: &Match,
        event: &Event,
        animations: &mut Animations,
        particles: &mut Particles,
        audio: &mut crate::source_audio::SourceAudio,
    ) {
        if self.generation != Some(battle.generation) {
            self.offsets.clear();
            self.bursts.clear();
            self.transitions.clear();
            self.generation = Some(battle.generation);
        }
        let fire = match event.action {
            VisualAction::PositionFx => true,
            VisualAction::TransitionClear { .. } | VisualAction::TransitionParticle { .. } => {
                self.attached_particle(battle, event, animations, particles);
                false
            }
            VisualAction::CollapseShudder { height, amplitude } => {
                // Native calls both real draws even when this drawable is hidden.
                let horizontal = animations.effect_real(battle.generation, -amplitude, amplitude);
                let vertical = animations.effect_real(battle.generation, -amplitude, amplitude);
                self.offsets
                    .insert(event.object, [horizontal, vertical, height]);
                false
            }
            VisualAction::Clear => {
                self.offsets.remove(&event.object);
                self.bursts.remove(&event.object);
                false
            }
            VisualAction::ToppleBegin { burst_delay } => {
                let delay = animations.effect_integer(battle.generation, burst_delay);
                self.bursts
                    .insert(event.object, event.frame + u64::from(delay));
                false
            }
            VisualAction::ToppleCheck { burst_delay } => {
                let due = self
                    .bursts
                    .get(&event.object)
                    .is_some_and(|frame| event.frame >= *frame);
                if due {
                    // Delay FX executes before sampling the following deadline.
                    if let Some(effect) = &event.effect {
                        audio.position_fx(effect, battle.observer_player.unwrap_or(0));
                        particles.structure_position_event(
                            &battle.simulation,
                            battle.generation,
                            effect,
                            battle.observer_player.unwrap_or(0),
                            animations,
                        );
                    }
                    let delay = animations.effect_integer(battle.generation, burst_delay);
                    self.bursts
                        .insert(event.object, event.frame + u64::from(delay));
                }
                false
            }
        };
        if fire && let Some(effect) = &event.effect {
            audio.position_fx(effect, battle.observer_player.unwrap_or(0));
            particles.structure_position_event(
                &battle.simulation,
                battle.generation,
                effect,
                battle.observer_player.unwrap_or(0),
                animations,
            );
        }
    }

    fn attached_particle(
        &mut self,
        battle: &Match,
        event: &Event,
        animations: &mut Animations,
        particles: &mut Particles,
    ) {
        match event.action {
            VisualAction::TransitionClear { module } => {
                if let Some(systems) = self.transitions.remove(&(event.object, module)) {
                    particles.stop_structure_systems(&systems);
                }
            }
            VisualAction::TransitionParticle {
                module,
                ref template,
                local,
            } => {
                if let Some(snapshot) = &event.effect {
                    let system =
                        particles.structure_particle(battle, snapshot, template, local, animations);
                    self.transitions
                        .entry((event.object, module))
                        .or_default()
                        .push(system);
                }
            }
            _ => unreachable!("only structure particle ownership callbacks"),
        }
    }
}
