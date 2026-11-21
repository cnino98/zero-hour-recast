use super::{DiveState, Mission, Phase, Rules, Skirmish};
use crate::skirmish::{ai_goals::COMMAND_FROM_AI, locomotor, physical_ocl::transform, startup};
use num_traits::ToPrimitive;
use std::f32::consts::FRAC_PI_2;

struct DeliveryLeg {
    start: [f32; 3],
    move_to: [f32; 3],
    target: [f32; 3],
    lead: bool,
}

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
            if transport.deliver_payload.is_none() || !list.payload.is_empty() && transport.contain.is_none()
                || list.formation_size == 0
                || !transport.update_modules.iter().any(|module| matches!(module, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "DeliverPayloadAIUpdate")) {
                return Err("Delivery transport lacks its actual native consumers");
            }
            if !list.container.is_empty() {
                find(&list.container)?;
            }
            if list.release.parachute_directly
                && (list.container.is_empty() || find(&list.container)?.parachute.is_none())
            {
                return Err(
                    "Direct parachute delivery requires original ParachuteContain container",
                );
            }
            for payload in &list.payload {
                if find(&payload.object)?.live_physics.is_none() {
                    return Err("Delivery payload lacks original physical body");
                }
            }
            if let Some(visible) = &list.visible
                && !visible.object.is_empty()
                && !find(&visible.object)?.projectile_rules.is_empty()
                && !(0..transport.weapon_template_count()).any(|index| {
                    transport
                        .weapon_template(index)
                        .is_some_and(|weapon| weapon.name == visible.weapon)
                })
            {
                return Err("Visible delivery projectile lacks its original weapon binding");
            }
        }
        self.delivery_lists = lists;
        Ok(())
    }

    pub(in crate::skirmish) fn create_delivery(
        &mut self,
        source: usize,
        list: usize,
        start: [f32; 3],
    ) -> usize {
        self.create_delivery_at(source, list, start, self.object_position(source), true)
    }

    pub(in crate::skirmish) fn create_delivery_at(
        &mut self,
        source: usize,
        list: usize,
        start: [f32; 3],
        target: [f32; 3],
        create_owner: bool,
    ) -> usize {
        let rules = self.delivery_lists[list].clone();
        let direction = [start[0] - target[0], start[1] - target[1]];
        let [x_squared, y_squared] = direction.map(|axis| axis * axis);
        let length = (x_squared + y_squared).sqrt();
        let mut first = source;
        for index in 0..rules.formation_size {
            let offset = if index == 0 {
                [0.0; 2]
            } else {
                let x = direction[0] / length;
                let y = direction[1] / length;
                let spacing = (index + 1).div_euclid(2).to_f32().expect("formation index")
                    * rules.formation_spacing;
                let spacing = spacing
                    .to_i32()
                    .expect("authored formation spacing")
                    .to_f32()
                    .expect("native spacing");
                let angle = if index % 2 == 0 {
                    -FRAC_PI_2
                } else {
                    FRAC_PI_2
                };
                let (sine, cosine) = (angle.sin(), angle.cos());
                let rotated_x = [x * cosine, y * -sine];
                let rotated_y = [x * sine, y * cosine];
                [
                    (rotated_x[0] + rotated_x[1] + x) * spacing,
                    (rotated_y[0] + rotated_y[1] + y) * spacing,
                ]
            };
            let start = [start[0] + offset[0], start[1] + offset[1], start[2]];
            let move_to = [target[0] + offset[0], target[1] + offset[1], target[2]];
            let convergence = 1.0 - rules.convergence;
            let [aim_x, aim_y] = offset.map(|axis| axis * convergence);
            let mut aim = [target[0] + aim_x, target[1] + aim_y, target[2]];
            if index != 0 && rules.error_radius > 1.0 {
                let radius = startup::random_real(&mut self.random, 0.0, rules.error_radius);
                let angle = startup::random_angle(&mut self.random);
                aim[0] += radius * angle.cos();
                aim[1] += radius * angle.sin();
            }
            let transport = self.create_delivery_member(
                source,
                list,
                &rules,
                &DeliveryLeg {
                    start,
                    move_to,
                    target: aim,
                    lead: index == 0,
                },
                create_owner,
            );
            if index == 0 {
                first = transport;
            }
        }
        first
    }

    fn create_delivery_member(
        &mut self,
        source: usize,
        list: usize,
        rules: &Rules,
        leg: &DeliveryLeg,
        create_owner: bool,
    ) -> usize {
        let DeliveryLeg {
            mut start,
            move_to,
            target,
            lead,
        } = *leg;
        let orientation = (move_to[1] - start[1]).atan2(move_to[0] - start[0]);
        if rules.delivery_distance > 0.0 {
            let x_offset = orientation.cos() * rules.delivery_distance * 1.5;
            let y_offset = orientation.sin() * rules.delivery_distance * 1.5;
            start[0] -= x_offset;
            start[1] -= y_offset;
        }
        let owner = self.entities[source].owner;
        let transport = if create_owner {
            let kind = self
                .catalog
                .iter()
                .position(|unit| unit.name == rules.transport)
                .expect("validated delivery transport");
            let transport = self.new_object(kind, owner, true);
            self.set_object_position(transport, start);
            self.set_object_orientation(transport, orientation);
            self.entities[transport].producer = Some(source);
            self.set_map_body_controls(transport, true, false);
            transport
        } else {
            source
        };
        let kind = self.entities[transport].type_index;
        let now = self.update_frame();
        if create_owner && rules.transport_start.delay_delivery_max > 0 {
            let frame =
                now.wrapping_add(self.random_between(rules.transport_start.delay_delivery_max));
            self.set_default_disabled_until(transport, frame);
        }
        self.set_special_power_completion_creator(transport, lead.then_some(source));
        if rules.transport_start.max_speed
            && create_owner
            && let Some(mass) = self.object_physics_mass(transport)
        {
            let speed = locomotor::speed(&self.entities[transport], &self.catalog[kind])
                * crate::skirmish::STEP_SECONDS;
            let direction = [orientation.cos(), orientation.sin(), 0.0];
            self.apply_physics_motive_force(
                transport,
                direction.map(|component| component * (speed * mass)),
            );
        }
        self.delivery_state_mut(transport).mission = Some(Mission {
            list,
            move_to,
            target,
            phase: Phase::Approach,
            entries: 0,
            drop_delay: 0,
            reentry_frame: 0,
            exit_direction: [0.0; 3],
            visible_delivered: 0,
            dive: if rules.dive.is_some() {
                DiveState::Before
            } else {
                DiveState::After
            },
        });
        self.delivery_state_mut(transport).previous_squared = 0.0;
        self.entities[transport].radius_decal = None;
        if lead && let Some((radius, template)) = &rules.decal {
            self.create_radius_decal(transport, template.clone(), *radius, target, false);
        }
        if let Some(visible) = &rules.visible {
            for index in 1..=visible.count {
                self.delivery_visible_sub_object(transport, &visible.sub_object, index, true);
            }
        }
        self.enter_delivery_phase(transport, Phase::Approach);
        if rules.transport_start.preferred_height && create_owner {
            start[2] =
                self.ground_height([start[0], start[1]]) + self.flight_preferred_height(transport);
            self.set_object_position(transport, start);
        }
        self.create_delivery_cargo(source, transport, rules, start, lead);
        transport
    }

    fn create_delivery_cargo(
        &mut self,
        source: usize,
        transport: usize,
        rules: &Rules,
        start: [f32; 3],
        lead: bool,
    ) {
        let owner = self.entities[source].owner;
        let container_kind = self
            .catalog
            .iter()
            .position(|unit| unit.name == rules.container);
        for payload in &rules.payload {
            let kind = self
                .catalog
                .iter()
                .position(|unit| unit.name == payload.object)
                .expect("validated delivery payload");
            for index in 0..payload.count {
                let creator = (lead && index == 0).then_some(source);
                let rider = self.new_object(kind, owner, true);
                self.set_object_position(rider, start);
                self.entities[rider].producer = Some(transport);
                self.set_special_power_completion_creator(rider, creator);
                let entry = if let Some(container_kind) = container_kind {
                    let chute = self.new_object(container_kind, owner, true);
                    self.set_object_position(chute, start);
                    self.entities[chute].producer = Some(transport);
                    self.set_special_power_completion_creator(chute, creator);
                    if self.native_container_admits(rider, chute, true) {
                        self.add_to_contain(rider, chute);
                        chute
                    } else {
                        rider
                    }
                } else {
                    rider
                };
                if self.native_container_admits(entry, transport, true) {
                    self.add_to_contain(entry, transport);
                }
            }
        }
    }

    pub(super) fn delivery_visible_sub_object(
        &mut self,
        id: usize,
        name: &str,
        index: u32,
        visible: bool,
    ) {
        if name.is_empty() || !self.entities[id].has_drawable() {
            return;
        }
        let name = format!("{name}{index:02}");
        let overrides = &mut self.entities[id].drawable.sub_objects;
        if let Some(entry) = overrides
            .iter_mut()
            .find(|entry| entry.name.eq_ignore_ascii_case(&name))
        {
            entry.visible = visible;
        } else {
            overrides.push(crate::skirmish::SubObjectOverride { name, visible });
        }
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
