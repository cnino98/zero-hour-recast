use super::{Entity, Motion, OBJECT_BRAKING, Rules, ULTRA_ACCURATE};

pub(super) fn integrate(
    actor: &mut Entity,
    state: &mut Motion,
    rules: &Rules,
    ground: f32,
    frame: u64,
) {
    // PhysicsBehavior snapshots the AI-applied acceleration before gravity and friction.
    state.previous_acceleration = state.acceleration;
    state.acceleration[2] += rules.gravity;
    let extra = rules.extra_friction + if state.flag(ULTRA_ACCURATE) { 0.5 } else { 0.0 };
    if rules.friction_2d || state.height - ground <= -9.0 * rules.gravity {
        let direction = [actor.facing.cos(), actor.facing.sin()];
        let lateral_x = state.velocity[0] * -direction[1];
        let lateral_y = state.velocity[1] * direction[0];
        let lateral = lateral_x + lateral_y;
        let friction = rules.mass * (rules.lateral_friction + extra).clamp(0.01, 0.99);
        let mut force = [
            -(friction * (lateral * -direction[1])),
            -(friction * (lateral * direction[0])),
        ];
        if state.motive_until <= frame {
            let forward_x = state.velocity[0] * direction[0];
            let forward_y = state.velocity[1] * direction[1];
            let forward = forward_x + forward_y;
            let friction = rules.mass * (rules.forward_friction + extra).clamp(0.01, 0.99);
            force[0] += -(friction * (forward * direction[0]));
            force[1] += -(friction * (forward * direction[1]));
        } else {
            // PhysicsBehavior::applyForce rejects longitudinal external forces while motive.
            let lateral_x = force[0] * -direction[1];
            let lateral_y = force[1] * direction[0];
            let lateral = lateral_x + lateral_y;
            force = [lateral * -direction[1], lateral * direction[0]];
        }
        let inverse = 1.0 / rules.mass;
        let ax = force[0] * inverse;
        let ay = force[1] * inverse;
        state.acceleration[0] += ax;
        state.acceleration[1] += ay;
    } else {
        let friction = -(rules.aerodynamic_friction + extra).clamp(0.0, 0.99);
        for (acceleration, velocity) in state.acceleration.iter_mut().zip(state.velocity) {
            let resistance = velocity * friction;
            *acceleration += resistance;
        }
    }
    for (velocity, acceleration) in state.velocity.iter_mut().zip(state.acceleration) {
        *velocity += acceleration;
        if velocity.abs() < 0.001 {
            *velocity = 0.0;
        }
    }
    if !state.flag(OBJECT_BRAKING) {
        actor.position[0] += state.velocity[0];
        actor.position[1] += state.velocity[1];
    }
    if actor.chinook.is_none() {
        state.height += state.velocity[2];
    } else {
        state.velocity[2] = 0.0;
    }
    state.acceleration = [0.0; 3];
}

pub(super) fn ground_collision(state: &mut Motion, ground: f32) {
    if state.height <= ground {
        state.velocity[2] += ground - state.height;
        state.velocity[2] = state.velocity[2].min(0.0);
        state.height = ground;
    }
}

pub(in crate::skirmish) fn lift(
    height: f32,
    velocity: f32,
    target: f32,
    gross: f32,
    gravity: f32,
    speed_limit_z: f32,
    precise: bool,
) -> f32 {
    let net = (gross + gravity).max(0.0);
    let maximum = if precise {
        if velocity < 0.0 {
            2.0 * net
        } else {
            -2.0 * net
        }
    } else if velocity < 0.0 {
        net
    } else {
        gravity
    };
    let desired = if maximum.abs() > 0.001 {
        let delta = target - height;
        let braking = (velocity * velocity) / maximum.abs();
        if braking.abs() > delta.abs() {
            maximum
        } else if velocity.abs() > speed_limit_z {
            speed_limit_z - velocity
        } else {
            2.0 * (delta - velocity)
        }
    } else {
        0.0
    };
    let force = desired - gravity;
    if precise {
        // Locomotor::getLiftToUse retains this order when a dying jet caps lift below zero.
        if force > 3.0 * gross {
            3.0 * gross
        } else if force < -gross {
            -gross
        } else {
            force
        }
    } else if force > gross {
        gross
    } else if force < 0.0 {
        0.0
    } else {
        force
    }
}
