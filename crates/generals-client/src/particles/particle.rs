use super::{Particle, Shader, Template};

impl Particle {
    pub(super) fn alpha_rate(&mut self) {
        let index = self.alpha_target;
        self.alpha_rate = if index >= 8 || self.alpha_keys[index].1 == 0 {
            0.0
        } else {
            let time = self.alpha_keys[index]
                .1
                .wrapping_sub(self.alpha_keys[index - 1].1);
            (self.alpha_keys[index].0 - self.alpha_keys[index - 1].0) / frame_duration(time)
        };
    }
    pub(super) fn color_rate(&mut self) {
        let index = self.color_target;
        self.color_rate = if index >= 8 || self.color_keys[index].1 == 0 {
            [0.0; 3]
        } else {
            // Particle::computeColorRate deliberately retains unsigned32
            // wrap for descending authored keys, including CleanupTrail01.
            let time = self.color_keys[index]
                .1
                .wrapping_sub(self.color_keys[index - 1].1);
            let time = frame_duration(time);
            std::array::from_fn(|axis| {
                (self.color_keys[index].0[axis] - self.color_keys[index - 1].0[axis]) / time
            })
        };
    }
    pub(super) fn update(&mut self, template: &Template, frame: u64) -> bool {
        // Native zero is an indefinite lifetime, still subject to color/alpha expiry.
        if self.remaining > 0.0 {
            self.remaining -= 1.0;
            if self.remaining == 0.0 {
                return false;
            }
        }
        self.velocity[2] += template.gravity;
        self.velocity = self.velocity.map(|v| v * self.damping);
        let elapsed = frame - self.born;
        if elapsed == 0 {
            return true;
        }
        self.angular_rate *= self.angular_damping;
        self.size_rate *= self.size_damping;
        if !matches!(template.drawing.shader, Shader::Additive) {
            // PRESERVE_RETAIL_PARTICLES=1 retains the original early ramp/hold.
            if self.alpha_target == 1 && elapsed == u64::from(self.alpha_keys[1].1) {
                self.alpha = self.alpha_keys[1].0;
                self.alpha_rate = 0.0;
            }
            if self.alpha_target < 8 && self.alpha_keys[self.alpha_target].1 != 0 {
                if elapsed > u64::from(self.alpha_keys[self.alpha_target].1) {
                    self.alpha_target += 1;
                    self.alpha_rate();
                }
            } else {
                self.alpha_rate = 0.0;
            }
        }
        if self.color_target < 8 && self.color_keys[self.color_target].1 != 0 {
            if elapsed > u64::from(self.color_keys[self.color_target].1) {
                self.color_target += 1;
                self.color_rate();
            }
        } else {
            self.color_rate = [0.0; 3];
        }
        if matches!(template.drawing.shader, Shader::AlphaTest) {
            true
        } else if matches!(template.drawing.shader, Shader::Additive) {
            !(self.color_target >= 8 || self.color_keys[self.color_target].1 == 0)
                || self.color.iter().any(|&v| v >= 0.01)
        } else {
            !(self.alpha_target >= 8 || self.alpha_keys[self.alpha_target].1 == 0)
                || self.alpha >= 0.01
        }
    }
    pub(super) fn draw(&mut self, template: &Template, time: f32, wind: Option<(f32, [f32; 3])>) {
        for (index, position) in self.position.iter_mut().enumerate() {
            let step = (self.velocity[index] + template.drift[index]) * time;
            *position += step;
        }
        if let Some((angle, origin)) = wind {
            super::wind::move_particle(
                &mut self.position,
                origin,
                angle,
                self.wind_randomness,
                time,
            );
        }
        let rotation = self.angular_rate * time;
        self.angle += rotation;
        if let Some(emitter) = self.up_emitter {
            self.angle =
                emitter_angle(self.position[0] - emitter[0], self.position[1] - emitter[1]);
        }
        let growth = self.size_rate * time;
        self.size += growth;
        if !matches!(template.drawing.shader, Shader::Additive) {
            let delta = self.alpha_rate * time;
            self.alpha = (self.alpha + delta).clamp(0.0, 1.0);
        }
        for (value, rate) in self.color.iter_mut().zip(self.color_rate) {
            let delta = rate * time;
            *value += delta;
            let scale = self.color_scale * time;
            *value = (*value + scale).clamp(0.0, 1.0);
        }
    }
}

fn frame_duration(time: u32) -> f32 {
    let bytes = time.to_le_bytes();
    let upper = f32::from(u16::from_le_bytes([bytes[2], bytes[3]])) * 65_536.0;
    upper + f32::from(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn emitter_angle(x: f32, y: f32) -> f32 {
    // ParticleSys.cpp::angleBetween with its fixed up vector (0,1). Preserve
    // the native orthogonal special case instead of substituting atan2.
    let squares = [x * x, y * y];
    let length = (squares[0] + squares[1]).sqrt();
    let theta = if length == 0.0 {
        0.0
    } else if y == 0.0 {
        if x > 0.0 { std::f32::consts::PI } else { 0.0 }
    } else {
        let theta = (y / length).acos();
        if x > 0.0 { theta } else { -theta }
    };
    theta + std::f32::consts::PI
}

#[cfg(test)]
mod tests;
