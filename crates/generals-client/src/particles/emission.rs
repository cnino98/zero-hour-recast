use super::{Emitter, Particle, Template, Velocity, Volume, normalize};

fn sphere(random: &mut impl FnMut(f32, f32) -> f32) -> [f32; 3] {
    // computePointOnUnitSphere retries only the zero vector, then normalizes.
    loop {
        let point = std::array::from_fn(|_| random(-1.0, 1.0));
        if point.iter().any(|&v| v != 0.0) {
            return normalize(point);
        }
    }
}

fn position(
    volume: &Volume,
    override_radius: Option<f32>,
    random: &mut impl FnMut(f32, f32) -> f32,
) -> [f32; 3] {
    match volume {
        Volume::None | Volume::Point => [0.0; 3],
        Volume::Box(half_size) => half_size.map(|half| random(-half, half)),
        Volume::Line(start, end) => {
            let t = random(0.0, 1.0);
            std::array::from_fn(|i| {
                let offset = t * (end[i] - start[i]);
                start[i] + offset
            })
        }
        Volume::Sphere { radius, hollow } => {
            let radius = override_radius.unwrap_or(*radius);
            let radius = if *hollow { radius } else { random(0.0, radius) };
            sphere(random).map(|v| v * radius)
        }
        Volume::Cylinder {
            radius,
            length,
            hollow,
        } => {
            let radius = override_radius.unwrap_or(*radius);
            let angle = random(0.0, std::f32::consts::TAU);
            let radius = if *hollow { radius } else { random(0.0, radius) };
            [
                radius * angle.cos(),
                radius * angle.sin(),
                random(-length / 2.0, length / 2.0),
            ]
        }
    }
}

fn velocity(
    template: &Template,
    position: [f32; 3],
    random: &mut impl FnMut(f32, f32) -> f32,
) -> [f32; 3] {
    match &template.velocity {
        Velocity::None => [0.0; 3],
        Velocity::Ortho(ranges) => ranges.each_ref().map(|range| range.sample(random)),
        Velocity::Cylindrical(radial, normal) => {
            let radial_speed = radial.sample(random);
            let angle = random(0.0, std::f32::consts::TAU);
            [
                radial_speed * angle.cos(),
                radial_speed * angle.sin(),
                normal.sample(random),
            ]
        }
        Velocity::Spherical(speed) => {
            let speed = speed.sample(random);
            sphere(random).map(|v| v * speed)
        }
        Velocity::Hemispherical(speed) => {
            let speed = speed.sample(random);
            loop {
                let point = [random(-1.0, 1.0), random(-1.0, 1.0), random(0.0, 1.0)];
                if point.iter().any(|&value| value != 0.0) {
                    break normalize(point).map(|value| speed * value);
                }
            }
        }
        Velocity::Outward(speed, other) => {
            let speed = speed.sample(random);
            let other = other.sample(random);
            match &template.volume {
                Volume::None => unreachable!("NONE volume with OUTWARD velocity is not admitted"),
                Volume::Point => sphere(random).map(|v| v * speed),
                Volume::Sphere { .. } | Volume::Box(_) => normalize(position).map(|v| v * speed),
                Volume::Cylinder { .. } => {
                    let direction = normalize([position[0], position[1], 0.0]);
                    [speed * direction[0], speed * direction[1], other]
                }
                Volume::Line(start, end) => {
                    let along = normalize(std::array::from_fn(|i| end[i] - start[i]));
                    let perpendicular = [-along[1], along[0], 0.0];
                    let squares = along.map(|value| value * value);
                    let up = [
                        -along[2] * along[0],
                        -along[2] * along[1],
                        squares[0] + squares[1],
                    ];
                    std::array::from_fn(|i| {
                        let a = speed * perpendicular[i];
                        let b = other * up[i];
                        a + b
                    })
                }
            }
        }
    }
}

impl Emitter {
    pub(super) fn generate(
        &mut self,
        template: &Template,
        scale: f32,
        number: u16,
        count: u16,
        frame: u64,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) -> Particle {
        let factor = 0.5 + scale / 2.0;
        let local = position(&template.volume, self.radius, random).map(|v| v * factor);
        let velocity = velocity(template, local, random).map(|v| v * factor);
        let previous = *self.previous.get_or_insert(self.transform.position);
        let step = 1.0 - f32::from(number) / f32::from(count);
        let rotated = self.transform.rotate(local);
        let position = std::array::from_fn(|i| {
            let adjustment = step * (self.transform.position[i] - previous[i]);
            self.transform.position[i] + rotated[i] - adjustment
        });
        let velocity = self.transform.rotate(velocity);
        let damping = template.velocity_damping.sample(random);
        let angular_damping = template.angular_damping.sample(random);
        let angle = template.angle.sample(random);
        let angular_rate = template.angular_rate.sample(random);
        let remaining = self
            .lifetime
            .unwrap_or_else(|| template.lifetime.sample(random))
            .trunc();
        let base_size = template.size.sample(random) * scale;
        let size_rate = template.size_rate.sample(random) * scale;
        let size_damping = template.size_rate_damping.sample(random);
        let size = base_size + self.size_bonus;
        self.size_bonus += template.start_size_rate.sample(random);
        self.size_bonus = self.size_bonus.min(50.0);
        let alpha_keys = std::array::from_fn(|i| {
            (
                template.alpha[i].value.sample(random),
                template.alpha[i].frame,
            )
        });
        let color_keys =
            std::array::from_fn(|i| (template.color[i].value, template.color[i].frame));
        let color_scale = random(
            template.color_scale.low / 255.0,
            template.color_scale.high / 255.0,
        );
        let wind_randomness = random(0.7, 1.3);
        let mut particle = Particle {
            controlled: None,
            position,
            up_emitter: template
                .up_towards_emitter
                .then_some([self.transform.position[0], self.transform.position[1]]),
            velocity,
            damping,
            remaining,
            born: frame,
            angle,
            angular_rate,
            angular_damping,
            size,
            size_rate,
            size_damping,
            alpha: alpha_keys[0].0,
            alpha_keys,
            alpha_rate: 0.0,
            alpha_target: 1,
            color: color_keys[0].0,
            color_keys,
            color_rate: [0.0; 3],
            color_target: 1,
            color_scale,
            personality: self.personality,
            wind_randomness,
        };
        particle.alpha_rate();
        particle.color_rate();
        particle
    }
}

impl Particle {
    pub(super) fn merge(&mut self, slave: &Self, offset: [f32; 3]) {
        // mergeRelatedParticleSystems: a fresh master info, not the existing
        // master's emitted particle, is promoted using the slave's attributes.
        self.remaining = slave.remaining;
        self.size *= slave.size;
        self.size_rate *= slave.size_rate;
        self.size_damping *= slave.size_damping;
        self.angle = slave.angle;
        self.angular_rate = slave.angular_rate;
        self.angular_damping = slave.angular_damping;
        self.alpha_keys = slave.alpha_keys;
        self.color_keys = slave.color_keys;
        self.color_scale = slave.color_scale;
        self.alpha = self.alpha_keys[0].0;
        self.color = self.color_keys[0].0;
        self.alpha_rate();
        self.color_rate();
        for (position, offset) in self.position.iter_mut().zip(offset) {
            *position += offset;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{Range, tests::template};

    #[test]
    fn none_emission_keeps_local_origin_and_zero_velocity_without_random_draws() {
        let mut rules = template("none");
        rules.volume = Volume::None;
        rules.velocity = Velocity::None;
        let mut random = |_, _| panic!("native NONE position and velocity consume no RNG");
        let local = position(&rules.volume, Some(400.0), &mut random);
        assert_eq!(local.map(f32::to_bits), [0.0_f32; 3].map(f32::to_bits));
        assert_eq!(
            velocity(&rules, local, &mut random).map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
    }

    #[test]
    fn hemispherical_velocity_samples_speed_then_retries_zero_direction_even_for_zero_speed() {
        for speed in [0.0, 2.0] {
            let mut rules = template("hemisphere");
            rules.velocity = Velocity::Hemispherical(Range {
                low: speed,
                high: speed,
            });
            let mut draws = Vec::new();
            let samples = [speed, 0.0, 0.0, 0.0, -0.3, 0.4, 0.5];
            let actual = velocity(&rules, [0.0; 3], &mut |low, high| {
                let result = samples[draws.len()];
                draws.push([low, high].map(f32::to_bits));
                result
            });
            assert_eq!(
                draws,
                [
                    [speed, speed],
                    [-1.0, 1.0],
                    [-1.0, 1.0],
                    [0.0, 1.0],
                    [-1.0, 1.0],
                    [-1.0, 1.0],
                    [0.0, 1.0]
                ]
                .map(|range| range.map(f32::to_bits))
            );
            let expected = normalize([-0.3, 0.4, 0.5]).map(|value| value * speed);
            assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
            assert!(actual[2] >= 0.0);
        }
    }

    #[test]
    fn non_hollow_box_samples_xyz_half_extents_without_radial_override() {
        let mut draws = Vec::new();
        let actual = position(
            &Volume::Box([10.0, 5.0, 3.0]),
            Some(400.0),
            &mut |low, high| {
                draws.push([low, high].map(f32::to_bits));
                low
            },
        );
        assert_eq!(
            draws,
            [[-10.0_f32, 10.0], [-5.0, 5.0], [-3.0, 3.0]].map(|range| range.map(f32::to_bits))
        );
        assert_eq!(
            actual.map(f32::to_bits),
            [-10.0_f32, -5.0, -3.0].map(f32::to_bits)
        );
    }
}
