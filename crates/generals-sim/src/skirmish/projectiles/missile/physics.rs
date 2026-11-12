use super::ProjectilePhysics;
use crate::skirmish::projectiles::Motion;

impl Motion {
    pub(super) fn motive_force(&mut self, force: [f32; 3], rules: &ProjectilePhysics, frame: u32) {
        // applyMotiveForce temporarily disables the motive lateral-force filter.
        self.motive_expires = 0;
        self.apply_force(force, rules.mass, frame);
        self.force_wake |= !force.iter().any(|value| value.is_nan());
        self.motive_expires = frame.wrapping_add(10);
    }

    pub(in crate::skirmish) fn apply_force(&mut self, mut force: [f32; 3], mass: f32, frame: u32) {
        if force.iter().any(|value| value.is_nan()) {
            return;
        }
        if self.motive_expires > frame && (force[0] != 0.0 || force[1] != 0.0) {
            let direction = self.direction_2d();
            let x = force[0] * -direction[1];
            let y = force[1] * direction[0];
            let lateral = x + y;
            force[0] = lateral * -direction[1];
            force[1] = lateral * direction[0];
        }
        let inverse_mass = 1.0 / mass;
        for (acceleration, force) in self.acceleration.iter_mut().zip(force) {
            let scaled = force * inverse_mass;
            *acceleration += scaled;
        }
    }

    fn direction_2d(&self) -> [f32; 2] {
        let angle = self.axes[0][1].atan2(self.axes[0][0]);
        [angle.cos(), angle.sin()]
    }

    fn friction(&mut self, rules: &ProjectilePhysics, frame: u32, surface: f32) {
        let airborne = self.position[2] - surface > -9.0 * rules.gravity;
        if !self.airborne_friction && airborne {
            let aerodynamic = -(rules.aerodynamic_friction + rules.extra_friction).clamp(0.0, 0.99);
            for (acceleration, velocity) in self.acceleration.iter_mut().zip(self.velocity) {
                let force = velocity * aerodynamic;
                *acceleration += force;
            }
        } else if self.velocity[0] != 0.0 || self.velocity[1] != 0.0 {
            let direction = self.direction_2d();
            let lateral_x = self.velocity[0] * -direction[1];
            let lateral_y = self.velocity[1] * direction[0];
            let lateral_dot = lateral_x + lateral_y;
            let lateral = [lateral_dot * -direction[1], lateral_dot * direction[0]];
            let friction =
                rules.mass * (rules.lateral_friction + rules.extra_friction).clamp(0.01, 0.99);
            let mut force = [-(friction * lateral[0]), -(friction * lateral[1]), 0.0];
            if self.motive_expires <= frame {
                let forward_x = self.velocity[0] * direction[0];
                let forward_y = self.velocity[1] * direction[1];
                let forward_dot = forward_x + forward_y;
                let friction =
                    rules.mass * (rules.forward_friction + rules.extra_friction).clamp(0.01, 0.99);
                for axis in 0..2 {
                    let forward = forward_dot * direction[axis];
                    force[axis] += -(friction * forward);
                }
            }
            self.apply_force(force, rules.mass, frame);
        }
    }

    // Returns the source airborne-to-ground transition, not every grounded tick.
    pub(in crate::skirmish::projectiles) fn integrate(
        &mut self,
        rules: &ProjectilePhysics,
        frame: u32,
        braking: bool,
        height: impl Fn([f32; 2]) -> f32,
        terrain_height: impl Fn([f32; 2]) -> f32,
    ) -> bool {
        let was_airborne = self.airborne_last.unwrap_or_else(|| {
            self.position[2] > terrain_height([self.position[0], self.position[1]])
        });
        self.acceleration[2] += rules.gravity;
        self.friction(
            rules,
            frame,
            terrain_height([self.position[0], self.position[1]]),
        );
        for (velocity, acceleration) in self.velocity.iter_mut().zip(self.acceleration) {
            *velocity += acceleration;
            if velocity.abs() < 0.001 {
                *velocity = 0.0;
            }
        }
        if !braking {
            for (position, velocity) in self.position.iter_mut().zip(self.velocity) {
                *position += velocity;
            }
        }
        let ground = height([self.position[0], self.position[1]]);
        if self.position[2] <= ground {
            self.velocity[2] += ground - self.position[2];
            if self.velocity[2] > 0.0 {
                self.velocity[2] = 0.0;
            }
            self.position[2] = ground;
        }
        self.acceleration = [0.0; 3];
        let airborne = self.position[2] > terrain_height([self.position[0], self.position[1]]);
        self.airborne_last = Some(airborne);
        was_airborne && !airborne
    }
}
