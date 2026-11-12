//! `ParticleSystem::updateWindMotion` and `Particle::doWindMotion`.
use super::Template;

pub enum Motion {
    Unused,
    PingPong,
    Circular,
}

impl Motion {
    pub fn parse(value: &str) -> Result<Self, String> {
        if value.eq_ignore_ascii_case("Unused") {
            Ok(Self::Unused)
        } else if value.eq_ignore_ascii_case("PingPong") {
            Ok(Self::PingPong)
        } else if value.eq_ignore_ascii_case("Circular") {
            Ok(Self::Circular)
        } else {
            Err(format!("unknown particle WindMotion {value}"))
        }
    }
}

pub struct State {
    pub angle: f32,
    change: f32,
    start: f32,
    end: f32,
    towards_end: bool,
}

impl State {
    pub const fn new(start: f32, end: f32, angle: f32) -> Self {
        Self {
            angle,
            change: 0.15,
            start,
            end,
            towards_end: true,
        }
    }

    pub fn update(&mut self, rules: &Template, random: &mut impl FnMut(f32, f32) -> f32) {
        match rules.wind_motion {
            Motion::Unused => {}
            Motion::PingPong => {
                let span = self.end - self.start;
                let half = span / 2.0;
                let distance = (half - self.angle + self.start).abs();
                let mut change = (1.0 - distance / half) * self.change;
                if change < 0.005 {
                    change = 0.005;
                }
                let reverse = if self.towards_end {
                    self.angle += change;
                    self.angle >= self.end
                } else {
                    self.angle -= change;
                    self.angle <= self.start
                };
                if reverse {
                    self.towards_end = !self.towards_end;
                    self.change = rules.wind_change.sample(random);
                    self.start = rules.wind_start.sample(random);
                    self.end = rules.wind_end.sample(random);
                }
            }
            Motion::Circular => {
                if self.change == 0.0 {
                    self.change = rules.wind_change.sample(random);
                }
                self.angle += self.change;
                if self.angle > std::f32::consts::TAU {
                    self.angle -= std::f32::consts::TAU;
                } else if self.angle < 0.0 {
                    self.angle += std::f32::consts::TAU;
                }
            }
        }
    }
}

pub fn move_particle(
    position: &mut [f32; 3],
    origin: [f32; 3],
    angle: f32,
    randomness: f32,
    time: f32,
) {
    let squares = std::array::from_fn::<_, 3, _>(|axis| {
        let offset = position[axis] - origin[axis];
        offset * offset
    });
    let distance = (squares[0] + squares[1] + squares[2]).sqrt();
    if distance < 200.0 {
        let mut force = 2.0 * randomness * time;
        if distance > 75.0 {
            force *= 1.0 - (distance - 75.0) / (200.0 - 75.0);
        }
        let step = [angle.cos() * force, angle.sin() * force];
        position[0] += step[0];
        position[1] += step[1];
    }
}

#[cfg(test)]
mod tests;
