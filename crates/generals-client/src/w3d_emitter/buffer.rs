//! WW3D's queued births, circular particle storage and lazy kinematic updates.
use super::random::Random;
use generals_assets::w3d::emitter::{Definition, ScalarKey};
use num_traits::ToPrimitive;
use std::{collections::VecDeque, sync::Arc};

pub(super) struct Particle {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub born: u32,
    slot: usize,
}

pub(super) struct Buffer {
    pub definition: Arc<Definition>,
    pub particles: VecDeque<Particle>,
    pub pending: VecDeque<Particle>,
    pub capacity: usize,
    pub max_age: u32,
    pub last_update: u32,
    pub dead: bool,
    next: usize,
    size_random: Vec<f32>,
    acceleration: [f32; 3],
}

impl Buffer {
    pub fn new(definition: Arc<Definition>, random: &mut Random, now: u32) -> Self {
        let emission = &definition.emission;
        let burst = emission
            .burst
            .max(1)
            .to_f32()
            .expect("bounded emitter burst");
        let lifetime = if emission.lifetime > 0.0 {
            emission.lifetime
        } else {
            1.0
        };
        let per_second = burst * emission.rate;
        let capacity = (per_second * (lifetime + 1.0))
            .to_usize()
            .expect("validated emitter size")
            .max(2);
        let capacity = if emission.maximum > 0.0 {
            capacity.min(emission.maximum.to_usize().expect("validated emitter cap"))
        } else {
            capacity
        }
        .max(2);
        let size_random = if definition.properties.size_random.abs() < 1.0e-12 {
            vec![0.0]
        } else {
            let scale = definition.properties.size_random * (1.0 / 2_147_483_648.0_f32);
            (0..capacity.next_power_of_two().min(32))
                .map(|_| random.property().to_f32().expect("native signed random") * scale)
                .collect()
        };
        let acceleration = definition
            .motion
            .acceleration
            .map(|value| value / 1_000_000.0);
        Self {
            definition,
            particles: VecDeque::with_capacity(capacity),
            pending: VecDeque::with_capacity(capacity),
            capacity,
            max_age: (lifetime * 1000.0).to_u32().expect("validated emitter age"),
            last_update: now,
            dead: false,
            next: 0,
            size_random,
            acceleration,
        }
    }

    pub fn enqueue(&mut self, position: [f32; 3], velocity: [f32; 3], born: u32) {
        if self.pending.len() == self.capacity {
            self.pending.pop_front();
        }
        self.pending.push_back(Particle {
            position,
            velocity,
            born,
            slot: 0,
        });
    }

    pub fn update(&mut self, now: u32) {
        let elapsed = now.wrapping_sub(self.last_update);
        if elapsed == 0 {
            return;
        }
        let elapsed = elapsed.to_f32().expect("native elapsed milliseconds");
        // Existing live particles receive a uniform step. New particles instead
        // receive their timestamp-relative initial displacement exactly once.
        self.particles
            .retain(|particle| now.wrapping_sub(particle.born) < self.max_age);
        for particle in &mut self.particles {
            for axis in 0..3 {
                let velocity_step = particle.velocity[axis] * elapsed;
                let half_elapsed = 0.5 * elapsed;
                let elapsed_squared = half_elapsed * elapsed;
                let acceleration_step = self.acceleration[axis] * elapsed_squared;
                let displacement = velocity_step + acceleration_step;
                particle.position[axis] += displacement;
                let acceleration_velocity = self.acceleration[axis] * elapsed;
                particle.velocity[axis] += acceleration_velocity;
            }
        }
        while let Some(mut particle) = self.pending.pop_front() {
            let age = now.wrapping_sub(particle.born);
            if age >= self.max_age {
                continue;
            }
            let age = age.to_f32().expect("native particle age");
            for axis in 0..3 {
                let half_acceleration = 0.5 * self.acceleration[axis];
                let acceleration_step = half_acceleration * age;
                let velocity = particle.velocity[axis] + acceleration_step;
                let displacement = velocity * age;
                particle.position[axis] += displacement;
                let acceleration_velocity = self.acceleration[axis] * age;
                particle.velocity[axis] += acceleration_velocity;
            }
            particle.slot = self.next;
            self.next = (self.next + 1) % self.capacity;
            if self.particles.len() == self.capacity {
                self.particles.pop_front();
            }
            self.particles.push_back(particle);
        }
        self.last_update = now;
    }

    pub fn size(&self, particle: &Particle, now: u32) -> f32 {
        let value = property(
            &self.definition.properties.size,
            now.wrapping_sub(particle.born),
            self.max_age,
        );
        let random = self.size_random[particle.slot & (self.size_random.len() - 1)];
        (value + random).max(0.0)
    }

    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.particles.front()?;
        let mut minimum = first.position;
        let mut maximum = first.position;
        let keys = &self.definition.properties.size;
        let at_end = property(keys, self.max_age, self.max_age);
        let size = keys
            .iter()
            .take_while(|key| key.time * 1000.0 < self.max_age.to_f32().expect("native max age"))
            .map(|key| key.value)
            .fold(at_end, f32::max)
            + self.definition.properties.size_random.abs();
        for particle in &self.particles {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(particle.position[axis]);
                maximum[axis] = maximum[axis].max(particle.position[axis]);
            }
        }
        Some((
            minimum.map(|value| value - size),
            maximum.map(|value| value + size),
        ))
    }
}

fn property(keys: &[ScalarKey], age: u32, max_age: u32) -> f32 {
    let mut start = &keys[0];
    for end in &keys[1..] {
        let end_ms = (end.time * 1000.0)
            .to_u32()
            .expect("validated property time");
        if end_ms >= max_age || age < end_ms {
            let start_ms = (start.time * 1000.0)
                .to_u32()
                .expect("validated property time");
            // The native last segment uses the real key time even when its
            // truncated millisecond is MaxAge; earlier segments use integers.
            let end_time = if end_ms >= max_age {
                end.time * 1000.0
            } else {
                end_ms.to_f32().expect("native key time")
            };
            let start_time = start_ms.to_f32().expect("native key time");
            let slope = (end.value - start.value) / (end_time - start_time);
            let passed = age
                .wrapping_sub(start_ms)
                .to_f32()
                .expect("native particle age");
            let change = slope * passed;
            return start.value + change;
        }
        start = end;
    }
    start.value
}

#[cfg(test)]
mod tests {
    use super::{ScalarKey, property};

    #[test]
    fn source_integer_segments_and_real_last_segment_ignore_unreachable_later_keys() {
        let keys = [
            ScalarKey {
                time: 0.0,
                value: 2.0,
            },
            ScalarKey {
                time: 0.0339,
                value: 4.0,
            },
            ScalarKey {
                time: 0.1009,
                value: 8.0,
            },
            ScalarKey {
                time: 0.5,
                value: 1000.0,
            },
        ];
        let integer_slope = (4.0_f32 - 2.0) / 33.0;
        let first_change = integer_slope * 16.0;
        assert_eq!(
            property(&keys, 16, 100).to_bits(),
            (2.0 + first_change).to_bits()
        );
        let real_end = 0.1009_f32 * 1000.0;
        let last_slope = (8.0 - 4.0) / (real_end - 33.0);
        let last_change = last_slope * 67.0;
        assert_eq!(
            property(&keys, 100, 100).to_bits(),
            (4.0 + last_change).to_bits()
        );
    }
}
