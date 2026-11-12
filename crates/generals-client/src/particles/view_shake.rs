//! `ViewShakeFXNugget` and fixed-step `W3DView` spring/damper, SH 2bfaea60.
use super::Particles;
use crate::{
    playtest::{CameraRig, Match},
    skirmish_data::{field, scalar},
};
use bevy::prelude::*;
use generals_assets::content::ContentLine;
use std::error::Error;

pub(super) struct Shake {
    intensities: [f32; 6],
    maximum: f32,
    range: f32,
    focus: [f32; 2],
    intensity: f32,
    direction: [f32; 2],
    offset: [f32; 2],
}

impl Shake {
    pub(super) const fn new() -> Self {
        Self {
            intensities: [0.5, 1.0, 2.5, 5.0, 8.0, 12.0],
            maximum: 10.0,
            range: 150.0,
            focus: [0.0; 2],
            intensity: 0.0,
            direction: [0.0; 2],
            offset: [0.0; 2],
        }
    }

    pub(super) fn load(lines: &[ContentLine]) -> Result<Self, Box<dyn Error>> {
        let mut result = Self::new();
        let number = |name, fallback| {
            field(lines, 0, name).map_or(Ok(fallback), |line| super::data::real(scalar(line)?))
        };
        for (index, key) in [
            "ShakeSubtleIntensity",
            "ShakeNormalIntensity",
            "ShakeStrongIntensity",
            "ShakeSevereIntensity",
            "ShakeCineExtremeIntensity",
            "ShakeCineInsaneIntensity",
        ]
        .into_iter()
        .enumerate()
        {
            result.intensities[index] = number(key, result.intensities[index])?;
        }
        result.maximum = number("MaxShakeIntensity", result.maximum)?;
        result.range = number("MaxShakeRange", result.range)?;
        if result.range <= 0.0 {
            return Err("MaxShakeRange must be positive".into());
        }
        Ok(result)
    }

    pub(super) const fn reset(&mut self) {
        self.intensity = 0.0;
        self.direction = [0.0; 2];
        self.offset = [0.0; 2];
    }

    pub(super) fn impulse(&mut self, kind: usize, position: [f32; 3], angle: f32) {
        // Source changes direction before rejecting a distant epicenter.
        let (sine, cosine) = angle.sin_cos();
        self.direction = [cosine, sine];
        let x = position[0] - self.focus[0];
        let y = position[1] - self.focus[1];
        let squares = [x * x, y * y];
        let distance = (squares[0] + squares[1]).sqrt();
        if distance > self.range {
            return;
        }
        let intensity = self.intensities[kind] * (1.0 - distance / self.range);
        self.intensity += intensity;
        // Preserve W3DView::shake's literal 3.0, not the configured threshold.
        if self.intensity > self.maximum {
            self.intensity = 3.0;
        }
    }

    pub(super) fn advance(&mut self) {
        if self.intensity > 0.01 {
            self.offset = self.direction.map(|axis| self.intensity * axis);
            self.intensity *= 0.75;
            self.direction = self.direction.map(|axis| -axis);
        } else {
            self.intensity = 0.0;
            self.offset = [0.0; 2];
        }
    }
}

pub(super) fn kind(value: Option<&str>) -> Result<usize, Box<dyn Error>> {
    match value.unwrap_or("NORMAL").to_ascii_uppercase().as_str() {
        "SUBTLE" => Ok(0),
        "NORMAL" => Ok(1),
        "STRONG" => Ok(2),
        "SEVERE" => Ok(3),
        "CINE_EXTREME" => Ok(4),
        "CINE_INSANE" => Ok(5),
        _ => Err("unknown native ViewShake Type".into()),
    }
}

impl Particles {
    #[cfg(test)]
    pub fn test_normal_camera_impulse(&mut self, focus: Vec2, angle: f32) {
        self.shake.reset();
        self.shake.focus = (focus * 10.0).to_array();
        let position = [self.shake.focus[0], self.shake.focus[1], 0.0];
        self.shake.impulse(1, position, angle);
        self.shake.advance();
    }

    pub fn camera_shake(&self) -> Vec2 {
        Vec2::from_array(self.shake.offset) * 0.1
    }
}

pub fn prepare(battle: Res<Match>, rig: Res<CameraRig>, mut particles: ResMut<Particles>) {
    let battle = battle.into_inner();
    let rig = rig.into_inner();
    if battle.started {
        particles.begin(battle.generation, battle.simulation.tick);
        particles.shake.focus = (rig.focus * 10.0).to_array();
    }
}

#[cfg(test)]
mod tests;
