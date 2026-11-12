use super::{Mission, Phase, Rules, Skirmish};
use crate::skirmish::{ai_goals::COMMAND_FROM_AI, locomotor, physical_ocl::transform};

impl Skirmish {
    /// Installs only delivery lists whose actual creation/AI/containment closure exists.
    /// # Errors
    /// Rejects missing transport, payload, parachute, Physics or AI consumers.
    pub fn configure_delivery_lists(&mut self, lists: Vec<Rules>) -> Result<(), &'static str> {
        if self.tick != 0 {
            return Err("Delivery lists must be installed before logic starts");
        }
        for list in &lists {
            let find = |name: &str| {
                self.catalog
                    .iter()
                    .find(|unit| unit.name == name)
                    .ok_or("Delivery template is missing")
            };
            let transport = find(&list.transport)?;
            if transport.deliver_payload.is_none() || transport.contain.is_none()
                || transport.live_physics.is_none() || transport.locomotor.flight.is_none()
                || !transport.update_modules.iter().any(|module| matches!(module, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "DeliverPayloadAIUpdate")) {
                return Err("Delivery transport lacks its actual native consumers");
            }
            if list.container.is_empty() || find(&list.container)?.parachute.is_none() {
                return Err("Pad delivery requires original ParachuteContain container");
            }
            for payload in &list.payload {
                if find(&payload.object)?.live_physics.is_none() {
                    return Err("Delivery payload lacks original physical body");
                }
            }
        }
        self.delivery_lists = lists;
        Ok(())
    }

    pub(in crate::skirmish) fn create_delivery(
        &mut self,
        source: usize,
        list: usize,
        mut start: [f32; 3],
    ) -> usize {
        let rules = self.delivery_lists[list].clone();
        let target = self.object_position(source);
        let orientation = (target[1] - start[1]).atan2(target[0] - start[0]);
        if rules.delivery_distance > 0.0 {
            let x_offset = orientation.cos() * rules.delivery_distance * 1.5;
            let y_offset = orientation.sin() * rules.delivery_distance * 1.5;
            start[0] -= x_offset;
            start[1] -= y_offset;
        }
        let owner = self.entities[source].owner;
        let kind = self
            .catalog
            .iter()
            .position(|unit| unit.name == rules.transport)
            .expect("validated delivery transport");
        // Native constructors and CREATE run at the zero pose before placement.
        let transport = self.new_object(kind, owner, true);
        self.set_object_position(transport, start);
        self.set_object_orientation(transport, orientation);
        self.entities[transport].producer = Some(source);
        self.set_map_body_controls(transport, true, false);
        if rules.max_speed {
            let speed = locomotor::speed(&self.entities[transport], &self.catalog[kind])
                * crate::skirmish::STEP_SECONDS;
            let mass = self
                .object_physics_mass(transport)
                .expect("delivery transport PhysicsBehavior");
            let direction = [orientation.cos(), orientation.sin(), 0.0];
            self.apply_physics_motive_force(
                transport,
                direction.map(|component| component * (speed * mass)),
            );
        }
        self.delivery_state_mut(transport).mission = Some(Mission {
            list,
            move_to: target,
            target,
            phase: Phase::Approach,
            entries: 0,
            drop_delay: 0,
            reentry_frame: 0,
            exit_direction: [0.0; 3],
        });
        self.enter_delivery_phase(transport, Phase::Approach);
        if rules.preferred_height {
            start[2] =
                self.ground_height([start[0], start[1]]) + self.flight_preferred_height(transport);
            self.set_object_position(transport, start);
        }
        let container_kind = self
            .catalog
            .iter()
            .position(|unit| unit.name == rules.container)
            .expect("validated delivery parachute");
        for payload in &rules.payload {
            let kind = self
                .catalog
                .iter()
                .position(|unit| unit.name == payload.object)
                .expect("validated delivery payload");
            for _ in 0..payload.count {
                // DeliverPayloadNugget creates the rider BEFORE its chute.
                let rider = self.new_object(kind, owner, true);
                self.set_object_position(rider, start);
                self.entities[rider].producer = Some(transport);
                let chute = self.new_object(container_kind, owner, true);
                self.set_object_position(chute, start);
                self.entities[chute].producer = Some(transport);
                assert!(
                    self.native_container_admits(rider, chute, true),
                    "original parachute admits delivery payload"
                );
                self.add_to_contain(rider, chute);
                assert!(
                    self.native_container_admits(chute, transport, true),
                    "original cargo transport admits loaded parachute"
                );
                self.add_to_contain(chute, transport);
            }
        }
        transport
    }

    pub(in crate::skirmish) fn delivery_closest_edge(&self, point: [f32; 3]) -> [f32; 3] {
        let mut edge = closest_edge(self.playable_extent, point);
        edge[2] = self.ground_height([edge[0], edge[1]]);
        edge
    }

    pub(in crate::skirmish) fn delivery_transport_on_removing(
        &mut self,
        rider: usize,
        carrier: usize,
    ) {
        let rules = self.catalog[self.entities[carrier].type_index]
            .deliver_payload
            .as_ref()
            .expect("delivery transport data");
        let pitch_rate = rules.exit_pitch_rate;
        if let Some(local) = self.logical_bone_position(carrier, &rules.exit_bone) {
            let (origin, basis) = self.object_transform(carrier);
            let offset = transform::vector(basis, local);
            self.set_object_position(
                rider,
                std::array::from_fn(|axis| origin[axis] + offset[axis]),
            );
        }
        self.transport_exit_orientation(rider, carrier);
        if self.object_physics_mass(carrier).is_some()
            && let Some(mass) = self.object_physics_mass(rider)
        {
            let velocity = self.entities[carrier].physics_velocity();
            self.apply_physics_motive_force(rider, velocity.map(|component| component * mass));
            let center = self.catalog[self.entities[rider].type_index]
                .physics_motion_rules()
                .expect("rider PhysicsBehavior")
                .center_of_mass_offset;
            self.entities[rider].set_physics_pitch_rate(center * pitch_rate);
        }
        if self.height_above_ground(carrier) > 0.0 {
            self.entities[rider].set_physics_allow_fall(true);
        }
        // ParachuteContain remains the authority for subsequent direct destination,
        // opening, FREEFALL/NORMAL locomotors, landing, release and producer rally.
    }

    pub(super) fn move_delivery(&mut self, id: usize, point: [f32; 3]) {
        self.native_move_from(id, point, COMMAND_FROM_AI);
    }
}

fn closest_edge(extent: [[f32; 2]; 2], point: [f32; 3]) -> [f32; 3] {
    let distances = [
        (point[1] - extent[0][1]).abs(),
        (point[0] - extent[1][0]).abs(),
        (point[1] - extent[1][1]).abs(),
        (point[0] - extent[0][0]).abs(),
    ];
    let mut best = 0;
    for index in 1..4 {
        if distances[index] < distances[best] {
            best = index;
        }
    }
    let mut result = point;
    match best {
        0 => result[1] = extent[0][1],
        1 => result[0] = extent[1][0],
        2 => result[1] = extent[1][1],
        _ => result[0] = extent[0][0],
    }
    result
}
