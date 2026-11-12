use super::TunnelRules;
use crate::skirmish::{
    Faction, Kind, Order, Skirmish,
    containment::{Art, PassengerWeapons, Rules},
    tests::{fixture_type, game},
};

fn setup() -> (Skirmish, usize, usize) {
    with_tunnels(game())
}

fn with_tunnels(game: Skirmish) -> (Skirmish, usize, usize) {
    with_networks(game, None)
}

fn with_networks(mut game: Skirmish, cave: Option<usize>) -> (Skirmish, usize, usize) {
    let mut unit = fixture_type(Faction::Gla, Kind::Defense);
    unit.name = "GLATunnelNetwork".into();
    unit.behavior.production_exit = None;
    unit.weapon.damage = 0.0;
    unit.contain = Some(Rules {
        cave_index: cave,
        payload: None,
        payload_templates: Vec::new(),
        experience_sink_for_rider: true,
        passengers_in_turret: false,
        orient_like_container_on_exit: false,
        assault_heal_ratio: None,
        heal: None,
        tunnel: Some(TunnelRules {
            full_heal_frames: 150.0,
            conditions: crate::skirmish::death_callbacks::Conditions {
                death_mask: u32::MAX,
                veterancy_mask: 15,
                exempt_status: 0,
                required_status: 0,
            },
        }),
        garrison: None,
        capacity: 10,
        allowed: vec!["INFANTRY".into()],
        forbidden: Vec::new(),
        weapons: PassengerWeapons {
            pass_bonuses: false,
            fire: false,
            upgrade_transport: false,
        },
        relationships: crate::skirmish::containment::Relationships {
            allies: true,
            enemies: true,
            neutral: true,
        },
        exit_flags: 0,
        exit_frames: 0,
        door_frames: 1,
        exit_paths: 1,
        regen_percent: 0.0,
        death: crate::skirmish::containment::RiderDeath {
            damage: 0.0,
            burned: false,
            destroy_if_not_free: false,
        },
        damage_thresholds: [0.5, 0.1],
        art: std::array::from_fn(|_| Art {
            exits: vec![([0.0, -15.0, 0.0], [0.0, -40.0, 0.0])],
            fire_points: Vec::new(),
        }),
    });
    crate::skirmish::tests::register_update(
        &mut unit,
        if cave.is_some() {
            "CaveContain"
        } else {
            "TunnelContain"
        },
    );
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let first = game.spawn(kind, 0, [355.0, 255.0], true);
    let second = game.spawn(kind, 0, [555.0, 255.0], true);
    if cave.is_some() {
        game.native_build_complete(first);
        game.native_build_complete(second);
    }
    game.rebuild_navigation();
    (game, first, second)
}

fn board(game: &mut Skirmish, id: usize, endpoint: usize) {
    game.entities[id].position = game.entities[endpoint].position;
    game.enter(id, endpoint).unwrap();
    assert!(game.container_contact(id, endpoint));
}

#[test]
fn shared_capacity_counts_objects_in_fifo_order_instead_of_transport_slots() {
    let (mut game, first, second) = setup();
    game.catalog[6].transport_slots = 5;
    let kind = game.entities[first].type_index;
    let rules = game.catalog[kind].contain.as_mut().unwrap();
    rules.relationships.allies = false;
    rules.forbidden.push("VEHICLE".into());
    let mut riders = Vec::new();
    for index in 0..10 {
        let id = game.spawn(if index % 2 == 0 { 5 } else { 6 }, 0, [0.0; 2], true);
        board(&mut game, id, if index % 2 == 0 { first } else { second });
        riders.push(id);
    }
    assert_eq!(game.passengers(first), riders);
    assert_eq!(game.passengers(second), riders);
    let overflow = game.spawn(5, 0, [355.0, 255.0], true);
    assert!(!game.can_enter(overflow, first));
    assert!(!game.can_enter(overflow, second));
    assert!(!game.can_enter(riders[0], second));
    assert!(game.entities[riders[0]].masked());
    assert!(
        !game.entities[riders[0]].selectable(&game.catalog[game.entities[riders[0]].type_index])
    );
    assert!(!game.passenger_can_fire(riders[0]));
    game.exit_container_at(riders[0], second).unwrap();
    game.update_container_exit(riders[0]);
    assert!(game.can_enter(overflow, first));
}

#[test]
fn remote_exit_uses_chosen_endpoint_bones_and_rally_without_rebinding_entry() {
    let (mut game, first, second) = setup();
    let rider = game.spawn(5, 0, [0.0; 2], true);
    board(&mut game, rider, first);
    let rally = [605.0, 355.0];
    game.set_rally_point(second, [rally[0], rally[1], 0.0])
        .unwrap();
    game.exit_container_at(rider, second).unwrap();
    assert_eq!(game.entities[rider].contained_by(), Some(first));
    assert!(game.entities[second].containment.waiting.contains(&rider));
    assert!(!game.entities[first].containment.waiting.contains(&rider));
    game.update_container_exit(rider);
    assert!(game.entities[rider].contained_by().is_none());
    assert!(game.passengers(first).is_empty());
    assert!(game.passengers(second).is_empty());
    assert_eq!(
        game.entities[rider].position.map(f32::to_bits),
        [555.0_f32, 240.0].map(f32::to_bits)
    );
    assert!(matches!(game.entities[rider].ai.order, Order::FollowPath));
    assert_eq!(
        game.entities[rider]
            .ai
            .waypoints
            .points
            .last()
            .map(|point| [point[0], point[1]]),
        Some(rally)
    );
    assert!(!game.entities[second].containment.waiting.contains(&rider));
    assert!(game.entities[second].container_door_open(game.tick));
}

#[test]
fn selected_tunnel_rally_uses_center_and_first_damaged_rotated_exit_end() {
    let (mut game, _, second) = setup();
    let kind = game.entities[second].type_index;
    let rules = game.catalog[kind].contain.as_mut().unwrap();
    rules.art[1].exits = vec![
        ([0.0, -15.0, 0.0], [0.0, -20.0, 0.0]),
        ([0.0, 15.0, 0.0], [0.0, 40.0, 0.0]),
    ];
    game.set_object_orientation(second, std::f32::consts::FRAC_PI_2);
    game.entities[second].health = game.entities[second].max_health * 0.4;
    game.entities[second].containment.next_exit = 1;
    let rally = [605.0, 355.0];
    game.set_rally_point(second, [rally[0], rally[1], 0.0])
        .unwrap();
    let path = game.producer_rally_path(second).unwrap();
    assert_eq!(path.len(), 3);
    for (actual, expected) in path.iter().zip([[555.0, 255.0], [575.0, 255.0], rally]) {
        assert!((actual[0] - expected[0]).abs() < 0.001);
        assert!((actual[1] - expected[1]).abs() < 0.001);
    }
    game.catalog[kind].contain.as_mut().unwrap().art[1]
        .exits
        .clear();
    let path = game.producer_rally_path(second).unwrap();
    assert_eq!(path[0].map(f32::to_bits), path[1].map(f32::to_bits));
}

#[test]
fn admission_rejects_aircraft_foreign_and_zero_slot_units_before_contact() {
    let (mut game, first, _) = setup();
    let enemy = game.spawn(5, 1, [355.0, 255.0], true);
    assert!(!game.can_enter(enemy, first));
    let mut air = fixture_type(Faction::Gla, Kind::Vehicle);
    air.kind_of = vec!["AIRCRAFT".into()];
    air.transport_slots = 1;
    let index = game.catalog.len();
    game.catalog.push(air);
    let aircraft = game.spawn(index, 0, [355.0, 255.0], true);
    assert!(!game.can_enter(aircraft, first));
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.catalog[5].transport_slots = 0;
    assert!(!game.can_enter(rider, first));
    game.catalog[5].transport_slots = 1;
    game.entities[first].build_progress = 0.5;
    assert!(!game.can_enter(rider, first));
}

#[test]
fn configured_source_healing_stacks_per_endpoint_and_resets_after_entry_loss() {
    let (mut game, first, second) = setup();
    let rider = game.spawn(5, 0, [0.0; 2], true);
    board(&mut game, rider, first);
    game.entities[rider].max_health = 150.0;
    game.entities[rider].health = 10.0;
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), 12.0_f32.to_bits());
    game.tick = 149;
    game.entities[first].health = 0.0;
    game.finish_death(first, None);
    assert_eq!(game.entities[rider].contained_by(), Some(second));
    assert_eq!(game.entities[rider].containment.entered_at, 149);
    game.tick = 150;
    game.run_object_updates();
    // Native TunnelContain keeps updating after onDie unregisters it, until actual deletion.
    assert_eq!(game.entities[rider].health.to_bits(), 14.0_f32.to_bits());
    game.tick = 299;
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), 150.0_f32.to_bits());
}

#[test]
fn zero_duration_heals_immediately_on_the_entry_frame() {
    let (mut game, first, _) = setup();
    let kind = game.entities[first].type_index;
    game.catalog[kind]
        .contain
        .as_mut()
        .unwrap()
        .tunnel
        .as_mut()
        .unwrap()
        .full_heal_frames = 0.0;
    let rider = game.spawn(5, 0, [0.0; 2], true);
    board(&mut game, rider, first);
    game.entities[rider].health = 1.0;
    game.update_tunnel(first);
    assert_eq!(
        game.entities[rider].health.to_bits(),
        game.entities[rider].max_health.to_bits()
    );
}

#[test]
fn losing_one_endpoint_retains_cargo_but_cave_in_deletes_nested_cargo_without_kill_credit() {
    let (game, carrier) = crate::skirmish::containment::tests::setup(0.0, 0.5);
    let (mut game, first, second) = with_tunnels(game);
    let rider = game.spawn(5, 0, [0.0; 2], true);
    // A transport may itself enter a tunnel while holding ordinary passengers.
    board(&mut game, rider, carrier);
    board(&mut game, carrier, first);
    let losses = game.players[0].statistics.units_lost;
    game.entities[first].health = 0.0;
    game.finish_death(first, None);
    assert!(game.entities[carrier].alive());
    assert_eq!(game.entities[carrier].contained_by(), Some(second));
    game.entities[second].health = 0.0;
    game.finish_death(second, None);
    assert!(game.update_objects[game.entities[carrier].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.entities[carrier].exists());
    assert!(!game.entities[rider].exists());
    assert!(game.entities[carrier].contained_by().is_none());
    assert!(game.entities[rider].contained_by().is_none());
    assert_eq!(game.players[0].statistics.units_lost, losses);
    assert!(game.passengers(second).is_empty());
    game.unregister_tunnel(second); // onDelete after onDie is harmless.
}

#[test]
fn simultaneous_sales_eject_at_the_last_endpoint_instead_of_caving_in() {
    let (mut game, first, second) = setup();
    let rider = game.spawn(5, 0, [0.0; 2], true);
    board(&mut game, rider, first);
    game.ai_busy_from_ai(rider);
    game.sell(0, first).unwrap();
    assert_eq!(game.entities[rider].contained_by(), Some(second));
    game.sell(0, second).unwrap();
    assert!(game.entities[rider].alive());
    assert!(game.entities[rider].contained_by().is_none());
    assert_eq!(
        game.entities[rider].position.map(f32::to_bits),
        game.entities[second].position.map(f32::to_bits)
    );
    assert!(game.entities[rider].ai_busy());
    assert!(game.tunnels[0].endpoints.is_empty());
    game.destroy_maintenance_object(first);
    game.destroy_maintenance_object(second);
    assert!(game.entities[rider].alive());
}

#[test]
fn creation_registers_unfinished_endpoints_before_build_completion_and_restart_clears_them() {
    let (mut game, first, second) = setup();
    let kind = game.entities[first].type_index;
    let foundation = game.spawn(kind, 0, [405.0, 105.0], false);
    let rider = game.spawn(5, 0, [0.0; 2], true);
    board(&mut game, rider, first);
    game.destroy_maintenance_object(second);
    assert!(game.entities[rider].alive());
    game.destroy_maintenance_object(first);
    assert!(game.entities[rider].alive());
    assert_eq!(game.entities[rider].contained_by(), Some(foundation));
    assert!(game.exit_container_at(rider, foundation).is_err());
    assert!(game.entities[foundation].alive());
    assert_eq!(game.tunnels[0].endpoints, [foundation]);
    game.process_update_destruction();
    game.entities[rider].health = 10.0;
    game.entities[rider].max_health = 150.0;
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), 11.0_f32.to_bits());
    game.entities[foundation].build_progress = 1.0;
    game.created_tunnel(foundation);
    assert_eq!(game.tunnels[0].endpoints, [foundation]);
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert!(
        game.tunnels
            .iter()
            .all(|network| network.endpoints.is_empty() && network.passengers.is_empty())
    );
}

#[test]
fn pending_exit_cancellation_does_not_leave_remote_endpoint_waiters() {
    let (mut game, first, second) = setup();
    let rider = game.spawn(5, 0, [0.0; 2], true);
    board(&mut game, rider, first);
    game.exit_container_at(rider, second).unwrap();
    game.exit_container_at(rider, first).unwrap();
    assert!(game.entities[first].containment.waiting.contains(&rider));
    assert!(!game.entities[second].containment.waiting.contains(&rider));
    game.entities[first].health = 0.0;
    game.finish_death(first, None);
    game.update_container_exit(rider);
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert_eq!(game.entities[rider].contained_by(), Some(second));
    assert!(!game.entities[first].containment.waiting.contains(&rider));
    game.exit_container_at(rider, second).unwrap();
    game.update_container_exit(rider);
    assert!(game.entities[rider].contained_by().is_none());
}

#[test]
fn owner_transfer_moves_only_the_endpoint_and_preserves_each_players_network() {
    let (mut game, first, second) = setup();
    let kind = game.entities[first].type_index;
    let foreign = game.spawn(kind, 1, [455.0, 355.0], true);
    let own_rider = game.spawn(5, 0, [0.0; 2], true);
    let foreign_rider = game.spawn(5, 1, [0.0; 2], true);
    board(&mut game, own_rider, first);
    board(&mut game, foreign_rider, foreign);
    // Base tunnels are immune to the ordinary capture ability. This exercises
    // the generic Object::onCapture lifecycle used by owner changes.
    game.transfer_maintenance_owner(first, 1);
    assert_eq!(game.entities[own_rider].contained_by(), Some(second));
    assert_eq!(game.passengers(first), &[foreign_rider]);
    assert_eq!(game.passengers(second), &[own_rider]);
    assert!(game.exit_container_at(own_rider, first).is_err());
    game.exit_container_at(foreign_rider, first).unwrap();
    game.update_container_exit(foreign_rider);
    assert!(game.entities[foreign_rider].contained_by().is_none());
    game.transfer_maintenance_owner(second, 1);
    assert!(game.update_objects[game.entities[own_rider].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.entities[own_rider].exists());
    assert!(game.tunnels[0].endpoints.is_empty());
    assert_eq!(game.tunnels[1].endpoints, [foreign, first, second]);
}

#[test]
fn cave_indices_preserve_shared_passengers_capture_restore_and_refuse_occupied_switches() {
    let (mut game, first, second) = with_networks(game(), Some(0));
    let prototype = game
        .team_membership
        .add_prototype(game.neutral_owner(), "teamPlyrCivilian".into());
    game.team_membership
        .set_prototype_singleton(prototype, true);
    let original_team = game.team_membership.create_team(prototype);
    for endpoint in [first, second] {
        game.entities[endpoint].owner = game.neutral_owner();
        game.register_map_member(endpoint, "teamPlyrCivilian".into());
    }
    game.set_cave_index(first, 1);
    game.set_cave_index(second, 1);
    assert_eq!(game.team_membership.team_of(first), Some(original_team));
    assert_eq!(game.team_membership.team_of(second), Some(original_team));
    let rider = game.spawn(6, 0, [355.0, 255.0], true);
    board(&mut game, rider, first);
    assert_eq!(game.entities[first].owner, 0);
    assert_eq!(game.entities[second].owner, 0);
    assert_eq!(game.passengers(second), [rider]);
    game.set_cave_index(first, 2);
    assert_eq!(game.entities[first].cave_index(), Some(1));
    game.remove_passenger(rider, second);
    assert!(game.entities[rider].alive());
    assert_eq!(game.entities[first].owner, game.neutral_owner());
    assert_eq!(game.entities[second].owner, game.neutral_owner());
    assert_eq!(game.team_membership.team_of(first), Some(original_team));
    assert_eq!(game.team_membership.team_of(second), Some(original_team));
    assert_eq!(
        game.team_membership.team_name(
            game.team_membership
                .team_of(first)
                .expect("object native team")
        ),
        "teamPlyrCivilian"
    );
    game.set_cave_index(first, 2);
    assert_eq!(game.entities[first].cave_index(), Some(2));
    assert!(game.passengers(first).is_empty());
    assert!(game.passengers(second).is_empty());
}

#[test]
fn authored_tunnel_update_uses_body_healing_armor_and_disabled_mask() {
    use crate::skirmish::DamageType;
    let (mut game, first, second) = setup();
    game.catalog[5].armor[DamageType::Healing as usize] = 0.5;
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.add_to_contain(rider, first);
    game.entities[rider].max_health = 150.0;
    game.entities[rider].health = 10.0;
    game.record_body_damage(rider, Some(second), DamageType::Explosion);
    game.set_hacked_until(first, crate::skirmish::ai_dispatch::native_frame(2));
    game.set_hacked_until(second, crate::skirmish::ai_dispatch::native_frame(2));
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), 10.0_f32.to_bits());
    game.tick = 2;
    game.expire_hacked();
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), 11.0_f32.to_bits());
    assert_eq!(game.entities[rider].damage_state.retained_source, None);
    assert!(matches!(
        game.entities[rider].damage_state.damage_type,
        DamageType::Healing
    ));
    assert_eq!(game.players[0].academy.units_entered_tunnel, 1);
}

#[test]
fn native_tunnel_removal_retains_ai_order_and_uses_endpoint_xyz_before_world_registration() {
    let (mut game, first, second) = setup();
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.add_to_contain(rider, first);
    game.ai_busy_from_ai(rider);
    game.set_object_position(second, [555.0, 255.0, 73.0]);
    let position = game.object_position(second);
    game.tick = 7;
    game.remove_passenger(rider, second);
    assert!(game.entities[rider].ai_busy());
    assert_eq!(game.entities[rider].contained_by(), None);
    assert_eq!(game.entities[rider].containment.entered_at, 0);
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        position.map(f32::to_bits)
    );
    game.update_ground_contacts();
    assert!(
        game.partition
            .range_objects([position[0], position[1]], 1.0)
            .contains(&rider)
    );
    assert!(!game.entities[rider].disabled_held());
    assert!(!game.entities[rider].drawable_effectively_hidden());
    assert_eq!(game.entities[rider].drawable.safe_occlusion_frame, 97);
    assert!(game.passengers(first).is_empty());
}

#[test]
fn filtered_tunnel_death_retains_registration_until_authored_delete() {
    let (mut game, first, second) = setup();
    let kind = game.entities[first].type_index;
    game.catalog[kind]
        .contain
        .as_mut()
        .unwrap()
        .tunnel
        .as_mut()
        .unwrap()
        .conditions
        .death_mask = 0;
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.add_to_contain(rider, first);
    game.entities[first].health = 0.0;
    game.finish_death(first, None);
    assert_eq!(game.tunnels[0].endpoints, [first, second]);
    assert_eq!(game.entities[rider].contained_by(), Some(first));
    game.destroy_maintenance_object(first);
    assert_eq!(game.tunnels[0].endpoints, [second]);
    assert_eq!(game.entities[rider].contained_by(), Some(second));
    game.destroy_maintenance_object(second);
    assert!(game.update_objects[game.entities[rider].update_owner].destroy_requested);
    assert!(game.entities[rider].health > 0.0);
    assert!(game.entities[rider].disabled_held());
    assert_eq!(game.entities[rider].containment.entered_at, 0);
}

#[test]
fn tunnel_damage_retains_first_enemy_until_strict_nemesis_expiry() {
    use crate::skirmish::DamageType;
    let (mut game, first, _) = setup();
    let a = game.spawn(6, 1, [700.0, 400.0], true);
    let b = game.spawn(6, 1, [700.0, 450.0], true);
    game.tick = 10;
    game.record_body_damage(first, Some(a), DamageType::Explosion);
    game.run_object_updates();
    assert_eq!(game.tunnels[0].nemesis, Some(a));
    game.tick = 130;
    game.record_body_damage(first, Some(b), DamageType::Explosion);
    game.run_object_updates();
    assert_eq!(game.tunnels[0].nemesis, Some(a));
    game.tick = 131;
    game.run_object_updates();
    assert_eq!(game.tunnels[0].nemesis, Some(b));
    assert_eq!(game.tunnels[0].nemesis_frame, 131);
}

#[test]
fn cave_create_assigns_index_but_only_build_complete_registers_once() {
    let (mut game, first, second) = with_networks(game(), Some(3));
    let kind = game.entities[first].type_index;
    let cave = game.new_object(kind, 0, false);
    assert_eq!(game.entities[cave].cave_index(), Some(3));
    assert_eq!(game.caves[3].as_ref().unwrap().endpoints, [first, second]);
    game.native_build_complete(cave);
    game.native_build_complete(cave);
    assert_eq!(
        game.caves[3].as_ref().unwrap().endpoints,
        [first, second, cave]
    );
    game.set_cave_index(first, 3);
    assert_eq!(
        game.caves[3].as_ref().unwrap().endpoints,
        [second, cave, first]
    );
}

#[test]
fn cave_update_runs_open_doors_and_waiters_without_tunnel_healing() {
    let (mut game, first, _) = with_networks(game(), Some(0));
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.entities[rider].health = 10.0;
    game.add_to_contain(rider, first);
    let state = &mut game.entities[first].containment;
    state.door_close_countdown = 1;
    state.door_flags = 1;
    let dead = game.spawn(5, 0, [755.0, 255.0], true);
    game.entities[first].containment.waiting.insert(dead);
    game.kill_object(dead, crate::skirmish::DeathType::Normal);
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[first].containment.player_entered_mask, 0);
    assert_eq!(game.entities[first].containment.door_flags, 2);
    assert!(!game.entities[first].containment.waiting.contains(&dead));
    assert_eq!(game.entities[rider].health.to_bits(), 10.0_f32.to_bits());
    assert_eq!(game.players[0].academy.units_entered_tunnel, 0);
}

#[test]
fn cave_removal_uses_requested_endpoint_xyz_without_safe_occlusion_or_exposure() {
    use crate::skirmish::containment::SoundHook;
    let (mut game, first, second) = with_networks(game(), Some(0));
    game.set_object_position(second, [555.0, 255.0, 95.0]);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.catalog[5].kind_of.push("STEALTH_GARRISON".into());
    game.add_to_contain(rider, first);
    let safe_frame = game.entities[rider].drawable.safe_occlusion_frame;
    let old_position = game.object_position(rider);
    assert!(game.entities[rider].disabled_held());
    assert!(game.entities[rider].masked());
    assert!(game.entities[rider].drawable_effectively_hidden());
    game.tick = 10;
    game.presentation.container_sounds.clear();
    game.remove_passenger_exposed(rider, second, true);
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        [555.0_f32, 255.0, 95.0].map(f32::to_bits)
    );
    assert_eq!(game.entities[rider].contained_by(), None);
    assert!(!game.entities[rider].disabled_held());
    assert!(!game.entities[rider].masked());
    assert!(!game.entities[rider].detected());
    assert!(!game.entities[rider].drawable_effectively_hidden());
    assert_eq!(
        game.entities[rider].drawable.safe_occlusion_frame,
        safe_frame
    );
    assert_eq!(game.presentation.container_sounds.len(), 3);
    assert!(matches!(
        game.presentation.container_sounds[0].hook,
        SoundHook::SoundExit
    ));
    assert!(matches!(
        game.presentation.container_sounds[1].hook,
        SoundHook::SoundFalling
    ));
    assert_eq!(
        game.presentation.container_sounds[1]
            .position
            .map(f32::to_bits),
        old_position.map(f32::to_bits)
    );
    assert!(matches!(
        game.presentation.container_sounds[2].hook,
        SoundHook::ExitSound
    ));
    game.remove_passenger(rider, second);
    assert_eq!(game.presentation.container_sounds.len(), 3);
}

#[test]
fn cave_die_reparents_shared_riders_then_last_endpoint_destroys_them() {
    let (mut game, first, second) = with_networks(game(), Some(0));
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.add_to_contain(rider, first);
    game.kill_object(first, crate::skirmish::DeathType::Normal);
    assert_eq!(game.entities[rider].contained_by(), Some(second));
    assert_eq!(game.caves[0].as_ref().unwrap().endpoints, [second]);
    assert!(game.entities[rider].disabled_held());
    game.kill_object(second, crate::skirmish::DeathType::Normal);
    assert!(game.caves[0].as_ref().unwrap().endpoints.is_empty());
    assert!(game.caves[0].as_ref().unwrap().passengers.is_empty());
    assert!(game.update_objects[game.entities[rider].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.entities[rider].exists());
}

#[test]
fn cave_direct_delete_keeps_tracker_entry_but_under_construction_die_never_registers() {
    let (mut game, first, second) = with_networks(game(), Some(0));
    let kind = game.entities[first].type_index;
    let unfinished = game.new_object(kind, 0, false);
    game.kill_object(unfinished, crate::skirmish::DeathType::Normal);
    assert_eq!(game.caves[0].as_ref().unwrap().endpoints, [first, second]);
    game.destroy_maintenance_object(first);
    game.process_update_destruction();
    // CaveContain's destructor and inherited onDelete do not unregister.
    assert_eq!(game.caves[0].as_ref().unwrap().endpoints, [first, second]);
    let rider = game.spawn(5, 0, [555.0, 255.0], true);
    game.add_to_contain(rider, second);
    assert_eq!(game.entities[rider].contained_by(), Some(second));
    game.kill_object(second, crate::skirmish::DeathType::Normal);
    assert_eq!(game.entities[rider].contained_by(), None);
    assert!(game.entities[rider].alive());
    assert!(!game.entities[rider].masked());
    assert!(game.entities[rider].disabled_held());
    assert!(game.entities[rider].containment.nested_removed);
    assert!(game.entities[rider].drawable_effectively_hidden());
    assert_ne!(
        game.entities[rider].maintenance.instance_flags
            & crate::skirmish::maintenance::INSTANCE_UNSELECTABLE,
        0
    );
    assert_eq!(game.entities[rider].containment.entered_at, game.tick);
    assert_eq!(game.caves[0].as_ref().unwrap().passengers, [rider]);
}

#[test]
fn cave_inherited_open_kill_all_leaves_tracker_while_tunnel_kills_only_owner_shared_list() {
    let (game, tunnel, other_tunnel) = setup();
    let (mut game, cave, other_cave) = with_networks(game, Some(3));
    let tunnel_rider = game.spawn(5, 0, [355.0, 255.0], true);
    let cave_rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.add_to_contain(tunnel_rider, tunnel);
    game.add_to_contain(cave_rider, cave);
    assert_eq!(game.passengers(other_tunnel), [tunnel_rider]);
    assert_eq!(game.passengers(other_cave), [cave_rider]);
    assert!(game.entities[cave].containment.passengers.is_empty());
    // CaveContain inherits OpenContain::killAllContained, whose literal list
    // does not contain the riders stored by Cave's virtual addToContainList.
    game.kill_all_contained(cave);
    assert!(game.entities[cave_rider].alive());
    assert_eq!(game.entities[cave_rider].contained_by(), Some(cave));
    assert_eq!(game.passengers(other_cave), [cave_rider]);
    assert_eq!(game.passengers(other_tunnel), [tunnel_rider]);
    game.kill_all_contained(tunnel);
    assert!(!game.entities[tunnel_rider].alive());
    assert!(game.entities[tunnel_rider].contained_by().is_none());
    assert!(game.passengers(tunnel).is_empty());
    assert!(game.passengers(other_tunnel).is_empty());
    assert!(game.entities[cave_rider].alive());
    assert_eq!(game.passengers(cave), [cave_rider]);
}
