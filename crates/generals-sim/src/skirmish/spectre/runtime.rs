use super::{Skirmish, Status, Strafe, length, normalized};
use crate::skirmish::{
    combat::{attack::Goal, targeting},
    module_updates::FOREVER,
};

impl Skirmish {
    pub(in crate::skirmish) fn update_spectre(&mut self, id: usize) -> i32 {
        if self.entities[id].is_effectively_dead() {
            return i32::try_from(FOREVER).expect("native sleep");
        }
        self.entities[id]
            .spectre
            .as_mut()
            .expect("Spectre state")
            .decals
            .frame = Some(self.update_frame());
        self.entities[id]
            .spectre
            .as_mut()
            .expect("Spectre state")
            .decals
            .draw_ui = self.draw_icon_ui;
        if matches!(
            self.entities[id]
                .spectre
                .as_ref()
                .expect("Spectre state")
                .status,
            Status::Inserting | Status::Orbiting
        ) {
            self.update_spectre_orbit_goal(id);
        }
        match self.entities[id]
            .spectre
            .as_ref()
            .expect("Spectre state")
            .status
        {
            Status::Orbiting => {
                if self.update_frame()
                    >= self.entities[id]
                        .spectre
                        .as_ref()
                        .expect("Spectre state")
                        .escape_frame
                {
                    self.spectre_cleanup(id);
                    self.entities[id]
                        .spectre
                        .as_mut()
                        .expect("Spectre state")
                        .status = Status::Departing;
                    self.depart_spectre(id);
                } else {
                    self.update_spectre_weapons(id);
                }
            }
            Status::Departing => {
                let position = self.entities[id].position;
                let extent = self.border_extent;
                if position[0] < extent[0][0]
                    || position[0] > extent[1][0]
                    || position[1] < extent[0][1]
                    || position[1] > extent[1][1]
                {
                    self.destroy_maintenance_object(id);
                    self.entities[id]
                        .spectre
                        .as_mut()
                        .expect("Spectre state")
                        .status = Status::Idle;
                    self.spectre_cleanup(id);
                }
            }
            Status::Inserting | Status::Idle => {}
        }
        1
    }

    fn update_spectre_orbit_goal(&mut self, id: usize) {
        let position = self.object_position(id);
        let state = self.entities[id].spectre.as_ref().expect("Spectre state");
        let initial = state.initial_target;
        let perigee = [position[0] - initial[0], position[1] - initial[1], 0.0];
        let distance = length(perigee);
        let perigee = normalized(perigee);
        let rules = self.catalog[self.entities[id].type_index]
            .spectre
            .as_ref()
            .expect("Spectre rules");
        let slope = rules.insertion_slope.clamp(0.5, 0.8);
        let remainder = 1.0 - slope;
        let radius = rules.orbit_radius;
        let constraint = rules.attack_radius - rules.reticle_radius;
        let orbit_frames = rules.orbit_frames;
        let mut satellite = state.satellite;
        let radial = perigee.map(|component| component * slope);
        let tangent = [-perigee[1] * remainder, perigee[0] * remainder];
        let orbit = [
            (radial[0] + tangent[0]) * radius,
            (radial[1] + tangent[1]) * radius,
        ];
        satellite[0] = initial[0] + orbit[0];
        satellite[1] = initial[1] + orbit[1];
        self.entities[id]
            .spectre
            .as_mut()
            .expect("Spectre state")
            .satellite = satellite;
        self.construction_move(id, satellite);
        let state = self.entities[id].spectre.as_mut().expect("Spectre state");
        let delta = std::array::from_fn(|axis| initial[axis] - state.override_target[axis]);
        if length(delta) > constraint {
            let delta = normalized(delta);
            let offset = delta.map(|component| component * constraint);
            state.override_target[0] = initial[0] - offset[0];
            state.override_target[1] = initial[1] - offset[1];
        }
        state.decals.positions = [state.initial_target, state.override_target];
        let gunner = state.gunner;
        if matches!(state.status, Status::Inserting) && distance < radius {
            state.status = Status::Orbiting;
            state.escape_frame =
                crate::skirmish::ai_dispatch::native_frame(self.tick).wrapping_add(orbit_frames);
            state.doors_open = true;
            if let Some(gunner) = gunner.filter(|&other| self.entities[other].exists()) {
                self.entities[gunner].disabled_status &= !(1 << 2);
                self.observe_spy_vision_disabled(gunner);
            }
            self.set_flight_locomotor(id, false);
            self.set_flight_precision(id, false, true);
            self.spectre_afterburner(id, false);
        }
    }

    fn update_spectre_weapons(&mut self, id: usize) {
        let rules = self.catalog[self.entities[id].type_index]
            .spectre
            .as_ref()
            .expect("Spectre rules");
        let (rate, follow, offset, reticle, area, orbit, increment) = (
            rules.firing_rate,
            rules.follow_lag,
            rules.howitzer_offset,
            rules.reticle_radius,
            rules.attack_radius,
            rules.orbit_radius,
            rules.strafe_increment,
        );
        let state = self.entities[id].spectre.as_ref().expect("Spectre state");
        let gunner = state.gunner.filter(|&other| self.entities[other].exists());
        let (target, initial) = (state.override_target, state.initial_target);
        if self.update_frame().is_multiple_of(rate) {
            self.entities[id]
                .spectre
                .as_mut()
                .expect("Spectre state")
                .shoot_at = target;
            let mut object = self.spectre_target(id, target, reticle, orbit);
            if object.is_none() && self.computer_player(self.entities[id].owner) {
                object = self.spectre_target(id, initial, area, orbit);
                if let Some(target) = object {
                    let position = self.object_position(target);
                    self.entities[id]
                        .spectre
                        .as_mut()
                        .expect("Spectre state")
                        .shoot_at = position;
                }
            }
            if let Some(gunner) = gunner {
                let position = self.entities[id]
                    .spectre
                    .as_ref()
                    .expect("Spectre state")
                    .gunner_target;
                self.ai_attack_goal(
                    gunner,
                    object.map_or(Goal::Position(position), Goal::Object),
                    9999,
                );
            }
            if self.entities[id]
                .spectre
                .as_ref()
                .expect("Spectre state")
                .howitzer_counter
                > follow
            {
                let unit = &self.catalog[self.entities[id].type_index];
                if let Some(weapon) = unit.spectre_weapon_index() {
                    let sound = unit
                        .spectre
                        .as_ref()
                        .expect("Spectre rules")
                        .howitzer_sound
                        .clone();
                    let mut position = self.entities[id]
                        .spectre
                        .as_ref()
                        .expect("Spectre state")
                        .gunner_target;
                    position[0] +=
                        crate::skirmish::startup::random_real(&mut self.random, -offset, offset);
                    position[1] +=
                        crate::skirmish::startup::random_real(&mut self.random, -offset, offset);
                    self.fire_temporary_weapon_at(id, weapon, position);
                    self.object_sound(id, sound, crate::skirmish::object_sounds::Action::Play);
                }
            }
        }
        self.update_spectre_strafe(id, gunner, increment);
    }

    fn update_spectre_strafe(&mut self, id: usize, gunner: Option<usize>, increment: f32) {
        // CRC=0: particle availability never controls authoritative strafing.
        if gunner.is_some_and(|gunner| self.entities[gunner].is_firing_weapon()) {
            let state = self.entities[id].spectre.as_mut().expect("Spectre state");
            let delta =
                std::array::from_fn(|axis| state.shoot_at[axis] - state.gunner_target[axis]);
            if length(delta) < increment {
                state.gunner_target = state.shoot_at;
                state.howitzer_counter = state.howitzer_counter.wrapping_add(1);
            } else {
                state.howitzer_counter = 0;
                let delta = normalized(delta);
                for (value, delta) in state.gunner_target.iter_mut().zip(delta) {
                    let step = delta * increment;
                    *value += step;
                }
            }
            let position = state.gunner_target;
            let particle = self.catalog[self.entities[id].type_index]
                .spectre
                .as_ref()
                .expect("Spectre rules")
                .strafe_particle
                .clone();
            let visible_players = (0..self.players.len()).fold(0, |mask, owner| {
                if matches!(
                    self.visibility_status(owner, id),
                    crate::skirmish::visibility::Status::Clear
                        | crate::skirmish::visibility::Status::Partial
                ) {
                    mask | (1 << owner)
                } else {
                    mask
                }
            });
            self.presentation
                .weapon_fx
                .push(crate::skirmish::WeaponFxEvent::SpectreStrafe(
                    self.presentation.spectre_strafes.len(),
                ));
            self.presentation.spectre_strafes.push(Strafe {
                ship: id,
                position,
                particle,
                visible_players,
            });
        }
    }

    fn spectre_target(
        &mut self,
        id: usize,
        position: [f32; 3],
        radius: f32,
        orbit: f32,
    ) -> Option<usize> {
        let owner = self.entities[id].owner;
        let off_map = self.entities[id].is_off_map();
        let mut candidates: Vec<_> = self
            .partition
            .fastest_range_objects([position[0], position[1]], radius)
            .into_iter()
            .filter_map(|target| {
                let actor = &self.entities[target];
                if actor.is_effectively_dead()
                    || actor.is_off_map() != off_map
                    || self.object_relationship(id, target) != 0
                    || !self.partition_stealth_visible(id, target)
                    || !matches!(
                        self.visibility_status(owner, target),
                        crate::skirmish::visibility::Status::Clear
                    )
                {
                    return None;
                }
                let delta = [
                    actor.position[0] - position[0],
                    actor.position[1] - position[1],
                ];
                let squares = delta.map(|component| component * component);
                let center = squares[0] + squares[1];
                let distance = (center.sqrt() - self.object_radius(target)).max(0.0);
                (distance * distance < radius * radius).then_some((target, distance * distance))
            })
            .collect();
        // Native stable SimpleObjectIterator mergesort retains prepend order on ties.
        candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
        for (target, _) in candidates {
            if !matches!(
                self.attack_target_result(
                    id,
                    Some(target),
                    self.object_position(target),
                    false,
                    false,
                    None
                ),
                targeting::Result::Possible | targeting::Result::PossibleAfterMoving
            ) {
                continue;
            }
            let delta = [
                self.entities[id].position[0] - self.entities[target].position[0],
                self.entities[id].position[1] - self.entities[target].position[1],
                0.0,
            ];
            if length(delta) > orbit * 0.75 {
                return Some(target);
            }
        }
        None
    }

    // PartitionFilterStealthedAndUndetected(false), including disguised units
    // and neutral containers whose occupants are all hidden from this team.
    pub(in crate::skirmish) fn partition_stealth_visible(&self, id: usize, target: usize) -> bool {
        let actor = &self.entities[target];
        if actor.stealthed() && !actor.detected() {
            if !self.catalog[actor.type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "DISGUISER")
            {
                return false;
            }
            if let Some(owner) = actor.disguised_player() {
                return self.enemies(self.entities[id].owner, owner);
            }
        } else {
            let passengers = self.passengers(target);
            if let Some(&first) = passengers.first()
                && passengers
                    .iter()
                    .all(|&member| self.attack_kind(member, "STEALTH_GARRISON"))
                && !self.entities[first].detected()
                && let Some(owner) = self.container_apparent_owner(target, self.entities[id].owner)
                && self.object_player_relationship(id, owner) == 0
            {
                return false;
            }
        }
        true
    }

    fn depart_spectre(&mut self, id: usize) {
        let position = self.object_position(id);
        let basis = self.entities[id].live_basis().unwrap_or_else(|| {
            crate::skirmish::physical_ocl::transform::yaw(self.entities[id].facing)
        });
        let forward = [basis[0][0], basis[1][0]].map(|component| component * 99999.0);
        let exit = [
            position[0] + forward[0],
            position[1] + forward[1],
            position[2] + basis[2][0],
        ];
        self.construction_move(id, exit);
        if let Some(gunner) = self.entities[id]
            .spectre
            .as_ref()
            .expect("Spectre state")
            .gunner
            .filter(|&other| self.entities[other].exists())
        {
            self.entities[gunner].disabled_status |= 1 << 2;
            self.observe_spy_vision_disabled(gunner);
        }
        self.set_flight_locomotor(id, true);
        self.set_flight_precision(id, false, true);
        self.entities[id]
            .spectre
            .as_mut()
            .expect("Spectre state")
            .doors_open = false;
        self.spectre_afterburner(id, true);
        self.spectre_cleanup(id);
    }
}
