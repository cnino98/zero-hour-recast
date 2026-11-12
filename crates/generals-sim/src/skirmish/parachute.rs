//! SH `ParachuteContain`: authored constructor, update, containment and collision callbacks.
use super::physical_ocl::transform;
use super::{DeathType, Entity, Locomotor, ModelConditions, Skirmish, startup};
use crate::collision_geometry::Shape;

#[cfg(test)]
pub(super) mod tests;

pub struct Rules {
    pub freefall: Locomotor,
    pub angular_rate: [f32; 2],
    pub low_damping: f32,
    pub open_distance: f32,
    pub water_slop: f32,
    pub fall_damage: f32,
    pub open_sound: String,
}

pub(super) struct State {
    opened: bool,
    start_height: Option<f32>,
    angles: [f32; 2],
    rates: [f32; 2],
    pub(super) landing_override: Option<[f32; 3]>,
    update_para_bones: bool,
    update_rider_bones: bool,
    para_sway_bone: [f32; 3],
    para_attach_bone: [f32; 3],
    rider_attach_bone: [f32; 3],
    para_sway_offset: [f32; 3],
    rider_sway_offset: [f32; 3],
    rider_attach_offset: [f32; 3],
}

impl Entity {
    #[must_use]
    pub const fn parachute_rider(&self) -> bool {
        self.crew.parachuting
    }

    #[must_use]
    pub fn parachute_opened(&self) -> bool {
        self.parachute.as_ref().is_some_and(|state| state.opened)
    }

    #[must_use]
    pub const fn drawable_instance_matrix(&self) -> Option<[[f32; 4]; 3]> {
        self.drawable.instance_matrix
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_parachute(&mut self, id: usize) {
        self.construct_open_contain(id);
        let rules = self.catalog[self.entities[id].type_index]
            .parachute
            .as_ref()
            .expect("ParachuteContain data");
        let rates = rules
            .angular_rate
            .map(|rate| startup::random_real(&mut self.random, -rate, rate));
        self.entities[id].parachute = Some(State {
            opened: false,
            start_height: None,
            angles: [0.0; 2],
            rates,
            landing_override: None,
            update_para_bones: true,
            update_rider_bones: true,
            para_sway_bone: [0.0; 3],
            para_attach_bone: [0.0; 3],
            rider_attach_bone: [0.0; 3],
            para_sway_offset: [0.0; 3],
            rider_sway_offset: [0.0; 3],
            rider_attach_offset: [0.0; 3],
        });
        self.entities[id].object_status |= 1 << 8;
        self.parachute_transform_changed(id);
    }

    pub(super) fn eject_hijacker(&mut self, id: usize, ordinal: usize, position: [f32; 3]) {
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .hijackers[ordinal];
        let Some(template) = self
            .catalog
            .iter()
            .position(|unit| unit.name == rules.parachute)
        else {
            return;
        };
        let owner = self.entities[id].owner;
        let team = self.team_membership.team_of(id);
        let chute = self.new_object_on_team(template, owner, team);
        self.set_object_position(chute, position);
        if self.native_container_admits(id, chute, true) {
            self.board_parachute(id, chute);
        }
    }

    pub(in crate::skirmish) fn update_parachute(&mut self, id: usize) -> i32 {
        self.update_open_contain(id);
        if self.entities[id].disabled_held() {
            return 1;
        }
        let position = self.object_position(id);
        let ground = self.ground_height(self.entities[id].position);
        let rider = self.passengers(id).first().copied();
        let rules = self.catalog[self.entities[id].type_index]
            .parachute
            .as_ref()
            .expect("ParachuteContain data");
        let minimum_fall = 2.0 * rules.open_distance;
        let state = self.entities[id]
            .parachute
            .as_mut()
            .expect("ParachuteContain state");
        let start = *state.start_height.get_or_insert_with(|| {
            if position[2] - ground < minimum_fall {
                ground + minimum_fall
            } else {
                position[2]
            }
        });
        let opening = !state.opened && (start - position[2]).abs() >= rules.open_distance;
        state.opened |= opening;
        let opened = state.opened;
        if opening {
            self.open_parachute(id, rider);
        } else if !opened && let Some(rider) = rider {
            let conditions = &mut self.entities[rider].drawable.model_conditions;
            conditions.set_flag(ModelConditions::PARACHUTING, false);
            conditions.set_flag(ModelConditions::FREEFALL, true);
        }
        self.parachute_transform_changed(id);
        for actor in std::iter::once(id).chain(rider) {
            if !opened || rider.is_none() {
                self.entities[actor].object_status |= 1 << 5;
            } else {
                self.entities[actor].object_status &= !(1 << 5);
            }
        }
        self.update_parachute_sway(id, rider, opened);
        let position = self.object_position(id);
        let ground = self.ground_height([position[0], position[1]]);
        let layer = self
            .movement_terrain
            .get_highest_layer_for_destination(position, ground, false);
        self.entities[id].layer = layer;
        if let Some(rider) = rider {
            self.entities[rider].layer = layer;
        }
        if self.passengers(id).is_empty() {
            self.kill_object(id, DeathType::Normal);
        }
        let slop = self.catalog[self.entities[id].type_index]
            .parachute
            .as_ref()
            .expect("ParachuteContain data")
            .water_slop;
        if !self.entities[id].is_effectively_dead()
            && self.entities[id].layer == 1
            && self
                .movement_terrain
                .parachute_water_height([position[0], position[1]], ground)
                .is_some_and(|water| position[2] - water < slop)
        {
            self.kill_object(id, DeathType::Normal);
        }
        1
    }

    fn update_parachute_sway(&mut self, id: usize, rider: Option<usize>, opened: bool) {
        if self.entities[id].ai.registration.is_some() && !self.entities[id].is_effectively_dead() {
            self.choose_locomotor_set(
                id,
                if opened {
                    super::locomotor::set::NORMAL
                } else {
                    super::locomotor::set::FREEFALL
                },
            );
            let rules = self.catalog[self.entities[id].type_index]
                .parachute
                .as_ref()
                .expect("ParachuteContain data");
            let damping = if rider.is_some_and(|rider| self.height_above_ground(rider) <= 20.0) {
                rules.low_damping
            } else {
                0.0
            };
            let locomotor =
                self.entities[id].active_locomotor(&self.catalog[self.entities[id].type_index]);
            if let Some(flight) = &locomotor.flight {
                let state = self.entities[id]
                    .parachute
                    .as_mut()
                    .expect("ParachuteContain state");
                if opened {
                    for axis in 0..2 {
                        let spring = -flight.draw.stiffness[axis] * state.angles[axis];
                        let drag = -(flight.draw.damping[axis] + damping) * state.rates[axis];
                        state.rates[axis] += spring + drag;
                    }
                    state.angles[0] += state.rates[0];
                    state.angles[1] += state.rates[1];
                    if state.landing_override.is_some() {
                        self.set_delivery_parachute_precision(id);
                    }
                }
                if self.entities[id].drawable.present {
                    self.update_parachute_bones(id);
                    self.update_parachute_offsets(id);
                    let state = self.entities[id]
                        .parachute
                        .as_ref()
                        .expect("ParachuteContain state");
                    self.entities[id].drawable.instance_matrix =
                        Some(sway_matrix(state.para_sway_offset, state.angles));
                }
                for rider in self.passengers(id).to_vec() {
                    self.position_parachute_rider(rider, id);
                }
            }
        }
    }

    pub(in crate::skirmish) fn parachute_transform_changed(&mut self, id: usize) {
        let opened = self.entities[id]
            .parachute
            .as_ref()
            .expect("ParachuteContain state")
            .opened;
        self.set_drawable_hidden(id, !opened);
    }

    fn open_parachute(&mut self, id: usize, rider: Option<usize>) {
        let state = self.entities[id]
            .parachute
            .as_mut()
            .expect("ParachuteContain state");
        state.update_para_bones = true;
        state.update_rider_bones |= rider.is_some();
        for actor in std::iter::once(id).chain(rider) {
            let conditions = &mut self.entities[actor].drawable.model_conditions;
            conditions.set_flag(ModelConditions::FREEFALL, false);
            conditions.set_flag(ModelConditions::PARACHUTING, true);
        }
        if let Some(rider) = rider {
            let sound = self.catalog[self.entities[id].type_index]
                .parachute
                .as_ref()
                .expect("ParachuteContain data")
                .open_sound
                .clone();
            self.object_sound(rider, sound, super::object_sounds::Action::Play);
        }
        if self.entities[id].ai.registration.is_none() {
            return;
        }
        let mut target = self.object_position(id);
        if let Some(direct) = self.entities[id]
            .parachute
            .as_ref()
            .expect("ParachuteContain state")
            .landing_override
        {
            target = direct;
            self.set_flight_ultra_accurate(id);
        } else if let Some(position) = self.find_default_position_around(target, 100.0, None) {
            target = position;
        }
        self.construction_move(id, target);
    }

    pub(in crate::skirmish) fn parachute_on_containing(&mut self, carrier: usize, rider: usize) {
        self.set_disabled_held(rider);
        self.entities[rider].object_status |= 1 << 8;
        self.entities[rider].crew.parachuting = true;
        let conditions = &mut self.entities[rider].drawable.model_conditions;
        conditions.set_flag(ModelConditions::PARACHUTING, false);
        conditions.set_flag(ModelConditions::FREEFALL, true);
        self.entities[carrier]
            .parachute
            .as_mut()
            .expect("ParachuteContain state")
            .update_rider_bones = true;
        self.position_parachute_rider(rider, carrier);
    }

    fn update_parachute_bones(&mut self, carrier: usize) {
        let state = self.entities[carrier]
            .parachute
            .as_ref()
            .expect("ParachuteContain state");
        let update_para = state.update_para_bones;
        let update_rider = state.update_rider_bones;
        let rider = self.passengers(carrier).first().copied();
        let para = (update_para && self.entities[carrier].drawable.present).then(|| {
            [
                self.logical_bone_position(carrier, "PARA_COG")
                    .unwrap_or([0.0; 3]),
                self.logical_bone_position(carrier, "PARA_ATTCH")
                    .unwrap_or([0.0; 3]),
            ]
        });
        let rider = rider
            .filter(|&id| update_rider && self.entities[id].drawable.present)
            .map(|id| {
                self.logical_bone_position(id, "PARA_MAN")
                    .unwrap_or_else(|| {
                        let geometry = self.body_geometry(id);
                        let height = if matches!(geometry.shape, Shape::Sphere) {
                            geometry.major
                        } else {
                            geometry.height
                        };
                        [0.0, 0.0, height]
                    })
            });
        let state = self.entities[carrier]
            .parachute
            .as_mut()
            .expect("ParachuteContain state");
        state.update_para_bones = false;
        state.update_rider_bones = false;
        if let Some([sway, attach]) = para {
            state.para_sway_bone = sway;
            state.para_attach_bone = attach;
        }
        if let Some(attach) = rider {
            state.rider_attach_bone = attach;
        }
    }

    fn parachute_bone_offset(&self, id: usize, bone: [f32; 3]) -> [f32; 3] {
        let (translation, basis) = self.object_transform(id);
        let rotated = transform::vector(basis, bone);
        let position = self.object_position(id);
        std::array::from_fn(|axis| (rotated[axis] + translation[axis]) - position[axis])
    }

    fn update_parachute_offsets(&mut self, carrier: usize) {
        let state = self.entities[carrier]
            .parachute
            .as_ref()
            .expect("ParachuteContain state");
        let sway = self.parachute_bone_offset(carrier, state.para_sway_bone);
        let attach = self.parachute_bone_offset(carrier, state.para_attach_bone);
        let rider_offset = self
            .passengers(carrier)
            .first()
            .map(|&rider| self.parachute_bone_offset(rider, state.rider_attach_bone));
        let state = self.entities[carrier]
            .parachute
            .as_mut()
            .expect("ParachuteContain state");
        state.para_sway_offset = sway;
        if let Some(rider_offset) = rider_offset {
            state.rider_attach_offset =
                std::array::from_fn(|axis| attach[axis] - rider_offset[axis]);
            state.rider_sway_offset =
                std::array::from_fn(|axis| sway[axis] - state.rider_attach_offset[axis]);
        }
    }

    pub(super) fn position_parachute_rider(&mut self, rider: usize, carrier: usize) {
        self.update_parachute_bones(carrier);
        self.update_parachute_offsets(carrier);
        let state = self.entities[carrier]
            .parachute
            .as_ref()
            .expect("ParachuteContain state");
        let offset = state.rider_attach_offset;
        let sway = sway_matrix(state.rider_sway_offset, state.angles);
        let origin = self.object_position(carrier);
        let mut position = std::array::from_fn(|axis| origin[axis] + offset[axis]);
        self.set_object_position(rider, position);
        let altitude = self.height_above_ground(rider);
        if altitude < 0.0 {
            position[2] -= altitude;
            self.set_object_position(rider, position);
        }
        self.entities[rider].containment.height = position[2];
        self.set_object_orientation(rider, self.entities[carrier].facing);
        if self.entities[rider].drawable.present {
            self.entities[rider].drawable.instance_matrix =
                self.entities[rider].disabled_held().then_some(sway);
        }
    }

    pub(super) fn parachute_remove(&mut self, rider: usize, carrier: usize) {
        self.entities[rider].object_status &= !(1 << 8);
        self.entities[rider].crew.parachuting = false;
        self.entities[carrier].object_status |= 1 << 5;
        self.position_parachute_rider(rider, carrier);
        let conditions = &mut self.entities[rider].drawable.model_conditions;
        conditions.set_flag(ModelConditions::PARACHUTING, false);
        conditions.set_flag(ModelConditions::FREEFALL, false);
        self.entities[carrier]
            .parachute
            .as_mut()
            .expect("ParachuteContain state")
            .update_rider_bones = true;
        if self.object_physics_mass(rider).is_some() {
            self.entities[rider].set_physics_allow_fall(true);
            self.physical_force(rider, [0.0; 3]);
        }
        if self.entities[rider].ai.registration.is_some() {
            if self.computer_player(self.entities[rider].owner) {
                self.ai_hunt_from_ai(rider);
            } else {
                let producer = self.entities[rider]
                    .producer
                    .filter(|&transport| self.entities[transport].exists())
                    .and_then(|transport| self.entities[transport].producer)
                    .filter(|&producer| self.entities[producer].exists())
                    .filter(|&producer| {
                        self.catalog[self.entities[producer].type_index]
                            .behavior
                            .production_exit
                            .as_ref()
                            .is_some_and(|exit| exit.use_spawn_rally_point)
                    });
                if let Some(producer) = producer {
                    self.exit_delivered_unit(producer, rider);
                } else {
                    self.ai_idle_from_ai(rider);
                }
            }
        }
        let point = self.entities[rider].position;
        let position = self.object_position(rider);
        let ground = self.ground_height(point);
        let slop = self.catalog[self.entities[carrier].type_index]
            .parachute
            .as_ref()
            .expect("ParachuteContain")
            .water_slop;
        if self.entities[rider].layer == 1
            && self
                .movement_terrain
                .parachute_water_height(point, ground)
                .is_some_and(|height| position[2] <= height + slop)
        {
            self.attempt_health_damage(
                rider,
                None,
                999_999.0,
                super::DamageType::Water,
                DeathType::Flooded,
            );
        }
        let kind = self.cell_at(point).and_then(|cell| {
            if self.entities[rider].layer == 1 {
                self.movement_terrain.cell(cell).map(|cell| cell.kind)
            } else {
                self.movement_terrain
                    .bridge_cell(self.entities[rider].layer, cell)
                    .map(|cell| cell.kind)
            }
        });
        if self.entities[rider].is_off_map()
            || kind.is_none_or(|kind| {
                matches!(
                    kind,
                    crate::terrain::CellType::Cliff
                        | crate::terrain::CellType::Water
                        | crate::terrain::CellType::Impassable
                )
            })
        {
            self.kill_object(rider, DeathType::Normal);
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn parachute_die(&mut self, carrier: usize, source: Option<usize>) {
        if self.height_above_ground(carrier) > -9.0 * self.gravity
            && let Some(rider) = self.passengers(carrier).first().copied()
        {
            for passenger in self.passengers(carrier).to_vec() {
                self.remove_passenger(passenger, carrier);
            }
            let fraction = self.catalog[self.entities[carrier].type_index]
                .parachute
                .as_ref()
                .expect("ParachuteContain data")
                .fall_damage;
            if fraction > 0.0 {
                let raw = self.entities[rider].max_health * fraction;
                self.attempt_health_damage(
                    rider,
                    source,
                    raw,
                    super::DamageType::Falling,
                    DeathType::Splatted,
                );
            }
            if self.object_physics_mass(rider).is_some() {
                self.start_parachute_freefall(rider);
                self.physical_force(rider, [0.0; 3]);
            }
        }
        self.containment_death(carrier);
    }

    pub(in crate::skirmish) fn parachute_ground_contact(&mut self, id: usize) {
        if self.entities[id].parachute.is_none() || self.entities[id].contained_by().is_some() {
            return;
        }
        for rider in self.passengers(id).to_vec() {
            self.remove_passenger(rider, id);
        }
        self.kill_object(id, DeathType::Normal);
    }
}

// Matrix3D::In_Place_Pre_Rotate rotates rows while retaining translation.
fn sway_matrix(offset: [f32; 3], angles: [f32; 2]) -> [[f32; 4]; 3] {
    let mut basis = transform::yaw(0.0);
    let (sine, cosine) = angles[1].sin_cos();
    let [_, y, z] = basis;
    basis[1] = std::array::from_fn(|column| {
        let products = [cosine * y[column], sine * z[column]];
        products[0] - products[1]
    });
    basis[2] = std::array::from_fn(|column| {
        let products = [sine * y[column], cosine * z[column]];
        products[0] + products[1]
    });
    let (sine, cosine) = angles[0].sin_cos();
    let [x, _, z] = basis;
    basis[0] = std::array::from_fn(|column| {
        let products = [cosine * x[column], sine * z[column]];
        products[0] + products[1]
    });
    basis[2] = std::array::from_fn(|column| {
        let products = [-sine * x[column], cosine * z[column]];
        products[0] + products[1]
    });
    let translated = transform::vector(basis, offset.map(|value| -value));
    std::array::from_fn(|axis| {
        [
            basis[axis][0],
            basis[axis][1],
            basis[axis][2],
            offset[axis] + translated[axis],
        ]
    })
}
