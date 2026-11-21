use super::{
    Appearance, BRAKING, Entity, Goal, Height, Motion, OBJECT_BRAKING, PRECISE_Z, Rules,
    ULTRA_ACCURATE, UnitType, locomotor,
};
use std::f32::consts::PI;

struct Drive<'a> {
    rules: &'a Rules,
    terrain: &'a crate::skirmish::physical_ocl::transform::TerrainContext<'a>,
    acceleration: f32,
    turn: f32,
    pivot: f32,
    frame: u64,
    maximum: f32,
    mass: f32,
}

pub(super) fn update(
    actor: &mut Entity,
    unit: &UnitType,
    mass: f32,
    state: &mut Motion,
    terrain: &crate::skirmish::physical_ocl::transform::TerrainContext<'_>,
    frame: u64,
    maximum: f32,
) {
    let ground = terrain.ground_height(actor.position);
    let loco = actor.active_locomotor(unit);
    let rules = loco.flight.as_ref().expect("flight rules");
    let damaged = actor.health / actor.max_health <= loco.damage_health_threshold;
    let acceleration = if damaged {
        rules.acceleration_damaged
    } else {
        rules.acceleration
    };
    let turn = if damaged {
        loco.turn_rate_damaged
    } else {
        loco.turn_rate
    }
    .min(state.turn_limit)
        * if state.flag(ULTRA_ACCURATE) { 2.0 } else { 1.0 };
    let drive = Drive {
        rules,
        terrain,
        acceleration,
        turn,
        pivot: loco.turn_pivot_offset,
        frame,
        maximum,
        mass,
    };
    if let Some(angle) = actor.ai.motion.angle_goal() {
        move_to_angle(actor, unit, state, angle, damaged, &drive);
        return;
    }
    if actor.physics_stunned() {
        if state.goal.take().is_some() {
            state.maintain = None;
        } else {
            let origin = [actor.position[0], actor.position[1], state.height];
            let maintain = *state.maintain.get_or_insert(origin);
            state.set(BRAKING, false);
            height(state, &drive, maintain[2], actor, damaged);
        }
        return;
    }
    set_jet_goal_if_absent(actor, state, maximum);
    if let Some(goal) = state.goal.take() {
        state.maintain = None;
        move_towards(actor, unit, state, &goal, ground, &drive);
        height(state, &drive, goal.position[2], actor, damaged);
    } else {
        let origin = [actor.position[0], actor.position[1], state.height];
        let maintain = *state.maintain.get_or_insert(origin);
        state.set(BRAKING, false);
        if state.motive_until > frame {
            match rules.appearance {
                Appearance::Hover => {
                    let minimum = rules.min_speed.max(1.0E-10);
                    let delta = minimum - forward_speed(state, actor.facing);
                    if delta.abs() > minimum {
                        let direction = [actor.facing.cos(), actor.facing.sin()];
                        change_speed(state, &drive, delta, direction);
                    }
                }
                Appearance::Wings if state.height > ground => {
                    let point = circle(actor, state, rules, maintain, turn);
                    let goal = Goal {
                        position: point,
                        distance: 0.0,
                        speed: rules.min_speed,
                        no_slow_down: false,
                    };
                    other(actor, unit, state, &goal, &drive);
                }
                Appearance::Wings => {}
            }
        }
        height(state, &drive, maintain[2], actor, damaged);
    }
}

// Locomotor.cpp868: ANGLE has the same dispatcher priority for flight and ground.
fn move_to_angle(
    actor: &mut Entity,
    unit: &UnitType,
    state: &mut Motion,
    angle: f32,
    damaged: bool,
    drive: &Drive<'_>,
) {
    state.maintain = None;
    state.goal = None;
    if actor.physics_stunned() {
        return;
    }
    let minimum = drive.rules.min_speed;
    if minimum > 0.0 {
        let offset = [angle.cos(), angle.sin()].map(|direction| direction * minimum * 2.0);
        let goal = Goal {
            position: [
                actor.position[0] + offset[0],
                actor.position[1] + offset[1],
                state.height,
            ],
            distance: 99_999.0,
            speed: minimum.min(drive.maximum),
            no_slow_down: false,
        };
        let ground = drive.terrain.ground_height(actor.position);
        move_towards(actor, unit, state, &goal, ground, drive);
        height(state, drive, goal.position[2], actor, damaged);
    } else {
        let offset = [angle.cos(), angle.sin()].map(|direction| direction * 1000.0);
        let target = [actor.position[0] + offset[0], actor.position[1] + offset[1]];
        state.height = locomotor::turn_flight(
            actor,
            unit,
            target,
            drive.turn,
            if state.flag(BRAKING) {
                0.0
            } else {
                drive.pivot
            },
            drive.terrain,
        );
        let current_height = state.height;
        height(state, drive, current_height, actor, damaged);
    }
}

fn set_jet_goal_if_absent(actor: &Entity, state: &mut Motion, maximum: f32) {
    if state.goal.is_none()
        && let Some(position) = actor.jet_goal_when_none(state.height)
    {
        let dx = position[0] - actor.position[0];
        let dy = position[1] - actor.position[1];
        let xx = dx * dx;
        let yy = dy * dy;
        state.goal = Some(Goal {
            position,
            distance: (xx + yy).sqrt(),
            speed: maximum,
            no_slow_down: false,
        });
    }
}

fn move_towards(
    actor: &mut Entity,
    unit: &UnitType,
    state: &mut Motion,
    goal: &Goal,
    ground: f32,
    drive: &Drive<'_>,
) {
    let rules = drive.rules;
    let frame = drive.frame;
    let dx = goal.position[0] - actor.position[0];
    let dy = goal.position[1] - actor.position[1];
    let xx = dx * dx;
    let yy = dy * dy;
    let distance = (xx + yy).sqrt();
    if goal.distance > 10.0
        && goal.distance
            > (drive.maximum / rules.braking.min(state.braking_limit)) * drive.maximum / 2.0
    {
        state.set(BRAKING, false);
    }
    if distance > goal.distance && distance > 2.0 * goal.distance {
        state.set(BRAKING, true);
    }
    if matches!(rules.appearance, Appearance::Wings) {
        state.set(BRAKING, false);
    }
    state.force_applied = true;
    state.motive_until = frame + 10;
    let was_braking = state.flag(OBJECT_BRAKING);
    if rules.airborne_force || state.height - ground <= -9.0 * rules.gravity {
        let adjusted = Goal {
            position: goal.position,
            distance: goal.distance.max(distance),
            speed: goal.speed,
            no_slow_down: goal.no_slow_down,
        };
        other(actor, unit, state, &adjusted, drive);
    }
    state.set(OBJECT_BRAKING, state.flag(BRAKING));
    if was_braking && distance > 0.001 {
        let speed = forward_speed(state, actor.facing)
            .abs()
            .max(10.0 / 30.0)
            .min(distance);
        let inverse = 1.0 / distance;
        let step_x = (dx * inverse) * speed;
        let step_y = (dy * inverse) * speed;
        actor.position[0] += step_x;
        actor.position[1] += step_y;
    }
}

fn other(actor: &mut Entity, unit: &UnitType, state: &mut Motion, goal: &Goal, drive: &Drive<'_>) {
    let rules = drive.rules;
    let actual = forward_speed(state, actor.facing);
    // The source snapshots the force direction BEFORE rotating the object.
    let mut direction = [actor.facing.cos(), actor.facing.sin()];
    let dx = goal.position[0] - actor.position[0];
    let dy = goal.position[1] - actor.position[1];
    if state.flag(ULTRA_ACCURATE)
        && dx.abs() <= goal.speed * rules.slide_frames
        && dy.abs() <= goal.speed * rules.slide_frames
    {
        actor.physics_control.turning = false;
        let xx = dx * dx;
        let yy = dy * dy;
        let magnitude = (xx + yy).sqrt();
        direction = if magnitude > 0.0 {
            [dx / magnitude, dy / magnitude]
        } else {
            [0.0; 2]
        };
    } else {
        state.height = locomotor::turn_flight(
            actor,
            unit,
            [goal.position[0], goal.position[1]],
            drive.turn,
            if state.flag(BRAKING) {
                0.0
            } else {
                drive.pivot
            },
            drive.terrain,
        );
    }
    let delta = actual - rules.min_speed;
    let slow = if delta <= 0.0 {
        0.0
    } else {
        ((delta * delta / rules.braking.min(state.braking_limit).abs()) * 0.5) * 1.05
    };
    let wanted = if !goal.no_slow_down && goal.distance < slow {
        rules.min_speed
    } else {
        goal.speed
    };
    change_speed(state, drive, wanted - actual, direction);
}

fn circle(actor: &Entity, state: &Motion, rules: &Rules, origin: [f32; 3], turn: f32) -> [f32; 3] {
    let mut radius = rules.circling_radius;
    if radius == 0.0 {
        radius = if turn > 0.0 {
            rules.min_speed / turn
        } else {
            99_999.0
        };
    }
    let dx = origin[0] - actor.position[0];
    let dy = origin[1] - actor.position[1];
    let angle = if dx.abs() < 0.001 && dy.abs() < 0.001 {
        actor.facing
    } else {
        dy.atan2(dx)
    };
    let mut aim = PI - PI / 8.0;
    if radius < 0.0 {
        radius = -radius;
        aim = -aim;
    }
    let angle = angle + aim;
    let dx = angle.cos() * radius;
    let dy = angle.sin() * radius;
    [origin[0] + dx, origin[1] + dy, state.height]
}

fn change_speed(state: &mut Motion, drive: &Drive<'_>, delta: f32, direction: [f32; 2]) {
    if delta == 0.0 {
        return;
    }
    let mut force = drive.mass
        * if delta > 0.0 {
            drive.acceleration
        } else {
            -drive.rules.braking.min(state.braking_limit)
        };
    let needed = drive.mass * delta;
    if force.abs() > needed.abs() {
        force = needed;
    }
    let inverse = 1.0 / drive.mass;
    let ax = (force * direction[0]) * inverse;
    let ay = (force * direction[1]) * inverse;
    state.acceleration[0] += ax;
    state.acceleration[1] += ay;
    state.force_applied |= !(ax.is_nan() || ay.is_nan());
    state.motive_until = drive.frame + 10;
}

pub(super) fn forward_speed(state: &Motion, facing: f32) -> f32 {
    let vx = state.velocity[0] * facing.cos();
    let vy = state.velocity[1] * facing.sin();
    let xx = vx * vx;
    let yy = vy * vy;
    let magnitude = (xx + yy).sqrt();
    if vx + vy >= 0.0 {
        magnitude
    } else {
        -magnitude
    }
}

fn height(state: &mut Motion, drive: &Drive<'_>, goal: f32, actor: &Entity, damaged: bool) {
    let rules = drive.rules;
    if actor.chinook.is_some() {
        return;
    }
    let preferred_height = state.preferred_height.unwrap_or(rules.preferred_height);
    let surface = match rules.height {
        Height::Surface | Height::FixedSurface => {
            let ground = drive.terrain.ground_height(actor.position);
            drive.terrain.water_surface(actor.position, ground)
        }
        Height::HighestLayer => {
            drive
                .terrain
                .flight_layer_surface(actor.position, state.height, actor.layer)
        }
        _ => 0.0,
    };
    if matches!(rules.height, Height::FixedSurface | Height::FixedAbsolute) {
        state.height = preferred_height + surface;
    } else if !matches!(rules.height, Height::None)
        && (preferred_height != 0.0 || state.flag(PRECISE_Z))
    {
        let preferred = if state.flag(PRECISE_Z) {
            goal
        } else {
            preferred_height + surface
        };
        let difference = (preferred - state.height) * rules.damping;
        let target = state.height + difference;
        let gross = if damaged {
            rules.lift_damaged
        } else {
            rules.lift
        };
        let force = super::physics::lift(
            state.height,
            state.velocity[2],
            target,
            gross.min(state.lift_limit),
            rules.gravity,
            rules.speed_limit_z,
            state.flag(ULTRA_ACCURATE),
        );
        if force != 0.0 {
            let acceleration = (force * drive.mass) * (1.0 / drive.mass);
            state.acceleration[2] += acceleration;
            state.force_applied |= !acceleration.is_nan();
            state.motive_until = drive.frame + 10;
        }
    }
}
