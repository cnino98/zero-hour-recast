use super::{ALLOW_AIR, Rules, SPECIAL_RETURN};
use crate::skirmish::{Entity, Skirmish, UnitType, locomotor};
use locomotor::set;

impl Entity {
    // JetAIUpdate::getSneakyTargetingOffset. This is checked at weapon launch
    // and collision, independently of the locomotor's shorter persistence.
    pub(in crate::skirmish) fn aircraft_targeting_offset(
        &self,
        unit: &UnitType,
        frame: u64,
    ) -> Option<[f32; 3]> {
        let state = self.aircraft.as_ref()?;
        if state.miss_until == 0
            || crate::skirmish::ai_dispatch::native_frame(frame) >= state.miss_until
        {
            return None;
        }
        let offset = unit.aircraft.as_ref()?.sneaky_offset;
        Some([self.facing.cos() * offset, self.facing.sin() * offset, 0.0])
    }

    pub(in crate::skirmish) fn requested_aircraft_locomotor(
        &self,
        unit: &UnitType,
        requested: u8,
    ) -> u8 {
        let Some(state) = &self.aircraft else {
            return requested;
        };
        let rules = unit.aircraft.as_ref().expect("aircraft rules");
        if !state.flag(ALLOW_AIR) {
            set::TAXI
        } else if state.attack_until != 0 {
            rules
                .attacking
                .as_ref()
                .map_or(set::NORMAL, |loco| loco.role)
        } else if state.flag(SPECIAL_RETURN) {
            rules
                .returning
                .as_ref()
                .map_or(set::NORMAL, |loco| loco.role)
        } else {
            requested
        }
    }

    pub(in crate::skirmish) fn aircraft_speed(&self, unit: &UnitType) -> Option<f32> {
        selected(unit.aircraft.as_ref()?, self.ai.locomotor_set).map(|set| set.speed)
    }
}

const fn selected(rules: &Rules, selection: u8) -> Option<&super::LocomotorSet> {
    if selection == set::TAXI {
        Some(&rules.taxi)
    } else if let Some(loco) = &rules.attacking
        && loco.role == selection
    {
        Some(loco)
    } else if let Some(loco) = &rules.returning
        && loco.role == selection
    {
        Some(loco)
    } else {
        None
    }
}

impl Skirmish {
    pub(in crate::skirmish::aircraft) fn choose_aircraft_locomotor(
        &mut self,
        id: usize,
        airborne: bool,
    ) {
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.set(ALLOW_AIR, airborne);
        self.choose_locomotor_set(id, set::NORMAL);
    }

    pub(in crate::skirmish::aircraft) fn update_aircraft_attack_locomotor(&mut self, id: usize) {
        let now = self.update_frame();
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules");
        let attacking = actor.attacking();
        let mut expired = false;
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        if attacking {
            state.attack_until = now.wrapping_add(rules.attack_locomotor_frames);
            state.miss_until = now.wrapping_add(rules.attackers_miss_frames);
        } else {
            if state.attack_until != 0 && now >= state.attack_until {
                state.attack_until = 0;
                expired = true;
            }
            if state.miss_until != 0 && now >= state.miss_until {
                state.miss_until = 0;
            }
        }
        if expired {
            self.choose_locomotor_set(id, set::NORMAL);
        }
    }

    pub(in crate::skirmish::aircraft) fn face_aircraft(
        &mut self,
        id: usize,
        angle: f32,
        tolerance: f32,
    ) -> bool {
        let actor = &mut self.entities[id];
        if locomotor::normalize_angle(actor.facing - angle).abs() <= tolerance {
            return true;
        }
        actor.ai.motion.request_angle(angle);
        false
    }

    pub(in crate::skirmish::aircraft) fn aircraft_explicit_flight_goal(
        &mut self,
        id: usize,
        point: [f32; 3],
    ) {
        let position = self.object_position(id);
        let dx = position[0] - point[0];
        let dy = position[1] - point[1];
        let xx = dx * dx;
        let yy = dy * dy;
        let actor = &self.entities[id];
        let speed = locomotor::speed(actor, &self.catalog[actor.type_index]) * (1.0 / 30.0);
        self.set_explicit_flight_goal(id, point, (xx + yy).sqrt(), speed, false);
    }
}

impl Skirmish {
    pub(in crate::skirmish::aircraft) fn set_aircraft_flight_limits(
        &mut self,
        id: usize,
        landing: bool,
    ) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let loco = actor.active_locomotor(unit);
        let rules = loco.flight.as_ref().expect("aircraft flight locomotor");
        let maximum_lift = if actor.health / actor.max_health <= loco.damage_health_threshold {
            rules.lift_damaged
        } else {
            rules.lift
        };
        let minimum_speed = rules.min_speed;
        self.entities[id]
            .aircraft
            .as_mut()
            .expect("aircraft state")
            .takeoff_lift = maximum_lift;
        if landing {
            self.set_flight_speed_limit(id, minimum_speed);
        }
        self.set_flight_lift_limit(id, if landing { 99_999.0 } else { 0.0 });
    }

    pub(in crate::skirmish::aircraft) fn reset_aircraft_flight_limits(&mut self, id: usize) {
        if !self.entities[id].is_effectively_dead() {
            self.set_flight_lift_limit(id, 99_999.0);
        }
    }

    pub(in crate::skirmish::aircraft) fn limit_aircraft_lift(&mut self, id: usize, ratio: f32) {
        let maximum = self.entities[id]
            .aircraft
            .as_ref()
            .expect("aircraft state")
            .takeoff_lift;
        self.set_flight_lift_limit(id, maximum * ratio);
    }
}
