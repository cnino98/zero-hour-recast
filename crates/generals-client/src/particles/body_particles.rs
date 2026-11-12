//! Native ActiveBody-owned auto fire/smoke systems, attached in Object space.
use super::{Particles, Pose, Template};
use crate::{
    model_animation::Animations,
    skirmish_data::{definition, field, scalar},
};
use generals_assets::{
    content::{self, ContentFile},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::body_particles::Event;
use std::{collections::BTreeMap, error::Error};

struct Source {
    prefix: String,
    system: Option<String>,
    maximum: u32,
}
struct Binding {
    prefix: String,
    template: Option<usize>,
    maximum: u32,
}
pub(super) struct Pending {
    sources: Vec<Source>,
}
pub(super) struct Bodies {
    bindings: [Binding; 7],
    systems: BTreeMap<usize, Vec<u64>>,
}
impl Bodies {
    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self {
            bindings: std::array::from_fn(|_| Binding {
                prefix: String::new(),
                template: None,
                maximum: 0,
            }),
            systems: BTreeMap::new(),
        }
    }
    pub(super) fn clear(&mut self) {
        self.systems.clear();
    }
}
impl Pending {
    pub(super) fn load(
        assets: &VirtualAssets,
        particles: &[ContentFile],
    ) -> Result<Self, Box<dyn Error>> {
        let files = [content::scan(assets.read("data/ini/gamedata.ini")?)?];
        let data = &definition(&files, "GameData", "")?.lines;
        let mut sources = Vec::new();
        for family in [
            "FireParticleSmall",
            "FireParticleMedium",
            "FireParticleLarge",
            "SmokeParticleSmall",
            "SmokeParticleMedium",
            "SmokeParticleLarge",
            "AflameParticle",
        ] {
            let value = |suffix: &str| -> Result<&str, Box<dyn Error>> {
                scalar(
                    field(data, 0, &format!("Auto{family}{suffix}"))
                        .ok_or("missing native body particle setting")?,
                )
            };
            let name = value("System")?;
            let system = particles
                .iter()
                .flat_map(|file| &file.declarations)
                .any(|entry| entry.kind == "ParticleSystem" && entry.name == name)
                .then(|| name.to_owned());
            let maximum: u32 = value("Max")?.parse()?;
            if maximum > u32::try_from(i32::MAX).expect("positive Int maximum") / 2 {
                return Err("body particle count exceeds native doubled Int range".into());
            }
            sources.push(Source {
                prefix: value("Prefix")?.into(),
                system,
                maximum,
            });
        }
        Ok(Self { sources })
    }
    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.sources
            .iter()
            .filter_map(|source| source.system.as_deref())
    }
    pub(super) fn resolve(self, templates: &[Template]) -> Result<Bodies, Box<dyn Error>> {
        let bindings: Vec<Binding> = self
            .sources
            .into_iter()
            .map(|source| {
                Ok(Binding {
                    prefix: source.prefix,
                    maximum: source.maximum,
                    template: source
                        .system
                        .map(|name| {
                            templates
                                .iter()
                                .position(|entry| entry.name == name)
                                .ok_or("missing parsed body particle dependency")
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<_, Box<dyn Error>>>()?;
        let bindings = bindings
            .try_into()
            .map_err(|_| "native body table must contain seven bindings")?;
        Ok(Bodies {
            bindings,
            systems: BTreeMap::new(),
        })
    }
}
impl Particles {
    pub(crate) fn body_particle_event(
        &mut self,
        game: &generals_sim::skirmish::Skirmish,
        generation: u32,
        index: usize,
        animations: &mut Animations,
    ) {
        self.begin(generation, game.tick);
        self.refresh_body_particles(
            &game.presentation.body_particles[index],
            generation,
            animations,
        );
    }
    pub(crate) fn clear_body_particles(&mut self, object: usize) {
        if let Some(systems) = self.bodies.systems.remove(&object) {
            for system in systems.into_iter().rev() {
                self.destroy(system);
            }
        }
    }
    fn refresh_body_particles(
        &mut self,
        event: &Event,
        generation: u32,
        animations: &mut Animations,
    ) {
        self.clear_body_particles(event.pose.object);
        let count = if event.aflame { 7 } else { 6 };
        let mut systems = Vec::new();
        for index in 0..count {
            let binding = &self.bodies.bindings[index];
            // Aflame replaces small fire with medium, medium/large with large;
            // all smoke becomes small fire. The seventh binding stays Aflame.
            let template_index = if event.aflame {
                [1, 2, 2, 0, 0, 0, 6][index]
            } else {
                index
            };
            let Some(template) = self.bodies.bindings[template_index].template else {
                continue;
            };
            let positions = event.positions(&binding.prefix);
            let maximum = binding.maximum * if event.aflame { 2 } else { 1 };
            let maximum = usize::try_from(maximum)
                .expect("native count")
                .min(positions.len());
            let mut used = [false; 16];
            for ordinal in 0..maximum {
                // Native uses maxSystems, deliberately not the full bone count.
                let choice = usize::try_from(animations.effect_integer(
                    generation,
                    [
                        0,
                        u32::try_from(maximum - ordinal - 1).expect("sixteen bones"),
                    ],
                ))
                .expect("native bone choice");
                let bone = (0..positions.len())
                    .filter(|&bone| !used[bone])
                    .nth(choice)
                    .expect("unused native bone");
                used[bone] = true;
                let system = self.create(
                    template,
                    Pose::at([0.0; 3]),
                    None,
                    &mut super::random(animations, generation),
                );
                let parent = Pose {
                    position: event.pose.position,
                    axes: std::array::from_fn(|column| {
                        std::array::from_fn(|row| event.pose.basis[row][column])
                    }),
                };
                let local = Pose::at(positions[bone]);
                let emitter = self
                    .emitters
                    .iter_mut()
                    .find(|emitter| emitter.id == system)
                    .expect("created body particle");
                emitter.transform = super::model_states::compose(&parent, &local);
                emitter.wind_origin = Some(std::array::from_fn(|axis| {
                    event.pose.position[axis] + local.position[axis]
                }));
                emitter.object_attachment = Some((event.pose.object, local));
                systems.push(system);
            }
        }
        self.bodies.systems.insert(event.pose.object, systems);
    }
}

#[cfg(test)]
mod installed;
#[cfg(test)]
mod tests;
