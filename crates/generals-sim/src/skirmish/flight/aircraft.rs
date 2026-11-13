//! Explicit locomotor requests and per-instance limits used by `JetAI` states.
use super::Goal;
use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn set_explicit_flight_goal(
        &mut self,
        id: usize,
        position: [f32; 3],
        speed: f32,
    ) {
        // AIUpdate.cpp2163/2327: POSITION_EXPLICIT replaces the prior goal kind
        // and supplies zero path distance. Locomotor then enters exact braking
        // before expanding the distance to the actual XY separation.
        self.entities[id].ai.motion.clear_goal();
        let state = self.entities[id].flight.as_mut().expect("flight state");
        state.goal = Some(Goal {
            position,
            distance: 0.0,
            speed: speed.min(state.speed_limit),
            no_slow_down: false,
        });
    }

    pub(in crate::skirmish) fn set_flight_lift_limit(&mut self, id: usize, limit: f32) {
        if let Some(state) = &mut self.entities[id].flight {
            state.lift_limit = limit;
        }
    }

    pub(in crate::skirmish) fn set_flight_speed_limit(&mut self, id: usize, limit: f32) {
        if let Some(state) = &mut self.entities[id].flight {
            state.speed_limit = limit;
        }
    }

    pub(in crate::skirmish) fn integrate_aircraft_taxi_height(&mut self, id: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if actor.live_physics_active()
            || actor.aircraft.is_none()
            || actor.active_locomotor(unit).ground.is_none()
        {
            return;
        }
        let Some(rules) = &unit.locomotor.flight else {
            return;
        };
        let ground = self.ground_height(actor.position);
        let state = self.entities[id]
            .flight
            .as_mut()
            .expect("aircraft flight state");
        state.velocity[2] += rules.gravity;
        if state.velocity[2].abs() < 0.001 {
            state.velocity[2] = 0.0;
        }
        state.height += state.velocity[2];
        super::physics::ground_collision(state, ground);
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn set_flight_turn_limit(&mut self, id: usize, limit: f32) {
        if let Some(state) = &mut self.entities[id].flight {
            state.turn_limit = limit;
        }
    }

    pub(in crate::skirmish) fn dead_aircraft_goal(&mut self, id: usize) {
        if let Some(state) = &mut self.entities[id].flight {
            state.goal = None;
            state.goal_z = None;
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn set_helicopter_braking(&mut self, id: usize, limit: f32) {
        if let Some(state) = &mut self.entities[id].flight {
            state.braking_limit = limit;
        }
    }
}
