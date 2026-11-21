//! Radius emitter owned by the original `GrantStealthBehavior` instance.
use super::{Particles, Pose};
use generals_sim::skirmish::{Skirmish, UnitType, stealth::Event};
use std::collections::BTreeMap;

pub(super) fn names(types: &[UnitType]) -> impl Iterator<Item = &str> {
    types
        .iter()
        .flat_map(|unit| &unit.behavior.gps_scramblers)
        .filter_map(|rules| rules.radius_particle.as_deref())
}

pub(super) type Emitters = BTreeMap<(usize, usize), u64>;

impl Particles {
    pub(super) fn gps_scrambler_effects(
        &mut self,
        game: &Skirmish,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        for event in &game.presentation.stealth_events {
            match *event {
                Event::GrantEmitter {
                    object,
                    ordinal,
                    position,
                } => {
                    let rules = &game.catalog[game.entities[object].type_index]
                        .behavior
                        .gps_scramblers[ordinal];
                    let Some(name) = &rules.radius_particle else {
                        continue;
                    };
                    let template = self
                        .templates
                        .iter()
                        .position(|template| &template.name == name)
                        .expect("loaded GPS radius ParticleSystem");
                    let emitter = self.create(template, Pose::at(position), None, random);
                    self.gps_scramblers.insert((object, ordinal), emitter);
                }
                Event::DestroyGrantEmitter { object, ordinal } => {
                    if let Some(emitter) = self.gps_scramblers.remove(&(object, ordinal)) {
                        self.destroy(emitter);
                    }
                }
                _ => {}
            }
        }
    }
}
