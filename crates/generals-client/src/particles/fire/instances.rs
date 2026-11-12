//! Native `FirestormDynamicGeometry` module owns sixteen free particle system IDs.
use super::{Particles, Pose};
use crate::particles::Volume;
use generals_sim::skirmish::fire::firestorm::Event;
use std::collections::BTreeMap;

pub(super) struct Instances {
    systems: BTreeMap<(usize, usize), [Option<u64>; 16]>,
}

impl Instances {
    pub(super) const fn empty() -> Self {
        Self {
            systems: BTreeMap::new(),
        }
    }
    pub(super) fn reset(&mut self) {
        self.systems.clear();
    }
}

impl Particles {
    pub fn firestorm_module_event(
        &mut self,
        game: &generals_sim::skirmish::Skirmish,
        generation: u32,
        index: usize,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        self.begin(generation, game.tick);
        let event = &game.presentation.firestorm_events[index];
        match event {
            Event::Start {
                object,
                ordinal,
                particles,
                position,
            } => {
                let mut systems = [None; 16];
                for (name, system) in particles.iter().zip(&mut systems) {
                    if let Some(name) = name {
                        let template = *self
                            .fires
                            .module_templates
                            .get(name)
                            .expect("admitted original firestorm particle");
                        *system = Some(self.create(template, Pose::at(*position), None, random));
                    }
                }
                self.fires
                    .instances
                    .systems
                    .insert((*object, *ordinal), systems);
            }
            Event::Resize {
                object,
                ordinal,
                radius,
            } => {
                if let Some(systems) = self.fires.instances.systems.get_mut(&(*object, *ordinal)) {
                    for system in systems {
                        if let Some(id) = *system {
                            if let Some(emitter) =
                                self.emitters.iter_mut().find(|emitter| emitter.id == id)
                            {
                                if matches!(
                                    self.templates[emitter.template].volume,
                                    Volume::Sphere { .. } | Volume::Cylinder { .. }
                                ) {
                                    emitter.radius = Some(*radius);
                                }
                            } else {
                                *system = None;
                            }
                        }
                    }
                }
            }
            Event::Scorch { position, size } => self.scorches.add(*position, *size, 0),
        }
    }
}
