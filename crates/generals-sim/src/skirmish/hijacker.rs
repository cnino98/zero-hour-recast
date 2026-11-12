//! SH `ConvertToHijackedVehicleCrateCollide` and authored `HijackerUpdate` callbacks.
use super::{Entity, Skirmish};

#[cfg(test)]
mod tests;

pub struct Rules {
    pub required: Vec<String>,
    pub forbidden: Vec<String>,
    pub pickup_science: Option<String>,
}

pub struct UpdateRules {
    // Parsed by the source module but never consumed by HijackerUpdate.
    pub attach_bone: String,
    pub parachute: String,
}

pub struct Event {
    pub frame: u64,
    pub source: usize,
    pub target: usize,
    pub victim: usize,
    pub position: [f32; 3],
}

pub(super) struct State {
    target: Option<usize>,
    enabled: bool,
    in_vehicle: bool,
    was_airborne: bool,
    eject: [f32; 3],
}

impl Entity {
    #[must_use]
    pub fn driving_vehicle(&self) -> Option<usize> {
        self.crew.hijackers.iter().find_map(|state| state.target)
    }
}

impl Skirmish {
    fn hijacker_host_airborne(&self, target: usize) -> bool {
        self.height_above_ground(target) > -9.0 * self.gravity
    }
    #[must_use]
    pub fn can_hijack(&self, id: usize, target: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            actor.complete()
                && !actor.masked()
                && !actor.hacked()
                && !actor.paralyzed()
                && actor.contained_by().is_none()
                && self.maintenance_target_visible(actor.owner, target)
                && self.hijacker_accepts(id, target)
        })
    }

    /// Uses `MSG_ENTER`; conversion only executes against this explicit goal.
    /// # Errors
    /// Rejects unavailable drivers and hidden or ineligible enemy vehicles.
    pub fn hijack(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_hijack(id, target) {
            return Err("Choose a visible enemy vehicle that can be hijacked");
        }
        self.begin_container_enter(id, target);
        Ok(())
    }

    pub(super) fn hijacker_accepts(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(host)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        let Some(rules) = &self.catalog[actor.type_index].hijacker else {
            return false;
        };
        let unit = &self.catalog[host.type_index];
        id != target
            && actor.alive()
            && !self.update_objects[actor.update_owner].destroy_requested
            && !actor.masked()
            && host.alive()
            && !host.is_effectively_dead()
            && host.ai.registration.is_some()
            && !host.masked()
            && host.owner != self.neutral_owner()
            && self.enemies(actor.owner, host.owner)
            && !host.hijacked()
            && self.height_above_ground(id) <= 0.0
            && unit.kind_of.iter().any(|kind| kind == "VEHICLE")
            && !unit.kind_of.iter().any(|kind| {
                matches!(
                    kind.as_str(),
                    "AIRCRAFT" | "BOAT" | "DRONE" | "PARACHUTE" | "IMMUNE_TO_CAPTURE"
                )
            })
            && (!unit.kind_of.iter().any(|kind| kind == "TRANSPORT") || !host.loaded())
            && rules
                .required
                .iter()
                .all(|kind| unit.kind_of.contains(kind))
            && !rules
                .forbidden
                .iter()
                .any(|kind| unit.kind_of.contains(kind))
            && rules
                .pickup_science
                .as_ref()
                .is_none_or(|science| self.players[host.owner].has_science(science))
    }

    pub(super) fn hijacker_contact(&mut self, id: usize, target: usize) -> bool {
        if (self.entities[id].ai.registration.is_some()
            && self.entities[id].ai.command.goal_object != Some(target))
            || !self.hijacker_accepts(id, target)
        {
            return false;
        }
        let owner = self.entities[id].owner;
        let victim = self.entities[target].owner;
        let level = self.entities[id]
            .veterancy
            .max(self.entities[target].veterancy);
        let keeps_driver = self.catalog[self.entities[target].type_index].has_eject_pilot();
        self.presentation.hijacker_events.push(Event {
            frame: self.tick,
            source: id,
            target,
            victim,
            position: self.object_position(target),
        });
        self.transfer_maintenance_owner(target, owner);
        self.entities[target].crew.hijacked = true;
        self.construction_move(target, self.object_position(target));
        self.ai_idle_from_ai(target);
        self.cancel_all_dozer_tasks(target);
        self.object_sound(
            id,
            "HijackDriver".into(),
            super::object_sounds::Action::Play,
        );
        self.transfer_crew_name(id, target);
        self.set_hijacker_veterancy(id, level, false);
        self.set_hijacker_veterancy(target, level, false);
        if !keeps_driver {
            self.destroy_maintenance_object(id);
            return true;
        }
        if let Some(state) = self.entities[id].crew.hijackers.first_mut() {
            state.target = Some(target);
            state.in_vehicle = true;
            state.enabled = true;
            self.entities[id].object_status |= 1 << 5;
            self.entities[id].maintenance.masked = true;
            self.entities[id].maintenance.instance_flags |=
                super::maintenance::INSTANCE_UNSELECTABLE;
        }
        self.entities[id].ai.command.ai_group = None;
        if self.entities[id].ai.registration.is_some() {
            self.ai_idle_from_ai(id);
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        self.entities[target].crew.sight =
            Some([actor.vision_range(unit), actor.shroud_range(unit)]);
        self.partition.remove(id);
        self.unregister_object_obstacle(id);
        self.set_drawable_hidden(id, true);
        self.update_position_claim(id);
        self.observe_ground_partition(id);
        self.observe_visibility(id);
        self.observe_visibility(target);
        true
    }

    pub(in crate::skirmish) fn construct_hijacker(&mut self, id: usize, ordinal: usize) {
        let states = &mut self.entities[id].crew.hijackers;
        assert_eq!(states.len(), ordinal);
        states.push(State {
            target: None,
            enabled: false,
            in_vehicle: false,
            was_airborne: false,
            eject: [0.0; 3],
        });
    }

    pub(in crate::skirmish) fn update_hijacker(&mut self, id: usize, ordinal: usize) -> i32 {
        let state = &self.entities[id].crew.hijackers[ordinal];
        if !state.enabled {
            return 1;
        }
        if !state.in_vehicle {
            self.entities[id].crew.hijackers[ordinal].was_airborne = false;
            return 1;
        }
        // findObjectByID includes hidden vehicles and zero-health corpses until deletion.
        if let Some(target) = state
            .target
            .filter(|&target| self.entities[target].exists())
        {
            let position = self.object_position(target);
            self.set_object_position(id, position);
            let airborne = self.hijacker_host_airborne(target);
            let state = &mut self.entities[id].crew.hijackers[ordinal];
            state.was_airborne = airborne;
            state.eject = position;
            let level = self.entities[id]
                .veterancy
                .max(self.entities[target].veterancy);
            self.set_hijacker_veterancy(id, level, true);
            self.set_hijacker_veterancy(target, level, true);
            return 1;
        }
        let eject = state.was_airborne.then_some(state.eject);
        self.partition
            .observe(id, self.object_position(id), self.entities[id].facing);
        self.set_drawable_hidden(id, false);
        self.entities[id].object_status &= !(1 << 5);
        self.entities[id].maintenance.masked = false;
        self.entities[id].maintenance.instance_flags &= !super::maintenance::INSTANCE_UNSELECTABLE;
        if self.entities[id].ai.registration.is_some() {
            self.ai_idle_from_ai(id);
        }
        self.update_position_claim(id);
        self.restore_object_obstacle(id);
        self.observe_ground_partition(id);
        self.observe_visibility(id);
        if let Some(position) = eject {
            self.eject_hijacker(id, ordinal, position);
        }
        let state = &mut self.entities[id].crew.hijackers[ordinal];
        state.target = None;
        state.in_vehicle = false;
        state.enabled = false;
        state.was_airborne = false;
        1
    }
}
