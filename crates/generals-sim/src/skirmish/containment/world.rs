//! Native `OpenContain` world membership and Drawable hidden state, SH 2bfaea60.
use crate::skirmish::{Entity, Skirmish};

impl Entity {
    #[must_use]
    pub const fn drawable_effectively_hidden(&self) -> bool {
        self.drawable.hidden || self.drawable.hidden_by_stealth
    }
}

impl Skirmish {
    pub fn set_drawable_hidden_by_stealth(&mut self, id: usize, hidden: bool) {
        if self.entities[id].has_drawable() {
            self.entities[id].drawable.hidden_by_stealth = hidden;
        }
    }

    pub(in crate::skirmish) fn set_drawable_hidden(&mut self, id: usize, hidden: bool) {
        if self.entities[id].has_drawable() {
            self.entities[id].drawable.hidden = hidden;
        }
    }

    pub(in crate::skirmish) fn container_encloses(&self, carrier: usize, rider: usize) -> bool {
        let unit = &self.catalog[self.entities[carrier].type_index];
        match unit.contain_family() {
            Some("ParachuteContain") => false,
            Some("OverlordContain") => {
                self.entities[carrier].containment.passengers.first() != Some(&rider)
            }
            Some("HelixContain") => self.entities[carrier].attachment_child() != Some(rider),
            Some("GarrisonContain") => unit
                .contain
                .as_ref()
                .and_then(|rules| rules.garrison.as_ref())
                .is_none_or(|rules| rules.stations.is_none()),
            _ => true,
        }
    }

    pub(in crate::skirmish) fn container_kicks_on_capture(&self, carrier: usize) -> bool {
        match self.catalog[self.entities[carrier].type_index].contain_family() {
            None | Some("TunnelContain" | "CaveContain") => false,
            Some("OverlordContain") => {
                let redirect = self.redirected_container(carrier);
                redirect != carrier && self.container_kicks_on_capture(redirect)
            }
            _ => true,
        }
    }

    pub(in crate::skirmish) fn containment_world_membership(&mut self, id: usize, add: bool) {
        self.entities[id].containment.nested_removed = !add;
        if add {
            self.partition
                .observe(id, self.object_position(id), self.entities[id].facing);
            self.set_drawable_hidden(id, false);
            self.restore_object_obstacle(id);
        } else {
            self.entities[id].ai.command.ai_group = None;
            self.partition.remove(id);
            self.set_drawable_hidden(id, true);
            self.unregister_object_obstacle(id);
            self.remove_position(id);
            self.remove_goal(id);
        }
        // Native virtual getter redirects Overlord contents; Helix portable stays outside its list.
        let riders = self.passengers(id).to_vec();
        for rider in riders {
            if !self.container_encloses(id, rider) {
                self.containment_world_membership(rider, add);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Faction, Kind, containment, tests};

    #[test]
    fn enclosing_transport_hides_nested_open_station_riders_and_restores_them_on_removal() {
        let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
        let mut unit = tests::fixture_type(Faction::Gla, Kind::Vehicle);
        let mut rules = containment::tests::rules(0.0, 0.0);
        rules.garrison = Some(containment::GarrisonRules {
            allow_enemies: false,
            allow_neutral: false,
            immune_to_clear: false,
            heal_frames: None,
            stations: Some(vec![[0.0, 0.0, 5.0]]),
        });
        unit.contain = Some(rules);
        tests::register_update(&mut unit, "GarrisonContain");
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let station = game.spawn(kind, 0, [355.0, 255.0], true);
        game.catalog[5].crush.squish_module = Some(game.catalog[5].behavior_modules.len());
        tests::register_behavior(&mut game.catalog[5], "SquishCollide", 0x10);
        let rider = game.spawn(5, 0, [355.0, 335.0], true);
        game.refresh_occupancy();
        assert!(game.occupancy.goal_cell(rider).is_some());
        game.add_to_contain(rider, station);
        game.position_station_passengers(station);
        assert!(!game.entities[rider].drawable_effectively_hidden());
        assert!(!game.entities[rider].containment.nested_removed);
        assert!(!game.entities[rider].masked());
        assert!(game.entities[rider].disabled_held());
        assert!(game.partition.registered(rider));
        assert!(game.occupancy.goal_cell(rider).is_none());
        let enemy_kind = game.type_for(1, Kind::Tank).unwrap();
        let enemy = game.spawn(enemy_kind, 1, [385.0, 255.0], true);
        let station_kind = game.entities[station].type_index;
        let carrier_kind = game.entities[carrier].type_index;
        game.catalog[station_kind]
            .kind_of
            .push("UNATTACKABLE".into());
        game.catalog[carrier_kind]
            .kind_of
            .push("UNATTACKABLE".into());
        game.update_visibility();
        assert_eq!(game.mood_target(enemy, true, true), Some(rider));
        game.add_to_contain(station, carrier);
        assert!(game.entities[station].drawable_effectively_hidden());
        assert!(game.entities[rider].drawable_effectively_hidden());
        assert!(game.entities[rider].containment.nested_removed);
        assert!(!game.entities[rider].masked());
        assert!(game.entities[rider].disabled_held());
        assert!(!game.partition.registered(rider));
        assert_eq!(game.mood_target(enemy, true, true), None);
        game.catalog[enemy_kind].crush.crusher_level = 1;
        game.entities[enemy].ai.motion.velocity = [-1.0, 0.0];
        let position = game.object_position(rider);
        game.set_object_position(enemy, [position[0] + 5.0, position[1], position[2]]);
        game.update_ground_contacts();
        assert!(game.entities[rider].alive());
        let position = game.object_position(rider);
        assert!(matches!(
            game.attack_target_result(enemy, Some(rider), position, false, false, None),
            crate::skirmish::combat::targeting::Result::Possible
        ));
        game.refresh_occupancy();
        assert!(game.occupancy.goal_cell(rider).is_none());
        let cell = game
            .movement_cell(rider, game.entities[rider].position)
            .unwrap();
        assert_ne!(
            game.occupancy
                .position_owner_on(crate::grid::LayerCell { cell, layer: 1 }),
            Some(rider)
        );
        assert_eq!(game.entities[rider].contained_by(), Some(station));
        game.remove_passenger(station, carrier);
        assert!(!game.entities[station].drawable_effectively_hidden());
        assert!(!game.entities[rider].drawable_effectively_hidden());
        assert!(!game.entities[rider].containment.nested_removed);
        assert!(!game.entities[rider].masked());
        assert!(game.partition.registered(rider));
        game.update_visibility();
        assert_eq!(game.mood_target(enemy, true, true), Some(rider));
        assert_eq!(game.entities[rider].contained_by(), Some(station));
        game.update_ground_contacts();
        assert!(!game.entities[rider].alive());
        assert!(matches!(
            game.entities[rider].damage_state.death_type,
            crate::skirmish::DeathType::Crushed
        ));
    }

    #[test]
    fn explicit_and_stealth_hidden_bits_are_independent_and_capture_does_not_eject_overlord_attachment()
     {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::China, Kind::Vehicle);
        unit.contain = Some(containment::tests::rules(0.0, 0.0));
        tests::register_update(&mut unit, "OverlordContain");
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let parent = game.spawn(kind, 0, [355.0, 255.0], true);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.add_to_contain(rider, parent);
        game.set_drawable_hidden(rider, true);
        game.set_drawable_hidden_by_stealth(rider, true);
        game.set_drawable_hidden(rider, false);
        assert!(game.entities[rider].drawable_effectively_hidden());
        game.set_drawable_hidden_by_stealth(rider, false);
        assert!(!game.entities[rider].drawable_effectively_hidden());
        assert!(!game.container_kicks_on_capture(parent));
        let team = game.team_membership.default_team(1);
        game.defect_to_team(parent, team, 0);
        assert_eq!(game.entities[rider].contained_by(), Some(parent));
        assert_eq!(game.entities[parent].owner, 1);
    }
    #[test]
    fn successful_ocl_containment_inherits_effective_source_hidden() {
        use crate::skirmish::object_creation::{CONTAIN_INSIDE_SOURCE, List, Object};
        let mut game = tests::game();
        game.catalog[5].name = "HiddenRiderTemplate".into();
        let mut parent = tests::fixture_type(Faction::China, Kind::Vehicle);
        parent.contain = Some(containment::tests::rules(0.0, 0.0));
        tests::register_update(&mut parent, "OverlordContain");
        parent.behavior.object_creation.push(Some(List {
            name: "HiddenRiderCreation".into(),
            objects: vec![Object {
                container: String::new(),
                invulnerable_time: 0,
                force_intensity: 0.0,
                force_magnitude: [0.0; 2],
                force_pitch: [0.0; 2],
                spin_rate: -1.0,
                formation_distances: [0.0; 3],
                fade_frames: 0,
                fade_sound: String::new(),
                names: vec![game.catalog[5].name.clone()],
                count: 1,
                offset: [0.0; 3],
                disposition: 0,
                health: [1.0; 2],
                lifetime: [0; 2],
                flags: CONTAIN_INSIDE_SOURCE,
            }],
        }));
        tests::register_behavior(&mut parent, "ObjectCreationUpgrade", 0x80);
        parent.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["HiddenRider".into()];
        let kind = game.catalog.len();
        game.catalog.push(parent);
        let parent = game.spawn(kind, 0, [355.0, 255.0], true);
        game.set_drawable_hidden_by_stealth(parent, true);
        game.grant_native_object_upgrade(parent, "HiddenRider");
        let child = game.passengers(parent)[0];
        assert!(game.entities[child].drawable.hidden);
        assert!(!game.entities[child].drawable.hidden_by_stealth);
        assert!(game.entities[child].drawable_effectively_hidden());
        game.set_drawable_hidden_by_stealth(parent, false);
        assert!(!game.entities[parent].drawable_effectively_hidden());
        assert!(game.entities[child].drawable_effectively_hidden());
    }
    #[test]
    fn exposed_stealth_garrison_removal_uses_native_reveal_deadline() {
        use crate::skirmish::stealth;
        let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
        let unit = &mut game.catalog[5];
        unit.kind_of.push("STEALTH_GARRISON".into());
        unit.behavior.stealth = Some(stealth::Rules {
            module_order: unit.behavior_modules.len(),
            delay: 30,
            forbidden: 0,
            threshold_speed: 0.0,
            required_status: vec![],
            forbidden_status: vec![],
            hint_status: vec![],
            innate: true,
            use_rider: false,
            idle_enemies_on_reveal: false,
            wake: stealth::Wake::Automatic,
            upgrades: vec![],
            reveal_distance: 0.0,
            black_market_frames: 0,
            friendly_opacity: [0.5, 1.0],
            pulse_frames: 30,
            disguise: None,
        });
        tests::register_update(unit, "StealthUpdate");
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.tick = 30;
        game.run_object_updates();
        assert!(game.entities[rider].stealthed());
        game.add_to_contain(rider, carrier);
        game.tick = 40;
        game.remove_passenger_exposed(rider, carrier, true);
        game.tick = 41;
        game.run_object_updates();
        assert!(game.entities[rider].detected());
        assert_eq!(game.entities[rider].contained_by(), None);
        assert!(!game.entities[rider].drawable_effectively_hidden());
        game.add_to_contain(rider, carrier);
        game.tick = 50;
        game.remove_passenger(rider, carrier);
        game.tick = 69;
        game.run_object_updates();
        assert!(game.entities[rider].detected());
        game.tick = 70;
        game.run_object_updates();
        assert!(!game.entities[rider].detected());
    }
    #[test]
    fn redirected_overlord_world_recursion_does_not_hide_its_portable_attachment() {
        let (mut game, outer) = containment::tests::setup(0.0, 0.0);
        let mut host = tests::fixture_type(Faction::China, Kind::Vehicle);
        host.contain = Some(containment::tests::rules(0.0, 0.0));
        tests::register_update(&mut host, "OverlordContain");
        let host_kind = game.catalog.len();
        game.catalog.push(host);
        let host = game.spawn(host_kind, 0, [355.0, 255.0], true);
        let mut portable = tests::fixture_type(Faction::China, Kind::Vehicle);
        portable.kind_of = vec!["PORTABLE_STRUCTURE".into()];
        portable.contain = Some(containment::tests::rules(0.0, 0.0));
        tests::register_update(&mut portable, "TransportContain");
        let portable_kind = game.catalog.len();
        game.catalog.push(portable);
        let portable = game.spawn(portable_kind, 0, [355.0, 255.0], true);
        game.add_to_contain(portable, host);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.add_to_contain(rider, portable);
        assert_eq!(game.redirected_container(host), portable);
        assert_eq!(game.passengers(host), &[rider]);
        assert!(!game.container_encloses(host, portable));
        assert!(game.container_encloses(host, rider));
        assert!(!game.entities[portable].drawable_effectively_hidden());
        game.add_to_contain(host, outer);
        assert!(game.entities[host].drawable_effectively_hidden());
        assert!(!game.entities[portable].drawable_effectively_hidden());
        assert!(!game.entities[portable].containment.nested_removed);
        assert!(game.entities[rider].drawable_effectively_hidden());
        assert_eq!(game.entities[portable].contained_by(), Some(host));
        game.remove_passenger(host, outer);
        assert!(!game.entities[host].drawable_effectively_hidden());
        assert!(!game.entities[portable].drawable_effectively_hidden());
        assert!(game.entities[rider].drawable_effectively_hidden());
    }
    #[test]
    fn removing_contained_structure_restores_native_pathfinder_footprint_before_detach() {
        let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
        let kind = game.type_for(0, Kind::Barracks).unwrap();
        let structure = game.spawn(kind, 0, [355.0, 255.0], true);
        game.rebuild_navigation();
        let cell = crate::Cell { x: 35, y: 25 };
        assert!(game.navigation.is_blocked(cell));
        game.add_to_contain(structure, carrier);
        assert!(game.entities[structure].masked());
        assert!(!game.navigation.is_blocked(cell));
        game.remove_passenger(structure, carrier);
        assert_eq!(game.entities[structure].contained_by(), None);
        assert!(!game.entities[structure].masked());
        assert!(game.navigation.is_blocked(cell));
        // Reclassification must agree with the immediate native add callback.
        game.rebuild_navigation();
        assert!(game.navigation.is_blocked(cell));
    }
}
