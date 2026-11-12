//! `ScriptEngine::reset/setSway`: native short period/version and unbounded authored angles.
use super::State;
use crate::skirmish::{Entity, Skirmish};
use std::f32::consts::PI;

/// Snapshot used by client updates while they mutate drawable instance matrices.
#[derive(Clone, Copy)]
pub struct Breeze {
    pub direction: [f32; 2],
    pub intensity: f32,
    pub lean: f32,
    pub randomness: f32,
    pub period: i16,
    pub version: i16,
}

impl Breeze {
    pub(super) fn native() -> Self {
        let direction = PI / 3.0;
        Self {
            direction: [direction.sin(), direction.cos()],
            intensity: (0.07 * PI) / 4.0,
            lean: (0.07 * PI) / 4.0,
            randomness: 0.2,
            period: 150,
            version: 0,
        }
    }
}

impl State {
    pub(super) fn set_sway(&mut self, angles: [f32; 3], period: i32, randomness: f32) {
        let bytes = period.to_le_bytes();
        self.breeze.version = self.breeze.version.wrapping_add(1);
        self.breeze.direction = [angles[0].sin(), angles[0].cos()];
        self.breeze.intensity = angles[1];
        self.breeze.lean = angles[2];
        self.breeze.period = i16::from_le_bytes([bytes[0], bytes[1]]).max(1);
        self.breeze.randomness = randomness;
    }
}

impl Skirmish {
    #[must_use]
    pub const fn breeze(&self) -> Breeze {
        self.map_scripts.breeze
    }
}

impl Entity {
    /// Client updates share `Drawable`'s instance matrix with logic-side contain modules.
    pub const fn set_client_instance_matrix(&mut self, matrix: [[f32; 4]; 3]) {
        self.drawable.instance_matrix = Some(matrix);
    }

    #[must_use]
    pub const fn burned(&self) -> bool {
        self.flammable.burned
    }
}
