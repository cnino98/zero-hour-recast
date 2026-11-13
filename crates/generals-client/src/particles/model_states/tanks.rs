//! SH `W3DTankDraw`: delayed, drawable-owned left/right tread-debris emitters.
use super::{Particles, Pose, Template};
use crate::skirmish_data::{field, scalar};
use generals_assets::content::ContentLine;
use std::error::Error;

pub(super) struct Names(pub [String; 2]);
impl Names {
    pub(super) fn load(draw: &[ContentLine]) -> Result<Self, Box<dyn Error>> {
        let name = |key, default| -> Result<String, Box<dyn Error>> {
            Ok(field(draw, 1, key)
                .map(scalar)
                .transpose()?
                .unwrap_or(default)
                .to_owned())
        };
        Ok(Self([
            name("TreadDebrisLeft", "TrackDebrisDirtLeft")?,
            name("TreadDebrisRight", "TrackDebrisDirtRight")?,
        ]))
    }
    pub(super) fn resolve(
        self,
        templates: &[Template],
    ) -> Result<[Option<usize>; 2], Box<dyn Error>> {
        let mut result = [None; 2];
        for (slot, name) in result.iter_mut().zip(self.0) {
            if !name.is_empty() && !name.eq_ignore_ascii_case("None") {
                *slot = Some(
                    templates
                        .iter()
                        .position(|template| template.name == name)
                        .ok_or_else(|| format!("missing tank ParticleSystem {name}"))?,
                );
            }
        }
        Ok(result)
    }
}

pub(super) struct Sample {
    pub pose: Pose,
    pub velocity: [f32; 3],
    pub draw: bool,
    pub hidden: bool,
}

pub(super) struct Active {
    source: usize,
    systems: [Option<u64>; 2],
}

impl Particles {
    pub(super) fn retire_tank_emitters(&mut self, key: (usize, usize)) {
        let Some(active) = self.model_states.tanks.remove(&key) else {
            return;
        };
        for system in active.systems.into_iter().flatten() {
            if let Some(emitter) = self
                .emitters
                .iter_mut()
                .find(|emitter| emitter.id == system)
            {
                emitter.drawable_attachment = None;
                emitter.lifecycle.destroyed = true;
            }
        }
    }
    pub(super) fn sync_tank_emitters(
        &mut self,
        key: (usize, usize),
        source: usize,
        sample: &Sample,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        if self
            .model_states
            .tanks
            .get(&key)
            .is_some_and(|active| active.source != source)
        {
            self.retire_tank_emitters(key);
        }
        let mut active = self.model_states.tanks.remove(&key).unwrap_or(Active {
            source,
            systems: [None; 2],
        });
        if sample.draw && !sample.hidden {
            let binding = self.model_states.tank_bindings[&(source, key.1)];
            for (system, template) in active.systems.iter_mut().zip(binding) {
                if system.is_none()
                    && let Some(template) = template
                {
                    *system = Some(self.create(template, sample.pose.clone(), None, random));
                    let emitter = self.emitters.last_mut().expect("created tank debris");
                    emitter.drawable_attachment = Some(key);
                    emitter.lifecycle.stopped = true;
                }
            }
        }
        let [x, y, _] = sample.velocity;
        let squares = [x * x, y * y];
        let square = squares[0] + squares[1];
        let speed = square.sqrt();
        let half_speed = 0.5 * speed;
        let xy = (half_speed + 0.1).min(1.0);
        let z = (speed + 0.1).min(1.0);
        for system in active.systems.into_iter().flatten() {
            let Some(emitter) = self
                .emitters
                .iter_mut()
                .find(|emitter| emitter.id == system)
            else {
                continue;
            };
            if sample.hidden {
                emitter.lifecycle.stopped = true;
            } else if sample.draw {
                emitter.lifecycle.stopped = square <= 0.00001;
                emitter.velocity_multiplier = [xy, xy, z];
                emitter.burst_multiplier = z;
            }
        }
        self.model_states.tanks.insert(key, active);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::tests::{system, template};

    #[test]
    fn tank_debris_defers_birth_preserves_stopped_systems_and_scales_planar_motion() {
        let mut left = template("left");
        left.system_lifetime = 0.0;
        left.burst_count.low = 10.0;
        left.burst_count.high = 10.0;
        let mut right = template("right");
        right.system_lifetime = 0.0;
        right.burst_count.low = 10.0;
        right.burst_count.high = 10.0;
        let mut particles = system(vec![left, right]);
        particles
            .model_states
            .tank_bindings
            .insert((4, 0), [Some(0), Some(1)]);
        let key = (8, 0);
        let mut sample = Sample {
            pose: Pose::at([100.0, 200.0, 0.0]),
            velocity: [0.0, 0.0, 2.0],
            draw: false,
            hidden: false,
        };
        particles.sync_tank_emitters(key, 4, &sample, &mut |_, _| {
            panic!("offscreen births consume RNG")
        });
        assert!(particles.emitters.is_empty());
        sample.draw = true;
        particles.sync_tank_emitters(key, 4, &sample, &mut |_, high| high);
        assert_eq!(particles.emitters.len(), 2);
        assert!(
            particles
                .emitters
                .iter()
                .all(|emitter| emitter.lifecycle.stopped)
        );
        assert_eq!(particles.emitters[0].template, 0);
        assert_eq!(particles.emitters[1].template, 1);
        sample.velocity = [0.3, 0.4, 2.0];
        particles.sync_tank_emitters(key, 4, &sample, &mut |_, _| {
            panic!("existing systems consume no constructor RNG")
        });
        let first = particles.emitters[0].id;
        assert_eq!(
            particles.emitters[0].velocity_multiplier.map(f32::to_bits),
            [0.35_f32, 0.35, 0.6].map(f32::to_bits)
        );
        assert!(!particles.emitters[0].lifecycle.stopped);
        particles.advance_frame(1, &mut |_, high| high, &|_| 0.0);
        assert_eq!(particles.emitters[0].particles.len(), 6);
        sample.hidden = true;
        particles.sync_tank_emitters(key, 4, &sample, &mut |_, _| {
            panic!("hidden draw created emitter")
        });
        assert!(particles.emitters[0].lifecycle.stopped);
        assert!(!particles.emitters[0].lifecycle.destroyed);
        sample.hidden = false;
        particles.sync_tank_emitters(key, 4, &sample, &mut |_, _| {
            panic!("unshroud recreated retained tank emitter")
        });
        assert_eq!(particles.emitters[0].id, first);
        particles.retire_tank_emitters(key);
        assert!(particles.emitters.iter().all(|emitter| emitter.lifecycle.destroyed && emitter.drawable_attachment.is_none()));
        assert_eq!(particles.emitters[0].particles.len(), 6);
    }
}
