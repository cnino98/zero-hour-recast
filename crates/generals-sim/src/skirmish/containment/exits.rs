use super::{Order, Skirmish};
use crate::skirmish::startup;

impl Skirmish {
    /// Virtual `ContainModuleInterface::killAllContained`, preserving callback list ownership.
    pub(in crate::skirmish) fn kill_all_contained(&mut self, carrier: usize) {
        match self.catalog[self.entities[carrier].type_index].contain_family() {
            Some("OverlordContain") => {
                let redirected = self.redirected_container(carrier);
                if redirected != carrier {
                    self.kill_all_contained(redirected);
                }
            }
            Some("TunnelContain") => {
                // TunnelContain swaps the owner's complete shared list before
                // callbacks can destroy an endpoint or reenter the network.
                let riders = std::mem::take(&mut self.tunnel_network_mut(carrier).passengers);
                for id in riders {
                    self.remove_passenger_exposed(id, carrier, true);
                    self.kill_object(id, crate::skirmish::DeathType::Normal);
                }
            }
            Some(_) => {
                while let Some(&id) = self.entities[carrier].containment.passengers.first() {
                    self.entities[carrier].containment.passengers.remove(0);
                    self.container_on_removing(id, carrier, false);
                    self.object_on_removed_from(id);
                    self.kill_object(id, crate::skirmish::DeathType::Normal);
                }
            }
            None => {}
        }
    }

    /// Transport's orientation callback follows exit-bone placement and precedes inherited velocity.
    pub(in crate::skirmish) fn transport_exit_orientation(&mut self, rider: usize, carrier: usize) {
        let unit = &self.catalog[self.entities[carrier].type_index];
        if matches!(
            unit.contain_family(),
            Some(
                "TransportContain"
                    | "InternetHackContain"
                    | "OverlordContain"
                    | "HelixContain"
                    | "RiderChangeContain"
            )
        ) && unit
            .contain
            .as_ref()
            .is_some_and(|rules| rules.orient_like_container_on_exit)
        {
            self.set_object_orientation(rider, self.entities[carrier].facing);
        }
    }

    /// CRC=0 detaches the literal `OpenContain` list while damage callbacks execute.
    pub(in crate::skirmish) fn process_damage_to_contained(
        &mut self,
        carrier: usize,
        percent: f32,
    ) {
        let death = if self
            .container_rules(carrier)
            .is_some_and(|rules| rules.death.burned)
        {
            crate::skirmish::DeathType::Burned
        } else {
            crate::skirmish::DeathType::Normal
        };
        let passengers = std::mem::take(&mut self.entities[carrier].containment.passengers);
        let mut survivors = Vec::with_capacity(passengers.len());
        for id in passengers {
            let amount = self.entities[id].max_health * percent;
            self.apply_native_damage(id, Some(carrier), amount, death);
            if !self.entities[id].is_effectively_dead() && percent.to_bits() == 1.0_f32.to_bits() {
                self.kill_object(id, crate::skirmish::DeathType::Normal);
            }
            if self.entities[id].is_effectively_dead() {
                // CRC=0 already detached the literal list; it invokes only the
                // callbacks, not iterator removal/world restoration/unload.
                self.container_on_removing(id, carrier, false);
                self.object_on_removed_from(id);
            } else {
                survivors.push(id);
            }
        }
        self.entities[carrier].containment.passengers = survivors;
    }

    pub(in crate::skirmish) fn container_rally_points(
        &self,
        carrier: usize,
    ) -> Option<[[f32; 3]; 2]> {
        // OpenContain has no exit position. Its natural rally uses ExitEnd (or
        // ExitEnd01), independently of the next door used by an exiting rider.
        let art = self.art(carrier)?;
        let center = self.object_position(carrier);
        let natural = art
            .exits
            .first()
            .map_or(center, |(_, end)| self.container_point(carrier, *end));
        Some([center, natural])
    }

    /// Requests the selected passenger's original container exit.
    /// # Errors
    /// Rejects missing/dead passengers, finished matches and unavailable exits.
    pub fn exit_container(&mut self, id: usize) -> Result<(), &'static str> {
        let carrier = self
            .entities
            .get(id)
            .and_then(crate::skirmish::Entity::contained_by)
            .ok_or("Unit is not contained")?;
        self.exit_container_at(id, carrier)
    }

    /// Exits through the commanded endpoint, including another tunnel in the network.
    /// # Errors
    /// Rejects unavailable passengers, foreign networks and unavailable exits.
    pub fn exit_container_at(&mut self, id: usize, carrier: usize) -> Result<(), &'static str> {
        if self.end_game_at.is_some() {
            return Err("Match has ended");
        }
        let rider = self.entities.get(id).ok_or("Passenger is missing")?;
        let entered = rider.contained_by().ok_or("Unit is not contained")?;
        // Deliberate repair of the pinned CRC0 Overlord membership bug:
        // the selected chassis owns the exit, while its active child owns riders.
        let redirected_member = self.catalog[self.entities[carrier].type_index].contain_family()
            == Some("OverlordContain")
            && self.redirected_container(carrier) == entered
            && self.entities[entered].contained_by() == Some(carrier)
            && self.entities[entered].containment.passengers.contains(&id);
        if entered != carrier
            && !redirected_member
            && !(self.is_tunnel(entered)
                && self.is_tunnel(carrier)
                && self.entities[entered].owner == self.entities[carrier].owner
                && self.passengers(carrier).contains(&id))
        {
            return Err("Passenger is not in this container network");
        }
        if rider.attachment_parent().is_some() {
            return Err("Portable attachment cannot exit");
        }
        if !rider.complete()
            || !self.entities[carrier].complete()
            || self.entities[carrier].subdued()
            || (self.entities[carrier].chinook.is_none() && !self.free_to_exit(id, carrier))
        {
            return Err("Passenger cannot exit here");
        }
        self.dozer_player_command(id);
        self.assault_command(id, Order::ExitContainer);
        self.native_exit_from(id, carrier, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
        self.finish_dozer_player_command(id);
        Ok(())
    }

    /// Orders every passenger to leave through the original timed exit paths.
    /// # Errors
    /// Rejects inactive objects and objects without a represented container.
    pub fn evacuate(&mut self, carrier: usize) -> Result<(), &'static str> {
        self.ready_actor(carrier)?;
        if self.entities[carrier].subdued() {
            return Err("Subdued container cannot evacuate");
        }
        if self.container_rules(carrier).is_none() {
            return Err("This object has no supported container");
        }
        // AssaultTransportAIUpdate resets its state for external AICMD_EVACUATE.
        // The base command only orders passenger exits; it does not stop the carrier.
        if let Some(state) = &mut self.entities[carrier].assault {
            **state = crate::skirmish::assault_transport::State::new();
        }
        if self.entities[carrier].chinook.is_some() {
            self.native_evacuate_from(carrier, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
            return Ok(());
        }
        for id in self.passengers(carrier).to_vec() {
            self.exit_container_at(id, carrier)?;
        }
        Ok(())
    }

    fn free_to_exit(&self, id: usize, carrier: usize) -> bool {
        let host = &self.entities[carrier];
        let host_type = &self.catalog[host.type_index];
        if !matches!(
            host_type.contain_family(),
            Some(
                "TransportContain"
                    | "InternetHackContain"
                    | "HelixContain"
                    | "OverlordContain"
                    | "RiderChangeContain"
            )
        ) {
            return true;
        }
        if host_type.deliver_payload.is_some()
            && (host.is_effectively_dead()
                || !host
                    .delivery
                    .as_ref()
                    .is_some_and(|state| state.free_to_exit))
        {
            return false;
        }
        if host.chinook.is_some() && !host.landed() || host.contained_by().is_some() {
            return false;
        }
        if host.ai.registration.is_some()
            && host_type
                .locomotor_in_set(host.ai.locomotor_set, host.ai.surface_locomotor)
                .is_some_and(|locomotor| locomotor.legal_surfaces & 8 != 0)
        {
            return true;
        }
        let rider = &self.entities[id];
        if rider.ai.registration.is_none() {
            return false;
        }
        self.catalog[rider.type_index]
            .locomotor_in_set(rider.ai.locomotor_set, rider.ai.surface_locomotor)
            .is_some_and(|locomotor| {
                self.movement_terrain.valid_movement_on(
                    host.layer,
                    host.position,
                    locomotor.legal_surfaces,
                )
            })
    }

    pub(in crate::skirmish) fn update_container_exit(&mut self, id: usize) {
        let Some(carrier) = self.container_exit_target(id) else {
            self.set_order(id, Order::Idle);
            return;
        };
        if self.entities[carrier].alive()
            && self.entities[carrier].chinook.is_some()
            && !self.entities[carrier].landed()
        {
            return;
        }
        if !self.entities[carrier].alive() || !self.free_to_exit(id, carrier) {
            self.set_order(id, Order::Idle);
            return;
        }
        if self.is_garrison(carrier) {
            self.exit_garrison(id, carrier);
            return;
        }
        if self.catalog[self.entities[carrier].type_index]
            .combat_bike
            .is_none()
            && self.tick < self.entities[carrier].containment.exit_ready
        {
            return;
        }
        self.exit_spawned_from_container(id, carrier);
    }

    pub(in crate::skirmish) fn exit_bike_rider_instantly(&mut self, id: usize, carrier: usize) {
        if self.entities[id].ai.registration.is_some()
            && self.native_ai_command_allowed(id)
            && self.entities[id].contained_by() == Some(carrier)
        {
            self.exit_spawned_from_container(id, carrier);
        }
    }

    // SpawnBehavior also calls OpenContain::exitObjectViaDoor for a newly
    // created object that was never on the container's passenger list.
    pub(in crate::skirmish) fn exit_spawned_from_container(&mut self, id: usize, carrier: usize) {
        let declared = self.catalog[self.entities[carrier].type_index]
            .contain
            .as_ref()
            .map_or(0, |rules| rules.exit_paths);
        let (path, count) = self
            .art(carrier)
            .filter(|_| declared > 0)
            .map_or((None, 0), |art| {
                (
                    art.exits
                        .get(self.entities[carrier].containment.next_exit)
                        .copied(),
                    art.exits.len(),
                )
            });
        self.remove_passenger(id, carrier);
        self.open_container_exit_door(carrier);
        if let Some((start, end)) = path {
            let (start, end) = if self.catalog[self.entities[carrier].type_index]
                .combat_bike
                .is_some()
            {
                (
                    self.logical_bone_position(carrier, "ExitStart")
                        .unwrap_or([0.0; 3]),
                    self.logical_bone_position(carrier, "ExitEnd")
                        .unwrap_or([0.0; 3]),
                )
            } else {
                (start, end)
            };
            let start = self.container_point(carrier, start);
            let mut end = self.container_point(carrier, end);
            self.entities[carrier].containment.next_exit =
                (self.entities[carrier].containment.next_exit + 1) % count;
            let can_fall = self.entities[id].physics_allows_fall();
            let orientation = self.entities[carrier].facing;
            self.set_object_position(id, start);
            self.set_object_orientation(id, orientation);
            self.entities[id].layer = self.entities[carrier].layer;
            self.restore_object_obstacle(id);
            self.update_position_claim(id);
            if self.entities[id].ai.registration.is_some() {
                if self.entities[carrier].ai.registration.is_some()
                    && self.entities[carrier].ai_idle()
                    && self.attack_kind(carrier, "VEHICLE")
                {
                    self.remove_position(carrier);
                    self.update_position_claim(carrier);
                    let position = self.object_position(carrier);
                    let layer = self.movement_terrain.get_layer_for_destination(
                        position,
                        self.ground_height([position[0], position[1]]),
                    );
                    self.reserve_goal_on(carrier, [position[0], position[1]], layer);
                }
                self.entities[id].ai.ignored_producer = None;
                self.entities[id].ai.blocking.ignore_until = self.tick + 30;
                if self.attack_ground_movement(id) {
                    end = self.adjusted_destination_world(id, end).unwrap_or(end);
                }
                let mut points = vec![end, end];
                points.extend(self.entities[carrier].rally_point);
                self.entities[id].set_physics_allow_fall(false);
                self.native_follow_path_from(
                    id,
                    points,
                    Some(carrier),
                    crate::skirmish::ai_goals::COMMAND_FROM_AI,
                    false,
                );
                let layer = self
                    .movement_terrain
                    .get_layer_for_destination(end, self.ground_height([end[0], end[1]]));
                self.reserve_goal_on(id, [end[0], end[1]], layer);
            }
            self.entities[id].set_physics_allow_fall(can_fall);
        } else {
            self.restore_object_obstacle(id);
            self.update_position_claim(id);
        }
        self.observe_ground_partition(id);
    }

    pub(in crate::skirmish) fn remove_passenger(&mut self, id: usize, carrier: usize) {
        self.remove_passenger_exposed(id, carrier, false);
    }

    pub(in crate::skirmish) fn remove_passenger_exposed(
        &mut self,
        id: usize,
        carrier: usize,
        expose: bool,
    ) {
        if self.entities[carrier].containment.tunnel.is_some() {
            self.remove_tunnel_passenger(id, carrier);
            return;
        }
        if self.entities[carrier].containment.cave.is_some() {
            self.remove_cave_passenger(id, carrier);
            return;
        }
        if self.remove_helix_portable(id, carrier) {
            return;
        }
        if self.catalog[self.entities[carrier].type_index].contain_family()
            == Some("OverlordContain")
        {
            let redirected = self.redirected_container(carrier);
            if redirected != carrier
                && self.entities[id].contained_by() == Some(redirected)
                && self.entities[redirected].contained_by() == Some(carrier)
                && self.entities[redirected]
                    .containment
                    .passengers
                    .contains(&id)
            {
                // OverlordContain::removeFromContain delegates the actual rider
                // callbacks/accounting to its active contained bunker.
                self.remove_passenger_exposed(id, redirected, expose);
                return;
            }
        }
        if self.entities[id].contained_by() != Some(carrier)
            || !self.entities[carrier].containment.passengers.contains(&id)
        {
            // Native removeFromContain searches the literal list. A callback
            // can reenter after erasure while containedBy still points here.
            return;
        }
        self.entities[carrier]
            .containment
            .passengers
            .retain(|&other| other != id);
        if expose && self.attack_kind(id, "STEALTH_GARRISON") {
            self.mark_detected(id, 0);
        }
        if self.container_encloses(carrier, id) {
            self.containment_world_membership(id, true);
            self.set_object_position(id, self.object_position(carrier));
        }
        self.entities[id].layer = self.entities[carrier].layer;
        self.container_on_removing(id, carrier, true);
        self.object_on_removed_from(id);
    }

    /// Container virtual `onRemoving` runs after list erasure, before the rider's
    /// `Object::onRemovedFrom` clears `containedBy`/MASKED/UNSELECTABLE.
    fn container_on_removing(&mut self, id: usize, carrier: usize, normal: bool) {
        let bike = self.catalog[self.entities[carrier].type_index]
            .combat_bike
            .is_some();
        if bike && self.entities[carrier].is_effectively_dead() {
            self.destroy_maintenance_object(id);
            return;
        }
        let parachute = self.entities[carrier].parachute.is_some();
        let stationed = self.entities[id].stationed();
        self.container_removal_sound(carrier, id, normal);
        self.remove_garrison_point(id, carrier);
        self.entities[carrier].containment.waiting.remove(&id);
        let transport = matches!(
            self.catalog[self.entities[carrier].type_index].contain_family(),
            Some(
                "TransportContain"
                    | "InternetHackContain"
                    | "HelixContain"
                    | "OverlordContain"
                    | "RiderChangeContain"
            )
        );
        if transport {
            let slots = i32::from_ne_bytes(
                self.catalog[self.entities[id].type_index]
                    .transport_slots
                    .to_ne_bytes(),
            );
            self.entities[carrier].containment.extra_slots_in_use = self.entities[carrier]
                .containment
                .extra_slots_in_use
                .wrapping_sub(slots.wrapping_sub(1));
        }
        if !matches!(
            self.catalog[self.entities[carrier].type_index].contain_family(),
            Some("OpenContain" | "HealContain")
        ) {
            self.clear_disabled_held(id);
        }
        self.entities[id].helix.passenger_bonus = false;
        self.entities[id].containment.placement = super::GarrisonPlacement::None;
        // Parachute landing/rally/damage callbacks retain the source parent
        // pointer even after PARACHUTING is cleared.
        if parachute {
            self.parachute_remove(id, carrier);
        }
        let delivery = self.catalog[self.entities[carrier].type_index]
            .deliver_payload
            .is_some();
        if delivery {
            self.delivery_transport_on_removing(id, carrier);
        } else {
            self.transport_exit_orientation(id, carrier);
            if transport && self.height_above_ground(carrier) > 0.0 {
                // TransportContain::onRemoving overrides the rider's ground
                // locomotor until its PhysicsBehavior reaches the terrain.
                self.entities[id].set_physics_allow_fall(true);
            }
        }
        if !stationed && !parachute && !delivery {
            self.entities[id].position = self.entities[carrier].position;
        }
        // GarrisonContain::onRemoving preserves station XY and resets Z from
        // that actual XY (including the original Fire Base sale altitude fix).
        if !parachute && !delivery {
            self.entities[id].containment.height = self.ground_height(self.entities[id].position);
        }
        if !self.is_garrison(carrier) && !parachute {
            self.set_order(id, Order::Idle);
        }
        if let Some(rules) = &self.catalog[self.entities[carrier].type_index].contain {
            if rules.exit_flags & 1 != 0 {
                self.entities[id].ai.mood.attitude = 2;
            }
            let reset_mood = rules.exit_flags & 2 != 0;
            let exit_frames = rules.exit_frames;
            if self.entities[carrier].is_effectively_dead()
                && matches!(
                    self.catalog[self.entities[carrier].type_index].contain_family(),
                    Some(
                        "TransportContain"
                            | "InternetHackContain"
                            | "HelixContain"
                            | "OverlordContain"
                    )
                )
            {
                self.scatter_passenger(id, carrier);
            }
            if reset_mood {
                self.entities[id].ai.mood.next_check = self.tick;
            }
            self.entities[carrier].containment.exit_ready = self.tick + u64::from(exit_frames);
        }
        self.recalculate_garrison_owner(carrier);
        self.update_armed_rider_weapons(carrier);
        if bike {
            self.bike_dismount(carrier, id);
        }
    }

    fn object_on_removed_from(&mut self, id: usize) {
        self.entities[id].maintenance.masked = false;
        self.entities[id].maintenance.instance_flags &=
            !crate::skirmish::maintenance::INSTANCE_UNSELECTABLE;
        self.entities[id].containment.carrier = None;
        self.entities[id].containment.entered_at = 0;
        self.update_position_claim(id);
        self.observe_ground_partition(id);
    }

    pub(in crate::skirmish) fn containment_death(&mut self, target: usize) {
        if let Some(carrier) = self.entities[target].contained_by()
            && self.passengers(carrier).contains(&target)
        {
            self.remove_passenger(target, carrier);
        }
        if self.is_tunnel(target) {
            if self.entities[target].containment.tunnel.is_none()
                && self.entities[target].containment.cave.is_none()
            {
                self.unregister_tunnel(target);
            }
            return;
        }
        // CRC=0 performs the transport eligibility deaths before its damage
        // percentage, then uses OpenContain's literal-list removal callbacks.
        let transport = matches!(
            self.catalog[self.entities[target].type_index].contain_family(),
            Some(
                "TransportContain"
                    | "InternetHackContain"
                    | "HelixContain"
                    | "OverlordContain"
                    | "RiderChangeContain"
            )
        );
        if transport {
            let death = if self
                .container_rules(target)
                .is_some_and(|rules| rules.death.burned)
            {
                crate::skirmish::DeathType::Burned
            } else {
                crate::skirmish::DeathType::Normal
            };
            for id in self.passengers(target).to_vec() {
                if !self.free_to_exit(id, target) {
                    if self
                        .container_rules(target)
                        .is_some_and(|rules| rules.death.destroy_if_not_free)
                    {
                        self.destroy_maintenance_object(id);
                    } else {
                        self.kill_object(id, death);
                    }
                }
            }
        }
        let damage = self
            .container_rules(target)
            .map_or(0.0, |rules| rules.death.damage);
        if damage > 0.0 {
            self.process_damage_to_contained(target, damage);
        }
        if self.is_garrison(target) {
            self.remove_garrison_contents(target);
        } else {
            while let Some(&id) = self.passengers(target).first() {
                self.remove_passenger(id, target);
            }
        }
    }

    fn scatter_passenger(&mut self, id: usize, carrier: usize) {
        let angle = startup::random_real(&mut self.random, 0.0, std::f32::consts::TAU);
        let minimum = self.object_radius(carrier);
        let maximum = minimum + minimum / 2.0;
        let distance = startup::random_real(&mut self.random, minimum, maximum);
        let position = self.entities[carrier].position;
        let x = distance * angle.cos();
        let y = distance * angle.sin();
        let point = [position[0] + x, position[1] + y];
        let height = self.movement_terrain.destination_layer_height(
            self.entities[carrier].layer,
            point,
            self.ground_height(point),
        );
        let goal = [point[0], point[1], height];
        self.set_object_orientation(id, angle);
        if self.entities[id].ai.registration.is_some() {
            self.set_object_position(id, self.object_position(carrier));
            self.entities[id].ai.ignored_producer = Some(carrier);
            if !self.entities[id].ai.command.locked {
                self.entities[id].ai.command.goal_object = None;
            }
            self.construction_move(id, goal);
        } else {
            self.set_object_position(id, goal);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Faction, Kind, LocomotorVariant, UnitAiUpdate, containment, tests};
    use crate::terrain::{BridgeGeometry, TerrainMap};

    #[test]
    fn transport_exit_checks_current_locomotor_and_carrier_bridge_layer() {
        let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
        let water = crate::Cell { x: 35, y: 25 };
        let mut terrain = TerrainMap::classify(80, 60, [], [water]).unwrap();
        let layer = terrain
            .add_bridge(
                BridgeGeometry {
                    from: [300.0, 255.0, 40.0],
                    to: [400.0, 255.0, 40.0],
                    corners: [
                        [300.0, 275.0, 40.0],
                        [300.0, 235.0, 40.0],
                        [400.0, 275.0, 40.0],
                        [400.0, 235.0, 40.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        game.configure_movement_terrain(terrain).unwrap();
        let mut swimming = tests::fixture_type(Faction::Gla, Kind::Infantry).locomotor;
        swimming.legal_surfaces = 2;
        game.catalog[5]
            .locomotor_sets
            .normal
            .push(LocomotorVariant {
                speed: 20.0,
                locomotor: swimming,
            });
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.board(rider, carrier);
        game.entities[carrier].layer = layer;
        assert!(game.free_to_exit(rider, carrier));
        game.entities[carrier].layer = 1;
        assert!(!game.free_to_exit(rider, carrier));
        game.select_surface_locomotor(rider);
        assert_eq!(game.entities[rider].ai.surface_locomotor, 1);
        assert!(game.free_to_exit(rider, carrier));
    }

    #[test]
    fn airborne_transport_allows_ailess_rider_but_nested_transport_does_not() {
        let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
        let mut cargo = tests::fixture_type(Faction::Gla, Kind::Infantry);
        cargo.ai.update = UnitAiUpdate::Absent;
        cargo.behavior_modules.clear();
        cargo.update_modules.retain(|module| {
            matches!(
                module,
                crate::skirmish::module_updates::ModuleRegistration::Helper(_)
            )
        });
        let kind = game.catalog.len();
        game.catalog.push(cargo);
        let rider = game.spawn(kind, 0, [355.0, 255.0], true);
        game.board(rider, carrier);
        assert!(game.entities[rider].ai.registration.is_none());
        assert!(!game.free_to_exit(rider, carrier));
        let host_type = game.entities[carrier].type_index;
        game.catalog[host_type].locomotor.legal_surfaces = 8;
        assert!(game.free_to_exit(rider, carrier));
        game.catalog[host_type]
            .contain
            .as_mut()
            .unwrap()
            .allowed
            .push("VEHICLE".into());
        let outer = game.spawn(host_type, 0, [355.0, 255.0], true);
        game.board(carrier, outer);
        assert_eq!(game.entities[carrier].contained_by(), Some(outer));
        assert!(!game.free_to_exit(rider, carrier));
    }

    #[test]
    fn destroy_ineligible_riders_queues_destruction_without_applying_death_damage() {
        let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
        let host_type = game.entities[carrier].type_index;
        game.catalog[host_type]
            .contain
            .as_mut()
            .unwrap()
            .death
            .destroy_if_not_free = true;
        let mut cargo = tests::fixture_type(Faction::Gla, Kind::Infantry);
        cargo.ai.update = UnitAiUpdate::Absent;
        cargo.behavior_modules.clear();
        cargo.update_modules.retain(|module| {
            matches!(
                module,
                crate::skirmish::module_updates::ModuleRegistration::Helper(_)
            )
        });
        let kind = game.catalog.len();
        game.catalog.push(cargo);
        let rider = game.spawn(kind, 0, [355.0, 255.0], true);
        game.board(rider, carrier);
        assert!(!game.free_to_exit(rider, carrier));
        game.kill_object(carrier, crate::skirmish::DeathType::Normal);
        assert!(game.update_objects[game.entities[rider].update_owner].destroy_requested);
        assert_eq!(
            game.entities[rider].health.to_bits(),
            game.entities[rider].max_health.to_bits()
        );
        assert!(game.entities[rider].contained_by().is_none());
    }
}

#[cfg(test)]
mod removal_tests;
