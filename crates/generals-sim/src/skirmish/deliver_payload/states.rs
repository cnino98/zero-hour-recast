use super::{DiveState, Phase, Skirmish, VisiblePayload};
use crate::skirmish::{
    Order, ai_goals::MainState, damage::WeaponTarget, locomotor, physical_ocl::transform, startup,
    weapons::Lock,
};

impl Skirmish {
    fn delivery_is_move_to(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        matches!(actor.ai.command.main_state, MainState::Active)
            && matches!(actor.ai.order, Order::Move(_))
    }

    fn delivery_close(&mut self, id: usize) -> bool {
        let mission = self.delivery_mission(id);
        let target = mission.target;
        let rules = &self.delivery_lists[mission.list];
        let allowed = rules.delivery_distance;
        let pre_open = rules.pre_open_distance;
        let point = self.object_position(id);
        let x = point[0] - target[0];
        let y = point[1] - target[1];
        let x_squared = x * x;
        let y_squared = y * y;
        let squared = x_squared + y_squared;
        let inbound = self.delivery_state(id).previous_squared > squared;
        self.delivery_state_mut(id).previous_squared = squared;
        let distance = if inbound { allowed + pre_open } else { allowed };
        distance * distance > squared
    }

    fn delivery_off_map(&self, id: usize) -> bool {
        let point = self.object_position(id);
        (0..2).any(|axis| {
            point[axis] < self.border_extent[0][axis] || point[axis] > self.border_extent[1][axis]
        })
    }

    fn delivery_turn_radius(&self, id: usize) -> (f32, f32) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let speed = locomotor::speed(actor, unit) * crate::skirmish::STEP_SECONDS;
        let turn = locomotor::turn_rate(actor, unit);
        let radius = if turn > 0.0 { speed / turn } else { 999_999.0 };
        (radius, radius / speed)
    }

    pub(super) fn enter_delivery_phase(&mut self, id: usize, mut phase: Phase) {
        loop {
            self.delivery_mission_mut(id).phase = phase;
            match phase {
                Phase::Approach => {
                    if self.entities[id].is_ai_dead() {
                        phase = Phase::Consider;
                        continue;
                    }
                    self.move_delivery(id, self.delivery_mission(id).move_to);
                }
                Phase::Delivering => {
                    let delay = self.catalog[self.entities[id].type_index]
                        .deliver_payload
                        .as_ref()
                        .expect("delivery transport data")
                        .door_delay;
                    self.delivery_state_mut(id).doors_open = Some(true);
                    self.delivery_mission_mut(id).drop_delay = delay;
                }
                Phase::Consider => {
                    if let Some(next) = self.enter_delivery_consider(id) {
                        phase = next;
                        continue;
                    }
                }
                Phase::Recover => self.enter_delivery_recover(id),
                Phase::HeadOffMap => self.enter_delivery_head_off_map(id),
                Phase::CleanUp => self.destroy_maintenance_object(id),
            }
            break;
        }
    }

    fn enter_delivery_consider(&mut self, id: usize) -> Option<Phase> {
        let list = self.delivery_mission(id).list;
        let entries = self.delivery_mission(id).entries.wrapping_add(1);
        self.delivery_mission_mut(id).entries = entries;
        if entries > self.delivery_lists[list].attempts {
            return Some(Phase::HeadOffMap);
        }
        let (radius, _) = self.delivery_turn_radius(id);
        let point = self.object_position(id);
        let facing = self.entities[id].facing;
        let distance = radius * 2.2;
        let x_offset = facing.cos() * distance;
        let y_offset = facing.sin() * distance;
        self.move_delivery(id, [point[0] + x_offset, point[1] + y_offset, 0.0]);
        self.entities[id].ai.motion.set_allow_invalid_position(true);
        // State::friend_checkForTransitions checks conditions on the
        // CONTINUE result of onEnter, in this same callback.
        if self.delivery_off_map(id) {
            return Some(Phase::Recover);
        }
        None
    }

    fn enter_delivery_recover(&mut self, id: usize) {
        self.move_delivery(id, self.object_position(id));
        let (_, time) = self.delivery_turn_radius(id);
        let delay = u32::from_ne_bytes(crate::native_real::ceil(time).to_ne_bytes());
        self.delivery_mission_mut(id).reentry_frame = self.update_frame().wrapping_add(delay);
        self.reset_dynamic_physics(id);
        self.partition.remove(id);
        self.set_drawable_hidden(id, true);
    }

    fn enter_delivery_head_off_map(&mut self, id: usize) {
        self.entities[id].radius_decal = None;
        let list = self.delivery_mission(id).list;
        if self.delivery_lists[list].self_destruct {
            self.destroy_maintenance_object(id);
            return;
        }
        let point = self.object_position(id);
        let facing = self.entities[id].facing;
        let width = self.playable_extent[1][0] - self.playable_extent[0][0];
        let height = self.playable_extent[1][1] - self.playable_extent[0][1];
        let width_squared = width * width;
        let height_squared = height * height;
        let distance = 1.2 * (width_squared + height_squared).sqrt();
        self.entities[id].ai.motion.set_allow_invalid_position(true);
        self.entities[id].ai.motion.set_ultra_accurate(true);
        self.set_flight_ultra_accurate(id);
        let x_offset = facing.cos() * distance;
        let y_offset = facing.sin() * distance;
        self.move_delivery(id, [point[0] + x_offset, point[1] + y_offset, point[2]]);
        self.delivery_state_mut(id).accepting_commands = false;
        self.delivery_mission_mut(id).exit_direction = self.entities[id].object_direction_3d();
    }

    fn transition_delivery(&mut self, id: usize, next: Phase) {
        match self.delivery_mission(id).phase {
            Phase::Delivering => {
                let state = self.delivery_state_mut(id);
                state.free_to_exit = false;
                state.doors_open = Some(false);
            }
            Phase::Consider => self.entities[id].ai.motion.set_allow_invalid_position(true),
            _ => {}
        }
        self.enter_delivery_phase(id, next);
    }

    pub(super) fn update_delivery_state(&mut self, id: usize) {
        if self.delivery_state(id).mission.is_none() {
            return;
        }
        match self.delivery_mission(id).phase {
            Phase::Approach => {
                if self.entities[id].is_ai_dead() {
                    self.transition_delivery(id, Phase::Consider);
                } else if self.delivery_close(id) {
                    self.transition_delivery(id, Phase::Delivering);
                } else if !self.delivery_is_move_to(id) {
                    if self.entities[id].ai_idle() {
                        self.transition_delivery(id, Phase::Consider);
                    } else {
                        self.move_delivery(id, self.delivery_mission(id).move_to);
                    }
                }
            }
            Phase::Delivering => self.update_delivery_drop(id),
            Phase::Consider => {
                // SUCCESS wins before conditions. Only CONTINUE checks isOffMap.
                if !self.delivery_is_move_to(id) {
                    self.transition_delivery(id, Phase::Approach);
                } else if self.delivery_off_map(id) {
                    self.transition_delivery(id, Phase::Recover);
                }
            }
            Phase::Recover => {
                if self.update_frame() < self.delivery_mission(id).reentry_frame {
                    return;
                }
                self.partition
                    .observe(id, self.object_position(id), self.entities[id].facing);
                self.set_drawable_hidden(id, false);
                let position = self.object_position(id);
                let mut enter = self.delivery_closest_edge(position);
                if self.height_above_ground(id) > 0.0 {
                    enter[2] = position[2];
                }
                self.set_object_position(id, enter);
                let target = self.delivery_mission(id).move_to;
                self.set_object_orientation(id, (target[1] - enter[1]).atan2(target[0] - enter[0]));
                self.reset_dynamic_physics(id);
                self.transition_delivery(id, Phase::Approach);
            }
            Phase::HeadOffMap => {
                if self.delivery_off_map(id) {
                    self.transition_delivery(id, Phase::CleanUp);
                    return;
                }
                if self.entities[id].physics_control.turning {
                    let first = self.delivery_mission(id).exit_direction;
                    let direction = self.entities[id].object_direction_3d();
                    let x = first[0] * direction[0];
                    let y = first[1] * direction[1];
                    let z = first[2] * direction[2];
                    let dot = (x + y) + z;
                    if dot < 0.3 {
                        self.kill_object(id, crate::skirmish::DeathType::Normal);
                    }
                }
            }
            Phase::CleanUp => {}
        }
    }

    fn update_delivery_drop(&mut self, id: usize) {
        let delay = self.delivery_mission(id).drop_delay;
        if delay > 0 {
            self.delivery_mission_mut(id).drop_delay = delay - 1;
            return;
        }
        let mission = self.delivery_mission(id);
        let list = mission.list;
        let target = mission.target;
        let rules = &self.delivery_lists[list];
        let offset = rules.release.drop_offset;
        let variance = rules.release.drop_variance;
        let direct = rules.release.parachute_directly;
        let delay = rules.release.drop_delay;
        let fire_weapon = rules.release.fire_weapon;
        let inherit_velocity = rules.release.inherit_velocity;
        let pitch_rate = rules.release.exit_pitch_rate;
        let visible = rules.visible.clone();
        self.delivery_state_mut(id).free_to_exit = true;
        self.delivery_mission_mut(id).drop_delay = delay;
        if !self.delivery_close(id) {
            self.transition_delivery(id, Phase::Consider);
            return;
        }
        let rider = self.passengers(id).first().copied();
        if rider.is_none()
            && visible
                .as_ref()
                .is_none_or(|visible| self.delivery_mission(id).visible_delivered == visible.count)
        {
            self.transition_delivery(id, Phase::HeadOffMap);
            return;
        }
        if let Some(rider) = rider {
            if fire_weapon {
                let position = std::array::from_fn(|axis| target[axis] + offset[axis]);
                self.fire_delivery_weapon(id, None, position);
                self.destroy_maintenance_object(rider);
            } else {
                if self.entities[rider].ai.registration.is_some() {
                    self.native_ai_exit(rider, id);
                }
                let mut point = self.object_position(rider);
                for (axis, amount) in variance.into_iter().enumerate() {
                    if amount > 0.0 {
                        point[axis] += startup::random_real(&mut self.random, -amount, amount);
                    }
                }
                self.set_object_position(
                    rider,
                    std::array::from_fn(|axis| point[axis] + offset[axis]),
                );
                let move_to = self.delivery_mission(id).move_to;
                if direct {
                    if let Some(parachute) = &mut self.entities[rider].parachute {
                        parachute.landing_override = Some(target);
                    }
                } else if self.entities[rider].ai.registration.is_some() {
                    self.move_delivery(rider, move_to);
                }
                self.set_first_minefield_target(rider, move_to);
                self.set_smart_bomb_target(rider, move_to);
                if inherit_velocity {
                    self.physical_force(rider, self.entities[id].physics_velocity());
                }
            }
        }
        if let Some(visible) = visible {
            self.drop_delivery_visible(id, &visible, inherit_velocity, pitch_rate);
        }
    }

    fn drop_delivery_visible(
        &mut self,
        id: usize,
        visible: &VisiblePayload,
        inherit: bool,
        pitch_rate: f32,
    ) {
        let first = self.delivery_mission(id).visible_delivered;
        let end = first
            .saturating_add(visible.per_interval)
            .min(visible.count);
        for index in first..end {
            self.delivery_visible_sub_object(id, &visible.sub_object, index + 1, false);
            if self.entities[id].has_drawable() && !visible.object.is_empty() {
                self.spawn_visible_delivery_payload(id, visible, index + 1, inherit, pitch_rate);
            }
            self.delivery_mission_mut(id).visible_delivered = index + 1;
        }
    }

    fn spawn_visible_delivery_payload(
        &mut self,
        carrier: usize,
        rules: &VisiblePayload,
        index: u32,
        inherit: bool,
        pitch_rate: f32,
    ) {
        let unit = &self.entities[carrier];
        let owner = unit.owner;
        let orientation = unit.facing;
        let velocity = unit.physics_velocity();
        let (origin, basis) = self.object_transform(carrier);
        let bone = format!("{}{index:02}", rules.bone);
        let mut point = self
            .logical_bone_position(carrier, &bone)
            .map_or(origin, |local| {
                let offset = transform::vector(basis, local);
                std::array::from_fn(|axis| origin[axis] + offset[axis])
            });
        let kind = self
            .catalog
            .iter()
            .position(|unit| unit.name == rules.object)
            .expect("validated visible delivery payload");
        let payload = self.new_object(kind, owner, true);
        self.entities[payload].producer = Some(carrier);
        if inherit {
            if let Some(mass) = self.object_physics_mass(payload) {
                self.apply_physics_motive_force(
                    payload,
                    velocity.map(|component| component * mass),
                );
            }
            point = std::array::from_fn(|axis| point[axis] - velocity[axis]);
        }
        self.set_object_position(payload, point);
        self.set_object_orientation(payload, orientation);
        if !self.launch_delivered_projectile(
            carrier,
            payload,
            &rules.weapon,
            point,
            self.delivery_mission(carrier).target,
        ) {
            if pitch_rate != 0.0
                && let Some(physics) = self.catalog[kind].physics_motion_rules()
            {
                self.entities[payload]
                    .set_physics_pitch_rate(physics.center_of_mass_offset * pitch_rate);
            }
            if self.entities[payload].ai.registration.is_some() {
                self.move_delivery(payload, self.delivery_mission(carrier).move_to);
            }
        }
    }

    pub(super) fn update_delivery_dive(&mut self, id: usize) {
        let Some(mission) = self.delivery_state(id).mission.as_ref() else {
            return;
        };
        let Some(rules) = self.delivery_lists[mission.list].dive.as_ref() else {
            return;
        };
        let position = self.object_position(id);
        let target = mission.target;
        let delta = std::array::from_fn::<_, 3, _>(|axis| position[axis] - target[axis]);
        let squared = delta.map(|axis| axis * axis);
        let squared_xy = squared[0] + squared[1];
        match mission.dive {
            DiveState::Before if squared_xy <= rules.start * rules.start => {
                self.delivery_mission_mut(id).dive = DiveState::Diving;
                if let Some(flight) = &mut self.entities[id].flight {
                    flight.set_precise_z(true);
                }
                if let Some(transport) = &self.catalog[self.entities[id].type_index].deliver_payload
                {
                    self.position_sound(id, transport.dive_sound.clone(), position);
                }
            }
            DiveState::Diving => {
                let squared_3d = squared_xy + squared[2];
                let finished = squared_3d <= rules.end * rules.end;
                let slot = rules.weapon_slot;
                let effect = rules.effect.clone();
                let span = rules.start - rules.end;
                let ratio = (rules.start - squared_3d.sqrt()) / span;
                if finished {
                    self.delivery_mission_mut(id).dive = DiveState::After;
                    if let Some(flight) = &mut self.entities[id].flight {
                        flight.set_precise_z(false);
                    }
                }
                let velocity = self.entities[id].physics_velocity();
                if let Some(slot) = slot
                    && velocity[2] < 5.0
                {
                    let x_squared = velocity[0] * velocity[0];
                    let y_squared = velocity[1] * velocity[1];
                    let length = (x_squared + y_squared).sqrt();
                    let movement = if length > 0.0 {
                        [
                            velocity[0] / length * (ratio * 100.0),
                            velocity[1] / length * (ratio * 100.0),
                        ]
                    } else {
                        [0.0; 2]
                    };
                    let [lead_x, lead_y] = movement.map(|axis| axis * 0.33);
                    let x = target[0] - lead_x + movement[0];
                    let y = target[1] - lead_y + movement[1];
                    let point = [x, y, self.ground_height([x, y])];
                    self.fire_delivery_weapon(id, Some(slot), point);
                    if !effect.is_empty() {
                        self.positional_fx_with_radius(id, effect, point, 0.0);
                    }
                }
            }
            _ => {}
        }
    }

    fn fire_delivery_weapon(&mut self, id: usize, slot: Option<usize>, position: [f32; 3]) {
        if let Some(slot) = slot
            && self.weapon_slot_present(id, slot)
            && !matches!(self.entities[id].weapon_set.lock, Lock::Permanent)
        {
            self.entities[id].weapon_set.current = slot;
            self.entities[id].weapon_set.lock = Lock::Temporary;
        }
        self.fire_current_weapon_payload(
            id,
            WeaponTarget {
                object: None,
                position,
                projectile_position: position,
                scatter_radius: 0.0,
            },
        );
    }
}
