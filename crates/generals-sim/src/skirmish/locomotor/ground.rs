use std::f32::consts::FRAC_PI_4;

use super::{
    Entity, Locomotor, UnitType, normalize_angle, rotate_actor, rotate_towards_angle, speed,
    turn_rate,
};
use crate::skirmish::{CELL_SIZE, STEP_SECONDS};

mod climb;
mod terrain_correction;
pub(super) mod wander;
mod wheels;

#[cfg(test)]
mod tests;

const BRAKING: u8 = 1;
const OBJECT_BRAKING: u8 = 2;
const ULTRA_ACCURATE: u8 = 4;
const ALLOW_INVALID_POSITION: u8 = 8;
const NO_SLOW_DOWN: u8 = 64;
const BIGNUM: f32 = 99_999.0;
const DONUT_FRAMES: u64 = 75;

pub enum GroundAppearance {
    Legs,
    Climber,
    Treads,
    Wheels {
        min_turn_speed: f32,
        can_move_backwards: bool,
    },
}

/// Source ground locomotor and `PhysicsBehavior` data in logic-frame units.
pub struct GroundRules {
    /// Native width and length factors; only `TWO_LEGS` applies the sideways offset.
    pub wander: Option<[f32; 2]>,
    pub appearance: GroundAppearance,
    pub is_dozer: bool,
    pub downhill_only: bool,
    pub acceleration: f32,
    pub acceleration_damaged: f32,
    pub braking: f32,
    pub min_speed: f32,
    pub close_enough: f32,
    pub mass: f32,
    pub forward_friction: f32,
    pub lateral_friction: f32,
    pub extra_friction: f32,
}

// Physics inputs remain local to this locomotor call; containment can change
// before the next force producer runs in the same frame.
struct Force<'a> {
    mass: f32,
    frame: u64,
    terrain: &'a super::TerrainContext<'a>,
}

pub struct GroundGoal {
    pub height: f32,
    pub position: [f32; 2],
    pub distance: f32,
    pub desired_speed: f32,
    pub no_slow_down: bool,
    pub ultra_accurate: bool,
}

pub(in crate::skirmish) enum GroundRequest {
    Path(GroundGoal),
    Angle(f32),
}

enum GoalKind {
    None,
    Angle,
    Path,
    OnPath,
}

pub(in crate::skirmish) struct SavedGroundState {
    wander: Option<wander::State>,
    braking_factor: f32,
    donut_until: u64,
    flags: u8,
}

impl SavedGroundState {
    pub(in crate::skirmish) fn from_template(
        frame: u64,
        locomotor: &Locomotor,
        random: &mut [u32; 6],
    ) -> Self {
        Self {
            wander: locomotor
                .wander_length
                .map(|length| wander::State::new(random, length)),
            braking_factor: 1.0,
            donut_until: frame + DONUT_FRAMES,
            flags: 0,
        }
    }
}

pub struct GroundMotion {
    pub(in crate::skirmish) climb_heights: [f32; 3],
    wander: Option<wander::State>,
    goal: GoalKind,
    pending: Option<GroundRequest>,
    movement_complete: bool,
    is_moving: bool,
    pub(in crate::skirmish) movement_active: bool,
    pub(crate) velocity: [f32; 2],
    pub(in crate::skirmish) command_frame: Option<u64>,
    pub(in crate::skirmish) acceleration: [f32; 2],
    pub(in crate::skirmish) motive_until: u64,
    braking_factor: f32,
    donut_until: u64,
    flags: u8,
}

impl GroundMotion {
    pub(in crate::skirmish) const fn climb_goal(&self) -> Option<[f32; 3]> {
        match &self.pending {
            Some(GroundRequest::Path(goal)) => {
                Some([goal.position[0], goal.position[1], goal.height])
            }
            _ => None,
        }
    }

    pub(in crate::skirmish) const fn swap_surface_state(&mut self, saved: &mut SavedGroundState) {
        let object = self.flags & OBJECT_BRAKING;
        self.flags &= !OBJECT_BRAKING;
        std::mem::swap(&mut self.flags, &mut saved.flags);
        std::mem::swap(&mut self.braking_factor, &mut saved.braking_factor);
        std::mem::swap(&mut self.donut_until, &mut saved.donut_until);
        std::mem::swap(&mut self.wander, &mut saved.wander);
        self.flags = (self.flags & !(ULTRA_ACCURATE | NO_SLOW_DOWN)) | object;
    }

    pub(in crate::skirmish) const fn clear_surface_precision(&mut self) {
        self.flags &= !(ULTRA_ACCURATE | NO_SLOW_DOWN);
        if let Some(GroundRequest::Path(goal)) = &mut self.pending {
            goal.ultra_accurate = false;
            goal.no_slow_down = false;
        }
    }

    pub(in crate::skirmish) const fn clear_ultra_accuracy(&mut self) {
        self.flags &= !ULTRA_ACCURATE;
        if let Some(GroundRequest::Path(goal)) = &mut self.pending {
            goal.ultra_accurate = false;
        }
    }

    pub(in crate::skirmish) const fn set_allow_invalid_position(&mut self, enabled: bool) {
        self.set(ALLOW_INVALID_POSITION, enabled);
    }

    pub(in crate::skirmish) const fn set_no_slow_down(&mut self, enabled: bool) {
        self.set(NO_SLOW_DOWN, enabled);
    }

    pub(in crate::skirmish) const fn no_slow_down(&self) -> bool {
        self.flags & NO_SLOW_DOWN != 0
    }

    pub(in crate::skirmish) const fn set_object_braking(&mut self, enabled: bool) {
        self.set(OBJECT_BRAKING, enabled);
    }

    pub(in crate::skirmish) const fn reset_dynamic_physics(&mut self) {
        self.velocity = [0.0; 2];
        self.acceleration = [0.0; 2];
    }

    pub(crate) const fn new(frame: u64) -> Self {
        Self {
            climb_heights: [0.0; 3],
            wander: None,
            movement_active: false,
            goal: GoalKind::None,
            pending: None,
            movement_complete: false,
            is_moving: false,
            velocity: [0.0; 2],
            command_frame: None,
            acceleration: [0.0; 2],
            motive_until: 0,
            braking_factor: 1.0,
            donut_until: frame + DONUT_FRAMES,
            flags: 0,
        }
    }

    pub(in crate::skirmish) const fn start_move(&mut self, frame: u64) {
        self.donut_until = frame + DONUT_FRAMES;
        self.starting_move();
    }

    pub(in crate::skirmish) const fn starting_move(&mut self) {
        self.movement_complete = false;
        self.is_moving = true;
    }

    pub(in crate::skirmish) const fn end_move(&mut self) {
        self.is_moving = false;
        self.movement_complete = true;
    }

    pub(in crate::skirmish) const fn is_moving(&self) -> bool {
        self.is_moving
    }

    pub(in crate::skirmish) const fn take_completion(&mut self) -> bool {
        let complete = self.movement_complete;
        self.movement_complete = false;
        complete
    }

    /// AI's `POSITION_ON_PATH` retains the path identity until `doLocomotor` samples it.
    pub(in crate::skirmish) const fn follow_path(&mut self) {
        self.goal = GoalKind::OnPath;
        self.pending = None;
    }

    pub(in crate::skirmish) const fn follows_path(&self) -> bool {
        matches!(self.goal, GoalKind::OnPath)
    }

    pub(in crate::skirmish) const fn path_force(&mut self, goal: GroundGoal) {
        self.pending = Some(GroundRequest::Path(goal));
    }

    pub(in crate::skirmish) const fn request_path(&mut self, goal: GroundGoal) {
        self.goal = GoalKind::Path;
        self.pending = Some(GroundRequest::Path(goal));
    }

    pub(in crate::skirmish) const fn request_angle(&mut self, angle: f32) {
        self.goal = GoalKind::Angle;
        self.pending = Some(GroundRequest::Angle(angle));
    }

    pub(in crate::skirmish) const fn angle_goal(&self) -> Option<f32> {
        match &self.pending {
            Some(GroundRequest::Angle(angle)) => Some(*angle),
            Some(GroundRequest::Path(_)) | None => None,
        }
    }

    pub(in crate::skirmish) const fn take_request(&mut self) -> Option<GroundRequest> {
        self.pending.take()
    }

    pub(in crate::skirmish) const fn mark_angle(&mut self, frame: u64) {
        self.command_frame = Some(frame);
        self.goal = GoalKind::Angle;
    }

    pub(in crate::skirmish) const fn has_goal(&self) -> bool {
        !matches!(self.goal, GoalKind::None)
    }

    pub(in crate::skirmish) const fn moving_backwards(&self) -> bool {
        self.flags & wheels::MOVING_BACKWARDS != 0
    }

    pub(in crate::skirmish) const fn clear_goal(&mut self) {
        self.goal = GoalKind::None;
        self.pending = None;
    }

    pub(in crate::skirmish) const fn object_braking(&self) -> bool {
        self.flags & OBJECT_BRAKING != 0
    }

    pub(in crate::skirmish) const fn braking(&self) -> bool {
        self.flags & BRAKING != 0
    }

    pub(in crate::skirmish) const fn ultra_accurate(&self) -> bool {
        self.flags & ULTRA_ACCURATE != 0
    }

    pub(in crate::skirmish) const fn set_ultra_accurate(&mut self, enabled: bool) {
        self.set(ULTRA_ACCURATE, enabled);
    }

    const fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }
}

pub(in crate::skirmish) fn validate(locomotor: &Locomotor) -> Result<(), &'static str> {
    if locomotor
        .wander_length
        .is_some_and(|length| !length.is_finite())
    {
        return Err("Invalid locomotor wander length");
    }
    if !locomotor.airborne_targeting_height.is_finite() {
        return Err("Invalid airborne targeting height");
    }
    if let Some(rules) = &locomotor.flight {
        crate::skirmish::flight::validate(rules)?;
    }
    let Some(rules) = &locomotor.ground else {
        return Ok(());
    };
    if [
        rules.acceleration,
        rules.acceleration_damaged,
        rules.braking,
        rules.close_enough,
        rules.mass,
    ]
    .iter()
    .any(|value| !value.is_finite() || *value <= 0.0)
        || rules.wander.is_some_and(|[width, length]| {
            !width.is_finite() || !length.is_finite() || length <= 0.0
        })
        || rules.min_speed != 0.0
        || [
            rules.forward_friction,
            rules.lateral_friction,
            rules.extra_friction,
        ]
        .iter()
        .any(|value| !value.is_finite())
    {
        return Err("Unsupported ground locomotor or physics values");
    }
    if let GroundAppearance::Wheels { min_turn_speed, .. } = rules.appearance
        && !min_turn_speed.is_finite()
    {
        return Err("Non-finite wheel minimum turn speed");
    }
    Ok(())
}

/// `AIUpdateInterface::doLocomotor`, grounded `LEGS/TREADS/FOUR_WHEELS` branch.
pub(in crate::skirmish) fn move_ground(
    actor: &mut Entity,
    unit: &UnitType,
    goal: &GroundGoal,
    mass: f32,
    frame: u64,
    terrain: &super::TerrainContext<'_>,
    mut terrain_valid: impl FnMut([f32; 2]) -> bool,
) {
    let rules = actor
        .active_locomotor(unit)
        .ground
        .as_ref()
        .expect("Ground rules were selected");
    let force = Force {
        mass,
        frame,
        terrain,
    };
    actor.ai.motion.command_frame = Some(frame);
    if !actor.ai.motion.follows_path() {
        actor.ai.motion.goal = GoalKind::Path;
    }
    actor.ai.motion.set(ULTRA_ACCURATE, goal.ultra_accurate);
    let maximum = (speed(actor, unit) * STEP_SECONDS).min(BIGNUM);
    actor.ai.blocking.begin_locomotor();
    let (desired_speed, mut blocked) = actor
        .ai
        .blocking
        .limit_speed(goal.desired_speed.min(maximum));
    let braking = rules.braking.min(BIGNUM);
    let stopping_at_maximum = (maximum / braking) * maximum / 2.0;
    let mut distance = goal.distance;
    if distance > CELL_SIZE && distance > stopping_at_maximum {
        actor.ai.motion.set(BRAKING, false);
        actor.ai.motion.braking_factor = 1.0;
    }
    if actor.ai.motion.flags & ALLOW_INVALID_POSITION == 0
        && !terrain_valid(actor.position)
        && terrain_correction::fix_invalid_position(actor, rules, mass, frame, &mut terrain_valid)
    {
        actor.ai.blocking.finish_locomotor(blocked);
        return;
    }
    let delta = [
        goal.position[0] - actor.position[0],
        goal.position[1] - actor.position[1],
    ];
    let x_squared = delta[0] * delta[0];
    let y_squared = delta[1] * delta[1];
    let direct_distance = (x_squared + y_squared).sqrt();
    if direct_distance > distance {
        if direct_distance > 2.0 * distance {
            actor.ai.motion.set(BRAKING, true);
        }
        distance = direct_distance;
    }
    // A zero motive force still marks this object as driven for ten frames.
    motive_force(actor, mass, [0.0; 2], frame);
    if blocked {
        let vx = actor.ai.motion.velocity[0] * actor.ai.motion.velocity[0];
        let vy = actor.ai.motion.velocity[1] * actor.ai.motion.velocity[1];
        if desired_speed > (vx + vy).sqrt() {
            blocked = false;
        }
    }
    if blocked {
        scrub_velocity(actor, desired_speed);
        if rules.wander.is_none() {
            let (_, turned) = rotate_actor(
                actor,
                unit,
                goal.position,
                turn_rate(actor, unit),
                None,
                terrain,
            );
            blocked = turned != 0.0;
        }
        actor.ai.blocking.finish_locomotor(blocked);
        return;
    }
    let was_braking = actor.ai.motion.flags & OBJECT_BRAKING != 0;
    match rules.appearance {
        GroundAppearance::Legs => move_legs(actor, unit, goal, distance, desired_speed, &force),
        GroundAppearance::Climber => {
            climb::move_climber(actor, unit, goal, distance, desired_speed, &force);
        }
        GroundAppearance::Treads => move_treads(actor, unit, goal, distance, desired_speed, &force),
        GroundAppearance::Wheels { .. } => wheels::move_wheels(
            actor,
            unit,
            goal,
            distance,
            desired_speed,
            &force,
            &mut terrain_valid,
        ),
    }
    actor
        .ai
        .motion
        .set(OBJECT_BRAKING, actor.ai.motion.braking());
    if was_braking && direct_distance > 0.001 {
        braking_displacement(actor, delta, direct_distance);
    }
    actor.ai.blocking.finish_locomotor(false);
}

fn braking_displacement(actor: &mut Entity, delta: [f32; 2], direct_distance: f32) {
    // Source uses the pre-turn delta and PREVIOUS object BRAKING state.
    let velocity = forward_speed(actor)
        .abs()
        .max(CELL_SIZE / 30.0)
        .min(direct_distance);
    let inverse = 1.0 / direct_distance;
    let direction = [delta[0] * inverse, delta[1] * inverse];
    let displacement = [direction[0] * velocity, direction[1] * velocity];
    actor.position[0] += displacement[0];
    actor.position[1] += displacement[1];
}

fn maximum_acceleration(actor: &Entity, unit: &UnitType, rules: &GroundRules) -> f32 {
    let value =
        if actor.health / actor.max_health > actor.active_locomotor(unit).damage_health_threshold {
            rules.acceleration
        } else {
            rules.acceleration_damaged
        };
    value.min(BIGNUM)
}

fn move_legs(
    actor: &mut Entity,
    unit: &UnitType,
    goal: &GroundGoal,
    distance: f32,
    desired_speed: f32,
    force: &Force<'_>,
) {
    let rules = actor
        .active_locomotor(unit)
        .ground
        .as_ref()
        .expect("Ground rules were selected");
    let actual_speed = forward_speed(actor);
    let mut desired_angle =
        (goal.position[1] - actor.position[1]).atan2(goal.position[0] - actor.position[0]);
    if let (Some([width, _]), Some(wander)) = (rules.wander, &mut actor.ai.motion.wander) {
        desired_angle = wander.angle(desired_angle, actual_speed, width);
    }
    let relative = normalize_angle(desired_angle - actor.facing);
    rotate_towards_angle(actor, unit, desired_angle, force.terrain);
    let angle_coefficient = (relative.abs() / FRAC_PI_4).min(1.0);
    let mut goal_speed = (1.0 - angle_coefficient) * desired_speed;
    let braking = rules.braking.min(BIGNUM);
    let delta = actual_speed - rules.min_speed;
    let slowdown = if delta <= 0.0 {
        0.0
    } else {
        let squared = delta * delta;
        let distance = (squared / braking.abs()) * 0.5;
        distance * 1.05
    };
    if distance < slowdown && !goal.no_slow_down {
        goal_speed = rules.min_speed;
    }
    let change = goal_speed - actual_speed;
    let acceleration = if change > 0.0 {
        maximum_acceleration(actor, unit, rules)
    } else {
        -braking
    };
    change_speed(actor, force.mass, change, acceleration, force.frame);
}

fn move_treads(
    actor: &mut Entity,
    unit: &UnitType,
    goal: &GroundGoal,
    distance: f32,
    desired_speed: f32,
    force: &Force<'_>,
) {
    let rules = actor
        .active_locomotor(unit)
        .ground
        .as_ref()
        .expect("Ground rules were selected");
    let mut relative = 0.0;
    let maximum_turn = turn_rate(actor, unit);
    rotate_actor(
        actor,
        unit,
        goal.position,
        maximum_turn,
        Some(&mut relative),
        force.terrain,
    );
    let coefficient = (relative.abs() / FRAC_PI_4).min(1.0);
    let mut goal_speed = (1.0 - coefficient) * desired_speed;
    // Treads measure forward speed after turning; legs measure it before.
    let actual_speed = forward_speed(actor);
    let braking = rules.braking.min(BIGNUM);
    let slowdown_time = actual_speed / braking;
    let slowdown = (actual_speed / 1.50) * slowdown_time;
    let dx = actor.position[0] - goal.position[0];
    let dy = actor.position[1] - goal.position[1];
    let x_squared = dx * dx;
    let y_squared = dy * dy;
    if x_squared + y_squared < (2.0 * CELL_SIZE).powi(2) && f64::from(coefficient) > 0.05 {
        goal_speed = actual_speed * 0.6;
    }
    if distance < slowdown && !actor.ai.motion.braking() && !goal.no_slow_down {
        actor.ai.motion.set(BRAKING, true);
        actor.ai.motion.braking_factor = 1.1;
    }
    if distance > CELL_SIZE && distance > 2.0 * slowdown {
        actor.ai.motion.set(BRAKING, false);
    }
    if actor.ai.motion.braking() {
        let factor = slowdown / distance;
        actor.ai.motion.braking_factor = factor * factor;
        if actor.ai.motion.braking_factor > 5.0 {
            actor.ai.motion.braking_factor = 5.0;
        }
        goal_speed = if slowdown > distance {
            (actual_speed - braking).max(0.0)
        } else if slowdown > distance * 0.75 {
            (actual_speed - braking / 2.0).max(0.0)
        } else {
            actual_speed
        };
    }
    let change = goal_speed - actual_speed;
    let acceleration = if change > 0.0 {
        maximum_acceleration(actor, unit, rules)
    } else {
        -actor.ai.motion.braking_factor * braking
    };
    change_speed(actor, force.mass, change, acceleration, force.frame);
}

fn change_speed(actor: &mut Entity, mass: f32, change: f32, acceleration: f32, frame: u64) {
    if change == 0.0 {
        return;
    }
    let mut force = mass * acceleration;
    let needed = mass * change;
    if force.abs() > needed.abs() {
        force = needed;
    }
    let direction = [actor.facing.cos(), actor.facing.sin()];
    motive_force(
        actor,
        mass,
        [force * direction[0], force * direction[1]],
        frame,
    );
}

fn motive_force(actor: &mut Entity, mass: f32, force: [f32; 2], frame: u64) {
    actor.ai.motion.motive_until = 0;
    apply_force(actor, mass, force, frame);
    actor.ai.motion.motive_until = frame + 10;
}

fn apply_force(actor: &mut Entity, mass: f32, mut force: [f32; 2], frame: u64) {
    if force.iter().any(|value| value.is_nan()) {
        return;
    }
    actor.physics_control.force_wake = true;
    if actor.ai.motion.motive_until > frame {
        let direction = [actor.facing.cos(), actor.facing.sin()];
        let x = force[0] * -direction[1];
        let y = force[1] * direction[0];
        let lateral = x + y;
        force = [lateral * -direction[1], lateral * direction[0]];
    }
    let inverse_mass = 1.0 / mass;
    let acceleration = [force[0] * inverse_mass, force[1] * inverse_mass];
    actor.ai.motion.acceleration[0] += acceleration[0];
    actor.ai.motion.acceleration[1] += acceleration[1];
}

fn scrub_velocity(actor: &mut Entity, desired: f32) {
    if desired < 0.001 {
        actor.ai.motion.velocity = [0.0; 2];
    } else {
        let x = actor.ai.motion.velocity[0] * actor.ai.motion.velocity[0];
        let y = actor.ai.motion.velocity[1] * actor.ai.motion.velocity[1];
        let current = (x + y).sqrt();
        if desired > current {
            return;
        }
        let factor = desired / current;
        actor.ai.motion.velocity[0] *= factor;
        actor.ai.motion.velocity[1] *= factor;
    }
}

// PhysicsBehavior::getForwardSpeed2D deliberately does NOT return the dot product.
fn forward_speed(actor: &Entity) -> f32 {
    let vx = actor.ai.motion.velocity[0] * actor.facing.cos();
    let vy = actor.ai.motion.velocity[1] * actor.facing.sin();
    let x_squared = vx * vx;
    let y_squared = vy * vy;
    let magnitude = (x_squared + y_squared).sqrt();
    if vx + vy >= 0.0 {
        magnitude
    } else {
        -magnitude
    }
}

/// Ground `maintainCurrentPositionOther`: stop driven velocity immediately.
pub(in crate::skirmish) const fn maintain_ground(actor: &mut Entity, frame: u64) {
    let blocked = actor.ai.blocking.begin_locomotor();
    actor.ai.blocking.finish_locomotor(blocked);
    actor.ai.motion.goal = GoalKind::None;
    actor.ai.motion.donut_until = frame + DONUT_FRAMES;
    actor.ai.motion.set(BRAKING, false);
    if actor.ai.motion.motive_until > frame {
        actor.ai.motion.velocity = [0.0; 2];
    }
}

/// Ground XY portion of `PhysicsBehavior::update`, after all initial updates.
pub(in crate::skirmish) fn integrate_ground(actor: &mut Entity, unit: &UnitType, frame: u64) {
    let rules = actor
        .active_locomotor(unit)
        .ground
        .as_ref()
        .expect("Ground rules were selected");
    friction(actor, rules, frame);
    for axis in 0..2 {
        actor.ai.motion.velocity[axis] += actor.ai.motion.acceleration[axis];
        if actor.ai.motion.velocity[axis].abs() < 0.001 {
            actor.ai.motion.velocity[axis] = 0.0;
        }
        if actor.ai.motion.flags & OBJECT_BRAKING == 0 {
            actor.position[axis] += actor.ai.motion.velocity[axis];
        }
    }
    actor.ai.motion.acceleration = [0.0; 2];
}

fn friction(actor: &mut Entity, rules: &GroundRules, frame: u64) {
    let velocity = actor.ai.motion.velocity;
    if velocity[0] == 0.0 && velocity[1] == 0.0 {
        return;
    }
    let direction = [actor.facing.cos(), actor.facing.sin()];
    let x = velocity[0] * -direction[1];
    let y = velocity[1] * direction[0];
    let lateral_dot = x + y;
    let lateral_velocity = [lateral_dot * -direction[1], lateral_dot * direction[0]];
    let ultra_friction = if actor.ai.motion.ultra_accurate() {
        0.5
    } else {
        0.0
    };
    let extra = rules.extra_friction + ultra_friction;
    let lateral_friction = rules.mass * (rules.lateral_friction + extra).clamp(0.01, 0.99);
    let mut force = [
        -(lateral_friction * lateral_velocity[0]),
        -(lateral_friction * lateral_velocity[1]),
    ];
    if actor.ai.motion.motive_until <= frame {
        let x = velocity[0] * direction[0];
        let y = velocity[1] * direction[1];
        let forward_dot = x + y;
        let forward_velocity = [forward_dot * direction[0], forward_dot * direction[1]];
        let forward_friction = rules.mass * (rules.forward_friction + extra).clamp(0.01, 0.99);
        let x = -(forward_friction * forward_velocity[0]);
        let y = -(forward_friction * forward_velocity[1]);
        force[0] += x;
        force[1] += y;
    }
    apply_force(actor, rules.mass, force, frame);
}
