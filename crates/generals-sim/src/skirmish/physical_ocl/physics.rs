use super::{Motion, MotionRules, Skirmish, transform};

impl Skirmish {
    pub(in crate::skirmish) fn integrate_physical_object(&mut self, id: usize) {
        let Some(mass) = self.object_physics_mass(id) else {
            return;
        };
        let held = self.entities[id].disabled_held();
        let actor = &mut self.entities[id];
        let Some(mut state) = actor.physical.take() else {
            return;
        };
        if state.physics_frame == Some(self.tick) {
            actor.physical = Some(state);
            return;
        }
        state.physics_frame = Some(self.tick);
        let rules = self.catalog[actor.type_index]
            .physical
            .as_ref()
            .expect("physical rules");
        let matrix_position = actor
            .transform
            .as_ref()
            .and_then(|matrix| matrix.translation);
        let layer = actor.layer;
        let ground =
            self.movement_layer_height(layer, [state.motion.position[0], state.motion.position[1]]);
        let was_airborne = state
            .was_airborne
            .unwrap_or(state.motion.position[2] > ground);
        let height_above_ground = state.motion.position[2] - ground;
        if !held && let Some(position) = matrix_position {
            state.motion.position = position;
        }
        let old_height = state.motion.position[2];
        let old_basis = state.motion.basis;
        if !held {
            integrate(
                &mut state.motion,
                &rules.motion,
                height_above_ground,
                self.entities[id].physics_control.airborne_friction,
                false,
                self.entities[id].ai.motion.motive_until > self.tick,
                mass,
            );
        }
        let ground =
            self.movement_layer_height(layer, [state.motion.position[0], state.motion.position[1]]);
        let bounce = if held {
            0.0
        } else {
            let bounce = state
                .motion
                .bounce(&rules.motion, old_height, ground, old_basis, mass);
            if state.motion.position[2] <= ground {
                state.motion.velocity[2] += ground - state.motion.position[2];
                state.motion.velocity[2] = state.motion.velocity[2].min(0.0);
                state.motion.position[2] = ground;
            }
            bounce
        };
        state.motion.acceleration = [0.0; 3];
        state.previous_overlap = state.current_overlap.take();
        if bounce > 0.0 && state.motion.bouncing {
            state.motion.force([0.0, 0.0, bounce], mass);
        }
        let resting = rules.kill_when_resting
            && state.motion.position[2] <= ground
            && state.motion.velocity.iter().all(|value| value.abs() < 0.01);
        let airborne = state.motion.position[2] > ground;
        let sound = (was_airborne && !airborne).then(|| state.bounce_sound.clone());
        state.was_airborne = Some(airborne);
        let actor = &mut self.entities[id];
        actor.position = [state.motion.position[0], state.motion.position[1]];
        if !held {
            actor.facing = state.motion.basis[1][0].atan2(state.motion.basis[0][0]);
            actor.transform = Some(transform::Matrix::from_basis(state.motion.basis));
        }
        actor.physical = Some(state);
        if let Some(sound) = sound {
            self.object_sound(id, sound, crate::skirmish::object_sounds::Action::Play);
        }
        if resting {
            self.kill_physical_object(id);
        }
        self.observe_ground_partition(id);
    }
}

pub(in crate::skirmish) fn integrate(
    state: &mut Motion,
    rules: &MotionRules,
    height_above_ground: f32,
    friction_airborne: bool,
    braking: bool,
    motive: bool,
    mass: f32,
) {
    state.acceleration[2] += rules.gravity;
    friction(
        state,
        rules,
        height_above_ground,
        friction_airborne,
        motive,
        mass,
    );
    for (velocity, acceleration) in state.velocity.iter_mut().zip(state.acceleration) {
        *velocity += acceleration;
        if velocity.abs() < 0.001 {
            *velocity = 0.0;
        }
    }
    for (axis, (position, velocity)) in state.position.iter_mut().zip(state.velocity).enumerate() {
        if !braking || axis == 2 {
            *position += velocity;
        }
    }
    if state.rates.iter().any(|rate| *rate != 0.0) {
        let mut angles = state.rates.map(|rate| rate * rules.angular_factor);
        if rules.center_of_mass_offset != 0.0 {
            // PhysicsBehavior aims the forward axis toward vertical as the object falls.
            let [x, y, z] = state.basis.map(|row| row[0]);
            // PhysicsUpdate.cpp squares each component before adding (no fused multiply-add).
            let squared = [x, y].map(|component| component * component);
            let horizontal_squared = squared[0] + squared[1];
            let pitch = z.atan2(horizontal_squared.sqrt());
            let remaining = if rules.center_of_mass_offset > 0.0 {
                std::f32::consts::FRAC_PI_2 - pitch
            } else {
                pitch - std::f32::consts::FRAC_PI_2
            };
            angles[1] *= remaining.sin();
        }
        transform::rotate(&mut state.basis, angles);
    }
}

impl Motion {
    pub(in crate::skirmish) fn bounce(
        &mut self,
        rules: &MotionRules,
        old_height: f32,
        ground: f32,
        old_basis: [[f32; 3]; 3],
        mass: f32,
    ) -> f32 {
        if !self.bouncing || self.position[2] > ground {
            return 0.0;
        }
        let force = if old_height > ground && self.velocity[2] < 0.0 {
            mass * (self.velocity[2].abs() * rules.stiffness.clamp(0.01, 0.99))
        } else {
            0.0
        };
        for rate in &mut self.rates {
            *rate *= 0.7;
        }
        if force <= 0.0 {
            self.bouncing = rules.allow_bouncing;
        } else {
            // update commits the integrated matrix on zero force; successful
            // bounce instead commits an upright matrix at the old object's yaw.
            self.basis = transform::yaw(old_basis[1][0].atan2(old_basis[0][0]));
        }
        force
    }
}

fn friction(
    state: &mut Motion,
    rules: &MotionRules,
    height_above_ground: f32,
    friction_airborne: bool,
    motive: bool,
    mass: f32,
) {
    if !friction_airborne && height_above_ground > -9.0 * rules.gravity {
        // Admitted physical objects have no AI locomotor enabling
        // APPLY_FRICTION2D_WHEN_AIRBORNE. Its default false (also explicitly
        // set on special bombs) selects aerodynamic drag, not zero friction.
        let resistance = -(rules.aerodynamic_friction + state.extra_friction).clamp(0.0, 0.99);
        for (acceleration, velocity) in state.acceleration.iter_mut().zip(state.velocity) {
            let value = velocity * resistance;
            *acceleration += value;
        }
        for rate in &mut state.rates {
            *rate *= 1.0 + resistance;
        }
        return;
    }
    for rate in &mut state.rates {
        *rate *= 1.0 - 0.15;
    }
    let angle = state.basis[1][0].atan2(state.basis[0][0]);
    let direction = [angle.cos(), angle.sin()];
    let x = state.velocity[0] * -direction[1];
    let y = state.velocity[1] * direction[0];
    let lateral = x + y;
    let coefficient = mass * (rules.lateral_friction + state.extra_friction).clamp(0.01, 0.99);
    let mut force = [
        -(coefficient * (lateral * -direction[1])),
        -(coefficient * (lateral * direction[0])),
        0.0,
    ];
    let x = state.velocity[0] * direction[0];
    let y = state.velocity[1] * direction[1];
    let forward = x + y;
    let coefficient = mass * (rules.forward_friction + state.extra_friction).clamp(0.01, 0.99);
    if motive {
        // applyForce projects the lateral friction force again while motive.
        let x = force[0] * -direction[1];
        let y = force[1] * direction[0];
        let lateral = x + y;
        force[0] = lateral * -direction[1];
        force[1] = lateral * direction[0];
    } else {
        force[0] += -(coefficient * (forward * direction[0]));
        force[1] += -(coefficient * (forward * direction[1]));
    }
    state.force(force, mass);
}
