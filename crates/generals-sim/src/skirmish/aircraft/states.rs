use super::{CIRCLING, LANDING, LANDING_SOUND_PLAYED, Phase, SPECIAL_RETURN, TAKEOFF, TAXI};
use crate::skirmish::{
    Order, Skirmish,
    ai_goals::MainState,
    ground_movement::MoveResult,
    path_following::{Node, Path},
};

impl Skirmish {
    pub(in crate::skirmish::aircraft) fn enter_aircraft_phase(
        &mut self,
        id: usize,
        mut phase: Phase,
    ) {
        // State::friend_checkForTransitions stops its recursive chain at 20.
        for _ in 0..20 {
            if self
                .prepare_order_state(id, Order::Idle, false, None, None)
                .is_none()
            {
                return;
            }
            if phase == Phase::Idle {
                self.enter_primary_idle(id);
                return;
            }
            self.entities[id]
                .aircraft
                .as_mut()
                .expect("Jet constructor")
                .phase = phase;
            self.entities[id].ai.command.main_state = crate::skirmish::ai_goals::MainState::Jet;
            self.ai_state_changed(id);
            match self.begin_aircraft_phase(id, phase) {
                None => return,
                Some(success) => phase = self.next_aircraft_phase(id, phase, success),
            }
        }
    }

    pub(in crate::skirmish::aircraft) fn next_aircraft_phase(
        &self,
        id: usize,
        phase: Phase,
        success: bool,
    ) -> Phase {
        let runway = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules")
            .needs_runway;
        if !success {
            return match phase {
                Phase::Return | Phase::ReturnDead => Phase::ReturnDead,
                Phase::Landing | Phase::Reload if !runway => Phase::Takeoff,
                _ => Phase::Idle,
            };
        }
        match phase {
            Phase::TaxiFromHangar if !runway => Phase::Idle,
            Phase::AwaitTakeoff if runway => Phase::TaxiTakeoff,
            Phase::AwaitTakeoff | Phase::Pause => Phase::Takeoff,
            Phase::TaxiTakeoff => Phase::Pause,
            Phase::Return => Phase::AwaitLanding,
            Phase::AwaitLanding if runway => Phase::Landing,
            Phase::TaxiFromHangar | Phase::TaxiFromLanding | Phase::AwaitLanding => Phase::Orient,
            Phase::Landing if runway => Phase::TaxiFromLanding,
            Phase::Landing => Phase::Reload,
            Phase::Orient if runway => Phase::Reload,
            Phase::Orient => Phase::Landing,
            Phase::ReturnDead => Phase::CircleDead,
            _ => Phase::Idle,
        }
    }

    fn begin_aircraft_phase(&mut self, id: usize, phase: Phase) -> Option<bool> {
        if phase == Phase::Idle {
            return None;
        }
        match phase {
            Phase::Idle => None,
            Phase::AwaitTakeoff | Phase::AwaitLanding => self.begin_aircraft_await(id, phase),
            Phase::TaxiFromHangar | Phase::TaxiTakeoff | Phase::TaxiFromLanding => {
                self.begin_aircraft_taxi(id, phase)
            }
            Phase::Takeoff | Phase::Landing => self.begin_aircraft_flight(id, phase),
            Phase::Orient => {
                if self.catalog[self.entities[id].type_index]
                    .aircraft
                    .as_ref()
                    .expect("aircraft rules")
                    .helipad
                {
                    return Some(true);
                }
                self.entities[id]
                    .aircraft
                    .as_mut()
                    .expect("aircraft state")
                    .set(LANDING, true);
                self.entities[id].ai.ignored_producer = self.entities[id].producer;
                None
            }
            Phase::Pause => self.begin_aircraft_pause(id),
            Phase::Reload => {
                let extra = self.passenger_bonus_flags(id);
                let actor = &self.entities[id];
                let reload = super::rearm::Reload::begin(
                    actor,
                    &self.catalog[actor.type_index],
                    self.tick,
                    extra,
                );
                let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
                state.set(TAKEOFF | LANDING | SPECIAL_RETURN, false);
                state.rearm = reload;
                None
            }
            Phase::Return => self.begin_aircraft_return(id),
            Phase::ReturnDead => {
                let goal = self.entities[id]
                    .aircraft
                    .as_ref()
                    .expect("aircraft state")
                    .producer_position
                    .expect("producer position");
                self.entities[id].ai.command.move_to.position = goal;
                self.entities[id].ai.command.move_to.adjusts = true;
                movement_result(&self.enter_primary_ground_move(id))
            }
            Phase::CircleDead => {
                if !self.aircraft_out_of_special_ammo(id) && self.entities[id].producer.is_none() {
                    return Some(false);
                }
                self.entities[id].ai.motion.clear_goal();
                self.dead_aircraft_goal(id);
                self.entities[id]
                    .aircraft
                    .as_mut()
                    .expect("aircraft state")
                    .check_airfield = 30;
                self.aircraft_low_fuel_sound(id);
                None
            }
        }
    }

    fn begin_aircraft_await(&mut self, id: usize, phase: Phase) -> Option<bool> {
        if !self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules")
            .needs_runway
        {
            return Some(true);
        }
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.set(TAKEOFF, phase == Phase::AwaitTakeoff);
        state.set(LANDING, phase == Phase::AwaitLanding);
        state.set(CIRCLING, true);
        None
    }

    fn begin_aircraft_taxi(&mut self, id: usize, phase: Phase) -> Option<bool> {
        self.entities[id].ai.blocking.path_through_units = true;
        if phase == Phase::TaxiFromLanding
            && !self.entities[id].behaviors.countermeasures.is_empty()
        {
            self.reload_countermeasures(id, 0);
        }
        self.choose_aircraft_locomotor(id, false);
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.set(TAKEOFF, phase != Phase::TaxiFromLanding);
        state.set(LANDING, phase == Phase::TaxiFromLanding);
        state.set(TAXI, true);
        let Some(info) = self.aircraft_parking(id) else {
            return Some(false);
        };
        let points = super::taxi::points(&info, phase);
        self.aircraft_route(id, points);
        self.entities[id].ai.ignored_producer = self.entities[id].producer;
        self.set_flight_precision(id, true, true);
        self.entities[id].ai.motion.set_allow_invalid_position(true);
        self.entities[id].ai.command.move_to.adjusts = true;
        movement_result(&self.enter_primary_ground_move(id))
    }

    fn begin_aircraft_pause(&mut self, id: usize) -> Option<bool> {
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.set(TAKEOFF, true);
        state.set(LANDING, false);
        state.pause = super::Pause::new();
        let Some(info) = self.aircraft_parking(id) else {
            return Some(true);
        };
        self.entities[id]
            .aircraft
            .as_mut()
            .expect("aircraft state")
            .endpoint = info.end;
        self.set_ai_goal_position(id, info.end);
        let actor = &self.entities[id];
        let loco = actor.active_locomotor(&self.catalog[actor.type_index]);
        let can_turn = loco.ground.as_ref().map_or_else(
            || {
                loco.flight
                    .as_ref()
                    .is_some_and(|flight| flight.min_speed == 0.0)
            },
            |ground| ground.min_speed == 0.0,
        );
        self.entities[id].aircraft.as_mut().unwrap().pause.can_turn = can_turn;
        None
    }

    fn begin_aircraft_flight(&mut self, id: usize, phase: Phase) -> Option<bool> {
        let landing = phase == Phase::Landing;
        self.choose_aircraft_locomotor(id, true);
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.set(TAKEOFF, !landing);
        state.set(LANDING, landing);
        state.set(LANDING_SOUND_PLAYED, false);
        self.set_flight_precision(id, true, true);
        self.entities[id].ai.ignored_producer = self.entities[id].producer;
        let runway = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .unwrap()
            .needs_runway;
        if runway {
            self.set_aircraft_flight_limits(id, landing);
        }
        let Some(producer) = self.aircraft_producer(id) else {
            return Some(false);
        };
        let rules = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules");
        if !rules.needs_runway {
            return self.begin_helicopter_flight(id, producer, landing);
        }
        let Some(info) = self.aircraft_parking(id) else {
            return Some(false);
        };
        if landing && !self.parking_reserve_runway(producer, id, true) {
            return Some(false);
        }
        let points = if landing {
            vec![info.approach, info.end, info.start]
        } else {
            vec![[info.end[0], info.end[1], info.approach[2]], info.approach]
        };
        let first = points.first().copied();
        self.entities[id].ai.waypoints.points = points;
        self.entities[id].ai.command.move_to.adjusts = false;
        movement_result(&self.enter_follow_path_state(id, first))
    }

    fn begin_helicopter_flight(
        &mut self,
        id: usize,
        producer: usize,
        landing: bool,
    ) -> Option<bool> {
        let helipad = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules")
            .helipad;
        let (parking, height) = if helipad {
            let position = if landing {
                self.entities[id]
                    .aircraft
                    .as_ref()
                    .expect("aircraft state")
                    .landing_position
            } else {
                self.object_position(id)
            };
            let rules = self.catalog[self.entities[producer].type_index]
                .parking
                .as_ref()
                .expect("parking rules");
            (position, rules.approach_height + rules.deck_height)
        } else {
            let Some(info) = self.aircraft_parking(id) else {
                return Some(false);
            };
            (info.parking.position, info.approach[2] - info.end[2])
        };
        let approach = [parking[0], parking[1], parking[2] + height];
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.heli_points = if landing {
            [approach, parking]
        } else {
            [parking, approach]
        };
        state.heli_index = 0;
        None
    }

    fn begin_aircraft_return(&mut self, id: usize) -> Option<bool> {
        if self.aircraft_producer(id).is_none() {
            self.entities[id].producer = None;
            let Some(producer) = self.find_airfield(id) else {
                return Some(false);
            };
            self.entities[id].producer = Some(producer);
        }
        let rules = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules");
        let helipad = rules.helipad;
        let runway = rules.needs_runway;
        let goal = if helipad {
            self.entities[id]
                .aircraft
                .as_ref()
                .expect("aircraft state")
                .landing_position
        } else {
            let Some(info) = self.aircraft_parking(id) else {
                return Some(false);
            };
            if runway {
                info.approach
            } else {
                info.parking.position
            }
        };
        self.entities[id].ai.command.move_to.position = goal;
        self.entities[id].ai.command.move_to.adjusts = false;
        movement_result(&self.enter_primary_ground_move(id))
    }

    pub(in crate::skirmish::aircraft) fn aircraft_route(
        &mut self,
        id: usize,
        points: Vec<[f32; 3]>,
    ) {
        let start = self.object_position(id);
        let end = *points.last().expect("nonempty aircraft route");
        let nodes = std::iter::once(start)
            .chain(points)
            .map(|position| Node {
                position,
                layer: 1,
                previous_layer: 1,
                can_optimize: false,
            })
            .collect();
        self.entities[id].ai.ground_path = Some(Path::new(nodes));
        self.entities[id].ai.command.move_to.position = end;
    }

    pub(in crate::skirmish) fn exit_aircraft_phase(&mut self, id: usize) {
        if !matches!(
            self.entities[id].ai.command.main_state,
            crate::skirmish::ai_goals::MainState::Jet
        ) {
            return;
        }
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules");
        let runway = rules.needs_runway;
        let keep = rules.keeps_space;
        let phase = actor.aircraft.as_ref().expect("aircraft state").phase;
        if matches!(phase, Phase::Takeoff | Phase::Landing) {
            self.reset_aircraft_flight_limits(id);
            if let Some(producer) = self.aircraft_producer(id) {
                if phase == Phase::Takeoff && !keep {
                    self.release_parking_space(producer, id);
                }
                if runway {
                    self.parking_release_runway(producer, id);
                }
            }
            if phase == Phase::Landing && !runway {
                self.choose_aircraft_locomotor(id, false);
            }
        }
        if matches!(
            phase,
            Phase::Takeoff
                | Phase::Landing
                | Phase::TaxiFromHangar
                | Phase::TaxiFromLanding
                | Phase::TaxiTakeoff
        ) {
            self.set_flight_precision(id, false, false);
        }
        if matches!(
            phase,
            Phase::TaxiFromHangar | Phase::TaxiFromLanding | Phase::TaxiTakeoff
        ) {
            self.entities[id]
                .ai
                .motion
                .set_allow_invalid_position(false);
            self.entities[id].ai.motion.end_move();
            self.entities[id].ai.ground_path = None;
            self.entities[id].ai.path.clear();
            self.entities[id].ai.path_request.cancel();
            self.entities[id].ai.blocking.path_through_units = false;
        }
        if matches!(phase, Phase::Return | Phase::ReturnDead) {
            self.entities[id].ai.motion.end_move();
        }
        if matches!(phase, Phase::Takeoff | Phase::Landing) && runway {
            self.set_aircraft_afterburner(id, false);
        }
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.set(TAKEOFF | LANDING | TAXI | CIRCLING, false);
        state.phase = Phase::Idle;
        self.entities[id].ai.ignored_producer = None;
    }
}

const fn movement_result(result: &MoveResult) -> Option<bool> {
    match result {
        MoveResult::Continue => None,
        MoveResult::Success => Some(true),
        MoveResult::Failure => Some(false),
    }
}
impl Skirmish {
    pub(in crate::skirmish) fn jet_taxi_state(&self, id: usize) -> bool {
        matches!(self.entities[id].ai.command.main_state, MainState::Jet)
            && self.entities[id].aircraft.as_ref().is_some_and(|state| {
                matches!(
                    state.phase,
                    Phase::TaxiFromHangar | Phase::TaxiFromLanding | Phase::TaxiTakeoff
                )
            })
    }
    pub(in crate::skirmish) fn jet_follow_path_index(&self, id: usize) -> Option<usize> {
        if !matches!(self.entities[id].ai.command.main_state, MainState::Jet) {
            return None;
        }
        let state = self.entities[id].aircraft.as_ref()?;
        if !self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()?
            .needs_runway
        {
            return None;
        }
        match state.phase {
            Phase::Takeoff => Some(2),
            Phase::Landing => Some(3),
            _ => None,
        }
    }
}
