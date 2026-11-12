use super::{Phase, Skirmish};
use crate::skirmish::{Order, ai_goals::MainState, locomotor};

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
        let offset = rules.drop_offset;
        let direct = rules.parachute_directly;
        let delay = rules.drop_delay;
        self.delivery_state_mut(id).free_to_exit = true;
        self.delivery_mission_mut(id).drop_delay = delay;
        if !self.delivery_close(id) {
            self.transition_delivery(id, Phase::Consider);
            return;
        }
        let Some(&rider) = self.passengers(id).first() else {
            self.transition_delivery(id, Phase::HeadOffMap);
            return;
        };
        if self.entities[rider].ai.registration.is_some() {
            self.native_ai_exit(rider, id);
        }
        let point = self.object_position(rider);
        self.set_object_position(
            rider,
            std::array::from_fn(|axis| point[axis] + offset[axis]),
        );
        if direct {
            if let Some(parachute) = &mut self.entities[rider].parachute {
                parachute.landing_override = Some(target);
            }
        } else if self.entities[rider].ai.registration.is_some() {
            self.move_delivery(rider, self.delivery_mission(id).move_to);
        }
    }
}
