//! SH `FirestormDynamicGeometryInfoUpdate` extends its per-instance `DynamicGeometry` update.
use super::geometry;
use crate::skirmish::{DamageType, DeathType, Skirmish};
use num_traits::ToPrimitive;

pub struct Rules {
    pub geometry: geometry::Rules,
    pub interval: f32,
    pub damage: f32,
    pub height: f32,
    pub particles: [Option<String>; 16],
    pub particle_height: f32,
    pub effect: String,
    pub scorch: f32,
}

pub(in crate::skirmish) struct State {
    geometry: geometry::State,
    effects_fired: bool,
    scorch_placed: bool,
    last_damage: u32,
}

pub enum Event {
    Start {
        object: usize,
        ordinal: usize,
        particles: Box<[Option<String>; 16]>,
        position: [f32; 3],
    },
    Resize {
        object: usize,
        ordinal: usize,
        radius: f32,
    },
    Scorch {
        position: [f32; 3],
        size: f32,
    },
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_firestorm_module(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.firestorms.len(), ordinal);
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .firestorms[ordinal];
        self.entities[id].behaviors.firestorms.push(State {
            geometry: geometry::State::new(&rules.geometry),
            effects_fired: false,
            scorch_placed: false,
            last_damage: 0,
        });
    }

    pub(in crate::skirmish) fn update_firestorm_module(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        let now = self.update_frame();
        let type_index = self.entities[id].type_index;
        let rules = &self.catalog[type_index].behavior.firestorms[ordinal];
        let state = &mut self.entities[id].behaviors.firestorms[ordinal];
        let dimensions = state.geometry.update(&rules.geometry);
        if let Some(dimensions) = dimensions {
            self.set_dynamic_geometry(id, dimensions);
            if let Some(presentation) = &mut self.entities[id].fire {
                presentation.dimensions = dimensions;
            }
        }
        let state = &self.entities[id].behaviors.firestorms[ordinal];
        if !state.geometry.started() {
            return 1;
        }
        let start = !state.effects_fired;
        let scorch = state.geometry.reversed() && !state.scorch_placed;
        let rules = &self.catalog[type_index].behavior.firestorms[ordinal];
        let damage = now
            .wrapping_sub(state.last_damage)
            .to_f32()
            .expect("native unsigned frame")
            >= rules.interval;
        let effect = start.then(|| rules.effect.clone());
        let particles = start.then(|| rules.particles.clone());
        let particle_height = rules.particle_height;
        let scorch_size = rules.scorch;
        let damage_amount = rules.damage;
        let damage_height = rules.height;
        let position = self.object_position(id);
        if let Some(presentation) = &mut self.entities[id].fire {
            if start {
                presentation.started.get_or_insert(self.tick);
            }
            if scorch {
                presentation.reversed.get_or_insert(self.tick);
            }
        }
        if let Some(particles) = particles {
            self.record_firestorm_event(Event::Start {
                object: id,
                ordinal,
                particles: Box::new(particles),
                position: [
                    position[0],
                    position[1],
                    self.ground_height([position[0], position[1]]) + particle_height,
                ],
            });
        }
        if let Some(effect) = effect {
            self.object_fx(id, effect);
            self.entities[id].behaviors.firestorms[ordinal].effects_fired = true;
        }
        let radius = self.body_geometry(id).major;
        self.record_firestorm_event(Event::Resize {
            object: id,
            ordinal,
            radius,
        });
        if scorch {
            self.record_firestorm_event(Event::Scorch {
                position,
                size: scorch_size,
            });
            self.entities[id].behaviors.firestorms[ordinal].scorch_placed = true;
        }
        if damage {
            // Native FROM_BOUNDINGSPHERE_2D iterator includes self and effectively dead objects.
            let geometry = self.body_geometry(id);
            let radius = if matches!(geometry.shape, crate::collision_geometry::Shape::Box) {
                let mut squared = geometry.major * geometry.major;
                let minor_squared = geometry.minor * geometry.minor;
                squared += minor_squared;
                squared.sqrt()
            } else {
                geometry.major
            };
            if radius != 0.0 {
                for target in self.firestorm_damage_targets(id, radius) {
                    if self.object_position(target)[2] > position[2] + damage_height {
                        continue;
                    }
                    let Some(target) = self.hive_damage_target(target, id, DamageType::Flame)
                    else {
                        continue;
                    };
                    self.apply_weapon_hit(
                        target,
                        id,
                        damage_amount,
                        DamageType::Flame,
                        DeathType::Burned,
                    );
                }
            }
            self.entities[id].behaviors.firestorms[ordinal].last_damage = now;
        }
        1
    }

    fn record_firestorm_event(&mut self, event: Event) {
        self.presentation
            .weapon_fx
            .push(crate::skirmish::WeaponFxEvent::Firestorm(
                self.presentation.firestorm_events.len(),
            ));
        self.presentation.firestorm_events.push(event);
    }
}
