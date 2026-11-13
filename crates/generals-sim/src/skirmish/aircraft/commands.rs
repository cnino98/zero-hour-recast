use super::{ALLOW_AIR, LANDING, Phase, RESUME_RELOAD, SPECIAL_RETURN, TAKEOFF};
use crate::skirmish::{
    Skirmish,
    ai_dispatch::stored_command::{Command, Kind},
};

impl Skirmish {
    pub(in crate::skirmish::aircraft) fn aircraft_out_of_special_ammo(
        &mut self,
        id: usize,
    ) -> bool {
        let now = self.update_frame();
        let mut specials = 0;
        let mut empty = 0;
        for slot in 0..3 {
            if !self.allocated_weapon(id, slot).is_some_and(|weapon| {
                matches!(weapon.reload, crate::skirmish::ClipReload::ReturnToBase)
            }) {
                continue;
            }
            specials += 1;
            let actor = &mut self.entities[id];
            if matches!(
                actor.weapon_status(slot, &self.catalog[actor.type_index], now),
                Some(crate::skirmish::ai_dispatch::WeaponStatus::OutOfAmmo)
            ) {
                empty += 1;
            }
        }
        specials > 0 && empty == specials
    }

    pub(in crate::skirmish) fn notify_attack_victim_dead(&mut self, id: usize) {
        if self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()
            .is_some_and(|rules| rules.needs_runway)
            && let Some(state) = &mut self.entities[id].aircraft
        {
            state.return_at = crate::skirmish::ai_dispatch::native_frame(self.tick);
        }
    }

    pub(in crate::skirmish) fn jet_command(&mut self, id: usize, command: Command) -> bool {
        self.initialize_aircraft_producer(id);
        if !self.native_ai_command_allowed_for_sleep(
            id,
            matches!(command.kind, Kind::MoveEvenIfSleeping(_)),
        ) {
            return true;
        }
        self.entities[id].aircraft.as_mut().unwrap().recent = Some(command.clone());
        let state = self.entities[id].aircraft.as_ref().unwrap();
        if state.transitioning()
            || matches!(command.kind, Kind::Idle) && state.phase == Phase::Reload
        {
            self.entities[id].aircraft.as_mut().unwrap().pending = Some(command);
            return true;
        }
        let helipad = self.attack_kind(id, "PRODUCED_AT_HELIPAD");
        if matches!(command.kind, Kind::Idle) && self.entities[id].is_airborne_target() && !helipad
        {
            self.clear_ai_goal(id);
            self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
            self.enter_aircraft_phase(id, Phase::Return);
            return true;
        }
        if !state.flag(ALLOW_AIR)
            && !matches!(
                command.kind,
                Kind::Idle
                    | Kind::Busy
                    | Kind::FollowPath {
                        exit_production: true,
                        ..
                    }
            )
        {
            if matches!(
                command.kind,
                Kind::Attack { .. }
                    | Kind::AttackMove { .. }
                    | Kind::AttackArea(_)
                    | Kind::Guard(_)
                    | Kind::GuardFlying(_)
                    | Kind::GuardObject(_)
                    | Kind::GuardArea(_)
                    | Kind::Retaliate { .. }
                    | Kind::Hunt
            ) && self.aircraft_out_of_special_ammo(id)
            {
                self.entities[id].aircraft.as_mut().unwrap().pending = Some(command);
                return true;
            }
            if matches!(command.kind, Kind::Enter(target) | Kind::Repair(target) if self.aircraft_producer(id)==Some(target))
                && !helipad
            {
                return true;
            }
            self.entities[id].aircraft.as_mut().unwrap().pending = Some(command);
            self.clear_ai_goal(id);
            self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
            self.enter_aircraft_phase(id, Phase::AwaitTakeoff);
            return true;
        }
        let state = self.entities[id].aircraft.as_mut().unwrap();
        state.set(
            RESUME_RELOAD,
            matches!(
                command.kind,
                Kind::Guard(_)
                    | Kind::GuardFlying(_)
                    | Kind::GuardObject(_)
                    | Kind::GuardArea(_)
                    | Kind::Retaliate { .. }
                    | Kind::Hunt
            ),
        );
        state.pending = None;
        false
    }

    pub(in crate::skirmish) fn jet_exit_production_from(
        &mut self,
        id: usize,
        ignore: Option<usize>,
        source: i32,
    ) {
        self.clear_ai_goal(id);
        self.entities[id].ai.ignored_producer = ignore;
        self.set_last_command_source(id, source);
        let phase = if self.attack_kind(id, "PRODUCED_AT_HELIPAD") {
            Phase::AwaitTakeoff
        } else {
            Phase::TaxiFromHangar
        };
        self.enter_aircraft_phase(id, phase);
    }

    pub(in crate::skirmish::aircraft) fn initialize_aircraft_producer(&mut self, id: usize) {
        if self.entities[id]
            .aircraft
            .as_ref()
            .expect("aircraft state")
            .producer_position
            .is_some()
        {
            return;
        }
        let producer = self.entities[id].producer;
        let position = self.object_position(producer.unwrap_or(id));
        let parked = self
            .aircraft_producer(id)
            .and_then(|producer| self.entities[producer].parking.as_ref())
            .is_some_and(|state| state.space_index(id).is_some());
        let state = self.entities[id].aircraft.as_mut().expect("aircraft state");
        state.producer_position = Some(position);
        self.choose_aircraft_locomotor(id, !parked);
    }

    /// Native aircraft Enter/GetRepaired command, retaining the chosen airfield.
    /// # Errors
    /// Rejects unavailable aircraft and nonallied, sold or unfinished airfields.
    pub fn land_aircraft(&mut self, id: usize, airfield: usize) -> Result<(), &'static str> {
        let actor = self.ready_actor(id)?;
        self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .ok_or("Object has no aircraft AI")?;
        let target = self
            .entities
            .get(airfield)
            .ok_or("Airfield does not exist")?;
        if !target.complete()
            || target.selling()
            || self.catalog[target.type_index].parking.is_none()
            || !self.allied(actor.owner, target.owner)
        {
            return Err("Choose an operational allied airfield");
        }
        if self.native_subclass_command(
            id,
            Command {
                source: crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
                kind: Kind::Enter(airfield),
            },
        ) {
            return Ok(());
        }
        self.native_jet_landing_from(
            id,
            airfield,
            crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
            false,
        );
        Ok(())
    }

    pub(in crate::skirmish) fn native_jet_landing_from(
        &mut self,
        id: usize,
        airfield: usize,
        source: i32,
        repair: bool,
    ) {
        if self.entities[id].aircraft.as_ref().unwrap().flag(LANDING) {
            return;
        }
        if if repair {
            !self.chinook_repair_admits(id, airfield, source)
        } else {
            !self.action_enter_admits(id, airfield, source, false)
        } {
            return;
        }
        let helipad = self.attack_kind(id, "PRODUCED_AT_HELIPAD");
        let position = self.aircraft_landing_position(id, airfield);
        self.build_parking_info(airfield);
        self.purge_parking(airfield);
        if !helipad && self.reserve_parking_space(airfield, id).is_none() {
            return;
        }
        if let Some(old) = self.aircraft_producer(id)
            && old != airfield
        {
            self.release_parking_space(old, id);
        }
        self.entities[id].producer = Some(airfield);
        let state = self.entities[id]
            .aircraft
            .as_mut()
            .expect("Jet constructor");
        state.landing_position = position;
        state.set(SPECIAL_RETURN | RESUME_RELOAD, false);
        self.set_last_command_source(id, source);
        self.enter_aircraft_phase(id, Phase::Return);
    }
}

impl super::State {
    pub(in crate::skirmish) const fn transitioning(&self) -> bool {
        self.flag(TAKEOFF | LANDING)
    }
}
