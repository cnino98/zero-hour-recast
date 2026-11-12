//! `Locomotor::handleBehaviorZ/calcLiftToUseAtPt` and `PhysicsBehavior` vertical integration.
use super::{Flight, Rules, Skirmish};

pub(super) fn lift(
    rules: &Rules,
    height: f32,
    velocity: f32,
    target: f32,
    damaged: bool,
    precise: bool,
    limit: f32,
) -> f32 {
    let gross = if damaged {
        rules.lift_damaged
    } else {
        rules.lift
    };
    crate::skirmish::flight::physics::lift(
        height,
        velocity,
        target,
        gross.min(limit),
        rules.gravity,
        rules.speed_limit_z,
        precise,
    )
}

impl Skirmish {
    pub(in crate::skirmish) fn update_chinook_lift(&mut self, id: usize) {
        let actor = &self.entities[id];
        let Some(state) = &actor.chinook else {
            return;
        };
        if actor.physics_stunned() && state.transitioning() {
            return;
        }
        let unit = &self.catalog[actor.type_index];
        let rules = unit.chinook.as_ref().expect("Chinook rules");
        let precise = actor.flight_ultra_accurate();
        let target = match state.flight {
            Flight::Landing(goal) | Flight::TakingOff(goal) => goal[2],
            Flight::Landed => {
                self.entities[id]
                    .chinook
                    .as_mut()
                    .expect("Chinook state")
                    .acceleration = 0.0;
                return;
            }
            Flight::Flying | Flight::CombatDrop => {
                self.ground_height(actor.position) + self.flight_preferred_height(id)
            }
        };
        let delta = (target - state.height) * rules.damping;
        let target = state.height + delta;
        let acceleration = lift(
            rules,
            state.height,
            state.velocity,
            target,
            actor.health / actor.max_health <= unit.locomotor.damage_health_threshold,
            precise,
            actor.flight_lift_limit(),
        );
        let mass = self
            .object_physics_mass(id)
            .expect("Chinook PhysicsBehavior");
        let force = acceleration * mass;
        self.entities[id]
            .chinook
            .as_mut()
            .expect("Chinook state")
            .acceleration = force * (1.0 / mass);
    }

    pub(in crate::skirmish) fn integrate_chinook(&mut self, id: usize) {
        if self.entities[id].live_physics_active() {
            return;
        }
        let actor = &mut self.entities[id];
        let Some(state) = &mut actor.chinook else {
            return;
        };
        let rules = self.catalog[actor.type_index]
            .chinook
            .as_ref()
            .expect("Chinook rules");
        // Selected Chinook Apply2DFrictionWhenAirborne excludes Z drag.
        state.acceleration += rules.gravity;
        state.velocity += state.acceleration;
        if state.velocity.abs() < 0.001 {
            state.velocity = 0.0;
        }
        state.height += state.velocity;
        state.acceleration = 0.0;
    }

    pub(in crate::skirmish) fn clip_chinook(&mut self, id: usize) {
        if self.entities[id].live_physics_active() {
            return;
        }
        let ground = self.ground_height(self.entities[id].position);
        let actor = &mut self.entities[id];
        let Some(state) = &mut actor.chinook else {
            return;
        };
        // PhysicsBehavior clips once, after integrating all position components.
        if state.height <= ground {
            state.velocity += ground - state.height;
            state.velocity = state.velocity.min(0.0);
            state.height = ground;
        }
        if let Some(flight) = &mut actor.flight {
            flight.height = state.height;
        }
    }
}
