use super::{Info, State};
use crate::skirmish::{Skirmish, aircraft, setup};

impl Skirmish {
    pub(in crate::skirmish) fn construct_parking(&mut self, id: usize) {
        self.entities[id].parking = Some(State::new());
    }

    pub(in crate::skirmish) fn parking_set_healee(
        &mut self,
        producer: usize,
        id: usize,
        add: bool,
    ) {
        self.entities[producer]
            .parking
            .as_mut()
            .expect("ParkingPlace constructor")
            .heal(id, add, self.tick);
    }

    pub(in crate::skirmish) fn build_parking_info(&mut self, id: usize) {
        let actor = &self.entities[id];
        if actor.under_construction()
            || actor.parking.as_ref().is_none_or(|state| state.got_info)
            || actor.selling()
        {
            return;
        }
        let Some(rules) = &self.catalog[actor.type_index].parking else {
            return;
        };
        let rows = rules.rows;
        let columns = rules.columns;
        let has_runways = rules.has_runways;
        let mut spaces = Vec::new();
        for row in 1..=rows {
            for column in 1..=columns {
                spaces.push(super::Space {
                    geometry: super::SpaceRules {
                        hangar: self
                            .parking_logical_pose(id, &format!("Runway{column}Park{row}Han")),
                        parking: self
                            .parking_logical_pose(id, &format!("Runway{column}Parking{row}")),
                        prep: self
                            .parking_logical_pose(id, &format!("Runway{column}Prep{row}"))
                            .position,
                        runway: usize::from(column - 1),
                    },
                    object: None,
                    reserved: false,
                    postponed: false,
                });
                if self.catalog[self.entities[id].type_index]
                    .production
                    .is_some()
                {
                    self.entities[id]
                        .production_doors
                        .set_hold(spaces.len() - 1, false, self.tick);
                }
            }
        }
        let mut runways = Vec::new();
        if has_runways {
            for column in 1..=columns {
                runways.push(super::Runway {
                    geometry: super::RunwayRules {
                        start: self
                            .parking_logical_pose(id, &format!("RunwayStart{column}"))
                            .position,
                        end: self
                            .parking_logical_pose(id, &format!("RunwayEnd{column}"))
                            .position,
                    },
                    object: None,
                    next: None,
                    was_in_line: false,
                });
            }
        }
        let state = self.entities[id]
            .parking
            .as_mut()
            .expect("ParkingPlace constructor");
        state.spaces = spaces;
        state.runways = runways;
        state.got_info = true;
    }

    pub(in crate::skirmish) fn purge_parking(&mut self, id: usize) {
        self.build_parking_info(id);
        let Some(mut state) = self.entities[id].parking.take() else {
            return;
        };
        let mut doors = std::mem::replace(
            &mut self.entities[id].production_doors,
            super::doors::State::new(),
        );
        let production = self.catalog[self.entities[id].type_index]
            .production
            .is_some();
        state.purge(self.tick, production.then_some(&mut doors), |object| {
            self.entities
                .get(object)
                .is_some_and(|actor| actor.exists() && !actor.is_effectively_dead())
        });
        self.entities[id].production_doors = doors;
        self.entities[id].parking = Some(state);
    }

    pub(in crate::skirmish) fn reserve_parking_space(
        &mut self,
        producer: usize,
        id: usize,
    ) -> Option<usize> {
        self.purge_parking(producer);
        let actor = &mut self.entities[producer];
        let doors = self.catalog[actor.type_index]
            .production
            .is_some()
            .then_some(&mut actor.production_doors);
        let index = actor
            .parking
            .as_mut()?
            .reserve_space(id, self.tick, doors)?;
        if self.catalog[actor.type_index].parking.as_ref()?.deck_height != 0.0 {
            self.entities[id].object_status |= 1 << 29;
        }
        Some(index)
    }

    pub(in crate::skirmish) fn release_parking_space(&mut self, producer: usize, id: usize) {
        self.purge_parking(producer);
        let actor = &mut self.entities[producer];
        if let Some(state) = &mut actor.parking {
            let doors = self.catalog[actor.type_index]
                .production
                .is_some()
                .then_some(&mut actor.production_doors);
            state.release_space(id, doors);
        }
        if let Some(actor) = self.entities.get_mut(id).filter(|actor| actor.exists()) {
            actor.object_status &= !(1 << 29);
        }
    }

    /// Native read-only capacity check for command buttons, including dead occupants.
    #[must_use]
    pub fn parking_available(&self, producer: usize, type_index: usize) -> bool {
        let Some(actor) = self.entities.get(producer) else {
            return false;
        };
        let Some(unit) = self.catalog.get(type_index) else {
            return false;
        };
        let helipad = unit
            .kind_of
            .iter()
            .any(|kind| kind == "PRODUCED_AT_HELIPAD");
        actor.parking.as_ref().is_some_and(|state| {
            state.available(helipad, |object| {
                self.entities
                    .get(object)
                    .is_some_and(|actor| actor.exists() && !actor.is_effectively_dead())
            })
        })
    }

    pub(in crate::skirmish) fn queue_parking_door(
        &mut self,
        producer: usize,
        type_index: usize,
    ) -> Result<Option<usize>, &'static str> {
        if self.catalog[self.entities[producer].type_index]
            .parking
            .is_none()
        {
            return Ok(None);
        }
        // ParkingPlaceBehavior checks the object's kind flag, independently
        // of whether its AI is JetAIUpdate or ChinookAIUpdate.
        let helicopter = self.catalog[type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "PRODUCED_AT_HELIPAD");
        self.build_parking_info(producer);
        self.purge_parking(producer);
        self.entities[producer]
            .parking
            .as_mut()
            .ok_or("Airfield parking is unavailable")?
            .reserve_door(helicopter)
    }

    pub(in crate::skirmish) fn parking_reserve_runway(
        &mut self,
        producer: usize,
        id: usize,
        landing: bool,
    ) -> bool {
        self.purge_parking(producer);
        self.entities[producer]
            .parking
            .as_mut()
            .expect("ParkingPlace constructor")
            .reserve_runway(id, landing)
    }

    pub(in crate::skirmish) fn parking_release_runway(&mut self, producer: usize, id: usize) {
        self.purge_parking(producer);
        self.entities[producer]
            .parking
            .as_mut()
            .expect("ParkingPlace constructor")
            .release_runway(id);
    }

    pub(in crate::skirmish) fn parking_transfer_runway(&mut self, producer: usize, id: usize) {
        self.purge_parking(producer);
        self.entities[producer]
            .parking
            .as_mut()
            .expect("ParkingPlace constructor")
            .transfer_runway(id);
    }

    pub(in crate::skirmish) fn update_parking_module(&mut self, id: usize) -> i32 {
        self.build_parking_info(id);
        self.purge_parking(id);
        let actor = &mut self.entities[id];
        let state = actor.parking.as_mut().expect("ParkingPlace constructor");
        let rules = self.catalog[actor.type_index]
            .parking
            .as_ref()
            .expect("parking rules");
        let interval_amount = 6.0 * rules.heal_per_second;
        let amount = interval_amount * (1.0 / 30.0);
        let healees = state.healing(self.tick).to_vec();
        for target in healees {
            self.apply_healing(target, Some(id), amount);
        }
        1
    }

    pub(in crate::skirmish) fn aircraft_parking(&mut self, id: usize) -> Option<Info> {
        let producer = self.aircraft_producer(id)?;
        self.build_parking_info(producer);
        self.purge_parking(producer);
        let offset = self.catalog[self.entities[id].type_index]
            .aircraft
            .as_ref()?
            .parking_offset;
        self.reserve_parking_space(producer, id)?;
        let actor = &self.entities[producer];
        actor
            .parking
            .as_ref()?
            .info(id, self.catalog[actor.type_index].parking.as_ref()?, offset)
    }

    pub(in crate::skirmish) fn aircraft_producer(&self, id: usize) -> Option<usize> {
        let producer = self.entities[id].producer?;
        self.entities
            .get(producer)
            .filter(|actor| {
                actor.exists()
                    && !actor.is_effectively_dead()
                    && !actor.selling()
                    && self.attack_kind(producer, "FS_AIRFIELD")
                    && actor.parking.is_some()
            })
            .map(|_| producer)
    }

    pub(in crate::skirmish) fn find_airfield(&mut self, id: usize) -> Option<usize> {
        let actor = &self.entities[id];
        let origin = actor.position;
        let owner = actor.owner;
        // FASTER_GCO visits outward rings/COIs, checks distance before the
        // reserving filter, and finishes only the first successful ring.
        let mut nearest = None;
        let mut minimum = f32::MAX;
        let mut found_ring = None;
        for (ring, producer) in self.partition.outward_rings(origin) {
            if found_ring.is_some_and(|found| ring > found) {
                break;
            }
            let other = &self.entities[producer];
            let distance = super::super::movement::distance_squared(origin, other.position);
            if distance >= minimum
                || !other.complete()
                || other.selling()
                || !setup::allied(&self.slots, owner, other.owner)
                || !self.attack_kind(producer, "FS_AIRFIELD")
                || other.parking.is_none()
                || other.is_off_map() != self.entities[id].is_off_map()
            {
                continue;
            }
            self.build_parking_info(producer);
            self.purge_parking(producer);
            if self.reserve_parking_space(producer, id).is_some() {
                nearest = Some(producer);
                minimum = distance;
                found_ring = Some(ring);
            }
        }
        nearest
    }

    pub(in crate::skirmish) fn defect_parked_aircraft(
        &mut self,
        producer: usize,
        team: usize,
        detection_frames: u32,
    ) {
        let owner = self.team_membership.team_owner(team);
        self.build_parking_info(producer);
        self.purge_parking(producer);
        let Some(state) = &self.entities[producer].parking else {
            return;
        };
        let parked: Vec<_> = state
            .spaces
            .iter()
            .filter_map(|space| space.object)
            .collect();
        for id in parked {
            let actor = &self.entities[id];
            let transition = actor
                .aircraft
                .as_ref()
                .is_some_and(aircraft::State::transitioning);
            if self.height_above_ground(id) > 0.0 && !transition {
                if actor.owner != owner {
                    self.release_parking_space(producer, id);
                    if self.entities[id].producer == Some(producer) {
                        self.entities[id].producer = None;
                    }
                }
            } else {
                self.defect_to_team(id, team, detection_frames);
            }
        }
        self.purge_parking(producer);
    }

    pub(in crate::skirmish) fn parking_death(&mut self, producer: usize) {
        self.purge_parking(producer);
        let Some(state) = &self.entities[producer].parking else {
            return;
        };
        let parked: Vec<_> = state
            .spaces
            .iter()
            .filter_map(|space| space.object)
            .collect();
        for id in parked {
            let actor = &self.entities[id];
            let transitioning = actor
                .aircraft
                .as_ref()
                .is_some_and(aircraft::State::transitioning);
            if actor.exists()
                && !actor.is_effectively_dead()
                && (transitioning || self.height_above_ground(id) <= 0.0)
            {
                self.kill_object(id, super::super::DeathType::Normal);
            }
        }
        self.purge_parking(producer);
    }
}
