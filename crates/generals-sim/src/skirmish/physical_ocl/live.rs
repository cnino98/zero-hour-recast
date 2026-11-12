//! Live `PhysicsBehavior` shock, spin and landing, SH 2bfaea60.
use super::{CollisionRules, Motion, MotionRules, Skirmish, physics, transform};
use crate::skirmish::{DamageType, DeathType, Entity, UnitType, startup};

pub struct LiveRules {
    pub motion: MotionRules,
    pub resistance: f32,
    pub kill_when_resting: bool,
    pub collision: CollisionRules,
    /// Source yaw, pitch and roll maximum rates.
    pub shock_rotation: [f32; 3],
    pub min_fall_speed: f32,
    pub fall_damage_factor: f32,
}

const STUNNED: u8 = 1;
const FLAILING: u8 = 2;
const AIRBORNE: u8 = 4;
const ALLOW_FALL: u8 = 8;
const FREEFALL: u8 = 16;
const UPDATE_EVER_RUN: u8 = 32;

pub(in crate::skirmish) struct LiveState {
    pub(in crate::skirmish) motion: Motion,
    pub(in crate::skirmish) last_collidee: Option<usize>,
    ignore_collisions_with: Option<usize>,
    flags: u8,
    frame: Option<u64>,
    freefall_disabled: bool,
    pub(super) current_overlap: Option<usize>,
    pub(super) previous_overlap: Option<usize>,
}

impl LiveState {
    pub(super) const fn allows_fall(&self) -> bool {
        self.has(ALLOW_FALL)
    }

    const fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    fn land(
        &mut self,
        ground: f32,
        stick: bool,
        bounce: f32,
        active_z: f32,
        rules: &LiveRules,
        mass: f32,
    ) -> f32 {
        if self.motion.position[2] <= ground {
            self.motion.velocity[2] += ground - self.motion.position[2];
            self.motion.velocity[2] = self.motion.velocity[2].min(0.0);
            self.motion.position[2] = ground;
            self.flags &= !ALLOW_FALL;
            if self.has(STUNNED) {
                self.flags &= !FLAILING;
            }
        } else if self.has(FREEFALL) {
            self.freefall_disabled = true;
        } else if stick && !self.has(ALLOW_FALL) {
            self.motion.position[2] = ground;
        }
        self.motion.acceleration = [0.0; 3];
        if bounce > 0.0 && self.motion.bouncing {
            self.motion.force([0.0, 0.0, bounce], mass);
        }
        let airborne = self.motion.position[2] > ground;
        if !airborne {
            self.flags &= !FREEFALL;
            self.freefall_disabled = false;
        }
        let net_speed = -active_z - rules.min_fall_speed;
        let falling = if self.has(AIRBORNE)
            && !airborne
            && net_speed > 0.0
            && self.motion.velocity[..2]
                .iter()
                .all(|v| v.abs() <= 0.01 || (active_z / *v).abs() >= 3.0)
        {
            net_speed * mass * rules.fall_damage_factor
        } else {
            0.0
        };
        self.flags = (self.flags & !AIRBORNE) | (u8::from(airborne) * AIRBORNE);
        falling
    }

    pub(super) fn synchronize_death(
        &mut self,
        actor: &mut Entity,
        tick: u64,
    ) -> Option<(bool, bool)> {
        let (braking, motive, falling) = actor.synchronize_death_physics(&mut self.motion, tick)?;
        self.flags = (self.flags & !ALLOW_FALL) | (u8::from(falling) * ALLOW_FALL);
        Some((braking, motive))
    }

    fn synchronize(&mut self, actor: &mut Entity, unit: &UnitType, tick: u64) -> (bool, bool) {
        let locomotor = actor.active_locomotor(unit);
        self.motion.extra_friction = locomotor
            .ground
            .as_ref()
            .map_or(0.0, |rules| rules.extra_friction)
            + if actor.ai.motion.ultra_accurate() {
                0.5
            } else {
                0.0
            };
        self.motion.position[0] = actor.position[0];
        self.motion.position[1] = actor.position[1];
        self.motion.basis = actor.object_basis();
        if let Some(input) = actor.aircraft_physics_input(unit) {
            self.motion.position[2] = input.height;
            self.motion.velocity = input.velocity;
            for axis in 0..3 {
                self.motion.acceleration[axis] += input.acceleration[axis];
            }
            self.motion.extra_friction = input.extra_friction;
            actor.consume_aircraft_acceleration(self.motion.acceleration);
            (input.object_braking, input.motive_until > tick)
        } else {
            for axis in 0..2 {
                self.motion.velocity[axis] = actor.ai.motion.velocity[axis];
                self.motion.acceleration[axis] += actor.ai.motion.acceleration[axis];
            }
            (
                actor.ai.motion.object_braking(),
                actor.ai.motion.motive_until > tick,
            )
        }
    }
}

impl Entity {
    pub(in crate::skirmish) const fn set_physics_airborne_friction(&mut self, allow: bool) {
        if let Some(flight) = &mut self.projectile_flight {
            flight.motion.airborne_friction = allow;
        } else if self.live_physics.is_some() || self.physical.is_some() {
            self.physics_control.airborne_friction = allow;
        }
    }

    pub(in crate::skirmish) fn set_physics_roll_rate(&mut self, rate: f32) {
        if let Some(state) = &mut self.projectile_flight {
            state.motion.rates[0] = rate;
        } else if let Some(state) = &mut self.physical {
            state.motion.rates[0] = rate;
        } else if let Some(state) = &mut self.live_physics {
            state.motion.rates[0] = rate;
        }
    }

    pub(in crate::skirmish) fn clear_physics_extra_friction(&mut self) {
        if let Some(state) = &mut self.physical {
            state.motion.extra_friction = 0.0;
        } else if let Some(state) = &mut self.live_physics {
            state.motion.extra_friction = 0.0;
        }
        // Admitted projectile Physics has constant zero extra friction.
    }

    pub(in crate::skirmish) fn prepare_ocl_physics(&mut self) {
        if let Some(state) = &mut self.projectile_flight {
            state.motion.allow_ocl_bouncing();
        } else if let Some(state) = &mut self.physical {
            state.motion.bouncing = true;
            state.motion.extra_friction = 0.0;
            state.bounce_sound.clear();
        } else if let Some(state) = &mut self.live_physics {
            state.motion.bouncing = true;
            state.motion.extra_friction = 0.0;
        }
    }

    /// Native rate setters receive yaw, roll, pitch in this order.
    pub(in crate::skirmish) fn set_physics_angular_rates(&mut self, rates: [f32; 3]) {
        let rates = [rates[1], rates[2], rates[0]];
        if let Some(state) = &mut self.projectile_flight {
            state.motion.rates = rates;
        } else if let Some(state) = &mut self.physical {
            state.motion.rates = rates;
        } else if let Some(state) = &mut self.live_physics {
            state.motion.rates = rates;
        }
    }

    pub(in crate::skirmish) fn set_physics_pitch_rate(&mut self, rate: f32) {
        if let Some(state) = &mut self.projectile_flight {
            state.motion.rates[1] = rate;
        } else if let Some(state) = &mut self.physical {
            state.motion.rates[1] = rate;
        } else if let Some(state) = &mut self.live_physics {
            state.motion.rates[1] = rate;
        }
    }

    pub(in crate::skirmish) fn physics_last_collidee(&self) -> Option<usize> {
        self.physical.as_ref().map_or_else(
            || {
                self.live_physics
                    .as_ref()
                    .and_then(|state| state.last_collidee)
            },
            |state| state.last_collidee,
        )
    }

    pub(in crate::skirmish) fn physics_ignores(&self, other: usize) -> bool {
        self.physical
            .as_ref()
            .is_some_and(|state| state.ignore_collisions_with == Some(other))
            || self
                .live_physics
                .as_ref()
                .is_some_and(|state| state.ignore_collisions_with == Some(other))
            || self
                .projectile_flight
                .as_ref()
                .is_some_and(|state| state.ignore_collisions_with == Some(other))
    }

    pub(in crate::skirmish) fn set_physics_ignored_object(&mut self, other: Option<usize>) {
        if let Some(state) = &mut self.physical {
            state.ignore_collisions_with = other;
        } else if let Some(state) = &mut self.live_physics {
            state.ignore_collisions_with = other;
        } else if let Some(state) = &mut self.projectile_flight {
            state.ignore_collisions_with = other;
        }
    }

    pub(in crate::skirmish) fn scrub_physics_velocity_2d(&mut self, desired: f32) {
        let mut velocity = self.physics_velocity();
        if desired < 0.001 {
            velocity[0] = 0.0;
            velocity[1] = 0.0;
        } else {
            let squares = velocity.map(|component| component * component);
            let current = (squares[0] + squares[1]).sqrt();
            if desired > current {
                return;
            }
            let scale = desired / current;
            velocity[0] *= scale;
            velocity[1] *= scale;
        }
        self.ai.motion.velocity = [velocity[0], velocity[1]];
        self.set_flight_velocity(velocity);
        if let Some((height, _, _)) = self.chinook_physics() {
            self.set_chinook_physics(height, velocity[2]);
        }
        if let Some(state) = &mut self.projectile_flight {
            state.motion.velocity = velocity;
        }
        if let Some(state) = &mut self.physical {
            state.motion.velocity = velocity;
        }
        if let Some(state) = &mut self.live_physics {
            state.motion.velocity = velocity;
        }
        self.set_death_physics_velocity(velocity);
    }

    pub(in crate::skirmish) fn scrub_physics_velocity_z(&mut self, desired: f32) {
        let mut velocity = self.physics_velocity();
        if desired.abs() < 0.001 {
            velocity[2] = 0.0;
        } else if desired < 0.0 && velocity[2] < desired || desired > 0.0 && velocity[2] > desired {
            velocity[2] = desired;
        }
        self.set_flight_velocity(velocity);
        if let Some((height, _, _)) = self.chinook_physics() {
            self.set_chinook_physics(height, velocity[2]);
        }
        if let Some(state) = &mut self.projectile_flight {
            state.motion.velocity = velocity;
        }
        if let Some(state) = &mut self.physical {
            state.motion.velocity = velocity;
        }
        if let Some(state) = &mut self.live_physics {
            state.motion.velocity = velocity;
        }
        self.set_death_physics_velocity(velocity);
    }

    pub(in crate::skirmish) fn set_physics_allow_fall(&mut self, allow: bool) {
        if let Some(state) = &mut self.live_physics {
            state.flags = (state.flags & !ALLOW_FALL) | (u8::from(allow) * ALLOW_FALL);
        }
    }

    pub(in crate::skirmish) fn physics_allows_fall(&self) -> bool {
        self.live_physics
            .as_ref()
            .is_some_and(LiveState::allows_fall)
    }

    pub(in crate::skirmish) fn physics_forward_speed(&self) -> f32 {
        let velocity = self.physics_velocity();
        let vx = velocity[0] * self.facing.cos();
        let vy = velocity[1] * self.facing.sin();
        let squares = [vx, vy].map(|component| component * component);
        let speed = (squares[0] + squares[1]).sqrt();
        if vx + vy >= 0.0 { speed } else { -speed }
    }

    pub(in crate::skirmish) fn physical_object_velocity(&self) -> Option<[f32; 3]> {
        self.physical.as_ref().map(|state| state.motion.velocity)
    }

    pub(in crate::skirmish) const fn set_live_physics_position(&mut self, position: [f32; 3]) {
        if let Some(state) = &mut self.live_physics {
            state.motion.position = position;
        }
    }
    pub(in crate::skirmish) const fn live_physics_active(&self) -> bool {
        self.live_physics.is_some()
    }
    pub(in crate::skirmish) fn physics_stunned(&self) -> bool {
        self.live_physics
            .as_ref()
            .is_some_and(|state| state.has(STUNNED))
    }
    pub(in crate::skirmish) fn physics_flailing(&self) -> bool {
        self.live_physics
            .as_ref()
            .is_some_and(|state| state.has(FLAILING))
    }
    pub(super) fn live_height(&self) -> Option<f32> {
        self.live_physics
            .as_ref()
            .map(|state| state.motion.position[2])
    }
    pub(in crate::skirmish) fn live_basis(&self) -> Option<[[f32; 3]; 3]> {
        self.live_physics.as_ref().map(|state| state.motion.basis)
    }
    pub(in crate::skirmish) fn live_velocity(&self) -> Option<[f32; 3]> {
        self.live_physics
            .as_ref()
            .map(|state| state.motion.velocity)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_object_physics(&mut self, id: usize) {
        let unit = &self.catalog[self.entities[id].type_index];
        if !unit.projectile_rules.is_empty() {
            self.construct_projectile_physics(id);
            return;
        }
        if unit.physical.is_some() {
            self.initialize_physical_object(id);
            return;
        }
        let rules = unit
            .live_physics
            .as_ref()
            .expect("PhysicsBehavior instance data");
        let position = self.object_position(id);
        let actor = &mut self.entities[id];
        actor.live_physics = Some(LiveState {
            motion: Motion {
                position,
                basis: actor.object_basis(),
                velocity: [0.0; 3],
                acceleration: [0.0; 3],
                rates: [0.0; 3],
                mass: rules.motion.mass,
                extra_friction: 0.0,
                bouncing: rules.motion.allow_bouncing,
            },
            flags: 0,
            frame: None,
            freefall_disabled: false,
            last_collidee: None,
            ignore_collisions_with: None,
            current_overlap: None,
            previous_overlap: None,
        });
    }

    pub(in crate::skirmish) fn update_object_physics(&mut self, id: usize) -> i32 {
        if self.entities[id].projectile_flight.is_some() {
            return self.update_projectile_physics(id);
        }
        if self.entities[id].physical.is_some() {
            self.integrate_physical_object(id);
        } else {
            self.integrate_live_physics(id);
        }
        self.object_physics_sleep(id)
    }

    fn object_physics_sleep(&self, id: usize) -> i32 {
        if self.entities[id].projectile_flight.is_some() {
            return self.projectile_physics_sleep(id);
        }
        let actor = &self.entities[id];
        let motion = actor.physical.as_ref().map_or_else(
            || &actor.live_physics.as_ref().expect("Physics state").motion,
            |state| &state.motion,
        );
        let motive_until = actor
            .aircraft_physics_input(&self.catalog[actor.type_index])
            .map_or(actor.ai.motion.motive_until, |input| input.motive_until);
        let updated = actor.physical.as_ref().map_or_else(
            || {
                actor
                    .live_physics
                    .as_ref()
                    .is_some_and(|state| state.has(UPDATE_EVER_RUN))
            },
            |state| state.was_airborne.is_some(),
        );
        if updated
            && motion.velocity.iter().all(|value| *value == 0.0)
            && motion.acceleration.iter().all(|value| *value == 0.0)
            && motion.rates.iter().all(|value| *value == 0.0)
            && motive_until <= self.tick
            && !actor.physics_has_overlap()
            && actor.layer == 1
            && motion.position[2] <= self.ground_height(actor.position)
        {
            i32::from_ne_bytes(crate::skirmish::module_updates::FOREVER.to_ne_bytes())
        } else {
            1
        }
    }

    /// PhysicsUpdate.cpp422: preserve motive expiry/flags, clear dynamic motion,
    /// then set the real Physics callback's next wake using calcSleepTime.
    pub(in crate::skirmish) fn reset_dynamic_physics(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        actor.ai.motion.reset_dynamic_physics();
        actor.physics_control.turning = false;
        actor.physics_control.force_wake = false;
        actor.reset_flight_dynamic_physics();
        actor.reset_death_dynamic_physics();
        actor.reset_chinook_dynamic_physics();
        if let Some(state) = &mut actor.projectile_flight {
            state.motion.reset_dynamic_physics();
        }
        if let Some(state) = &mut actor.physical {
            state.motion.velocity = [0.0; 3];
            state.motion.acceleration = [0.0; 3];
            state.motion.rates = [0.0; 3];
        }
        if let Some(state) = &mut actor.live_physics {
            state.motion.velocity = [0.0; 3];
            state.motion.acceleration = [0.0; 3];
            state.motion.rates = [0.0; 3];
        }
        let sleep = self.object_physics_sleep(id);
        self.wake_family_update(
            id,
            "PhysicsBehavior",
            0,
            u32::try_from(sleep).expect("Physics sleep"),
        );
    }

    /// `BridgeBehavior::handleObjectsOnBridgeOnDie` sets `ALLOW_TO_FALL` on
    /// the existing `PhysicsBehavior`, preserving velocity and consuming no RNG.
    pub(in crate::skirmish) fn start_bridge_fall(&mut self, id: usize, height: f32) {
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = unit
            .live_physics
            .as_ref()
            .expect("bridge passenger PhysicsBehavior");
        let basis = actor.object_basis();
        let state = actor.live_physics.get_or_insert_with(|| LiveState {
            motion: Motion {
                position: [actor.position[0], actor.position[1], height],
                basis,
                velocity: [
                    actor.ai.motion.velocity[0],
                    actor.ai.motion.velocity[1],
                    0.0,
                ],
                acceleration: [
                    actor.ai.motion.acceleration[0],
                    actor.ai.motion.acceleration[1],
                    0.0,
                ],
                rates: [0.0; 3],
                mass: rules.motion.mass,
                extra_friction: 0.0,
                bouncing: rules.motion.allow_bouncing,
            },
            flags: AIRBORNE | ALLOW_FALL,
            frame: None,
            freefall_disabled: false,
            last_collidee: None,
            ignore_collisions_with: None,
            current_overlap: None,
            previous_overlap: None,
        });
        state.flags |= ALLOW_FALL;
    }

    pub(in crate::skirmish) fn clear_live_physics(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        actor.ai.motion.acceleration = [0.0; 2];
        actor.ai.motion.velocity = [0.0; 2];
        if let Some(state) = &mut actor.live_physics {
            state.motion.acceleration = [0.0; 3];
            state.motion.velocity[0] = 0.0;
            state.motion.velocity[1] = 0.0;
        }
    }

    pub(in crate::skirmish) fn apply_live_shock(&mut self, id: usize, force: [f32; 3]) {
        let contained_mass = self.contained_physics_mass(id);
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let Some(rules) = &unit.live_physics else {
            return;
        };
        let locomotor = actor.active_locomotor(unit);
        let stick = locomotor.stick_to_ground;
        let position = self.object_position(id);
        let airborne = self.height_above_ground(id) > 0.0;
        let mut rotation = [0.0; 3];
        if !stick {
            for (axis, maximum) in [2, 1, 0].into_iter().zip(rules.shock_rotation) {
                // applyRandomRotation uses the integer RNG, unlike shell tumble.
                let modifier = match startup::random_between(&mut self.random, 2) {
                    0 => -1.0,
                    1 => 0.0,
                    _ => 1.0,
                };
                rotation[axis] = maximum * modifier;
            }
        }
        let resistance = 1.0 - rules.resistance.clamp(0.0, 1.0);
        let aircraft = actor.aircraft_physics_input(unit);
        let velocity = aircraft
            .as_ref()
            .map_or_else(|| actor.physics_velocity(), |input| input.velocity);
        let motive_until = aircraft
            .as_ref()
            .map_or(actor.ai.motion.motive_until, |input| input.motive_until);
        let actor = &mut self.entities[id];
        let mut state = actor.live_physics.take().unwrap_or_else(|| LiveState {
            motion: Motion {
                position,
                basis: actor.object_basis(),
                velocity,
                acceleration: [0.0; 3],
                rates: [0.0; 3],
                mass: rules.motion.mass,
                extra_friction: 0.0,
                bouncing: rules.motion.allow_bouncing,
            },
            flags: u8::from(airborne) * AIRBORNE,
            frame: None,
            freefall_disabled: false,
            last_collidee: None,
            ignore_collisions_with: None,
            current_overlap: None,
            previous_overlap: None,
        });
        state.synchronize_death(actor, self.tick);
        let mut force = force.map(|value| value * resistance);
        if motive_until > self.tick {
            let direction = [actor.facing.cos(), actor.facing.sin()];
            let x = force[0] * -direction[1];
            let y = force[1] * direction[0];
            let lateral = x + y;
            force[0] = lateral * -direction[1];
            force[1] = lateral * direction[0];
        }
        state
            .motion
            .force(force, state.motion.mass + contained_mass);
        if !stick {
            state.motion.bouncing = true;
            for (rate, addition) in state.motion.rates.iter_mut().zip(rotation) {
                *rate += addition;
            }
        }
        actor.set_death_physics(&state.motion, state.allows_fall());
        actor.live_physics = Some(state);
        if !force.iter().any(|value| value.is_nan()) {
            self.wake_family_update(id, "PhysicsBehavior", 0, 1);
        }
    }

    pub(in crate::skirmish) fn apply_damage_shock(
        &mut self,
        id: usize,
        vector: [f32; 3],
        amount: f32,
        radius: f32,
        taper: f32,
    ) {
        if amount <= 0.0 || radius <= 0.0 {
            return;
        }
        let unit = &self.catalog[self.entities[id].type_index];
        if unit.live_physics.is_none()
            || unit.kind_of.iter().any(|kind| kind == "PROJECTILE")
            || self.entities[id].is_airborne_target()
        {
            return;
        }
        let length = length3(vector);
        let distance = (length / radius).min(1.0);
        let scale = amount * (1.0 - distance * (1.0 - taper));
        let mut force = vector;
        if length != 0.0 {
            for value in &mut force {
                *value /= length;
            }
        }
        for value in &mut force {
            *value *= scale;
        }
        force[2] = length3(force);
        self.apply_live_shock(id, force);
        if let Some(state) = &mut self.entities[id].live_physics {
            state.flags |= STUNNED;
            state.flags |= FLAILING;
        }
    }

    pub(in crate::skirmish) fn integrate_live_physics(&mut self, id: usize) {
        let Some(mass) = self.object_physics_mass(id) else {
            return;
        };
        let held = self.entities[id].disabled_held();
        let actor = &mut self.entities[id];
        let Some(mut state) = actor.live_physics.take() else {
            return;
        };
        if state.frame == Some(self.tick) {
            actor.live_physics = Some(state);
            return;
        }
        let first = !state.has(UPDATE_EVER_RUN);
        state.frame = Some(self.tick);
        let unit = &self.catalog[actor.type_index];
        let rules = unit.live_physics.as_ref().expect("live PhysicsBehavior");
        let locomotor = actor.active_locomotor(unit);
        let stick = locomotor.stick_to_ground;
        let friction_airborne = actor.physics_control.airborne_friction;
        let (braking, motive) = if let Some(physics) = state.synchronize_death(actor, self.tick) {
            physics
        } else {
            state.synchronize(actor, unit, self.tick)
        };
        let matrix_position = actor
            .transform
            .as_ref()
            .and_then(|matrix| matrix.translation);
        let position = actor.position;
        let layer = actor.layer;
        let ground = self.movement_layer_height(layer, position);
        if first {
            state.flags = (state.flags & !AIRBORNE)
                | (u8::from(state.motion.position[2] > ground) * AIRBORNE);
        }
        if held {
            self.hold_object_physics(id, state, ground);
            return;
        }
        let old_position = state.motion.position;
        let height_above_ground = old_position[2] - ground;
        if let Some(position) = matrix_position {
            state.motion.position = position;
        }
        let old_height = state.motion.position[2];
        let old_basis = state.motion.basis;
        physics::integrate(
            &mut state.motion,
            &rules.motion,
            height_above_ground,
            friction_airborne,
            braking,
            motive,
            mass,
        );
        if state.motion.position.iter().any(|value| value.is_nan()) {
            self.entities[id].live_physics = Some(state);
            self.destroy_maintenance_object(id);
            return;
        }
        // The source checks the old object's altitude before committing its new matrix.
        if state.has(STUNNED)
            && (state.motion.velocity.iter().all(|v| v.abs() < 0.5)
                || height_above_ground <= -9.0 * rules.motion.gravity)
        {
            state.flags &= !STUNNED;
        }
        let ground =
            self.movement_layer_height(layer, [state.motion.position[0], state.motion.position[1]]);
        let active_z = state.motion.velocity[2];
        let bounce = state
            .motion
            .bounce(&rules.motion, old_height, ground, old_basis, mass);
        if bounce > 0.0 && state.has(STUNNED) && self.stunned_landing_invalid(id, old_basis) {
            state = self.kill_stunned_landing(id, state, old_position, old_basis);
        }
        state.previous_overlap = state.current_overlap.take();
        let was_airborne = state.has(AIRBORNE);
        let mass = state.motion.mass + self.contained_physics_mass(id);
        let falling = state.land(
            ground,
            stick,
            bounce,
            active_z,
            self.catalog[self.entities[id].type_index]
                .live_physics
                .as_ref()
                .expect("Physics data"),
            mass,
        );
        self.commit_object_physics(id, state, was_airborne, falling);
        self.entities[id]
            .live_physics
            .as_mut()
            .expect("Physics state")
            .flags |= UPDATE_EVER_RUN;
    }

    fn commit_object_physics(
        &mut self,
        id: usize,
        state: LiveState,
        was_airborne: bool,
        falling: f32,
    ) {
        let actor = &mut self.entities[id];
        actor.position = [state.motion.position[0], state.motion.position[1]];
        actor.facing = state.motion.basis[1][0].atan2(state.motion.basis[0][0]);
        actor.transform =
            Some(crate::skirmish::physical_ocl::transform::Matrix::from_basis(state.motion.basis));
        actor.ai.motion.velocity = [state.motion.velocity[0], state.motion.velocity[1]];
        actor.ai.motion.acceleration = [0.0; 2];
        actor.set_aircraft_physics(state.motion.position, state.motion.velocity);
        actor.set_death_physics(&state.motion, state.allows_fall());
        let landed = was_airborne && !state.has(AIRBORNE);
        actor.live_physics = Some(state);
        if landed {
            self.object_collide(id, None);
        }
        self.observe_spy_vision_disabled(id);
        if falling > 0.0 && self.entities[id].alive() {
            self.apply_weapon_hit(id, id, falling, DamageType::Falling, DeathType::Splatted);
        }
        self.kill_resting_physics(id);
        self.update_position_claim(id);
        self.observe_ground_partition(id);
    }

    fn hold_object_physics(&mut self, id: usize, mut state: LiveState, ground: f32) {
        state.previous_overlap = state.current_overlap.take();
        state.motion.acceleration = [0.0; 3];
        let airborne = state.motion.position[2] > ground;
        let landed = state.has(AIRBORNE) && !airborne;
        if !airborne {
            state.flags &= !FREEFALL;
            state.freefall_disabled = false;
        }
        state.flags = (state.flags & !AIRBORNE) | (u8::from(airborne) * AIRBORNE);
        let actor = &mut self.entities[id];
        actor.ai.motion.acceleration = [0.0; 2];
        actor.set_death_physics(&state.motion, state.allows_fall());
        actor.live_physics = Some(state);
        if landed {
            self.object_collide(id, None);
        }
        self.observe_spy_vision_disabled(id);
        self.kill_resting_physics(id);
        self.entities[id]
            .live_physics
            .as_mut()
            .expect("Physics state")
            .flags |= UPDATE_EVER_RUN;
    }

    fn kill_resting_physics(&mut self, id: usize) {
        let unit = &self.catalog[self.entities[id].type_index];
        let state = self.entities[id]
            .live_physics
            .as_ref()
            .expect("Physics state");
        if unit
            .live_physics
            .as_ref()
            .expect("Physics data")
            .kill_when_resting
            && !state.has(AIRBORNE)
            && state.motion.velocity.iter().all(|value| value.abs() < 0.01)
            && (!unit.kind_of.iter().any(|kind| kind == "DRONE")
                || !self.entities[id].alive()
                || self.entities[id].unmanned())
        {
            self.kill_object(id, DeathType::Normal);
        }
    }

    fn kill_stunned_landing(
        &mut self,
        id: usize,
        mut state: LiveState,
        old_position: [f32; 3],
        old_basis: [[f32; 3]; 3],
    ) -> LiveState {
        // handleBounce kills at the old object transform before update
        // commits its separately integrated matrix. Death FX see this pose.
        let position = state.motion.position;
        let basis = state.motion.basis;
        state.motion.position = old_position;
        state.motion.basis = transform::yaw(old_basis[1][0].atan2(old_basis[0][0]));
        if old_basis[2][2] <= 0.0 {
            transform::rotate(&mut state.motion.basis, [std::f32::consts::PI, 0.0, 0.0]);
        }
        self.entities[id].live_physics = Some(state);
        self.apply_native_damage(id, None, self.entities[id].health, DeathType::Normal);
        state = self.entities[id]
            .live_physics
            .take()
            .expect("live physics during death");
        state.motion.position = position;
        state.motion.basis = basis;
        state
    }

    fn stunned_landing_invalid(&self, id: usize, basis: [[f32; 3]; 3]) -> bool {
        let actor = &self.entities[id];
        let Some(surface) = self
            .movement_terrain
            .physics_surfaces(actor.position, self.ground_height(actor.position))
        else {
            return true;
        };
        let unit = &self.catalog[actor.type_index];
        basis[2][2] < 0.0
            || (!matches!(unit.ai.update, crate::skirmish::UnitAiUpdate::Absent)
                && surface & !unit.locomotor_surfaces() != 0)
    }
}

fn length3(vector: [f32; 3]) -> f32 {
    let square = vector.map(|value| value * value);
    ((square[0] + square[1]) + square[2]).sqrt()
}

impl Entity {
    pub(in crate::skirmish) fn freefall_disabled(&self) -> bool {
        self.live_physics
            .as_ref()
            .is_some_and(|state| state.freefall_disabled)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn start_parachute_freefall(&mut self, id: usize) {
        let height = self.object_position(id)[2];
        self.start_bridge_fall(id, height);
        // ParachuteContain::onDie explicitly sets this flag. An ordinary bridge
        // ALLOW_TO_FALL does not imply IS_IN_FREEFALL or DISABLED_FREEFALL.
        let state = self.entities[id]
            .live_physics
            .as_mut()
            .expect("parachute rider PhysicsBehavior");
        state.flags |= FREEFALL;
        state.frame = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Kind, tests::game};

    fn fixture() -> (Skirmish, usize) {
        let (mut game, kind) = fixture_catalog();
        let id = game.spawn(kind, 0, [200.0, 200.0], true);
        (game, id)
    }

    fn fixture_catalog() -> (Skirmish, usize) {
        let mut game = game();
        let kind = game.type_for(0, Kind::Builder).unwrap();
        game.catalog[kind].live_physics = Some(LiveRules {
            motion: MotionRules {
                center_of_mass_offset: 0.0,
                mass: 2.0,
                gravity: -0.1,
                stiffness: 0.5,
                forward_friction: 0.15,
                lateral_friction: 0.15,
                aerodynamic_friction: 0.0,
                angular_factor: 2.0,
                allow_bouncing: false,
            },
            resistance: 0.25,
            kill_when_resting: false,
            collision: CollisionRules {
                stiffness: 0.5,
                rubble_height: 0.0,
                allow_force: true,
                weapons: [None, None],
            },
            shock_rotation: [0.05, 0.025, 0.025],
            min_fall_speed: 99.0,
            fall_damage_factor: 1.0,
        });
        (game, kind)
    }

    #[test]
    fn live_shock_preserves_rotation_rng_and_integrates_in_same_physics_phase_once() {
        let (mut game, id) = fixture();
        // Pinned C++ integer-RNG fixture: yaw +1, pitch 0, roll -1.
        game.random = startup::seed_random(37);
        game.apply_live_shock(id, [0.0, 0.0, 10.0]);
        assert_eq!(
            game.random,
            [
                0xe827_fff8,
                0xc0d9_54f9,
                0xef24_fcc6,
                0x80dd_e8e2,
                0xedd2_f240,
                0x6fdf_3b8c,
            ]
        );
        let state = game.entities[id].live_physics.as_ref().unwrap();
        assert_eq!(
            state.motion.rates.map(f32::to_bits),
            [-0.025_f32, 0.0, 0.05].map(f32::to_bits)
        );
        assert_eq!(state.motion.acceleration[2].to_bits(), 3.75_f32.to_bits());
        game.integrate_live_physics(id);
        assert_eq!(game.height_above_ground(id).to_bits(), 3.65_f32.to_bits());
        let position = game.object_position(id);
        game.integrate_live_physics(id);
        assert_eq!(
            game.object_position(id).map(f32::to_bits),
            position.map(f32::to_bits)
        );
    }

    #[test]
    fn stick_to_ground_consumes_no_spin_rng_and_motive_force_keeps_only_lateral_shock() {
        let (mut game, id) = fixture();
        let kind = game.entities[id].type_index;
        game.catalog[kind].locomotor.stick_to_ground = true;
        game.entities[id].facing = 0.0;
        game.entities[id].ai.motion.motive_until = game.tick + 10;
        let random = game.random;
        game.apply_live_shock(id, [10.0, 4.0, 8.0]);
        assert_eq!(game.random, random);
        let state = game.entities[id].live_physics.as_ref().unwrap();
        assert_eq!(
            state.motion.acceleration.map(f32::to_bits),
            [0.0, 1.5, 3.0].map(f32::to_bits)
        );
        assert_eq!(
            state.motion.rates.map(f32::to_bits),
            [0.0; 3].map(f32::to_bits)
        );
        game.integrate_live_physics(id);
        assert_eq!(game.height_above_ground(id).to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn low_ground_vehicle_accepts_repeat_shock_until_airborne_target_threshold() {
        let (mut game, id) = fixture();
        game.apply_damage_shock(id, [5.0, 0.0, 0.0], 10.0, 10.0, 0.5);
        game.integrate_live_physics(id);
        assert!(game.height_above_ground(id) > 0.0);
        let random = game.random;
        game.apply_damage_shock(id, [5.0, 0.0, 0.0], 10.0, 10.0, 0.5);
        assert_ne!(game.random, random);
        game.catalog[game.entities[id].type_index]
            .locomotor
            .airborne_targeting_height = 0.0;
        // AIUpdate::doLocomotor publishes the status after locomotor work;
        // damage consumes the cached status from that callback.
        assert!(!game.entities[id].is_airborne_target());
        game.update_ground_locomotor(id);
        assert!(game.entities[id].is_airborne_target());
        let random = game.random;
        game.apply_damage_shock(id, [5.0, 0.0, 0.0], 10.0, 10.0, 0.5);
        assert_eq!(game.random, random);
        game.catalog[game.entities[id].type_index]
            .locomotor
            .airborne_targeting_height = game.height_above_ground(id);
        assert!(game.entities[id].is_airborne_target());
        game.update_ground_locomotor(id);
        assert!(!game.entities[id].is_airborne_target());
    }

    #[test]
    fn immobile_and_missing_locomotor_preserve_cached_airborne_target() {
        for immobile in [true, false] {
            let (mut game, id) = fixture();
            game.apply_damage_shock(id, [5.0, 0.0, 0.0], 10.0, 10.0, 0.5);
            game.integrate_live_physics(id);
            let kind = game.entities[id].type_index;
            game.catalog[kind].locomotor.airborne_targeting_height = 0.0;
            game.update_ground_locomotor(id);
            assert!(game.entities[id].is_airborne_target());
            game.catalog[kind].locomotor.airborne_targeting_height = 1000.0;
            if immobile {
                game.catalog[kind].kind_of.push("IMMOBILE".into());
            } else {
                game.catalog[kind].locomotor.wander_length = None;
            }
            game.update_ground_locomotor(id);
            assert!(game.entities[id].is_airborne_target());
        }
    }

    #[test]
    fn stunned_landing_uses_old_terrain_and_null_attacker_without_armor() {
        for (water_x, ai, killed) in [(20, true, true), (22, true, false), (20, false, false)] {
            let (mut game, kind) = fixture_catalog();
            game.catalog[kind].armor = [0.0; crate::skirmish::DAMAGE_TYPE_COUNT];
            game.catalog[kind]
                .behavior
                .death_fx
                .push(crate::skirmish::death_callbacks::FxRules {
                    conditions: crate::skirmish::death_callbacks::Conditions {
                        death_mask: u32::MAX,
                        veterancy_mask: 15,
                        exempt_status: 0,
                        required_status: 0,
                    },
                    effect: "LandingDeath".into(),
                    orient_to_object: true,
                    starts_active: true,
                    triggers: Vec::new(),
                    conflicts: Vec::new(),
                    requires_all: false,
                    removals: Vec::new(),
                    upgrade_fx: String::new(),
                });
            crate::skirmish::tests::register_behavior(&mut game.catalog[kind], "FXListDie", 0x82);
            game.catalog[kind].ai.update = if ai {
                crate::skirmish::UnitAiUpdate::Ordinary
            } else {
                crate::skirmish::UnitAiUpdate::Absent
            };
            game.catalog[kind].locomotor.legal_surfaces = 1;
            let id = game.spawn(kind, 0, [200.0, 200.0], true);
            game.movement_terrain = crate::terrain::TerrainMap::classify(
                game.grid.width(),
                game.grid.height(),
                [],
                [crate::Cell { x: water_x, y: 20 }],
            )
            .unwrap();
            let left = i32::from(water_x) * 10 - 5;
            game.movement_terrain.set_water_polygons(vec![
                crate::terrain::WaterPolygon::new(vec![
                    [left, 195, 10],
                    [left + 10, 195, 10],
                    [left + 10, 215, 10],
                    [left, 215, 10],
                ])
                .unwrap(),
            ]);
            game.apply_live_shock(id, [0.0; 3]);
            game.entities[id].ai.motion.velocity = [20.0, 0.0];
            let state = game.entities[id].live_physics.as_mut().unwrap();
            state.motion.position[2] = 1.0;
            state.motion.velocity[2] = -4.0;
            state.motion.rates = [0.0; 3];
            state.flags |= STUNNED;
            game.integrate_live_physics(id);
            assert_eq!(!game.entities[id].alive(), killed);
            assert!(game.entities[id].damage_state.last_attacker.is_none());
            if killed {
                let effect = game
                    .presentation
                    .object_fx_events
                    .iter()
                    .find(|event| event.effect == "LandingDeath")
                    .unwrap();
                assert_eq!(
                    effect.position.map(f32::to_bits),
                    [200.0, 200.0, 1.0].map(f32::to_bits)
                );
            }
        }
    }
    #[test]
    fn parachute_freefall_disable_is_explicit_physics_state_and_clears_on_landing() {
        let (mut game, id) = fixture();
        game.start_bridge_fall(id, 40.0);
        game.integrate_live_physics(id);
        assert!(!game.entities[id].freefall_disabled());
        game.start_parachute_freefall(id);
        assert!(!game.entities[id].freefall_disabled());
        game.integrate_live_physics(id);
        assert!(game.entities[id].freefall_disabled());
        game.tick += 1;
        game.set_flight_position(id, [200.0, 200.0, 0.0]);
        game.integrate_live_physics(id);
        assert!(!game.entities[id].freefall_disabled());
        assert!(game.entities[id].alive());
    }
    fn registered_fixture() -> (Skirmish, usize) {
        let (mut game, kind) = fixture_catalog();
        crate::skirmish::tests::register_update(&mut game.catalog[kind], "PhysicsBehavior");
        let id = game.spawn(kind, 0, [200.0, 200.0], true);
        (game, id)
    }

    #[test]
    fn terrain_oriented_physics_integrates_matrix_position_but_uses_cached_altitude_for_drag() {
        let (mut game, id) = registered_fixture();
        let kind = game.entities[id].type_index;
        game.catalog[kind]
            .kind_of
            .push("STICK_TO_TERRAIN_SLOPE".into());
        let samples = (0..64_u16)
            .map(|index| f32::from(index % 8) * 10.0)
            .collect();
        game.set_projectile_terrain(8, 8, [0.0; 2], samples, 20.0)
            .unwrap();
        game.set_object_transform(id, [25.0, 25.0, 70.0], transform::yaw(0.0));
        game.set_object_orientation(id, 0.0);
        assert_eq!(game.object_position(id)[2].to_bits(), 70.0_f32.to_bits());
        assert_eq!(game.object_transform(id).0[2].to_bits(), 25.0_f32.to_bits());
        game.entities[id].ai.motion.velocity = [2.0, 0.0];
        game.tick += 1;
        game.update_object_physics(id);
        // PhysicsUpdate.cpp642 integrates the matrix; applyFrictionalForces
        // still sees cached Object altitude and selects zero aerodynamic drag.
        assert_eq!(
            game.object_position(id).map(f32::to_bits),
            [27.0_f32, 25.0, 27.0].map(f32::to_bits)
        );
        assert_eq!(
            game.entities[id].physics_velocity()[0].to_bits(),
            2.0_f32.to_bits()
        );
        assert_eq!(
            game.object_transform(id).0.map(f32::to_bits),
            game.object_position(id).map(f32::to_bits)
        );
    }

    #[test]
    fn contained_mass_recurses_only_through_physics_and_changes_on_removal() {
        let (mut game, kind) = fixture_catalog();
        game.catalog[kind].contain = Some(crate::skirmish::containment::tests::rules(0.0, 1.0));
        game.catalog[kind].live_physics.as_mut().unwrap().resistance = 0.0;
        crate::skirmish::tests::register_update(&mut game.catalog[kind], "OpenContain");
        crate::skirmish::tests::register_update(&mut game.catalog[kind], "PhysicsBehavior");
        let carrier = game.spawn(kind, 0, [200.0, 200.0], true);
        let rider = game.spawn(kind, 0, [210.0, 200.0], true);
        let nested = game.spawn(kind, 0, [220.0, 200.0], true);
        game.add_to_contain(nested, rider);
        game.add_to_contain(rider, carrier);
        assert_eq!(
            game.object_physics_mass(carrier).unwrap().to_bits(),
            6.0_f32.to_bits()
        );
        game.apply_live_shock(carrier, [6.0, 0.0, 0.0]);
        assert_eq!(
            game.entities[carrier]
                .live_physics
                .as_ref()
                .unwrap()
                .motion
                .acceleration[0]
                .to_bits(),
            1.0_f32.to_bits()
        );
        game.remove_passenger(nested, rider);
        game.apply_live_shock(carrier, [4.0, 0.0, 0.0]);
        assert_eq!(
            game.entities[carrier]
                .live_physics
                .as_ref()
                .unwrap()
                .motion
                .acceleration[0]
                .to_bits(),
            2.0_f32.to_bits()
        );

        // A child without Physics also hides its own physical cargo from this sum.
        let without_physics = game.type_for(0, Kind::Infantry).unwrap();
        game.catalog[without_physics].contain =
            Some(crate::skirmish::containment::tests::rules(0.0, 1.0));
        crate::skirmish::tests::register_update(&mut game.catalog[without_physics], "OpenContain");
        let nonphysical = game.spawn(without_physics, 0, [230.0, 200.0], true);
        game.add_to_contain(nested, nonphysical);
        game.add_to_contain(nonphysical, carrier);
        assert!(game.object_physics_mass(nonphysical).is_none());
        assert_eq!(
            game.object_physics_mass(carrier).unwrap().to_bits(),
            4.0_f32.to_bits()
        );
    }

    #[test]
    fn contained_mass_sums_own_list_before_adding_base_mass() {
        let (mut game, kind) = fixture_catalog();
        game.catalog[kind].contain = Some(crate::skirmish::containment::tests::rules(0.0, 1.0));
        crate::skirmish::tests::register_update(&mut game.catalog[kind], "OpenContain");
        crate::skirmish::tests::register_update(&mut game.catalog[kind], "PhysicsBehavior");
        let carrier = game.spawn(kind, 0, [200.0, 200.0], true);
        let first = game.spawn(kind, 0, [210.0, 200.0], true);
        let second = game.spawn(kind, 0, [220.0, 200.0], true);
        game.entities[carrier]
            .live_physics
            .as_mut()
            .unwrap()
            .motion
            .mass = 16_777_216.0;
        game.entities[first]
            .live_physics
            .as_mut()
            .unwrap()
            .motion
            .mass = 1.0;
        game.entities[second]
            .live_physics
            .as_mut()
            .unwrap()
            .motion
            .mass = 1.0;
        game.add_to_contain(first, carrier);
        game.add_to_contain(second, carrier);
        // Adding each rider directly to base mass would lose both units.
        assert_eq!(
            game.object_physics_mass(carrier).unwrap().to_bits(),
            16_777_218.0_f32.to_bits()
        );
    }

    #[test]
    fn aircraft_force_reads_loaded_mass_again_after_passenger_removal() {
        let (mut game, kind) = fixture_catalog();
        game.catalog[kind].contain = Some(crate::skirmish::containment::tests::rules(0.0, 1.0));
        game.catalog[kind].locomotor.flight = Some(crate::skirmish::flight::tests::rules(
            crate::skirmish::flight::Appearance::Hover,
        ));
        crate::skirmish::tests::register_update(&mut game.catalog[kind], "OpenContain");
        crate::skirmish::tests::register_update(&mut game.catalog[kind], "PhysicsBehavior");
        let carrier = game.spawn(kind, 0, [200.0, 200.0], true);
        let rider = game.spawn(kind, 0, [210.0, 200.0], true);
        game.add_to_contain(rider, carrier);
        game.apply_flight_force(carrier, [4.0, 0.0, 0.0]);
        assert_eq!(
            game.entities[carrier]
                .aircraft_physics_input(&game.catalog[kind])
                .unwrap()
                .acceleration[0]
                .to_bits(),
            1.0_f32.to_bits()
        );
        game.remove_passenger(rider, carrier);
        game.apply_flight_force(carrier, [2.0, 0.0, 0.0]);
        assert_eq!(
            game.entities[carrier]
                .aircraft_physics_input(&game.catalog[kind])
                .unwrap()
                .acceleration[0]
                .to_bits(),
            2.0_f32.to_bits()
        );
    }

    fn physics_sleep(game: &Skirmish, id: usize) -> u32 {
        use crate::skirmish::module_updates::ModuleRegistration;
        let registration = game.catalog[game.entities[id].type_index]
            .update_modules
            .iter()
            .position(|entry| matches!(entry, ModuleRegistration::Behavior { family, .. } if family == "PhysicsBehavior"))
            .unwrap();
        game.update_remaining_sleep(id, registration)
    }

    #[test]
    fn native_motive_force_keeps_forward_impulse_and_reaches_actual_physics_once() {
        let (mut game, id) = registered_fixture();
        game.set_object_position(id, [200.0, 200.0, 20.0]);
        game.entities[id].ai.motion.motive_until = 100;
        game.entities[id].set_physics_pitch_rate(0.25);
        game.apply_physics_motive_force(id, [4.0, 6.0, 8.0]);
        let actor = &game.entities[id];
        let motion = &actor.live_physics.as_ref().unwrap().motion;
        assert_eq!(
            motion.acceleration.map(f32::to_bits),
            [2.0_f32, 3.0, 4.0].map(f32::to_bits)
        );
        assert_eq!(motion.rates[1].to_bits(), 0.25_f32.to_bits());
        assert_eq!(actor.ai.motion.motive_until, 10);
        game.tick = 1;
        game.integrate_live_physics(id);
        assert_eq!(
            game.entities[id].physics_velocity()[0].to_bits(),
            2.0_f32.to_bits()
        );
        assert_eq!(
            game.entities[id].physics_velocity()[1].to_bits(),
            3.0_f32.to_bits()
        );
        assert_eq!(
            game.entities[id].physics_velocity()[2].to_bits(),
            3.9_f32.to_bits()
        );
    }

    #[test]
    fn reset_dynamic_physics_clears_motion_and_retains_motive_and_fall_flags() {
        let (mut game, id) = registered_fixture();
        game.set_object_position(id, [200.0, 200.0, 20.0]);
        game.entities[id].set_physics_allow_fall(true);
        game.entities[id].ai.motion.motive_until = 10;
        game.entities[id].ai.motion.velocity = [4.0, 5.0];
        game.entities[id].ai.motion.acceleration = [6.0, 7.0];
        let motion = &mut game.entities[id].live_physics.as_mut().unwrap().motion;
        motion.velocity = [4.0, 5.0, -6.0];
        motion.acceleration = [1.0, 2.0, 3.0];
        motion.rates = [0.1, 0.2, 0.3];
        game.reset_dynamic_physics(id);
        let actor = &game.entities[id];
        assert_eq!(actor.physics_velocity().map(f32::to_bits), [0; 3]);
        assert_eq!(actor.ai.motion.velocity.map(f32::to_bits), [0; 2]);
        assert_eq!(actor.ai.motion.acceleration.map(f32::to_bits), [0; 2]);
        let motion = &actor.live_physics.as_ref().unwrap().motion;
        assert_eq!(motion.acceleration.map(f32::to_bits), [0; 3]);
        assert_eq!(motion.rates.map(f32::to_bits), [0; 3]);
        assert_eq!(actor.ai.motion.motive_until, 10);
        assert!(actor.physics_allows_fall());
        assert_eq!(physics_sleep(&game, id), 1);
    }

    #[test]
    fn parachute_freefall_does_not_clear_the_physics_update_lifetime() {
        use crate::skirmish::module_updates::FOREVER;
        let (mut game, id) = registered_fixture();
        game.tick = 1;
        game.run_object_updates();
        game.start_parachute_freefall(id);
        assert!(
            game.entities[id]
                .live_physics
                .as_ref()
                .unwrap()
                .frame
                .is_none()
        );
        game.reset_dynamic_physics(id);
        assert_eq!(physics_sleep(&game, id) + game.update_frame(), FOREVER);
        assert!(
            game.entities[id]
                .live_physics
                .as_ref()
                .unwrap()
                .has(FREEFALL)
        );
    }

    #[test]
    fn reset_dynamic_physics_waits_for_first_update_before_sleeping_forever() {
        use crate::skirmish::module_updates::FOREVER;
        let (mut game, id) = registered_fixture();
        game.reset_dynamic_physics(id);
        assert_eq!(physics_sleep(&game, id), 1);
        game.tick = 1;
        game.run_object_updates();
        game.reset_dynamic_physics(id);
        assert_eq!(physics_sleep(&game, id) + game.update_frame(), FOREVER);
    }

    #[test]
    fn ordinary_physics_constructor_sleeps_at_rest_and_force_wakes_the_real_callback() {
        for masked in [false, true] {
            use crate::skirmish::module_updates::FOREVER;
            let (mut game, id) = registered_fixture();
            assert!(game.entities[id].live_physics_active());
            assert_eq!(physics_sleep(&game, id), 1);
            game.tick = 1;
            game.run_object_updates();
            // GameLogic caps the absolute wake frame, not the remaining delay.
            assert_eq!(physics_sleep(&game, id) + game.update_frame(), FOREVER);
            // AIIdleState::doInitIdleState snaps to its reserved goal cell at frame 1.
            // Measure the Physics impulse from that already committed resting pose.
            let resting_x = game.entities[id].position[0];
            game.mask_object(id, masked);
            let kind = game.entities[id].type_index;
            game.catalog[kind].locomotor.stick_to_ground = true;
            game.apply_live_shock(id, [4.0, 0.0, 0.0]);
            assert_eq!(physics_sleep(&game, id), 1);
            game.tick = 2;
            game.run_object_updates();
            assert_eq!(
                game.entities[id].position[0].to_bits(),
                (resting_x + 1.5).to_bits()
            );
            assert_eq!(
                game.entities[id].physics_forward_speed().to_bits(),
                1.5_f32.to_bits()
            );
            game.run_object_updates();
            assert_eq!(
                game.entities[id].position[0].to_bits(),
                (resting_x + 1.5).to_bits()
            );
        }
    }

    #[test]
    fn held_ordinary_callback_discards_force_then_released_body_keeps_its_velocity() {
        for masked in [false, true] {
            let (mut game, id) = registered_fixture();
            game.mask_object(id, masked);
            game.set_object_position(id, [200.0, 200.0, 20.0]);
            game.entities[id].disabled_status |= 1 << 3;
            game.entities[id].ai.motion.velocity = [2.0, 0.0];
            game.entities[id].ai.motion.acceleration = [5.0, 0.0];
            let state = game.entities[id].live_physics.as_mut().unwrap();
            state.motion.velocity = [2.0, 0.0, -1.0];
            state.motion.acceleration = [0.0, 0.0, 3.0];
            game.tick = 1;
            game.run_object_updates();
            assert_eq!(
                game.object_position(id).map(f32::to_bits),
                [200.0_f32, 200.0, 20.0].map(f32::to_bits)
            );
            assert_eq!(
                game.entities[id].live_velocity().unwrap().map(f32::to_bits),
                [2.0_f32, 0.0, -1.0].map(f32::to_bits)
            );
            assert_eq!(
                game.entities[id]
                    .live_physics
                    .as_ref()
                    .unwrap()
                    .motion
                    .acceleration
                    .map(f32::to_bits),
                [0.0_f32; 3].map(f32::to_bits)
            );
            game.entities[id].disabled_status &= !(1 << 3);
            game.tick = 2;
            game.run_object_updates();
            assert_eq!(
                game.object_position(id).map(f32::to_bits),
                [202.0_f32, 200.0, 18.9].map(f32::to_bits)
            );
        }
    }

    #[test]
    fn ordinary_bridge_floor_landing_applies_fall_damage_but_cannot_sleep_forever() {
        let (mut game, id) = registered_fixture();
        let layer = game
            .movement_terrain
            .add_bridge(
                crate::terrain::BridgeGeometry {
                    from: [150.0, 200.0, 80.0],
                    to: [250.0, 200.0, 80.0],
                    corners: [
                        [150.0, 225.0, 80.0],
                        [150.0, 175.0, 80.0],
                        [250.0, 225.0, 80.0],
                        [250.0, 175.0, 80.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        game.entities[id].layer = layer;
        game.set_object_position(id, [200.0, 200.0, 81.0]);
        let kind = game.entities[id].type_index;
        game.catalog[kind]
            .live_physics
            .as_mut()
            .unwrap()
            .min_fall_speed = 0.0;
        game.entities[id]
            .live_physics
            .as_mut()
            .unwrap()
            .motion
            .velocity[2] = -4.0;
        let health = game.entities[id].health;
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(game.object_position(id)[2].to_bits(), 80.0_f32.to_bits());
        assert_eq!(game.entities[id].health.to_bits(), (health - 8.2).to_bits());
        assert!(
            !game.entities[id]
                .live_physics
                .as_ref()
                .unwrap()
                .has(AIRBORNE)
        );
        assert_eq!(physics_sleep(&game, id), 1);
    }

    #[test]
    fn ordinary_physics_callback_is_removed_with_its_object() {
        let (mut game, id) = registered_fixture();
        game.apply_live_shock(id, [0.0, 0.0, 10.0]);
        game.destroy_maintenance_object(id);
        game.process_update_destruction();
        let position = game.object_position(id).map(f32::to_bits);
        game.tick = 1;
        game.run_object_updates();
        assert!(!game.entities[id].exists());
        assert!(
            game.update_objects[game.entities[id].update_owner]
                .handles
                .is_empty()
        );
        assert_eq!(game.object_position(id).map(f32::to_bits), position);
    }

    #[test]
    fn native_forward_speed_retains_projected_component_magnitude_and_sign() {
        let (mut game, id) = registered_fixture();
        game.entities[id].facing = std::f32::consts::FRAC_PI_4;
        game.entities[id]
            .live_physics
            .as_mut()
            .unwrap()
            .motion
            .velocity = [3.0, 4.0, 99.0];
        let vx = 3.0 * std::f32::consts::FRAC_PI_4.cos();
        let vy = 4.0 * std::f32::consts::FRAC_PI_4.sin();
        let squares = [vx, vy].map(|component| component * component);
        let expected = (squares[0] + squares[1]).sqrt();
        assert_eq!(
            game.entities[id].physics_forward_speed().to_bits(),
            expected.to_bits()
        );
        assert_ne!(
            game.entities[id].physics_forward_speed().to_bits(),
            (vx + vy).to_bits()
        );
        game.entities[id]
            .live_physics
            .as_mut()
            .unwrap()
            .motion
            .velocity = [-3.0, -4.0, 99.0];
        assert_eq!(
            game.entities[id].physics_forward_speed().to_bits(),
            (-expected).to_bits()
        );
    }

    #[test]
    fn registered_overlap_retains_one_previous_frame_and_uses_actual_crush_contact() {
        use crate::skirmish::{Faction, UnitAiUpdate, tests::fixture_type};
        let (mut game, id) = registered_fixture();
        let kind = game.entities[id].type_index;
        game.catalog[kind].crush.crusher_level = 1;
        game.catalog[kind].ai.update = UnitAiUpdate::Absent;
        let mut victim = fixture_type(Faction::Civilian, Kind::Barracks);
        victim.crush.crushable_level = 0;
        victim.geometry.major = 20.0;
        let victim_kind = game.catalog.len();
        game.catalog.push(victim);
        let other = game.spawn(victim_kind, 1, [198.0, 200.0], true);
        game.entities[other].facing = std::f32::consts::FRAC_PI_2;
        game.entities[id].facing = 0.0;
        game.entities[id].ai.motion.velocity = [1.0, 0.0];
        game.entities[id]
            .live_physics
            .as_mut()
            .unwrap()
            .motion
            .velocity = [1.0, 0.0, 0.0];
        game.object_contact(id, other, false);
        assert!(!game.entities[other].alive());
        assert_eq!(
            game.entities[id]
                .live_physics
                .as_ref()
                .unwrap()
                .current_overlap,
            Some(other)
        );
        game.clear_live_physics(id);
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[id]
                .live_physics
                .as_ref()
                .unwrap()
                .previous_overlap,
            Some(other)
        );
        assert_eq!(physics_sleep(&game, id), 1);
        game.tick = 2;
        game.run_object_updates();
        assert!(!game.entities[id].physics_has_overlap());
        assert_eq!(
            physics_sleep(&game, id) + game.update_frame(),
            crate::skirmish::module_updates::FOREVER
        );
    }
}

#[cfg(test)]
mod airborne_friction_tests {
    use super::*;
    use crate::skirmish::tests;

    #[test]
    fn airborne_friction_is_per_object_and_defaults_off_before_any_locomotor_update() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(
            crate::skirmish::Faction::Gla,
            crate::skirmish::Kind::Vehicle,
        );
        unit.ai.update = crate::skirmish::UnitAiUpdate::Absent;
        unit.update_modules.clear();
        unit.behavior_modules.clear();
        unit.live_physics = Some(LiveRules {
            motion: MotionRules {
                center_of_mass_offset: 0.0,
                mass: 1.0,
                gravity: -0.1,
                stiffness: 0.0,
                forward_friction: 0.15,
                lateral_friction: 0.15,
                aerodynamic_friction: 0.0,
                angular_factor: 1.0,
                allow_bouncing: false,
            },
            resistance: 1.0,
            kill_when_resting: false,
            collision: CollisionRules {
                stiffness: 0.0,
                rubble_height: 0.0,
                allow_force: false,
                weapons: [None, None],
            },
            shock_rotation: [0.0; 3],
            min_fall_speed: 99.0,
            fall_damage_factor: 0.0,
        });
        // A template's locomotor setting alone must not mutate constructor Physics flags.
        unit.locomotor.friction_when_airborne = true;
        tests::register_update(&mut unit, "PhysicsBehavior");
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let first = game.new_object(kind, 0, true);
        let second = game.new_object(kind, 0, true);
        for (id, x) in [(first, 200.0), (second, 300.0)] {
            game.set_object_position(id, [x, 200.0, 100.0]);
            game.entities[id].ai.motion.velocity = [10.0, 0.0];
        }
        game.entities[first].set_physics_airborne_friction(true);
        assert!(!game.entities[second].physics_control.airborne_friction);
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[first].physics_velocity()[0].to_bits(),
            8.5_f32.to_bits()
        );
        assert_eq!(
            game.entities[second].physics_velocity()[0].to_bits(),
            10.0_f32.to_bits()
        );
        assert!(game.catalog[kind].locomotor.friction_when_airborne);
        game.entities[first].set_physics_airborne_friction(false);
        game.tick = 2;
        game.run_object_updates();
        assert_eq!(
            game.entities[first].physics_velocity()[0].to_bits(),
            8.5_f32.to_bits()
        );
    }
}
