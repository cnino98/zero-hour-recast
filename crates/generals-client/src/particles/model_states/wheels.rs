//! SH 2bfaea60 `W3DTruckDraw` wheel effects and Drawable airborne counters.
use super::{Particles, Pose, Template, WheelSound};
use crate::skirmish_data::{field, scalar};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::combat_bike::DrawMotion;
use std::error::Error;

pub(super) struct Names {
    pub effects: [Option<String>; 3],
    left_tire: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Binding {
    effects: [Option<usize>; 3],
    left_tire: bool,
}

impl Names {
    pub(super) fn load(draw: &[ContentLine]) -> Result<Self, Box<dyn Error>> {
        let name = |key| {
            field(draw, 1, key)
                .map(scalar)
                .transpose()
                .map(|name| name.map(str::to_owned))
        };
        Ok(Self {
            effects: [name("Dust")?, name("DirtSpray")?, name("PowerslideSpray")?],
            left_tire: field(draw, 1, "LeftFrontTireBone").is_some()
                || field(draw, 1, "LeftRearTireBone").is_some(),
        })
    }

    pub(super) fn resolve(self, templates: &[Template]) -> Result<Binding, Box<dyn Error>> {
        let mut effects = [None; 3];
        for (slot, name) in effects.iter_mut().zip(self.effects) {
            if let Some(name) = name {
                *slot = Some(
                    templates
                        .iter()
                        .position(|template| template.name == name)
                        .ok_or_else(|| format!("missing wheel ParticleSystem {name}"))?,
                );
            }
        }
        Ok(Binding {
            effects,
            left_tire: self.left_tire,
        })
    }
}

pub(super) struct Sample {
    pub pose: Pose,
    pub visibility: Visibility,
    pub motorcycle: bool,
    pub motion: DrawMotion,
}

#[derive(PartialEq, Eq)]
pub(super) enum Visibility {
    Hidden,
    Shrouded,
    Draw,
    Retain,
}

pub(super) struct Active {
    source: usize,
    systems: [Option<u64>; 3],
    airborne_frames: u32,
    shrouded: bool,
}

impl Active {
    const fn new(source: usize) -> Self {
        Self {
            source,
            systems: [None; 3],
            airborne_frames: 0,
            shrouded: false,
        }
    }
}

impl Particles {
    pub(super) fn toss_wheel_emitters(&mut self, state: &mut Active) {
        for id in std::mem::take(&mut state.systems).into_iter().flatten() {
            if let Some(emitter) = self.emitters.iter_mut().find(|emitter| emitter.id == id) {
                emitter.transform = Pose::at([0.0; 3]);
                emitter.wind_origin = Some([0.0; 3]);
                emitter.object_attachment = None;
                self.destroy(id);
            }
        }
    }

    pub(super) fn sync_wheel_emitters(
        &mut self,
        id: usize,
        source: usize,
        sample: &Sample,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        let binding = self.model_states.wheel_bindings[&source];
        let mut state = self
            .model_states
            .wheels
            .remove(&id)
            .unwrap_or_else(|| Active::new(source));
        if state.source != source {
            self.toss_wheel_emitters(&mut state);
            self.model_states.wheel_sounds.push(WheelSound::Retire(id));
            state = Active::new(source);
        }
        let shrouded = sample.visibility == Visibility::Shrouded;
        let unshrouded = state.shrouded && !shrouded;
        state.shrouded = shrouded;
        if shrouded {
            self.toss_wheel_emitters(&mut state);
        } else {
            let draw = sample.visibility == Visibility::Draw;
            let hidden = sample.visibility == Visibility::Hidden;
            if !hidden && (draw || unshrouded) {
                self.create_wheel_emitters(&mut state, binding, sample, id, random);
            }
            if hidden || draw {
                let (landing, sliding) =
                    self.control_wheel_emitters(&mut state, sample, draw, binding.left_tire);
                if draw {
                    self.model_states.wheel_sounds.push(WheelSound::Draw {
                        object: id,
                        landing,
                        sliding,
                    });
                }
            }
        }
        for system in state.systems.into_iter().flatten() {
            if let Some(emitter) = self
                .emitters
                .iter_mut()
                .find(|emitter| emitter.id == system)
                && emitter.delay == 0.0
            {
                if emitter.previous.is_some()
                    && emitter.transform.position.map(f32::to_bits)
                        != sample.pose.position.map(f32::to_bits)
                {
                    emitter.previous = Some(emitter.transform.position);
                }
                emitter.transform = sample.pose.clone();
                emitter.wind_origin = Some(sample.pose.position);
            }
        }
        self.model_states.wheels.insert(id, state);
    }

    fn create_wheel_emitters(
        &mut self,
        state: &mut Active,
        binding: Binding,
        sample: &Sample,
        object: usize,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        for (system, template) in state.systems.iter_mut().zip(binding.effects) {
            if system.is_none()
                && let Some(template) = template
            {
                *system = Some(self.create(template, sample.pose.clone(), None, random));
                let emitter = self.emitters.last_mut().expect("created wheel particles");
                emitter.object_attachment = Some((object, Pose::at([0.0; 3])));
                emitter.wind_origin = Some(sample.pose.position);
            }
        }
    }

    fn control_wheel_emitters(
        &mut self,
        state: &mut Active,
        sample: &Sample,
        draw: bool,
        left_tire: bool,
    ) -> (bool, bool) {
        let motion = &sample.motion;
        // calcPhysicsXformMotorcycle consumes its airborne counter in the same
        // call. calcPhysicsXformWheels returns early in the airborne branch.
        let mut landed = 0;
        if draw && !sample.motorcycle {
            if motion.airborne {
                state.airborne_frames += 1;
            } else {
                landed = std::mem::take(&mut state.airborne_frames);
            }
        }
        let moving = draw && motion.motive && !motion.airborne;
        let [ax, ay] = motion.acceleration;
        let [vx, vy, vz] = motion.velocity;
        let velocity_squared = [vx * vx, vy * vy, vz * vz];
        let speed = (velocity_squared[0] + velocity_squared[1] + velocity_squared[2]).sqrt();
        let speed = if left_tire && motion.backwards {
            -speed
        } else {
            speed
        }
        .min(2.0);
        let acceleration_squared = [ax * ax, ay * ay];
        let acceleration = (acceleration_squared[0] + acceleration_squared[1]).sqrt();
        let acceleration_dot = [ax * vx, ay * vy];
        let accelerating = acceleration > 0.01 && acceleration_dot[0] + acceleration_dot[1] >= 0.0;
        let active = [
            moving,
            moving && accelerating,
            moving && motion.turning != 0,
        ];
        let mut landing_sound = false;
        let mut sliding = false;
        for (slot, system) in state.systems.iter().enumerate() {
            let Some(emitter) =
                system.and_then(|id| self.emitters.iter_mut().find(|emitter| emitter.id == id))
            else {
                continue;
            };
            emitter.lifecycle.stopped = !active[slot];
            if slot == 0 && moving {
                emitter.size_multiplier = speed;
                if landed > 3 {
                    landing_sound = true;
                    let factor = if landed >= 16 { 2.0 } else { 1.0 };
                    emitter.size_multiplier = factor * 2.0;
                    emitter.delay = 0.0;
                    emitter.burst_delay = 0.0;
                }
            }
            if slot == 2 {
                sliding = active[slot];
            }
        }
        (landing_sound, sliding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::tests::{system, template};

    fn fixture(motorcycle: bool) -> (Particles, Sample) {
        let mut particles = system(
            ["Dust", "Dirt", "Slide"]
                .map(|name| {
                    let mut source = template(name);
                    source.system_lifetime = 0.0;
                    source
                })
                .into(),
        );
        particles.model_states.wheel_bindings.insert(
            0,
            Binding {
                effects: [Some(0), Some(1), Some(2)],
                left_tire: true,
            },
        );
        (
            particles,
            Sample {
                pose: Pose::at([70.0, 65.0, 0.0]),
                visibility: Visibility::Draw,
                motorcycle,
                motion: DrawMotion {
                    velocity: [1.0, 0.0, 0.0],
                    acceleration: [0.02, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    altitude: 0.0,
                    airborne: false,
                    motive: true,
                    turning: 1,
                    backwards: false,
                    locomotor_set: 0,
                    surface: 0,
                },
            },
        )
    }

    #[test]
    fn truck_dust_dirt_and_slide_start_in_order_and_keep_world_particles_when_shrouded() {
        let (mut particles, mut sample) = fixture(false);
        let mut random = |_, high| high;
        sample.visibility = Visibility::Retain;
        particles.sync_wheel_emitters(7, 0, &sample, &mut random);
        assert!(particles.emitters.is_empty());
        sample.visibility = Visibility::Draw;
        particles.sync_wheel_emitters(7, 0, &sample, &mut random);
        assert_eq!(
            particles
                .emitters
                .iter()
                .map(|emitter| emitter.template)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        particles.advance_frame(1, &mut random, &|_| 0.0);
        assert!(
            particles
                .emitters
                .iter()
                .all(|emitter| emitter.particles.len() == 1)
        );
        sample.motion.acceleration[0] = -0.05;
        sample.motion.turning = 0;
        particles.sync_wheel_emitters(7, 0, &sample, &mut random);
        assert_eq!(
            particles
                .emitters
                .iter()
                .map(|emitter| emitter.lifecycle.stopped)
                .collect::<Vec<_>>(),
            [false, true, true]
        );
        sample.visibility = Visibility::Hidden;
        particles.sync_wheel_emitters(7, 0, &sample, &mut random);
        assert!(
            particles
                .emitters
                .iter()
                .all(|emitter| emitter.lifecycle.stopped)
        );
        sample.visibility = Visibility::Shrouded;
        particles.sync_wheel_emitters(7, 0, &sample, &mut random);
        assert!(
            particles
                .emitters
                .iter()
                .all(|emitter| emitter.lifecycle.destroyed
                    && emitter.object_attachment.is_none()
                    && !emitter.particles.is_empty())
        );
        sample.visibility = Visibility::Retain;
        particles.sync_wheel_emitters(7, 0, &sample, &mut random);
        assert_eq!(particles.next_id, 6);
        assert!(
            particles.emitters[3..]
                .iter()
                .all(|emitter| !emitter.lifecycle.destroyed)
        );
    }

    #[test]
    fn four_wheel_landing_triggers_dust_but_motorcycle_consumes_airborne_counter() {
        for (bike, expected) in [(false, 4.0_f32), (true, 1.0_f32)] {
            let (mut particles, mut sample) = fixture(bike);
            let mut random = |_, high| high;
            sample.motion.airborne = true;
            for _ in 0..16 {
                particles.sync_wheel_emitters(7, 0, &sample, &mut random);
            }
            particles.emitters[0].burst_delay = 7.0;
            sample.motion.airborne = false;
            particles.sync_wheel_emitters(7, 0, &sample, &mut random);
            let dust = &particles.emitters[0];
            assert_eq!(dust.size_multiplier.to_bits(), expected.to_bits());
            let delay = if bike { 7.0_f32 } else { 0.0_f32 };
            assert_eq!(dust.burst_delay.to_bits(), delay.to_bits());
        }
    }
}
