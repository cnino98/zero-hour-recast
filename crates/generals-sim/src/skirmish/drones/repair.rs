use super::{RepairState, Skirmish, startup};

impl Skirmish {
    pub(super) fn end_drone_repair(&mut self, id: usize, ordinal: usize) {
        let state = &mut self.entities[id].behaviors.slaves[ordinal];
        if !matches!(state.repair, RepairState::None) {
            state.repair = RepairState::None;
            state.wait = 30 / 4;
            state.repairing = false;
            self.set_slave_model_condition(id, "PACKING");
        }
        self.choose_locomotor_set(id, crate::skirmish::locomotor::set::NORMAL);
        self.entities[id].ai.motion.set_ultra_accurate(false);
        self.set_flight_precision(id, false, false);
    }

    pub(super) fn repair_drone(&mut self, id: usize, ordinal: usize, parent: usize) {
        let distance = self.range_distance(id, parent);
        let close = distance * distance < 12.0 * 12.0;
        if close {
            let state = &self.entities[id].behaviors.slaves[ordinal];
            match state.repair {
                RepairState::None => self.drone_repair_ready(id, ordinal, parent),
                RepairState::Ready | RepairState::Extending if state.wait == 0 => {
                    self.drone_repair_weld(id, ordinal);
                }
                RepairState::Unpacking | RepairState::Welding | RepairState::Retracting
                    if state.wait == 0 =>
                {
                    self.drone_repair_ready(id, ordinal, parent);
                }
                _ => {}
            }
        } else {
            self.entities[id]
                .behaviors
                .slaves
                .get_mut(ordinal)
                .expect("SlavedUpdate instance")
                .repairing = false;
            let radius = self.catalog[self.entities[parent].type_index]
                .geometry_bounds
                .sphere_radius
                * 2.0;
            let precise = distance * distance < radius * radius;
            let repair = self.catalog[self.entities[id].type_index].behavior.slaves[ordinal]
                .repair
                .as_ref()
                .expect("repair drone rules");
            let mut position = self.object_position(parent);
            position[2] +=
                startup::random_real(&mut self.random, repair.altitude[0], repair.altitude[1]);
            let ultra = self.entities[id].flight_ultra_accurate();
            self.set_flight_precision(id, precise, ultra);
            self.construction_move(id, position);
            if self.entities[id].behaviors.slaves[ordinal].wait == 0 {
                self.drone_repair_ready(id, ordinal, parent);
            }
        }
        if close
            && self.entities[id]
                .behaviors
                .slaves
                .get(ordinal)
                .expect("SlavedUpdate instance")
                .repairing
        {
            let repair = self.catalog[self.entities[id].type_index].behavior.slaves[ordinal]
                .repair
                .as_ref()
                .expect("repair drone rules");
            let amount = repair.per_second / 30.0;
            // SlavedUpdate leaves DamageInfo's source at INVALID_ID.
            self.apply_healing(parent, None, amount);
        }
    }

    fn drone_repair_ready(&mut self, id: usize, ordinal: usize, parent: usize) {
        let state = &self.entities[id].behaviors.slaves[ordinal];
        match state.repair {
            RepairState::Ready => {}
            RepairState::None => {
                let state = &mut self.entities[id].behaviors.slaves[ordinal];
                state.repair = RepairState::Unpacking;
                state.wait = 15;
                self.set_slave_model_condition(id, "UNPACKING");
            }
            RepairState::Welding => {
                let state = &mut self.entities[id].behaviors.slaves[ordinal];
                state.repair = RepairState::Retracting;
                state.wait = 5;
                self.set_slave_model_condition(id, "FIRING_C");
                self.drone_repair_spot(id, ordinal, parent);
            }
            _ => {
                let repair = self.catalog[self.entities[id].type_index].behavior.slaves[ordinal]
                    .repair
                    .as_ref()
                    .expect("repair drone rules");
                let [low, high] = repair.ready_frames;
                let wait = self.slave_random_frames([low, high]);
                let state = &mut self.entities[id].behaviors.slaves[ordinal];
                state.repair = RepairState::Ready;
                state.wait = wait;
            }
        }
    }

    fn drone_repair_weld(&mut self, id: usize, ordinal: usize) {
        let state = &self.entities[id].behaviors.slaves[ordinal];
        match state.repair {
            RepairState::Welding => {}
            RepairState::Ready => {
                let state = &mut self.entities[id].behaviors.slaves[ordinal];
                state.repair = RepairState::Extending;
                state.wait = 5;
                self.set_slave_model_condition(id, "FIRING_B");
            }
            _ => {
                let repair = self.catalog[self.entities[id].type_index].behavior.slaves[ordinal]
                    .repair
                    .as_ref()
                    .expect("repair drone rules");
                let [low, high] = repair.weld_frames;
                let wait = self.slave_random_frames([low, high]);
                let position = self.object_position(id);
                let actor = &mut self.entities[id];
                let state = &mut actor.behaviors.slaves[ordinal];
                state.repair = RepairState::Welding;
                state.wait = wait;
                // Original code multiplies frames by LOGICFRAMES_PER_SECOND.
                let weld = super::Weld {
                    frame: self.tick,
                    lifetime: num_traits::ToPrimitive::to_f32(
                        &wait
                            .checked_mul(30)
                            .expect("native welding lifetime overflow"),
                    )
                    .expect("native Real lifetime"),
                    position,
                    health_ratio: actor.health / actor.max_health,
                };
                self.presentation
                    .weapon_fx
                    .push(crate::skirmish::WeaponFxEvent::SlaveWeld(
                        self.presentation.slave_welds.len(),
                    ));
                self.presentation.slave_welds.push(super::WeldEvent {
                    object: id,
                    type_index: actor.type_index,
                    ordinal,
                    weld,
                });
                state.repairing = true;
            }
        }
    }

    fn drone_repair_spot(&mut self, id: usize, ordinal: usize, parent: usize) {
        let repair = self.catalog[self.entities[id].type_index].behavior.slaves[ordinal]
            .repair
            .as_ref()
            .expect("repair drone rules");
        let range = repair.range;
        let altitude = repair.altitude;
        if range == 0.0 {
            return;
        }
        let offset = self.drone_random_offset(range);
        let mut position = self.object_position(parent);
        position[0] += offset[0];
        position[1] += offset[1];
        position[2] = self.ground_height([position[0], position[1]]);
        position[2] += startup::random_real(&mut self.random, altitude[0], altitude[1]);
        // This source branch stores an absolute point in guardPointOffset.
        self.entities[id]
            .behaviors
            .slaves
            .get_mut(ordinal)
            .expect("SlavedUpdate instance")
            .offset = position;
        self.choose_locomotor_set(id, crate::skirmish::locomotor::set::PANIC);
        self.entities[id].ai.motion.set_ultra_accurate(true);
        self.set_flight_precision(id, false, true);
        self.construction_move(id, position);
        self.set_flight_precision(id, true, true);
    }
}
