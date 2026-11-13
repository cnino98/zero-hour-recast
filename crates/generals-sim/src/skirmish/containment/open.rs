//! `OpenContain` constructor, update and transform notification, SH 2bfaea60.
use super::Skirmish;

impl Skirmish {
    // ContainModuleInterface::isValidContainerFor is distinct from the
    // ActionManager checks used by a player enter command.
    pub(in crate::skirmish) fn native_container_admits(
        &self,
        mut rider: usize,
        carrier: usize,
        capacity: bool,
    ) -> bool {
        let unit = &self.catalog[self.entities[carrier].type_index];
        let family = unit.contain_family().expect("authored container interface");
        if unit.is_helix_container()
            && self.entities[carrier].attachment_child().is_none()
            && self.catalog[self.entities[rider].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "PORTABLE_STRUCTURE")
        {
            return true;
        }
        if family == "OverlordContain" {
            let redirect = self.redirected_container(carrier);
            if redirect != carrier {
                return self.native_container_admits(rider, redirect, capacity);
            }
        }
        let transport = matches!(
            family,
            "TransportContain"
                | "InternetHackContain"
                | "HelixContain"
                | "OverlordContain"
                | "RiderChangeContain"
        );
        if transport
            && self.entities[rider].parachute.is_some()
            && let Some(&first) = self.passengers(rider).first()
        {
            rider = first;
        }
        let actor = &self.entities[rider];
        let candidate = &self.catalog[actor.type_index];
        if self.is_tunnel(carrier) {
            return !candidate.kind_of.iter().any(|kind| kind == "AIRCRAFT")
                && (!capacity
                    || self.passengers(carrier).len()
                        < usize::try_from(unit.contain.as_ref().expect("Tunnel data").capacity)
                            .expect("native tunnel capacity"));
        }
        let host = &self.entities[carrier];
        let rules = unit.contain.as_ref().expect("container data");
        if !(rules.allowed.is_empty()
            || rules
                .allowed
                .iter()
                .any(|kind| kind == "ALL" || candidate.kind_of.contains(kind)))
            || rules
                .forbidden
                .iter()
                .any(|kind| kind == "ALL" || candidate.kind_of.contains(kind))
            || !self.container_relationship(rider, carrier, rules)
        {
            return false;
        }
        if family == "GarrisonContain" {
            return host.health > 0.0
                && self.garrison_damage_allows_entry(carrier)
                && !candidate.kind_of.iter().any(|kind| kind == "NO_GARRISON")
                && (!capacity
                    || self.passengers(carrier).len()
                        < usize::try_from(rules.capacity).unwrap_or(usize::MAX));
        }
        if family == "ParachuteContain" {
            return self.passengers(carrier).is_empty()
                && (candidate.transport_slots != 0
                    || candidate
                        .kind_of
                        .iter()
                        .any(|kind| matches!(kind.as_str(), "INFANTRY" | "PARACHUTABLE")));
        }
        if !transport {
            return true;
        }
        if actor.owner != host.owner || candidate.transport_slots == 0 {
            return false;
        }
        if family == "RiderChangeContain" {
            return self.bike_admits_rider(carrier, rider);
        }
        if !capacity {
            return true;
        }
        let count = i32::from_ne_bytes(
            u32::try_from(self.passengers(carrier).len())
                .expect("native contain count")
                .to_ne_bytes(),
        );
        let slots = i32::from_ne_bytes(candidate.transport_slots.to_ne_bytes());
        self.entities[carrier]
            .containment
            .extra_slots_in_use
            .wrapping_add(count)
            .wrapping_add(slots)
            <= i32::from_ne_bytes(rules.capacity.to_ne_bytes())
    }

    pub(in crate::skirmish) fn construct_open_contain(&mut self, id: usize) {
        let allowed = self.catalog[self.entities[id].type_index]
            .contain
            .as_ref()
            .expect("OpenContain data")
            .weapons
            .fire;
        let state = &mut self.entities[id].containment;
        state.firing.allowed = allowed;
        state.passengers.clear();
        state.waiting.clear();
        state.player_entered_mask = 0;
        state.audio.frames = [0; 2];
        state.door_close_countdown = 0;
        state.door_flags = 0;
        state.drawable_conditions.clear();
        state.next_exit = 0;
        state.audio.enabled = true;
    }

    pub(in crate::skirmish) fn construct_transport_contain(&mut self, id: usize) {
        self.construct_open_contain(id);
        self.entities[id].containment.exit_ready = 0;
        self.entities[id].containment.extra_slots_in_use = 0;
        self.entities[id].containment.payload_created = false;
    }

    pub(in crate::skirmish) fn update_transport_contain(&mut self, id: usize) -> i32 {
        if !self.entities[id].containment.payload_created
            && !self.update_objects[self.entities[id].update_owner].destroy_requested
        {
            self.create_transport_payload(id);
        }
        let percent = self.catalog[self.entities[id].type_index]
            .contain
            .as_ref()
            .expect("TransportContain data")
            .regen_percent;
        if percent != 0.0 {
            for rider in self.passengers(id).to_vec() {
                let actor = &self.entities[rider];
                if actor.health < actor.max_health {
                    let amount = actor.max_health * percent / 100.0 * super::super::STEP_SECONDS;
                    self.apply_healing(rider, Some(id), amount);
                }
            }
        }
        self.update_open_contain(id)
    }

    pub(in crate::skirmish) fn create_contain_template_payload(&mut self, id: usize) {
        let count = self.catalog[self.entities[id].type_index]
            .contain
            .as_ref()
            .expect("Contain data")
            .payload_templates
            .len();
        self.entities[id].containment.audio.enabled = false;
        for index in 0..count {
            let name = &self.catalog[self.entities[id].type_index]
                .contain
                .as_ref()
                .expect("Contain data")
                .payload_templates[index];
            // Native missing template pointers are ignored in this vector path.
            let Some(kind) = self.catalog.iter().position(|unit| &unit.name == name) else {
                continue;
            };
            let rider = self.new_object(kind, self.entities[id].owner, true);
            if self.native_container_admits(rider, id, true) {
                self.add_to_contain(rider, id);
            }
        }
        self.entities[id].containment.audio.enabled = true;
        self.entities[id].containment.payload_created = true;
    }

    pub(in crate::skirmish) fn update_helix_contain(&mut self, id: usize) -> i32 {
        if let Some(portable) = self.entities[id]
            .attachment_child()
            .filter(|&child| self.entities[child].exists())
        {
            let position = self.object_position(id);
            self.set_object_position(portable, position);
            let orientation = self.entities[id].facing;
            self.set_object_orientation(portable, orientation);
        }
        self.update_transport_contain(id)
    }

    pub(in crate::skirmish) fn update_open_contain(&mut self, id: usize) -> i32 {
        self.entities[id].containment.player_entered_mask = 0;
        let actor = &self.entities[id];
        if actor.has_drawable() {
            let thresholds = self.catalog[actor.type_index]
                .contain
                .as_ref()
                .expect("OpenContain data")
                .damage_thresholds;
            let conditions = self.model_flags(actor, thresholds);
            if actor.containment.drawable_conditions != conditions {
                self.deploy_passengers(id);
                self.entities[id].containment.drawable_conditions = conditions;
            }
        }
        let state = &mut self.entities[id].containment;
        if state.door_close_countdown != 0 {
            state.door_close_countdown -= 1;
            if state.door_close_countdown == 0 {
                state.door_flags = (state.door_flags & !1) | 2;
            }
        }
        let dead = self.entities[id]
            .containment
            .waiting
            .iter()
            .copied()
            .filter(|&rider| {
                self.entities
                    .get(rider)
                    .is_none_or(|actor| !actor.exists() || actor.is_effectively_dead())
            })
            .collect::<Vec<_>>();
        for rider in dead {
            self.entities[id].containment.waiting.remove(&rider);
        }
        1
    }

    pub(in crate::skirmish) fn containment_transform_changed(&mut self, id: usize) {
        if self.entities[id].parachute.is_some() {
            self.parachute_transform_changed(id);
        } else if self.catalog[self.entities[id].type_index].contain.is_some() {
            self.deploy_passengers(id);
        }
    }

    pub(in crate::skirmish::containment) fn open_container_exit_door(&mut self, id: usize) {
        let frames = self.catalog[self.entities[id].type_index]
            .contain
            .as_ref()
            .expect("OpenContain data")
            .door_frames;
        let state = &mut self.entities[id].containment;
        state.door_close_countdown = frames;
        if frames != 0 {
            state.door_flags = (state.door_flags & !2) | 1;
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn set_safe_occlusion_frame(&mut self, id: usize, frame: u32) {
        self.entities[id].drawable.safe_occlusion_frame = frame;
    }

    pub(in crate::skirmish) fn initialize_safe_occlusion(&mut self, id: usize) {
        let delay = self.catalog[self.entities[id].type_index].occlusion_delay;
        self.set_safe_occlusion_frame(id, self.update_frame().wrapping_add(delay));
    }
}
