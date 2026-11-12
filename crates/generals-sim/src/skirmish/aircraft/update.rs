use super::{ALLOW_AIR, Phase, RESUME_RELOAD, SPECIAL_RETURN};
use crate::skirmish::{Order, Skirmish, locomotor};

impl Skirmish {
    /// JetAIUpdate.cpp1991: the derived callback precedes the common AI machine,
    /// including callbacks after its late transition into `AI_DEAD`.
    pub(in crate::skirmish) fn update_jet_ai_wrapper(&mut self, id: usize) {
        if self.entities[id].aircraft.is_none() {
            return;
        }
        self.initialize_aircraft_producer(id);
        self.aircraft_idle_update(id);
        self.update_aircraft_presentation(id);
        self.update_aircraft_attack_locomotor(id);
        self.update_aircraft_lockon(id);
        let state = self.entities[id].aircraft.as_ref().expect("aircraft state");
        if state.attack_until != 0 || state.flag(SPECIAL_RETURN) {
            self.choose_locomotor_set(id, locomotor::set::NORMAL);
        }
        self.update_aircraft_engine(id);
    }

    pub(in crate::skirmish) fn update_aircraft(&mut self, id: usize) -> bool {
        if self.entities[id].aircraft.is_none() {
            return false;
        }
        if self.update_move_aside(id) {
            return true;
        }
        let phase = self.entities[id]
            .aircraft
            .as_ref()
            .expect("aircraft state")
            .phase;
        let revision = self.entities[id].ai.command_revision;
        let result = if self.entities[id].is_effectively_dead() {
            Some(false)
        } else {
            match phase {
                Phase::Idle => return false,
                Phase::AwaitTakeoff | Phase::AwaitLanding => {
                    self.await_aircraft_runway(id, phase == Phase::AwaitLanding)
                }
                Phase::TaxiFromHangar
                | Phase::TaxiFromLanding
                | Phase::TaxiTakeoff
                | Phase::Return
                | Phase::ReturnDead => match self.update_internal_move(id) {
                    crate::skirmish::ground_movement::MoveResult::Continue => None,
                    crate::skirmish::ground_movement::MoveResult::Success => Some(true),
                    crate::skirmish::ground_movement::MoveResult::Failure => Some(false),
                },
                Phase::Pause => self.pause_aircraft_takeoff(id).then_some(true),
                Phase::Takeoff | Phase::Landing => {
                    self.update_aircraft_flight_state(id, phase == Phase::Landing)
                }
                Phase::Orient => self.orient_aircraft(id),
                Phase::Reload => {
                    if self.aircraft_producer(id).is_none() {
                        Some(false)
                    } else {
                        self.reload_aircraft(id).then_some(true)
                    }
                }
                Phase::CircleDead => self.circle_dead_airfield(id),
            }
        };
        if self.entities[id].ai.command_revision != revision {
            return true;
        }
        if let Some(success) = result {
            let next = self.next_aircraft_phase(id, phase, success);
            self.enter_aircraft_phase(id, next);
        }
        true
    }

    fn aircraft_idle_update(&mut self, id: usize) {
        let empty = self.aircraft_out_of_special_ammo(id);
        let actor = &self.entities[id];
        let state = actor.aircraft.as_ref().expect("aircraft state");
        let idle = matches!(
            actor.ai.command.main_state,
            crate::skirmish::ai_goals::MainState::Jet
        ) && state.phase == Phase::Reload
            || matches!(
                actor.ai.command.main_state,
                crate::skirmish::ai_goals::MainState::Active
            ) && matches!(actor.ai.order, Order::Idle);
        let rules = self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules");
        let airborne = state.flag(ALLOW_AIR);
        let resume = state.flag(RESUME_RELOAD);
        let idle_time = rules.idle_return;
        let healed_heli = idle
            && !airborne
            && state.pending.is_none()
            && rules.helipad
            && actor.health.partial_cmp(&actor.max_health) == Some(std::cmp::Ordering::Equal);
        if let Some(producer) = self.aircraft_producer(id) {
            self.parking_set_healee(producer, id, idle && !airborne && !healed_heli);
            if healed_heli {
                self.entities[id]
                    .aircraft
                    .as_mut()
                    .unwrap()
                    .set(ALLOW_AIR, true);
                self.clear_ai_goal(id);
                self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
                self.enter_aircraft_phase(id, Phase::AwaitTakeoff);
                if let Some(rally) = self.entities[producer].rally_point {
                    self.native_move_from(id, rally, crate::skirmish::ai_goals::COMMAND_FROM_AI);
                }
            }
        }
        let state = self.entities[id].aircraft.as_ref().expect("aircraft state");
        let pending = state.pending.clone();
        let return_at = state.return_at;
        let reload = state.phase == Phase::Reload;
        let airborne = state.flag(ALLOW_AIR);
        if idle {
            if empty
                && self.entities[id]
                    .aircraft
                    .as_ref()
                    .expect("aircraft state")
                    .flag(ALLOW_AIR)
            {
                self.prune_attack_targeters(id);
                let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
                state.return_at = 0;
                state.set(SPECIAL_RETURN, true);
                self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
                self.enter_aircraft_phase(id, Phase::Return);
            } else if let Some(order) = pending
                && !reload
            {
                let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
                state.return_at = 0;
                state.pending = None;
                self.replay_stored_command(id, order);
            } else if return_at != 0 && self.update_frame() >= return_at && airborne {
                let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
                state.return_at = 0;
                state.set(SPECIAL_RETURN, false);
                self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
                self.enter_aircraft_phase(id, Phase::Return);
            } else if return_at == 0 && idle_time > 0 && airborne {
                self.entities[id]
                    .aircraft
                    .as_mut()
                    .expect("aircraft state")
                    .return_at = self.update_frame().wrapping_add(idle_time);
            }
        } else {
            let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
            state.return_at = 0;
            if resume && empty && airborne {
                state.set(SPECIAL_RETURN, true);
                state.set(RESUME_RELOAD, false);
                state.pending = state.recent.clone();
                self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
                self.enter_aircraft_phase(id, Phase::Return);
            }
        }
    }

    fn await_aircraft_runway(&mut self, id: usize, landing: bool) -> Option<bool> {
        let Some(producer) = self.aircraft_producer(id) else {
            return Some(false);
        };
        if self.aircraft_parking(id).is_none() {
            return Some(false);
        }
        self.parking_reserve_runway(producer, id, landing)
            .then_some(true)
    }

    fn pause_aircraft_takeoff(&mut self, id: usize) -> bool {
        let goal = self.entities[id]
            .aircraft
            .as_ref()
            .expect("aircraft state")
            .endpoint;
        let position = self.entities[id].position;
        let angle = crate::skirmish::combat::relative_angle(
            position,
            self.entities[id].facing,
            [goal[0], goal[1]],
        );
        let oriented = angle.abs() < 0.035;
        if !oriented {
            if self.entities[id].aircraft.as_ref().unwrap().pause.can_turn {
                let desired = self.entities[id].facing + angle;
                self.entities[id].ai.motion.request_angle(desired);
            } else {
                self.aircraft_explicit_flight_goal(id, goal);
            }
        }
        let waiter = self.aircraft_takeoff_waiter(id);
        let frame = self.update_frame();
        let duration = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules")
            .takeoff_pause;
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        if let Some(other) = waiter {
            if state.pause.waited_for.is_none() {
                state.pause.waited_for = Some(other);
            }
            if state.pause.waited_for == Some(other) {
                return false;
            }
        }
        if state.pause.takeoff == 0 {
            state.pause.transfer = if state.pause.waited_for.is_none() {
                state.pause.waited_for = Some(id);
                frame.wrapping_add(1)
            } else {
                frame.wrapping_add(2)
            };
            state.pause.takeoff = state.pause.transfer.max(frame.wrapping_add(duration));
            self.set_aircraft_afterburner(id, true);
            return false;
        }
        if frame >= state.pause.transfer {
            state.pause.transfer = u32::MAX;
            if let Some(producer) = self.aircraft_producer(id) {
                self.parking_transfer_runway(producer, id);
            }
        }
        frame
            >= self.entities[id]
                .aircraft
                .as_ref()
                .expect("aircraft state")
                .pause
                .takeoff
            && oriented
    }

    fn aircraft_takeoff_waiter(&mut self, id: usize) -> Option<usize> {
        let producer = self.aircraft_producer(id)?;
        self.purge_parking(producer);
        let state = self.entities[producer].parking.as_ref()?;
        let own = state.runway_index(id)?;
        let count = state.runway_count();
        (0..count).filter(|&index| index != own).find_map(|index| {
            let other = state.runway_owner(index)?;
            self.entities
                .get(other)?
                .aircraft
                .as_ref()
                .filter(|state| state.phase == Phase::TaxiTakeoff)
                .map(|_| other)
        })
    }

    fn orient_aircraft(&mut self, id: usize) -> Option<bool> {
        let Some(info) = self.aircraft_parking(id) else {
            return Some(false);
        };
        let angle = info.parking.orientation();
        if locomotor::normalize_angle(self.entities[id].facing - angle).abs() <= 0.001 {
            return Some(true);
        }
        self.entities[id].scrub_physics_velocity_2d(0.0);
        let height = self.object_position(id)[2];
        self.set_object_position(
            id,
            [info.parking.position[0], info.parking.position[1], height],
        );
        self.face_aircraft(id, angle, 0.001);
        None
    }

    fn update_aircraft_flight_state(&mut self, id: usize, landing: bool) -> Option<bool> {
        let runway = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules")
            .needs_runway;
        if !runway {
            return self.update_helicopter_landing(id, landing);
        }
        if landing {
            self.set_flight_lift_limit(id, 99_999.0);
            self.aircraft_landing_sound(id);
        } else {
            let Some(producer) = self.aircraft_producer(id) else {
                return Some(false);
            };
            self.parking_transfer_runway(producer, id);
            let Some(info) = self.aircraft_parking(id) else {
                return Some(false);
            };
            let position = self.object_position(id);
            let delta = std::array::from_fn::<_, 3, _>(|axis| info.end[axis] - position[axis]);
            let xx = delta[0] * delta[0];
            let yy = delta[1] * delta[1];
            let zz = delta[2] * delta[2];
            let distance = ((xx + yy) + zz).sqrt();
            let ratio = 1.0 - distance / info.takeoff_distance;
            self.limit_aircraft_lift(id, (ratio * ratio).clamp(0.0, 1.0));
        }
        match self.update_follow_path_state(id) {
            crate::skirmish::ground_movement::MoveResult::Continue => None,
            crate::skirmish::ground_movement::MoveResult::Success => Some(true),
            crate::skirmish::ground_movement::MoveResult::Failure => Some(false),
        }
    }

    fn update_helicopter_landing(&mut self, id: usize, landing: bool) -> Option<bool> {
        let state = self.entities[id].aircraft.as_ref().expect("aircraft state");
        let index = state.heli_index;
        let mut point = state.heli_points[index];
        let helipad = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules")
            .helipad;
        if helipad || !landing {
            if let Some(adjusted) = self.adjusted_destination_world(id, point) {
                point = adjusted;
            }
            self.entities[id].aircraft.as_mut().unwrap().heli_points[index] = point;
            self.reserve_goal_on(id, [point[0], point[1]], 1);
        }
        self.aircraft_explicit_flight_goal(id, point);
        let position = self.object_position(id);
        let delta = std::array::from_fn::<_, 3, _>(|axis| position[axis] - point[axis]);
        let xx = delta[0] * delta[0];
        let yy = delta[1] * delta[1];
        let zz = delta[2] * delta[2];
        if (xx + yy) + zz <= 9.0 {
            let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
            state.heli_index += 1;
            if state.heli_index == 2 {
                return Some(true);
            }
        }
        None
    }

    fn circle_dead_airfield(&mut self, id: usize) -> Option<bool> {
        self.entities[id].ai.motion.clear_goal();
        self.dead_aircraft_goal(id);
        let actor = &self.entities[id];
        let percent = self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("aircraft rules")
            .out_of_ammo_damage;
        let amount = (percent * (1.0 / 30.0)) * actor.max_health;
        if amount > 0.0 {
            self.apply_native_damage(id, None, amount, crate::skirmish::DeathType::Normal);
        }
        if !self.entities[id].alive() {
            return None;
        }
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.check_airfield -= 1;
        if state.check_airfield == 0 {
            state.check_airfield = 30;
            if let Some(producer) = self.find_airfield(id) {
                self.entities[id].producer = Some(producer);
                return Some(true);
            }
        }
        None
    }
}
