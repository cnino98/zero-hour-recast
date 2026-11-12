//! `MissileAIUpdate`, projectile `AIMoveToState`, and the selected THRUST branch.
mod integration;
mod physics;
#[cfg(test)]
mod tests;
mod thrust;

use super::{Motion, ProjectileLaunch, direction_axes};
use crate::collision_geometry::Geometry;
use thrust::{length, normalize, subtract};

// Native Locomotor::LocoFlag positions, Locomotor.h:425–438.
const IS_BRAKING: u32 = 1;
const PRECISE_Z_POS: u32 = 1 << 3;

pub struct MissileRules {
    pub retirement: MissileRetirement,
    pub initial_velocity: f32,
    pub jam_scatter: f32,
    pub ignition_delay: u32,
    pub fuel_lifetime: u32,
    pub initial_distance: f32,
    pub dive_distance: f32,
    pub lock_distance: f32,
    pub follow_target: bool,
    pub use_weapon_speed: bool,
    pub detonate_on_no_fuel: bool,
    pub thrust: ThrustRules,
    pub physics: ProjectilePhysics,
    pub geometry: Geometry,
    pub clear: Option<super::ClearBuilding>,
}

pub struct MissileRetirement {
    pub calls_kill: bool,
    pub delay: u32,
}

pub struct ThrustRules {
    pub max_speed: f32,
    pub min_speed: f32,
    pub acceleration: f32,
    pub braking: f32,
    pub turn_rate: f32,
    pub max_thrust_angle: f32,
    pub preferred_height: f32,
    pub height_damping: f32,
    pub close_enough: f32,
    pub airborne_motive: bool,
}

pub struct ProjectilePhysics {
    pub mass: f32,
    pub gravity: f32,
    pub forward_friction: f32,
    pub lateral_friction: f32,
    pub aerodynamic_friction: f32,
    pub extra_friction: f32,
    pub apply_friction_when_airborne: bool,
}

pub(super) struct Target {
    pub position: [f32; 3],
    pub max_height: f32,
    pub center_height: f32,
    pub sphere_radius: f32,
    pub velocity: [f32; 3],
    pub direction: [f32; 3],
}

enum Collision {
    Inert,
    Armed,
}

enum State {
    Prelaunch,
    Launch,
    AttackNoTurn,
    Attack,
    Kill,
    Retiring,
    Dead,
}

struct MoveGoal {
    position: [f32; 3],
    idle: bool,
    maintain: Option<[f32; 3]>,
}

pub(in crate::skirmish) struct Missile {
    state: State,
    collision: Collision,
    state_frame: u32,
    fuel_expires: u32,
    no_turn_distance: f32,
    previous_position: [f32; 3],
    original_target: [f32; 3],
    goal: MoveGoal,
    maximum_acceleration: f32,
    maximum_speed: f32,
    acceleration_cap: f32,
    turn_cap: f32,
    tracking: bool,
    decoy_frame: u32,
    no_damage: bool,
    launched: bool,
    locomotor_flags: u32,
}

pub(in crate::skirmish) enum Event {
    None,
    Detonate([f32; 3]),
    Remove,
}

impl Missile {
    pub(in crate::skirmish) const fn set_countermeasure_delay(&mut self, deadline: u32) {
        self.decoy_frame = deadline;
    }
    pub(in crate::skirmish) const fn countermeasure_due(&mut self, now: u32) -> bool {
        if self.decoy_frame == 0 || self.decoy_frame > now {
            return false;
        }
        self.decoy_frame = 0;
        self.no_damage = true;
        true
    }
    pub(in crate::skirmish) const fn countermeasure_divert(&mut self, position: [f32; 3]) {
        self.goal = MoveGoal {
            position,
            idle: false,
            maintain: None,
        };
        self.original_target = position;
        self.tracking = true;
    }
    pub(in crate::skirmish) const fn deals_damage(&self) -> bool {
        !self.no_damage
    }

    pub(in crate::skirmish) fn jam_destination(&self, target: Option<[f32; 3]>) -> [f32; 3] {
        target
            .filter(|_| self.tracking)
            .unwrap_or(self.goal.position)
    }

    pub(in crate::skirmish) const fn jam(&mut self, position: [f32; 3]) {
        self.tracking = false;
        self.original_target = position;
        self.goal = MoveGoal {
            position,
            idle: false,
            maintain: None,
        };
    }

    pub(in crate::skirmish) const fn new(
        position: [f32; 3],
        rules: &MissileRules,
        frame: u32,
    ) -> Self {
        Self {
            state: State::Prelaunch,
            collision: Collision::Inert,
            state_frame: frame,
            fuel_expires: 0,
            no_turn_distance: rules.initial_distance,
            previous_position: position,
            original_target: [0.0; 3],
            goal: MoveGoal {
                position: [0.0; 3],
                idle: true,
                maintain: None,
            },
            maximum_acceleration: 99_999.0,
            maximum_speed: 99_999.0,
            acceleration_cap: 0.0,
            turn_cap: 0.0,
            tracking: false,
            decoy_frame: 0,
            no_damage: false,
            launched: false,
            locomotor_flags: 0,
        }
    }

    pub(in crate::skirmish::projectiles) fn fire(
        &mut self,
        motion: &mut Motion,
        launch: &ProjectileLaunch<'_>,
        target: Option<&Target>,
        rules: &MissileRules,
    ) {
        let ProjectileLaunch {
            transform: launch,
            destination,
            speed: weapon_speed,
            frame,
        } = *launch;
        self.launched = true;
        let delta = subtract(destination, launch.position);
        let squares = delta.map(|value| value * value);
        let xy_squared = squares[0] + squares[1];
        let xy_distance = xy_squared.sqrt().max(1.0);
        let z_factor = if delta[2] > 0.0 {
            delta[2] / xy_distance
        } else {
            0.0
        };
        let mut direction = normalize(launch.axes[0]);
        let lift = 2.0 * z_factor;
        direction[2] += lift;
        direction = normalize(direction);
        let initial_velocity = if rules.use_weapon_speed {
            weapon_speed
        } else {
            rules.initial_velocity
        };
        let tracking = target.is_some() && rules.follow_target;
        let original_target = target
            .filter(|_| tracking)
            .map_or(destination, |target| target.position);
        let mut goal = original_target;
        if !tracking && rules.lock_distance > 0.0 {
            goal[2] += 10.0;
        }
        motion.position = launch.position;
        motion.axes = direction_axes(direction);
        self.state = State::Launch;
        self.state_frame = frame;
        self.original_target = original_target;
        self.goal = MoveGoal {
            position: goal,
            idle: false,
            maintain: None,
        };
        self.tracking = tracking;
        if rules.use_weapon_speed {
            self.maximum_acceleration = weapon_speed;
            self.maximum_speed = weapon_speed;
        }
        if initial_velocity > 0.0 {
            let impulse = rules.physics.mass * initial_velocity;
            motion.motive_force(
                direction.map(|value| impulse * value),
                &rules.physics,
                frame,
            );
        }
    }

    pub(in crate::skirmish::projectiles) const fn physics_braking(&self) -> bool {
        self.locomotor_flags & IS_BRAKING != 0
    }

    pub(super) fn update(
        &mut self,
        motion: &mut Motion,
        target: Option<&Target>,
        rules: &MissileRules,
        frame: u32,
        surface_height: f32,
    ) -> Event {
        if self.no_turn_distance > 0.0 && !matches!(self.state, State::Prelaunch | State::Launch) {
            self.no_turn_distance -= length(subtract(motion.position, self.previous_position));
            self.previous_position = motion.position;
        }
        if motion.position[2] < 0.0 {
            self.state = State::Dead;
            return Event::Remove;
        }
        let event = match self.state {
            State::Prelaunch => {
                self.acceleration_cap = 0.0;
                self.turn_cap = 0.0;
                Event::None
            }
            State::Launch => {
                self.ignite(rules, frame);
                Event::None
            }
            State::AttackNoTurn | State::Attack => self.attack_state(motion, target, rules, frame),
            State::Kill => self.kill_state(motion, target, rules, frame),
            State::Retiring => {
                if self.state_frame > frame.wrapping_sub(rules.retirement.delay) {
                    Event::None
                } else {
                    Event::Remove
                }
            }
            State::Dead => Event::None,
        };
        if matches!(event, Event::None) {
            self.update_move(motion, target, rules, frame, surface_height);
        }
        event
    }

    const fn ignite(&mut self, rules: &MissileRules, frame: u32) {
        self.acceleration_cap = 0.0;
        self.turn_cap = 0.0;
        if frame.wrapping_sub(self.state_frame) >= rules.ignition_delay {
            self.acceleration_cap = self.maximum_acceleration;
            self.collision = Collision::Armed;
            self.fuel_expires = if rules.fuel_lifetime == 0 {
                0x7fff_ffff
            } else {
                frame.wrapping_add(rules.fuel_lifetime)
            };
            self.state = State::AttackNoTurn;
            self.state_frame = frame;
        }
    }

    fn attack_state(
        &mut self,
        motion: &Motion,
        target: Option<&Target>,
        rules: &MissileRules,
        frame: u32,
    ) -> Event {
        if frame >= self.fuel_expires {
            if rules.detonate_on_no_fuel {
                return Event::Detonate(motion.position);
            }
            self.acceleration_cap = 0.0;
            self.turn_cap = 0.0;
        } else {
            self.acceleration_cap = self.maximum_acceleration;
            self.turn_cap = if matches!(self.state, State::Attack) {
                99_999.0
            } else {
                0.0
            };
        }
        let destination = target
            .filter(|_| self.tracking)
            .map_or(self.goal.position, |target| target.position);
        let delta = subtract(motion.position, destination);
        let squares = delta.map(|value| value * value);
        let distance_squared = squares[0] + squares[1];
        if rules.lock_distance > 0.0 {
            let lock = if self.tracking {
                rules.lock_distance
            } else {
                rules.lock_distance * 0.5
            };
            if distance_squared < lock * lock {
                if !self.tracking {
                    self.goal.position = self.original_target;
                    self.goal.idle = false;
                }
                self.state = State::Kill;
                self.state_frame = frame;
                return Event::None;
            }
        }
        // RETAIL_COMPATIBLE_CRC=0: use the live target's position when diving.
        if rules.thrust.preferred_height > 0.0
            && distance_squared < rules.dive_distance * rules.dive_distance
        {
            self.locomotor_flags |= PRECISE_Z_POS;
        }
        if self.no_turn_distance <= 0.0 {
            self.state = State::Attack;
            self.state_frame = frame;
        }
        if self.tracking && target.is_none() {
            self.target_gone(frame);
        }
        Event::None
    }

    fn kill_state(
        &mut self,
        motion: &mut Motion,
        target: Option<&Target>,
        rules: &MissileRules,
        frame: u32,
    ) -> Event {
        if frame >= self.fuel_expires {
            if rules.detonate_on_no_fuel {
                return Event::Detonate(motion.position);
            }
            self.target_gone(frame);
            return Event::None;
        }
        self.locomotor_flags |= IS_BRAKING;
        self.acceleration_cap = self.maximum_acceleration;
        self.turn_cap = 99_999.0;
        if self.goal.idle {
            if let Some(target) = target.filter(|_| self.tracking) {
                let mut center = target.position;
                center[2] += target.center_height;
                let distance = (length(subtract(motion.position, center))
                    - rules.geometry.major
                    - target.sphere_radius)
                    .max(0.0);
                let close = self.maximum_speed.min(rules.thrust.max_speed);
                if distance * distance < close * close {
                    motion.position = target.position;
                    return Event::Detonate(motion.position);
                }
                self.goal.idle = false;
            } else {
                return Event::Detonate(motion.position);
            }
        }
        if self.tracking && target.is_none() {
            self.target_gone(frame);
        }
        Event::None
    }

    const fn target_gone(&mut self, frame: u32) {
        self.fuel_expires = frame;
        self.state = State::Retiring;
        self.state_frame = frame;
    }

    pub(super) const fn finish_detonation(&mut self, frame: u32) {
        self.state = State::Retiring;
        self.state_frame = frame;
    }

    pub(super) const fn can_collide(&self) -> bool {
        matches!(self.collision, Collision::Armed) && !matches!(self.state, State::Dead)
    }

    pub(super) const fn collide(&self, motion: &Motion, _frame: u32) -> Event {
        if !self.can_collide() {
            return Event::None;
        }
        Event::Detonate(motion.position)
    }

    pub(super) const fn finish_removal(&mut self) {
        self.state = State::Dead;
    }

    pub(super) fn ground_contact(&self, motion: &Motion, frame: u32) -> Event {
        if !self.can_collide() {
            return Event::None;
        }
        let delta = subtract(motion.position, self.goal.position);
        if delta[2] > 10.0 && length(delta) > 30.0 {
            return Event::None;
        }
        self.collide(motion, frame)
    }

    fn update_move(
        &mut self,
        motion: &mut Motion,
        target: Option<&Target>,
        rules: &MissileRules,
        frame: u32,
        surface: f32,
    ) {
        if self.goal.idle {
            let goal = *self.goal.maintain.get_or_insert(motion.position);
            self.thrust(
                motion,
                &crate::skirmish::projectiles::missile::thrust::ThrustRequest {
                    position: goal,
                    distance: 0.0,
                    speed: rules.thrust.min_speed,
                },
                rules,
                frame,
                surface,
            );
            return;
        }
        let destination = self.move_destination(motion, target);
        let delta = subtract(destination, motion.position);
        let distance = length(delta);
        if distance < rules.thrust.close_enough {
            self.goal.idle = true;
            self.goal.maintain = Some(motion.position);
            self.thrust(
                motion,
                &crate::skirmish::projectiles::missile::thrust::ThrustRequest {
                    position: motion.position,
                    distance: 0.0,
                    speed: rules.thrust.min_speed,
                },
                rules,
                frame,
                surface,
            );
        } else {
            self.move_towards(motion, destination, distance, rules, frame, surface);
        }
    }

    fn move_destination(&self, motion: &Motion, target: Option<&Target>) -> [f32; 3] {
        let target = target.filter(|_| self.tracking);
        let mut goal = target.map_or(self.goal.position, |target| {
            let mut goal = target.position;
            let half_height = target.max_height * 0.5;
            goal[2] += half_height;
            goal
        });
        let delta_z = goal[2] - motion.position[2];
        if delta_z > 0.0 {
            goal[2] += delta_z;
        }
        if let Some(target) = target {
            let my_speed = length(motion.velocity).max(5.0);
            let target_speed = length(target.velocity);
            let lead = 0.5 * length(subtract(goal, motion.position)) * target_speed / my_speed;
            for (coordinate, direction) in goal.iter_mut().zip(target.direction) {
                let offset = lead * direction;
                *coordinate += offset;
            }
        }
        goal
    }
}
