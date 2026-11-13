use std::f32::consts::{FRAC_PI_2, PI};

use super::{
    BIGNUM, BRAKING, CELL_SIZE, DONUT_FRAMES, Entity, Force, GroundAppearance, GroundGoal,
    GroundRules, STEP_SECONDS, UnitType, change_speed, forward_speed, maximum_acceleration,
    motive_force, normalize_angle, rotate_actor, speed, turn_rate,
};

#[cfg(test)]
mod tests;

pub(super) const MOVING_BACKWARDS: u8 = 8;
const THREE_POINT_TURN: u8 = 16;

// Locomotor::moveTowardsPositionWheels, grounded FOUR_WHEELS branch.
pub(super) fn move_wheels(
    actor: &mut Entity,
    unit: &UnitType,
    goal: &GroundGoal,
    distance: f32,
    mut desired_speed: f32,
    force: &Force<'_>,
    terrain_valid: &mut impl FnMut([f32; 2]) -> bool,
) {
    let frame = force.frame;
    let rules = actor
        .active_locomotor(unit)
        .ground
        .as_ref()
        .expect("Ground rules selected");
    let GroundAppearance::Wheels {
        min_turn_speed,
        can_move_backwards,
    } = rules.appearance
    else {
        unreachable!("Wheeled rules selected");
    };
    let maximum_speed = (speed(actor, unit) * STEP_SECONDS).min(BIGNUM);
    let turn_speed = min_turn_speed.max(maximum_speed / 4.0);
    let desired_angle =
        (goal.position[1] - actor.position[1]).atan2(goal.position[0] - actor.position[0]);
    let (backwards, three_point, relative) = backward_motion(
        actor,
        can_move_backwards,
        distance,
        unit.geometry.major,
        desired_angle,
    );
    if relative.abs() > PI / 20.0 && desired_speed > turn_speed {
        desired_speed = turn_speed;
    }
    let mut actual_speed = forward_speed(actor);
    if backwards {
        actual_speed = -actual_speed;
    }
    if relative.abs() > PI / 12.0
        && !valid_turn_projection(
            actor,
            unit,
            desired_speed,
            actual_speed,
            turn_speed,
            relative,
            terrain_valid,
        )
    {
        let maximum_turn = turn_rate(actor, unit);
        rotate_actor(
            actor,
            unit,
            goal.position,
            maximum_turn,
            None,
            force.terrain,
        );
        motive_force(actor, force.mass, [0.0; 2], frame);
        return;
    }
    let goal_speed = braking_speed(
        actor,
        rules,
        distance,
        actual_speed,
        desired_speed,
        goal.no_slow_down,
        frame,
    );
    let mut turn_factor = actual_speed / turn_speed;
    if turn_factor < 0.0 {
        turn_factor = -turn_factor;
    }
    if turn_factor > 1.0 {
        turn_factor = 1.0;
    }
    let turn_amount = turn_factor * turn_rate(actor, unit);
    rotate_wheels(
        actor,
        unit,
        goal.position,
        turn_amount,
        backwards,
        three_point,
        force.terrain,
    );
    let change = if backwards {
        -goal_speed + actual_speed
    } else {
        goal_speed - actual_speed
    };
    let acceleration = maximum_acceleration(actor, unit, rules);
    let braking = rules.braking.min(BIGNUM);
    let acceleration = if backwards {
        if change < 0.0 {
            -acceleration
        } else {
            actor.ai.motion.braking_factor * braking
        }
    } else if change > 0.0 {
        acceleration
    } else {
        -actor.ai.motion.braking_factor * braking
    };
    change_speed(actor, force.mass, change, acceleration, force.frame);
}

fn backward_motion(
    actor: &mut Entity,
    can_move_backwards: bool,
    distance: f32,
    major_radius: f32,
    mut desired_angle: f32,
) -> (bool, bool, f32) {
    let mut relative = normalize_angle(desired_angle - actor.facing);
    if forward_speed(actor) == 0.0 {
        actor.ai.motion.set(MOVING_BACKWARDS, false);
        if can_move_backwards && relative.abs() > FRAC_PI_2 {
            actor.ai.motion.set(MOVING_BACKWARDS, true);
            actor
                .ai
                .motion
                .set(THREE_POINT_TURN, distance > 5.0 * major_radius);
        }
    }
    let mut backwards = false;
    let mut three_point = false;
    if actor.ai.motion.flags & MOVING_BACKWARDS != 0 {
        if relative.abs() < FRAC_PI_2 {
            actor.ai.motion.set(MOVING_BACKWARDS, false);
        } else {
            backwards = true;
            actor
                .ai
                .motion
                .set(THREE_POINT_TURN, distance > 5.0 * major_radius);
            three_point = actor.ai.motion.flags & THREE_POINT_TURN != 0;
            if !three_point {
                desired_angle = normalize_angle(desired_angle - PI);
                relative = normalize_angle(desired_angle - actor.facing);
            }
        }
    }
    (backwards, three_point, relative)
}

fn valid_turn_projection(
    actor: &Entity,
    unit: &UnitType,
    goal_speed: f32,
    actual_speed: f32,
    turn_speed: f32,
    relative: f32,
    terrain_valid: &mut impl FnMut([f32; 2]) -> bool,
) -> bool {
    const PROJECT_FRAMES: f32 = 15.0;
    let combined_speed = goal_speed + actual_speed;
    let distance = PROJECT_FRAMES * combined_speed / 2.0;
    let turn_factor = ((combined_speed / 2.0) / turn_speed).min(1.0);
    let turn_amount = PROJECT_FRAMES * turn_factor * turn_rate(actor, unit) / 4.0;
    let mut angle = actor.facing;
    if relative < 0.0 {
        angle -= turn_amount;
    } else {
        angle += turn_amount;
    }
    let offset = [angle.cos() * distance, angle.sin() * distance];
    let next = [actor.position[0] + offset[0], actor.position[1] + offset[1]];
    let half_x = offset[0] / 2.0;
    let half_y = offset[1] / 2.0;
    let half = [actor.position[0] + half_x, actor.position[1] + half_y];
    // Native short-circuit order matters: never probe full distance after a
    // failed half-distance terrain check.
    terrain_valid(half) && terrain_valid(next)
}

fn braking_speed(
    actor: &mut Entity,
    rules: &GroundRules,
    distance: f32,
    actual_speed: f32,
    mut goal_speed: f32,
    no_slow_down: bool,
    frame: u64,
) -> f32 {
    let braking = rules.braking.min(BIGNUM);
    let slowdown_time = actual_speed / braking + 1.0;
    let slowing = (actual_speed / 1.5) * slowdown_time;
    let slowdown = slowing + actual_speed;
    if distance < slowdown.max(CELL_SIZE) && !actor.ai.motion.braking() && !no_slow_down {
        actor.ai.motion.set(BRAKING, true);
        actor.ai.motion.braking_factor = 1.1;
    }
    if distance > CELL_SIZE && distance > 2.0 * slowdown {
        actor.ai.motion.set(BRAKING, false);
    }
    if distance > 4.0 * CELL_SIZE {
        actor.ai.motion.donut_until = frame + DONUT_FRAMES;
    } else if actor.ai.motion.donut_until < frame {
        actor.ai.motion.set(BRAKING, true);
    }
    if actor.ai.motion.braking() {
        // The native wheel branch explicitly overwrites its computed/clamped
        // factor with 1.0; the tread branch does not.
        actor.ai.motion.braking_factor = 1.0;
        goal_speed = if slowdown > distance {
            (actual_speed - braking).max(0.0)
        } else if slowdown > distance * 0.75 {
            (actual_speed - braking / 2.0).max(0.0)
        } else {
            actual_speed
        };
    }
    goal_speed
}

fn rotate_wheels(
    actor: &mut Entity,
    unit: &UnitType,
    goal: [f32; 2],
    amount: f32,
    backwards: bool,
    three_point: bool,
    terrain: &super::super::TerrainContext<'_>,
) {
    let goal = if backwards && !three_point {
        let mut behind = actor.position;
        behind[0] += -(goal[0] - actor.position[0]);
        behind[1] += -(goal[1] - actor.position[1]);
        behind
    } else {
        goal
    };
    rotate_actor(actor, unit, goal, amount, None, terrain);
}
