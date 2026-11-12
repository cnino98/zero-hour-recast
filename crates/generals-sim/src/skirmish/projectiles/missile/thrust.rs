use super::{Missile, MissileRules};
use crate::skirmish::projectiles::Motion;
use crate::skirmish::projectiles::direction_axes;

pub(super) fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

pub(super) fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    let products: [f32; 3] = std::array::from_fn(|axis| left[axis] * right[axis]);
    products[0] + products[1] + products[2]
}

pub(super) fn length(vector: [f32; 3]) -> f32 {
    dot(vector, vector).sqrt()
}

pub(super) fn normalize(vector: [f32; 3]) -> [f32; 3] {
    let squared = dot(vector, vector);
    if squared == 0.0 {
        vector
    } else {
        let inverse = 1.0 / squared.sqrt();
        vector.map(|value| value * inverse)
    }
}

// Locomotor::tryToRotateVector3D and Matrix3D(axis, angle), including the
// original zero cross-product behavior for opposite input directions.
fn rotate(maximum: f32, current: [f32; 3], goal: [f32; 3]) -> ([f32; 3], f32) {
    if maximum.abs() < 0.001 {
        return (current, 0.0);
    }
    let original = current;
    let current = normalize(current);
    let goal = normalize(goal);
    let angle = dot(current, goal).clamp(-1.0, 1.0).acos();
    let maximum = if maximum < 0.0 {
        -maximum * angle
    } else {
        maximum
    };
    if maximum.abs() < 0.001 {
        return (original, 0.0);
    }
    if angle.abs() <= maximum {
        return (goal, angle);
    }
    let axis = normalize(std::array::from_fn(|index| {
        let next = (index + 1) % 3;
        let last = (index + 2) % 3;
        let positive = current[next] * goal[last];
        let negative = current[last] * goal[next];
        positive - negative
    }));
    let cosine = maximum.cos();
    let sine = maximum.sin();
    let one_minus_cosine = 1.0 - cosine;
    let rows: [[f32; 3]; 3] = std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            if row == column {
                let square = axis[row] * axis[row];
                let scaled = cosine * (1.0 - square);
                square + scaled
            } else {
                let other = 3 - row - column;
                // Matrix3D writes XZ as axis[2]*axis[0], preserving its order.
                let product = if row == 0 && column == 2 {
                    axis[2] * axis[0]
                } else if row == 1 && column == 0 {
                    axis[0] * axis[1]
                } else if row == 2 && column == 1 {
                    axis[1] * axis[2]
                } else {
                    axis[row] * axis[column]
                };
                let scaled = product * one_minus_cosine;
                let offset = axis[other] * sine;
                if (row + 1) % 3 == column {
                    scaled - offset
                } else {
                    scaled + offset
                }
            }
        })
    });
    (rows.map(|row| dot(row, current)), maximum)
}

pub(super) struct ThrustRequest {
    pub(super) position: [f32; 3],
    pub(super) distance: f32,
    pub(super) speed: f32,
}

impl Missile {
    fn thrust_direction(
        motion: &Motion,
        goal: [f32; 3],
        acceleration: f32,
        gravity: f32,
    ) -> [f32; 3] {
        let delta = subtract(goal, motion.position);
        if dot(delta, delta).abs() < 0.001 {
            return motion.axes[0];
        }
        let mut velocity = motion.velocity;
        velocity[2] += gravity;
        let distance = length(delta);
        let velocity_squared = dot(velocity, velocity);
        let magnitude = velocity_squared.sqrt();
        let acceleration_squared = acceleration * acceleration;
        let denominator = velocity_squared - acceleration_squared;
        if denominator.abs() >= 0.001 {
            let mut time = distance * (magnitude + acceleration) / denominator;
            let second_time = distance * (magnitude - acceleration) / denominator;
            if time >= 0.0 || second_time >= 0.0 {
                if time < 0.0 || (second_time >= 0.0 && second_time < time) {
                    time = second_time;
                }
                if time.abs() >= 0.001 {
                    return normalize(std::array::from_fn(|axis| {
                        delta[axis] / time - velocity[axis]
                    }));
                }
            }
        }
        normalize(delta)
    }

    pub(super) fn move_towards(
        &mut self,
        motion: &mut Motion,
        goal: [f32; 3],
        distance: f32,
        rules: &MissileRules,
        frame: u32,
        surface: f32,
    ) {
        self.goal.maintain = None;
        let maximum = self.maximum_speed.min(rules.thrust.max_speed);
        let delta = subtract(goal, motion.position);
        let was_braking = self.locomotor_flags & super::IS_BRAKING;
        // Source applies even a zero motive force before dispatching appearance.
        motion.motive_force([0.0; 3], &rules.physics, frame);
        if rules.thrust.airborne_motive
            || motion.position[2] - surface <= -9.0 * rules.physics.gravity
        {
            self.thrust(
                motion,
                &crate::skirmish::projectiles::missile::thrust::ThrustRequest {
                    position: goal,
                    distance,
                    speed: maximum,
                },
                rules,
                frame,
                surface,
            );
        }
        // For the selected quick, exact AIR path, direct XY distance cannot
        // exceed its three-dimensional path distance: IS_BRAKING stays false.
        self.locomotor_flags = (self.locomotor_flags & !super::IS_BRAKING) | was_braking;
        if was_braking != 0 {
            let distance = length(delta);
            let speed = length(motion.velocity).max(10.0 / 30.0).min(distance);
            if distance > 0.001 {
                let inverse = 1.0 / distance;
                for (position, component) in motion.position.iter_mut().zip(delta) {
                    let direction = component * inverse;
                    let travel = direction * speed;
                    *position += travel;
                }
            }
        }
    }

    pub(super) fn thrust(
        &self,
        motion: &mut Motion,
        request: &ThrustRequest,
        rules: &MissileRules,
        frame: u32,
        surface: f32,
    ) {
        let ThrustRequest {
            position: goal,
            distance,
            speed: desired,
        } = *request;
        let settings = &rules.thrust;
        let mut maximum = self.maximum_speed.min(settings.max_speed);
        let mut desired = desired.clamp(settings.min_speed, maximum);
        let components = std::array::from_fn(|axis| motion.velocity[axis] * motion.axes[0][axis]);
        let actual = if components[0] + components[1] + components[2] >= 0.0 {
            length(components)
        } else {
            -length(components)
        };
        if settings.braking > 0.0 {
            let difference = actual - settings.min_speed;
            let slowing = if difference <= 0.0 {
                0.0
            } else {
                ((difference * difference) / settings.braking.abs()) * 0.5 * 1.05
            };
            if distance < slowing {
                desired = settings.min_speed;
            }
        }
        let mut local_goal = goal;
        if settings.preferred_height != 0.0 && self.locomotor_flags & super::PRECISE_Z_POS == 0 {
            let height = settings.preferred_height + surface;
            let adjustment = (height - motion.position[2]) * settings.height_damping;
            local_goal[2] = motion.position[2] + adjustment;
        }
        let forward_delta = desired - actual;
        let acceleration = if forward_delta > 0.0 || settings.braking == 0.0 {
            self.acceleration_cap.min(settings.acceleration)
        } else {
            -settings.braking
        };
        let turn = self.turn_cap.min(settings.turn_rate);
        let desired_direction =
            Self::thrust_direction(motion, local_goal, acceleration, rules.physics.gravity);
        let maximum_angle = if turn > 0.0 {
            settings.max_thrust_angle
        } else {
            0.0
        };
        let (direction, angle) = rotate(maximum_angle, motion.axes[0], desired_direction);
        self.orient(motion, goal, turn);
        if forward_delta != 0.0 || angle != 0.0 {
            if maximum <= 0.0 {
                maximum = 0.01;
            }
            let damping = (acceleration / maximum).clamp(0.0, 1.0);
            let force = std::array::from_fn(|axis| {
                let thrust = direction[axis] * acceleration;
                let drag = motion.velocity[axis] * damping;
                rules.physics.mass * (thrust - drag)
            });
            motion.motive_force(force, &rules.physics, frame);
        }
    }

    fn orient(&self, motion: &mut Motion, goal: [f32; 3], mut turn: f32) {
        if length(motion.velocity).abs() < 0.001 {
            return;
        }
        let direction = if self.locomotor_flags & super::IS_BRAKING != 0 {
            let direction = subtract(goal, motion.position);
            if dot(direction, direction).abs() < 0.001 {
                return;
            }
            turn *= 3.0;
            direction
        } else {
            motion.velocity
        };
        let (direction, angle) = rotate(turn, motion.axes[0], direction);
        if angle != 0.0 {
            motion.axes = direction_axes(direction);
        }
    }
}
