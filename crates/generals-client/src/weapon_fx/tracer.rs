//! Original tracer drawable state and `Line3DClass` solid prism topology.
use super::TracerRules;
use super::line::axes;
use bevy::prelude::*;

#[derive(Component)]
pub struct TracerVisual {
    pub source: usize,
    pub length: f32,
    pub width: f32,
    pub color: [f32; 3],
    pub opacity: f32,
    pub origin: [f32; 3],
    pub born: u64,
    pub(super) generation: u32,
    position: [f32; 3],
    axes: [[f32; 3]; 3],
    speed: f32,
    frames: f32,
}

impl TracerVisual {
    pub(super) fn new(
        rules: &TracerRules,
        primary_speed: f32,
        primary: [f32; 3],
        secondary: [f32; 3],
        source: usize,
        born: u64,
        generation: u32,
    ) -> Result<Self, &'static str> {
        let delta = std::array::from_fn::<_, 3, _>(|axis| secondary[axis] - primary[axis]);
        let squared = delta.map(|value| value * value);
        let distance = (squared[0] + squared[1] + squared[2]).sqrt();
        let speed = if rules.speed == 0.0 {
            primary_speed
        } else {
            rules.speed
        };
        let remaining = distance - rules.length;
        let frames = if remaining >= 0.0 && speed >= 0.0 {
            remaining / speed
        } else {
            1.0
        };
        let frames = (frames * rules.decay_at).ceil();
        if !frames.is_finite() || !(0.0..=65_535.0).contains(&frames) {
            return Err("tracer lifetime is outside the supported finite client range");
        }
        let direction = if distance == 0.0 {
            delta
        } else {
            let inverse = 1.0 / distance;
            delta.map(|value| value * inverse)
        };
        Ok(Self {
            source,
            length: rules.length,
            width: rules.width,
            color: rules.color,
            opacity: 1.0,
            origin: primary,
            position: primary,
            axes: axes(direction),
            speed,
            born,
            frames,
            generation,
        })
    }

    fn remaining(&self, now: u64) -> f32 {
        let elapsed = u16::try_from(now.saturating_sub(self.born)).map_or(f32::INFINITY, f32::from);
        self.frames - elapsed
    }

    pub(super) fn expired(&self, now: u64) -> bool {
        // ExpirationDate zero is the native drawable's "never expire" sentinel.
        (self.born != 0 || self.frames != 0.0) && self.remaining(now) <= 0.0
    }

    pub(super) fn draw(&mut self, now: u64) {
        if self.born != 0 || self.frames != 0.0 {
            self.opacity -= self.opacity / self.remaining(now);
        }
        for (value, direction) in self.position.iter_mut().zip(self.axes[0]) {
            let movement = direction * self.speed;
            *value += movement;
        }
    }

    pub(super) fn vertices(&self) -> Vec<[f32; 3]> {
        super::line::vertices(self.axes, self.length, self.width)
    }

    pub(super) fn transform(&self) -> Transform {
        Transform::from_xyz(
            self.position[0] * 0.1,
            self.position[2] * 0.1,
            -self.position[1] * 0.1,
        )
    }

    pub(super) fn mesh(&self) -> Mesh {
        super::line::mesh(self.vertices())
    }
}

#[cfg(test)]
mod tests {
    use super::{TracerRules, TracerVisual};
    const fn rules() -> TracerRules {
        TracerRules {
            speed: 20.0,
            decay_at: 1.0,
            length: 10.0,
            width: 0.3,
            probability: 0.5,
            color: [1.0; 3],
        }
    }

    #[test]
    fn each_draw_moves_one_source_step_and_fades_again_at_the_same_logic_frame() {
        let mut tracer =
            TracerVisual::new(&rules(), 0.0, [0.0; 3], [100.0, 0.0, 0.0], 0, 10, 0).unwrap();
        tracer.draw(10);
        assert_eq!(tracer.position[0].to_bits(), 20.0_f32.to_bits());
        assert!((tracer.opacity - 0.8).abs() < 0.000_001);
        tracer.draw(10);
        assert_eq!(tracer.position[0].to_bits(), 40.0_f32.to_bits());
        assert!((tracer.opacity - 0.64).abs() < 0.000_001);
        assert!(!tracer.expired(14));
        assert!(tracer.expired(15));
    }

    #[test]
    fn near_target_uses_one_frame_but_exact_length_expires_before_first_draw() {
        let near = TracerVisual::new(&rules(), 0.0, [0.0; 3], [9.0, 0.0, 0.0], 0, 10, 0).unwrap();
        assert!(!near.expired(10));
        assert!(near.expired(11));
        let exact = TracerVisual::new(&rules(), 0.0, [0.0; 3], [10.0, 0.0, 0.0], 0, 10, 0).unwrap();
        assert!(exact.expired(10));
    }

    #[test]
    fn tilted_prism_keeps_original_width_length_and_closed_triangle_faces() {
        let tracer = TracerVisual::new(
            &rules(),
            0.0,
            [5.0, 10.0, 3.0],
            [50.0, 40.0, 20.0],
            0,
            10,
            0,
        )
        .unwrap();
        let mesh = tracer.mesh();
        let points = tracer.vertices();
        assert_eq!(mesh.count_vertices(), 8);
        assert_eq!(mesh.indices().unwrap().len(), 36);
        let edge = |a: usize, b: usize| {
            (bevy::prelude::Vec3::from_array(points[a])
                - bevy::prelude::Vec3::from_array(points[b]))
            .length()
        };
        assert!((edge(0, 1) - 0.03).abs() < 0.000_001);
        assert!((edge(0, 2) - 0.03).abs() < 0.000_001);
        assert!((edge(0, 4) - 1.0).abs() < 0.000_001);
    }
}
