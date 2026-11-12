//! `PhysicsBehavior`'s existing ground body after `SlowDeath` applies its fling.
use super::{Physics, Skirmish, startup};
use std::f32::consts::PI;

#[cfg(test)]
mod tests;

pub(in crate::skirmish) struct Body {
    pub(in crate::skirmish) height: f32,
    velocity: [f32; 3],
    acceleration: [f32; 3],
    motive_until: u64,
    extra_friction: f32,
    braking: bool,
    falling: bool,
    bouncing: bool,
}

impl Skirmish {
    pub(in crate::skirmish::death_lifecycle) fn initialize_death_body(&mut self, id: usize) {
        if self.entities[id].physical.is_some() {
            return;
        }
        if self.catalog[self.entities[id].type_index]
            .death_lifecycle
            .as_ref()
            .is_none_or(|rules| rules.physics.is_none())
        {
            return;
        }
        if self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("lifecycle")
            .body
            .is_some()
        {
            return;
        }
        let velocity = self.entities[id].physics_velocity();
        let falling = self.entities[id].physics_allows_fall();
        let height = self.object_position(id)[2];
        let actor = &mut self.entities[id];
        actor.death_lifecycle.as_mut().expect("lifecycle").body = Some(Body {
            height,
            velocity,
            acceleration: actor.live_physics.as_ref().map_or(
                [
                    actor.ai.motion.acceleration[0],
                    actor.ai.motion.acceleration[1],
                    0.0,
                ],
                |state| state.motion.acceleration,
            ),
            motive_until: actor.ai.motion.motive_until,
            extra_friction: self.catalog[actor.type_index]
                .locomotor
                .ground
                .as_ref()
                .map_or(0.0, |ground| {
                    ground.extra_friction
                        + if actor.ai.motion.ultra_accurate() {
                            0.5
                        } else {
                            0.0
                        }
                }),
            braking: actor.ai.motion.object_braking(),
            falling,
            bouncing: actor
                .live_physics
                .as_ref()
                .is_some_and(|state| state.motion.bouncing),
        });
    }

    pub(in crate::skirmish::death_lifecycle) fn fling_corpse(&mut self, id: usize, index: usize) {
        let ground = self.ground_height(self.entities[id].position);
        let actor = &mut self.entities[id];
        let rules = self.catalog[actor.type_index]
            .death_lifecycle
            .as_ref()
            .expect("death rules");
        let fling = rules.slow[index].fling.as_ref().expect("fling");
        // calcRandomForce: angle, pitch, magnitude; Matrix3D Scale, Rotate_Z, Rotate_Y.
        let angle = startup::random_real(&mut self.random, -PI, PI);
        let pitch = startup::random_real(
            &mut self.random,
            fling.pitch[0],
            fling.pitch[0] + fling.pitch[1],
        );
        let magnitude = startup::random_real(
            &mut self.random,
            fling.force[0],
            fling.force[0] + fling.force[1],
        );
        let force = [
            pitch.cos() * (angle.cos() * magnitude),
            pitch.cos() * (angle.sin() * magnitude),
            -(-pitch).sin() * magnitude,
        ];
        let state = actor
            .death_lifecycle
            .as_mut()
            .expect("lifecycle")
            .body
            .as_mut()
            .expect("body");
        if state.height - ground < 1.0 {
            state.height += 1.0;
        }
        state.falling = true;
        state.bouncing = true;
        state.extra_friction = -3.0 * super::super::STEP_SECONDS;
        apply_force(
            state,
            actor.facing,
            force,
            rules.physics.as_ref().expect("fling physics").mass,
            self.tick,
        );
        self.set_object_orientation(id, force[1].atan2(force[0]));
        self.entities[id].set_physics_allow_fall(true);
        self.wake_family_update(id, "PhysicsBehavior", 0, 1);
    }

    pub(in crate::skirmish) fn update_dead_ground(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        if actor.masked()
            || actor.alive()
            || self.catalog[actor.type_index].locomotor.ground.is_none()
        {
            return;
        }
        let Some(state) = &mut actor.death_lifecycle else {
            return;
        };
        if state.ai_frame == Some(self.tick) {
            return;
        }
        let Some(body) = &mut state.body else {
            return;
        };
        state.ai_frame = Some(self.tick);
        // AIDeadState::update. doLocomotor resets physics options, but skips
        // movement for the admitted LocomotorWorksWhenDead=false templates.
        actor.ai.motion.clear_goal();
        body.extra_friction = self.catalog[actor.type_index]
            .locomotor
            .ground
            .as_ref()
            .expect("ground locomotor")
            .extra_friction
            + if actor.ai.motion.ultra_accurate() {
                0.5
            } else {
                0.0
            };
        if !self.catalog[actor.type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "INFANTRY")
        {
            return;
        }
        let x = body.velocity[0] * body.velocity[0];
        let y = body.velocity[1] * body.velocity[1];
        let z = body.velocity[2] * body.velocity[2];
        let desired = ((x + y) + z).sqrt() * 0.8;
        let current = (x + y).sqrt();
        if desired < 0.001 {
            body.velocity[0] = 0.0;
            body.velocity[1] = 0.0;
        } else if desired <= current {
            let factor = desired / current;
            body.velocity[0] *= factor;
            body.velocity[1] *= factor;
        }
    }

    pub(in crate::skirmish) fn integrate_death_body(&mut self, id: usize) {
        let actor = &self.entities[id];
        let Some(state) = &actor.death_lifecycle else {
            return;
        };
        if state.body.is_none() || actor.masked() || actor.disabled_held() {
            return;
        }
        let ground = self.ground_height(actor.position);
        let actor = &mut self.entities[id];
        let rules = &self.catalog[actor.type_index]
            .death_lifecycle
            .as_ref()
            .expect("rules")
            .physics
            .as_ref()
            .expect("corpse physics");
        let state = actor.death_lifecycle.as_mut().expect("lifecycle");
        let body = state.body.as_mut().expect("body");
        let old_height = body.height;
        body.acceleration[2] += rules.gravity;
        friction(
            body,
            actor.facing,
            rules,
            body.extra_friction,
            ground,
            self.tick,
        );
        for (velocity, acceleration) in body.velocity.iter_mut().zip(body.acceleration) {
            *velocity += acceleration;
            if velocity.abs() < 0.001 {
                *velocity = 0.0;
            }
        }
        if !body.braking {
            actor.position[0] += body.velocity[0];
            actor.position[1] += body.velocity[1];
        }
        body.height += body.velocity[2];
        let mass = rules.mass;
        let stiffness = rules.stiffness.clamp(0.01, 0.99);
        let stick_to_ground = rules.stick_to_ground;
        let ground = self.ground_height(self.entities[id].position);
        let facing = self.entities[id].facing;
        let body = self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("lifecycle")
            .body
            .as_mut()
            .expect("body");
        // handleBounce intentionally never reads m_extraBounciness in the pinned source.
        let bounce_force = if body.bouncing && body.height <= ground {
            if old_height > ground && body.velocity[2] < 0.0 {
                mass * (body.velocity[2].abs() * stiffness)
            } else {
                body.bouncing = false;
                0.0
            }
        } else {
            0.0
        };
        if body.height <= ground {
            body.falling = false;
            body.velocity[2] += ground - body.height;
            body.velocity[2] = body.velocity[2].min(0.0);
            body.height = ground;
        } else if stick_to_ground && !body.falling {
            body.height = ground;
        }
        body.acceleration = [0.0; 3];
        if bounce_force > 0.0 {
            apply_force(body, facing, [0.0, 0.0, bounce_force], mass, self.tick);
        }
    }
}

fn friction(body: &mut Body, facing: f32, rules: &Physics, extra: f32, ground: f32, frame: u64) {
    if !rules.airborne_friction && body.height - ground > -9.0 * rules.gravity {
        let resistance = -(rules.aerodynamic + extra).clamp(0.0, 0.99);
        for (acceleration, velocity) in body.acceleration.iter_mut().zip(body.velocity) {
            let force = velocity * resistance;
            *acceleration += force;
        }
        return;
    }
    let direction = [facing.cos(), facing.sin()];
    let lateral_x = body.velocity[0] * -direction[1];
    let lateral_y = body.velocity[1] * direction[0];
    let lateral = lateral_x + lateral_y;
    let coefficient = rules.mass * (rules.lateral + extra).clamp(0.01, 0.99);
    let mut force = [
        -(coefficient * (lateral * -direction[1])),
        -(coefficient * (lateral * direction[0])),
        0.0,
    ];
    if body.motive_until <= frame {
        let forward_x = body.velocity[0] * direction[0];
        let forward_y = body.velocity[1] * direction[1];
        let forward = forward_x + forward_y;
        let coefficient = rules.mass * (rules.forward + extra).clamp(0.01, 0.99);
        force[0] += -(coefficient * (forward * direction[0]));
        force[1] += -(coefficient * (forward * direction[1]));
    }
    apply_force(body, facing, force, rules.mass, frame);
}

fn apply_force(body: &mut Body, facing: f32, mut force: [f32; 3], mass: f32, frame: u64) {
    if body.motive_until > frame {
        let direction = [facing.cos(), facing.sin()];
        let x = force[0] * -direction[1];
        let y = force[1] * direction[0];
        let lateral = x + y;
        force[0] = lateral * -direction[1];
        force[1] = lateral * direction[0];
    }
    let inverse = 1.0 / mass;
    for (acceleration, force) in body.acceleration.iter_mut().zip(force) {
        let amount = force * inverse;
        *acceleration += amount;
    }
}

impl crate::skirmish::Entity {
    pub(in crate::skirmish) fn set_death_motive_expiry(&mut self, expiry: u64) {
        if let Some(body) = self
            .death_lifecycle
            .as_mut()
            .and_then(|state| state.body.as_mut())
        {
            body.motive_until = expiry;
        }
    }

    pub(in crate::skirmish) fn synchronize_death_physics(
        &mut self,
        motion: &mut crate::skirmish::physical_ocl::Motion,
        frame: u64,
    ) -> Option<(bool, bool, bool)> {
        let body = self.death_lifecycle.as_mut()?.body.as_mut()?;
        motion.position = [self.position[0], self.position[1], body.height];
        motion.velocity = body.velocity;
        motion.acceleration = body.acceleration;
        body.acceleration = [0.0; 3];
        motion.extra_friction = body.extra_friction;
        motion.bouncing = body.bouncing;
        Some((body.braking, body.motive_until > frame, body.falling))
    }

    pub(in crate::skirmish) fn reset_death_dynamic_physics(&mut self) {
        if let Some(body) = self
            .death_lifecycle
            .as_mut()
            .and_then(|state| state.body.as_mut())
        {
            body.velocity = [0.0; 3];
            body.acceleration = [0.0; 3];
        }
    }

    pub(in crate::skirmish) fn set_death_physics_velocity(&mut self, velocity: [f32; 3]) {
        if let Some(body) = self
            .death_lifecycle
            .as_mut()
            .and_then(|state| state.body.as_mut())
        {
            body.velocity = velocity;
        }
    }

    pub(in crate::skirmish) fn set_death_physics(
        &mut self,
        motion: &crate::skirmish::physical_ocl::Motion,
        falling: bool,
    ) {
        if let Some(body) = self
            .death_lifecycle
            .as_mut()
            .and_then(|state| state.body.as_mut())
        {
            body.height = motion.position[2];
            body.velocity = motion.velocity;
            body.acceleration = motion.acceleration;
            body.bouncing = motion.bouncing;
            body.falling = falling;
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish::death_lifecycle) fn fling_slow_death(
        &mut self,
        id: usize,
        index: usize,
    ) {
        let altitude = self.height_above_ground(id);
        if altitude < 1.0 {
            let mut point = self.object_position(id);
            point[2] += 1.0;
            self.set_object_position(id, point);
        }
        let actor = &self.entities[id];
        let fling = self.catalog[actor.type_index]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath rules")
            .slow[index]
            .fling
            .as_ref()
            .expect("positive fling force");
        let angle = startup::random_real(&mut self.random, -PI, PI);
        let pitch = startup::random_real(
            &mut self.random,
            fling.pitch[0],
            fling.pitch[0] + fling.pitch[1],
        );
        let magnitude = startup::random_real(
            &mut self.random,
            fling.force[0],
            fling.force[0] + fling.force[1],
        );
        let force = [
            pitch.cos() * (angle.cos() * magnitude),
            pitch.cos() * (angle.sin() * magnitude),
            -(-pitch).sin() * magnitude,
        ];
        let mut applied = force;
        if actor.ai.motion.motive_until > self.tick {
            let direction = [actor.facing.cos(), actor.facing.sin()];
            let x = force[0] * -direction[1];
            let y = force[1] * direction[0];
            let lateral = x + y;
            applied[0] = lateral * -direction[1];
            applied[1] = lateral * direction[0];
        }
        let mass = self.object_physics_mass(id).expect("flung PhysicsBehavior");
        let actor = &mut self.entities[id];
        actor.set_physics_allow_fall(true);
        let motion = if let Some(state) = &mut actor.physical {
            &mut state.motion
        } else {
            &mut actor
                .live_physics
                .as_mut()
                .expect("ordinary PhysicsBehavior")
                .motion
        };
        motion.force(applied, mass);
        motion.extra_friction = -3.0 * super::super::STEP_SECONDS;
        motion.bouncing = true;
        // The source stores extraBounciness=-1, but handleBounce never reads it.
        let position = self.object_position(id);
        self.set_object_transform(
            id,
            position,
            crate::skirmish::physical_ocl::transform::yaw(force[1].atan2(force[0])),
        );
        self.wake_family_update(id, "PhysicsBehavior", 0, 1);
    }
}
