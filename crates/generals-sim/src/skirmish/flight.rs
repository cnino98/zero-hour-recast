//! Native HOVER/WINGS locomotion and its separate `PhysicsBehavior` phase.
//! Source: SH 2bfaea60, Locomotor.cpp and PhysicsUpdate.cpp.
use super::{Entity, Skirmish, UnitType, locomotor};

mod aircraft;
pub mod draw;
mod motion;
pub(super) mod physics;
#[cfg(test)]
pub(super) mod tests;

const BRAKING: u8 = 1;
const OBJECT_BRAKING: u8 = 2;
const PRECISE_Z: u8 = 4;
const ULTRA_ACCURATE: u8 = 8;

pub enum Appearance {
    Hover,
    Wings,
}

pub enum Height {
    None,
    Surface,
    Absolute,
    FixedSurface,
    FixedAbsolute,
}

pub struct Rules {
    pub draw: draw::Rules,
    pub appearance: Appearance,
    pub height: Height,
    pub acceleration: f32,
    pub acceleration_damaged: f32,
    pub braking: f32,
    pub min_speed: f32,
    pub close_enough: f32,
    pub close_3d: bool,
    pub preferred_height: f32,
    pub lift: f32,
    pub lift_damaged: f32,
    pub damping: f32,
    pub speed_limit_z: f32,
    pub gravity: f32,
    pub mass: f32,
    pub forward_friction: f32,
    pub lateral_friction: f32,
    pub aerodynamic_friction: f32,
    pub extra_friction: f32,
    pub friction_2d: bool,
    pub airborne_force: bool,
    pub slide_frames: f32,
    pub circling_radius: f32,
}

/// AI-applied motion before the single authoritative `PhysicsBehavior` update.
pub(in crate::skirmish) struct PhysicsInput {
    pub(in crate::skirmish) height: f32,
    pub(in crate::skirmish) velocity: [f32; 3],
    pub(in crate::skirmish) acceleration: [f32; 3],
    pub(in crate::skirmish) motive_until: u64,
    pub(in crate::skirmish) object_braking: bool,
    pub(in crate::skirmish) extra_friction: f32,
}

struct Goal {
    position: [f32; 3],
    distance: f32,
    speed: f32,
    no_slow_down: bool,
}

pub(super) struct Motion {
    pub(super) height: f32,
    velocity: [f32; 3],
    acceleration: [f32; 3],
    previous_acceleration: [f32; 3],
    motive_until: u64,
    force_applied: bool,
    goal: Option<Goal>,
    close_override: Option<(f32, bool)>,
    maintain: Option<[f32; 3]>,
    goal_z: Option<f32>,
    path: Option<super::path_following::Path>,
    endpoint: Option<[f32; 3]>,
    flags: u8,
    speed_limit: f32,
    lift_limit: f32,
    preferred_height: Option<f32>,
    braking_limit: f32,
    turn_limit: f32,
}

impl Motion {
    const fn flag(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    pub(in crate::skirmish) const fn set_precise_z(&mut self, enabled: bool) {
        self.set(PRECISE_Z, enabled);
    }

    const fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    pub(in crate::skirmish) const fn replace_locomotor(&mut self) {
        self.speed_limit = 99_999.0;
        self.lift_limit = 99_999.0;
        self.preferred_height = None;
        self.braking_limit = 99_999.0;
        self.turn_limit = 99_999.0;
        self.flags &= OBJECT_BRAKING;
        self.close_override = None;
        self.maintain = None;
        if let Some(goal) = &mut self.goal {
            goal.no_slow_down = false;
        }
    }

    pub(super) fn ordered(&mut self) {
        self.goal = None;
        self.goal_z = None;
        self.path = None;
        self.endpoint = None;
        self.maintain = None;
    }

    pub(super) const fn new(height: f32) -> Self {
        Self {
            height,
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            previous_acceleration: [0.0; 3],
            motive_until: 0,
            force_applied: false,
            goal: None,
            close_override: None,
            maintain: None,
            goal_z: None,
            path: None,
            endpoint: None,
            flags: 0,
            speed_limit: 99_999.0,
            lift_limit: 99_999.0,
            preferred_height: None,
            braking_limit: 99_999.0,
            turn_limit: 99_999.0,
        }
    }
}

impl Entity {
    pub(in crate::skirmish) fn flight_lift_limit(&self) -> f32 {
        self.flight
            .as_ref()
            .expect("flight locomotor instance")
            .lift_limit
    }

    fn set_physics_motive_expiry(&mut self, expiry: u64) {
        self.ai.motion.motive_until = expiry;
        if let Some(state) = &mut self.flight {
            state.motive_until = expiry;
        }
        self.set_death_motive_expiry(expiry);
    }

    pub(in crate::skirmish) fn reset_flight_dynamic_physics(&mut self) {
        if let Some(state) = &mut self.flight {
            state.velocity = [0.0; 3];
            state.acceleration = [0.0; 3];
            state.previous_acceleration = [0.0; 3];
        }
    }

    pub(in crate::skirmish) fn set_flight_velocity(&mut self, velocity: [f32; 3]) {
        if let Some(state) = &mut self.flight {
            state.velocity = velocity;
        }
    }

    pub(in crate::skirmish) fn physics_velocity(&self) -> [f32; 3] {
        if let Some(velocity) = self
            .projectile_velocity()
            .or_else(|| self.physical_object_velocity())
            .or_else(|| self.live_velocity())
        {
            return velocity;
        }
        let mut velocity = self.flight.as_ref().map_or(
            [self.ai.motion.velocity[0], self.ai.motion.velocity[1], 0.0],
            |state| state.velocity,
        );
        if let Some((_, vertical, _)) = self.chinook_physics() {
            velocity[2] = vertical;
        }
        velocity
    }

    pub(in crate::skirmish) fn aircraft_physics_input(
        &self,
        unit: &UnitType,
    ) -> Option<PhysicsInput> {
        if self.flight.is_none() && self.chinook.is_none() {
            return None;
        }
        let locomotor = self.active_locomotor(unit);
        let mut input = if let (Some(state), Some(rules)) = (&self.flight, &locomotor.flight) {
            PhysicsInput {
                height: state.height,
                velocity: state.velocity,
                acceleration: state.acceleration,
                motive_until: state.motive_until,
                object_braking: state.flag(OBJECT_BRAKING),
                extra_friction: rules.extra_friction
                    + if state.flag(ULTRA_ACCURATE) { 0.5 } else { 0.0 },
            }
        } else {
            PhysicsInput {
                height: self.flight.as_ref().map_or(0.0, |state| state.height),
                velocity: [
                    self.ai.motion.velocity[0],
                    self.ai.motion.velocity[1],
                    self.flight.as_ref().map_or(0.0, |state| state.velocity[2]),
                ],
                acceleration: [
                    self.ai.motion.acceleration[0],
                    self.ai.motion.acceleration[1],
                    0.0,
                ],
                motive_until: self.ai.motion.motive_until,
                object_braking: self.ai.motion.object_braking(),
                extra_friction: locomotor
                    .ground
                    .as_ref()
                    .map_or(0.0, |rules| rules.extra_friction)
                    + if self.ai.motion.ultra_accurate() {
                        0.5
                    } else {
                        0.0
                    },
            }
        };
        if let Some((height, velocity, acceleration)) = self.chinook_physics() {
            input.height = height;
            input.velocity[2] = velocity;
            input.acceleration[2] = acceleration;
        }
        Some(input)
    }

    pub(in crate::skirmish) fn consume_aircraft_acceleration(&mut self, total: [f32; 3]) {
        if let Some(state) = &mut self.flight {
            state.previous_acceleration = total;
            state.acceleration = [0.0; 3];
        }
        self.ai.motion.acceleration = [0.0; 2];
        self.consume_chinook_acceleration();
    }

    pub(in crate::skirmish) fn set_aircraft_physics(
        &mut self,
        position: [f32; 3],
        velocity: [f32; 3],
    ) {
        self.position = [position[0], position[1]];
        self.ai.motion.velocity = [velocity[0], velocity[1]];
        if let Some(state) = &mut self.flight {
            state.height = position[2];
            state.velocity = velocity;
        }
        self.set_chinook_physics(position[2], velocity[2]);
    }

    pub(super) fn flight_ultra_accurate(&self) -> bool {
        self.flight
            .as_ref()
            .is_some_and(|state| state.flag(ULTRA_ACCURATE))
    }
}

impl Skirmish {
    /// PhysicsUpdate.cpp414: motive force bypasses the current lateral-force filter.
    pub(in crate::skirmish) fn apply_physics_motive_force(&mut self, id: usize, force: [f32; 3]) {
        let Some(mass) = self.object_physics_mass(id) else {
            return;
        };
        let now = self.update_frame();
        if self.entities[id].projectile_flight.is_some() {
            self.apply_projectile_motive_force(id, force, mass, now);
            return;
        }
        self.entities[id].set_physics_motive_expiry(0);
        self.physical_force(id, force);
        self.entities[id].set_physics_motive_expiry(u64::from(now.wrapping_add(10)));
    }

    pub(super) fn clear_flight_velocity(&mut self, id: usize) {
        if let Some(state) = &mut self.entities[id].flight {
            state.velocity = [0.0; 3];
        }
        self.entities[id].ai.motion.velocity = [0.0; 2];
    }

    // CountermeasuresBehavior::launchVolley: transferVelocityTo, then applyMotiveForce.
    pub(in crate::skirmish) fn countermeasure_flare_force(
        &mut self,
        id: usize,
        inherited: [f32; 3],
        force: [f32; 3],
    ) {
        let now = self.update_frame();
        let mass = self.object_physics_mass(id).unwrap_or_else(|| {
            self.catalog[self.entities[id].type_index]
                .live_physics
                .as_ref()
                .expect("admitted countermeasure flare PhysicsBehavior")
                .motion
                .mass
        });
        let actor = &mut self.entities[id];
        let state = actor
            .flight
            .as_mut()
            .expect("admitted countermeasure HOVER locomotor");
        for (velocity, inherited) in state.velocity.iter_mut().zip(inherited) {
            *velocity += inherited;
        }
        state.motive_until = 0;
        let valid = !force.iter().any(|component| component.is_nan());
        if valid {
            let inverse = 1.0 / mass;
            for (acceleration, force) in state.acceleration.iter_mut().zip(force) {
                let change = force * inverse;
                *acceleration += change;
            }
        }
        state.motive_until = u64::from(now.wrapping_add(10));
        if valid {
            self.wake_family_update(id, "PhysicsBehavior", 0, 1);
        }
    }

    pub(super) fn apply_flight_force(&mut self, id: usize, force: [f32; 3]) {
        let unit = &self.catalog[self.entities[id].type_index];
        let Some(rules) = &unit.locomotor.flight else {
            return;
        };
        let mass = self.object_physics_mass(id).unwrap_or(rules.mass);
        let Some(state) = &mut self.entities[id].flight else {
            return;
        };
        if force.iter().any(|value| value.is_nan()) {
            return;
        }
        let inverse = 1.0 / mass;
        for (acceleration, force) in state.acceleration.iter_mut().zip(force) {
            let value = force * inverse;
            *acceleration += value;
        }
        self.wake_family_update(id, "PhysicsBehavior", 0, 1);
    }
    pub(super) fn set_flight_position(&mut self, id: usize, position: [f32; 3]) {
        self.set_object_position(id, position);
    }

    // Object::setPosition changes the transform; it does not wake PhysicsUpdate.
    pub(in crate::skirmish) fn set_object_position(&mut self, id: usize, position: [f32; 3]) {
        let position = if self.attack_kind(id, "STICK_TO_TERRAIN_SLOPE") {
            let (position, basis) =
                self.terrain_aligned_transform(position, self.entities[id].facing);
            self.entities[id].set_object_basis(basis);
            position
        } else {
            position
        };
        self.write_object_position(id, position);
        self.note_drawable_transform(id, true);
        self.update_position_claim(id);
        self.observe_ground_partition(id);
    }

    // Common transform write; caller performs its source-specific notifications.
    pub(in crate::skirmish) fn write_object_position(&mut self, id: usize, position: [f32; 3]) {
        let xy = [position[0], position[1]];
        let ground = self
            .movement_terrain
            .bridge_height(self.entities[id].layer, xy)
            .unwrap_or_else(|| self.ground_height(xy));
        self.entities[id].write_object_position(position, ground);
    }

    pub(super) fn set_flight_locomotor(&mut self, id: usize, panic: bool) {
        self.choose_locomotor_set(
            id,
            if panic {
                locomotor::set::PANIC
            } else {
                locomotor::set::NORMAL
            },
        );
    }

    pub(super) fn set_flight_precision(&mut self, id: usize, precise: bool, ultra: bool) {
        if let Some(state) = &mut self.entities[id].flight {
            state.set(PRECISE_Z, precise);
            state.set(ULTRA_ACCURATE, ultra);
        }
    }

    pub(super) fn aim_moving_flight(&mut self, id: usize, target: [f32; 3]) -> bool {
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let relative =
            super::combat::relative_angle(actor.position, actor.facing, [target[0], target[1]]);
        let tolerance = actor.active_weapon(unit).aim_delta.max(0.035);
        let speed = locomotor::speed(actor, unit) * super::STEP_SECONDS;
        let state = actor.flight.as_mut().expect("moving aircraft flight state");
        // AIAttackAimAtTargetState cannot turn a nonzero-minimum-speed aircraft
        // in place. POSITION_EXPLICIT continues its approach inside weapon range;
        // AIUpdate::doLocomotor supplies zero remaining path distance.
        state.goal = Some(Goal {
            position: target,
            distance: 0.0,
            speed: speed.min(state.speed_limit),
            no_slow_down: false,
        });
        relative.abs() < tolerance
    }

    /// `AIUpdate` `POSITION_EXPLICIT` bypasses path sampling and passes zero path distance.
    pub(in crate::skirmish) fn set_flight_explicit_goal(&mut self, id: usize, position: [f32; 3]) {
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let speed = (locomotor::speed(actor, unit) * super::STEP_SECONDS)
            .min(actor.ai.command.desired_speed);
        let state = actor.flight.as_mut().expect("flight explicit goal");
        state.goal = Some(Goal {
            position,
            distance: 0.0,
            speed,
            no_slow_down: false,
        });
    }

    pub(super) fn travel_flight(&mut self, id: usize, goal: [f32; 2], stop: f32) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = actor
            .active_locomotor(unit)
            .flight
            .as_ref()
            .expect("flight rules");
        let (close, close_3d) = actor
            .flight
            .as_ref()
            .expect("flight state")
            .close_override
            .unwrap_or((rules.close_enough, rules.close_3d));
        let current = self.object_position(id);
        let state = actor.flight.as_ref().expect("flight state");
        let retained_endpoint = actor
            .ai
            .ground_path
            .as_ref()
            .and_then(super::path_following::Path::last_position);
        let endpoint = retained_endpoint.unwrap_or_else(|| {
            [
                goal[0],
                goal[1],
                actor.ai.blocking.temporary_world_goal().map_or_else(
                    || {
                        state
                            .goal_z
                            .unwrap_or_else(|| self.movement_goal_position(id, goal)[2])
                    },
                    |point| point[2],
                ),
            ]
        });
        let dx = endpoint[0] - current[0];
        let dy = endpoint[1] - current[1];
        let dz = if close_3d {
            endpoint[2] - current[2]
        } else {
            0.0
        };
        let xx = dx * dx;
        let yy = dy * dy;
        let zz = dz * dz;
        let direct = ((xx + yy) + zz).sqrt();
        let no_slow_down = self.intermediate_waypoint(id) || actor.ai.motion.no_slow_down();
        let speed = locomotor::speed(actor, unit) * super::STEP_SECONDS;
        let path = (retained_endpoint.is_none() && state.endpoint != Some(endpoint))
            .then(|| self.native_aircraft_path(id, endpoint, None));
        let actor = &mut self.entities[id];
        let state = actor.flight.as_mut().expect("flight state");
        if let Some(path) = path {
            state.endpoint = Some(endpoint);
            state.path = Some(path);
        }
        let sample = if let Some(path) = &mut actor.ai.ground_path {
            path.flight_sample(current)
        } else {
            state
                .path
                .as_mut()
                .expect("flight path")
                .flight_sample(current)
        };
        let arrival = if close_3d {
            direct
        } else {
            sample.distance.min(direct)
        };
        if arrival < stop.max(close) {
            actor
                .drawable
                .model_conditions
                .set_flag(crate::skirmish::ModelConditions::MOVING, false);
            state.path = None;
            state.endpoint = None;
            return true;
        }
        actor
            .drawable
            .model_conditions
            .set_flag(crate::skirmish::ModelConditions::MOVING, true);
        state.goal = Some(Goal {
            position: sample.position,
            distance: sample.distance,
            speed: speed
                .min(state.speed_limit)
                .min(actor.ai.command.desired_speed),
            no_slow_down,
        });
        false
    }

    pub(super) fn update_flight(&mut self, id: usize) {
        let mass = self.object_physics_mass(id);
        let terrain = super::physical_ocl::transform::TerrainContext {
            terrain: self.projectile_terrain.as_ref(),
            layers: &self.movement_terrain,
            height: self.object_transform(id).0[2],
        };
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let loco = actor.active_locomotor(unit);
        if loco.flight.is_none() || actor.flight.is_none() {
            return;
        }
        let maximum = locomotor::speed(actor, unit) * super::STEP_SECONDS;
        let mut state = actor.flight.take().expect("flight state");
        let maximum = maximum.min(state.speed_limit);
        state.velocity[..2].copy_from_slice(&actor.ai.motion.velocity);
        // Locomotor selection changes while the object's PhysicsBehavior survives.
        state.motive_until = actor.ai.motion.motive_until;
        state.set(OBJECT_BRAKING, actor.ai.motion.object_braking());
        if !actor.unmanned() && !actor.hacked() && (!actor.is_ai_dead() || loco.works_when_dead) {
            let mass = mass.unwrap_or_else(|| loco.flight.as_ref().expect("flight rules").mass);
            motion::update(actor, unit, mass, &mut state, &terrain, self.tick, maximum);
        }
        actor.ai.motion.motive_until = state.motive_until;
        actor
            .ai
            .motion
            .set_object_braking(state.flag(OBJECT_BRAKING));
        actor.physics_control.force_wake |= state.force_applied;
        state.force_applied = false;
        actor.flight = Some(state);
        self.drain_ai_physics_force_wake(id);
    }

    pub(super) fn integrate_flight(&mut self, id: usize) {
        if self.entities[id].live_physics_active()
            || self.entities[id].fire_physics_asleep(self.tick)
        {
            return;
        }
        let ground = self.ground_height(self.entities[id].position);
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = actor.active_locomotor(unit).flight.as_ref();
        let Some(rules) = rules else { return };
        let Some(mut state) = actor.flight.take() else {
            return;
        };
        // Chinook's existing source lift/docking state owns its Z component.
        if let Some(chinook) = &actor.chinook {
            state.height = chinook.height;
        }
        physics::integrate(actor, &mut state, rules, ground, self.tick);
        actor
            .ai
            .motion
            .velocity
            .copy_from_slice(&state.velocity[..2]);
        // PhysicsBehavior samples the clipping surface at the newly integrated XY.
        let position = actor.position;
        let chinook = actor.chinook.is_some();
        let ground = self.ground_height(position);
        if !chinook {
            physics::ground_collision(&mut state, ground);
        }
        let crashed = self.entities[id].unmanned() && state.height <= ground;
        let actor = &mut self.entities[id];
        actor.flight = Some(state);
        if crashed {
            self.apply_hit(id, id, f32::MAX);
        }
    }
}

/// Validates values consumed by the translated hover and wing force paths.
/// # Errors
/// Rejects nonfinite values and unsupported force configurations.
pub fn validate(rules: &Rules) -> Result<(), &'static str> {
    // FreeFallLocomotor authors a passive HOVER force path with no motive,
    // braking or lift. Its PhysicsBehavior still applies native gravity.
    let freefall = matches!(rules.appearance, Appearance::Hover)
        && rules.acceleration == 0.0
        && rules.acceleration_damaged == 0.0
        && rules.braking == 0.0
        && rules.min_speed == 0.0
        && rules.lift == 0.0
        && rules.lift_damaged == 0.0;
    for (value, error, allow_zero) in [
        (
            rules.acceleration,
            "Aircraft acceleration must be positive and finite",
            freefall,
        ),
        (
            rules.acceleration_damaged,
            "Aircraft damaged acceleration must be positive and finite",
            freefall,
        ),
        (
            rules.braking,
            "Aircraft braking must be positive and finite",
            freefall,
        ),
        (
            rules.mass,
            "Aircraft mass must be positive and finite",
            false,
        ),
        (
            rules.close_enough,
            "Aircraft close-enough distance must be positive and finite",
            false,
        ),
    ] {
        if !value.is_finite() || value < 0.0 || (value == 0.0 && !allow_zero) {
            return Err(error);
        }
    }
    if [
        rules.min_speed,
        rules.preferred_height,
        rules.lift,
        rules.lift_damaged,
        rules.damping,
        rules.speed_limit_z,
        rules.gravity,
        rules.forward_friction,
        rules.lateral_friction,
        rules.aerodynamic_friction,
        rules.extra_friction,
        rules.slide_frames,
        rules.circling_radius,
    ]
    .into_iter()
    .any(|value| !value.is_finite())
    {
        return Err("Nonfinite native aircraft locomotor value");
    }
    Ok(())
}

impl Skirmish {
    pub(super) fn set_flight_ultra_accurate(&mut self, id: usize) {
        if let Some(state) = &mut self.entities[id].flight {
            state.set(ULTRA_ACCURATE, true);
        }
    }

    pub(super) fn set_delivery_parachute_precision(&mut self, id: usize) {
        self.entities[id]
            .flight
            .as_mut()
            .expect("parachute normal locomotor")
            .close_override = Some((10.0, false));
    }
}

impl Entity {
    pub(in crate::skirmish) fn write_object_position(&mut self, position: [f32; 3], ground: f32) {
        let xy = [position[0], position[1]];
        self.position = xy;
        if let Some(matrix) = &mut self.transform {
            matrix.translation = None;
        }
        self.placed_height_offset = Some(position[2] - ground);
        self.set_projectile_position(position);
        self.set_physical_position(position);
        if let Some(state) = &mut self.chinook {
            state.height = position[2];
        }
        self.set_corpse_height(position[2]);
        self.set_fire_height(position[2]);
        if let Some(state) = &mut self.flight {
            state.height = position[2];
        }
        if self.contained_by().is_some() {
            self.containment.height = position[2];
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn flight_preferred_height(&self, id: usize) -> f32 {
        let actor = &self.entities[id];
        actor
            .flight
            .as_ref()
            .and_then(|state| state.preferred_height)
            .unwrap_or_else(|| {
                actor
                    .active_locomotor(&self.catalog[actor.type_index])
                    .flight
                    .as_ref()
                    .map_or(self.catalog[actor.type_index].preferred_height, |rules| {
                        rules.preferred_height
                    })
            })
    }

    pub(in crate::skirmish) fn set_flight_preferred_height(&mut self, id: usize, height: f32) {
        self.entities[id]
            .flight
            .as_mut()
            .expect("airborne locomotor instance")
            .preferred_height = Some(height);
    }
}
