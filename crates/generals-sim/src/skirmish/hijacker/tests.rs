use super::*;
use crate::skirmish::{Kind, Order, tests::game};

fn fixture(stored: bool) -> (Skirmish, usize, usize) {
    let mut game = game();
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    game.catalog[5].hijacker = Some(Rules {
        required: vec!["VEHICLE".into()],
        forbidden: Vec::new(),
        pickup_science: None,
    });
    game.catalog[5].behavior.hijackers.push(UpdateRules {
        attach_bone: String::new(),
        parachute: "AmericaParachute".into(),
    });
    crate::skirmish::tests::register_update(&mut game.catalog[5], "HijackerUpdate");
    game.catalog[5].weapon = crate::skirmish::WeaponRules::unarmed();
    game.catalog[5].crush.hijacker = true;
    crate::skirmish::tests::register_behavior(
        &mut game.catalog[5],
        "ConvertToHijackedVehicleCrateCollide",
        0x10,
    );
    let vehicle = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[vehicle].kind_of.push("VEHICLE".into());
    if stored {
        let mut conditions = crate::skirmish::tests::death_conditions();
        conditions.veterancy_mask = 14;
        conditions.exempt_status = 1 << 10;
        game.catalog[vehicle]
            .behavior
            .eject_pilots
            .push(crate::skirmish::eject_pilot::Rules {
                conditions,
                ground: None,
                airborne: None,
                sounds: [String::new(), String::new()],
            });
        crate::skirmish::tests::register_behavior(&mut game.catalog[vehicle], "EjectPilotDie", 2);
    }
    let actor = game.spawn(5, 0, [300.0, 300.0], true);
    let host = game.spawn(vehicle, 1, [350.0, 300.0], true);
    game.update_visibility();
    (game, actor, host)
}

#[test]
fn explicit_goal_converts_without_driver_death_credit_or_incidental_pickup() {
    let (mut game, actor, host) = fixture(false);
    let health = game.entities[host].health;
    let lost = game.players[0].statistics.units_lost;
    let destroyed = game.players[1].statistics.units_destroyed;
    game.map_objects.insert("Driver".into(), actor);
    assert!(!game.hijacker_contact(actor, host));
    game.hijack(actor, host).unwrap();
    assert!(game.hijacker_contact(actor, host));
    assert_eq!(game.entities[host].owner, 0);
    assert!(game.entities[host].hijacked());
    assert_eq!(game.entities[host].health.to_bits(), health.to_bits());
    assert!(matches!(game.entities[host].ai.order, Order::Idle));
    assert_eq!(game.named_map_object("Driver"), Some(host));
    // GameLogic::destroyObject queues removal without inflicting lethal damage.
    assert!(game.entities[actor].exists());
    assert!(game.update_objects[game.entities[actor].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.entities[actor].exists());
    assert!(game.entities[actor].masked());
    assert_eq!(game.players[0].statistics.units_lost, lost);
    assert_eq!(game.players[1].statistics.units_destroyed, destroyed);
    assert_eq!(game.presentation.hijacker_events.len(), 1);
    assert_eq!(game.presentation.hijacker_events[0].victim, 1);
    assert!(!game.hijacker_contact(actor, host));
}

#[test]
fn enter_completion_cannot_replace_a_suppressed_physical_crate_contact() {
    let (mut game, actor, host) = fixture(true);
    let vehicle = game.entities[host].type_index;
    game.catalog[vehicle].kind_of.push("NO_COLLIDE".into());
    // AIEnter's border fallback sees a close eligible goal, but only the
    // object's actual collision callback executes its conversion crate.
    game.entities[actor].position = game.entities[host].position;
    game.update_visibility();
    assert!(game.can_hijack(actor, host));
    game.hijack(actor, host).unwrap();
    for _ in 0..20 {
        game.step();
    }
    assert_eq!(game.entities[host].owner, 1);
    assert!(!game.entities[host].hijacked());
    assert!(game.entities[actor].alive());
    assert!(!game.entities[actor].masked());
    assert!(game.presentation.hijacker_events.is_empty());
}

#[test]
fn admission_rejects_occupied_transport_immunity_masks_and_airborne_driver() {
    let (mut game, actor, host) = fixture(false);
    let vehicle = game.entities[host].type_index;
    assert!(game.can_hijack(actor, host));
    for kind in [
        "AIRCRAFT",
        "BOAT",
        "DRONE",
        "IMMUNE_TO_CAPTURE",
        "PARACHUTE",
    ] {
        game.catalog[vehicle].kind_of.push(kind.into());
        assert!(!game.can_hijack(actor, host), "{kind}");
        game.catalog[vehicle].kind_of.pop();
    }
    game.catalog[vehicle].kind_of.push("TRANSPORT".into());
    assert!(game.can_hijack(actor, host));
    game.entities[host].containment.passengers.push(actor);
    assert!(!game.can_hijack(actor, host));
    game.entities[host].containment.passengers.clear();
    game.entities[actor].placed_height_offset = Some(1.0);
    assert!(!game.can_hijack(actor, host));
    game.entities[actor].placed_height_offset = None;
    game.catalog[5]
        .hijacker
        .as_mut()
        .unwrap()
        .required
        .push("SELECTABLE".into());
    game.catalog[vehicle]
        .kind_of
        .retain(|kind| kind != "SELECTABLE");
    assert!(!game.can_hijack(actor, host));
    game.catalog[vehicle].kind_of.push("SELECTABLE".into());
    assert!(game.can_hijack(actor, host));
    game.catalog[5]
        .hijacker
        .as_mut()
        .unwrap()
        .forbidden
        .push("TRANSPORT".into());
    assert!(!game.can_hijack(actor, host));
}

#[test]
fn regular_host_module_presence_retains_driver_until_final_destruction() {
    let (mut game, actor, host) = fixture(true);
    game.hijack(actor, host).unwrap();
    assert!(game.hijacker_contact(actor, host));
    assert!(game.entities[actor].alive());
    assert!(game.entities[actor].masked());
    assert_eq!(game.entities[actor].driving_vehicle(), Some(host));
    assert_eq!(game.entities[host].veterancy, 0);
    game.entities[host].position = [420.0, 350.0];
    game.update_hijacker(actor, 0);
    assert_eq!(
        game.entities[actor].position.map(f32::to_bits),
        game.entities[host].position.map(f32::to_bits)
    );
    // A zero-health host still has an object ID; only final destruction ejects.
    game.entities[host].health = 0.0;
    game.update_hijacker(actor, 0);
    assert!(game.entities[actor].masked());
    assert_eq!(game.entities[actor].driving_vehicle(), Some(host));
    game.destroy_maintenance_object(host);
    game.update_hijacker(actor, 0);
    assert_eq!(game.entities[actor].driving_vehicle(), Some(host));
    game.process_update_destruction();
    game.update_hijacker(actor, 0);
    assert!(game.entities[actor].alive());
    assert!(!game.entities[actor].masked());
    assert_eq!(game.entities[actor].driving_vehicle(), None);
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
}

#[test]
fn pickup_science_belongs_to_victim_and_hijacked_host_cannot_be_claimed_twice() {
    let (mut game, actor, host) = fixture(true);
    game.catalog[5].hijacker.as_mut().unwrap().pickup_science = Some("SCIENCE_Test".into());
    game.players[0].sciences.push("SCIENCE_Test".into());
    assert!(!game.can_hijack(actor, host));
    game.players[1].sciences.push("SCIENCE_Test".into());
    assert!(game.can_hijack(actor, host));
    game.hijack(actor, host).unwrap();
    assert!(game.hijacker_contact(actor, host));
    let next = game.spawn(5, 1, [430.0, 300.0], true);
    game.update_visibility();
    assert!(!game.can_hijack(next, host));
}

#[test]
fn hijack_guard_uses_nested_enter_against_an_eligible_vehicle_without_a_weapon() {
    let (mut game, actor, host) = fixture(true);
    // AIEnterState requires a ground locomotor to approach the contact target.
    game.catalog[5].locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    game.catalog[5].ai.vision_range = 200.0;
    let position = game.entities[actor].position;
    game.guard(actor, position).unwrap();
    for _ in 0..600 {
        game.step();
        if game.entities[host].hijacked() {
            break;
        }
    }
    assert!(game.entities[host].hijacked());
    assert_eq!(game.entities[host].owner, 0);
    assert!(game.entities[actor].alive());
    assert!(game.entities[actor].masked());
    assert!(
        !game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == actor)
    );
}

#[test]
fn stored_driver_stays_with_host_hidden_inside_another_transport() {
    let (mut game, actor, host) = fixture(true);
    game.hijack(actor, host).unwrap();
    game.hijacker_contact(actor, host);
    let mut carrier =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Vehicle);
    let mut rules = crate::skirmish::containment::tests::rules(0.0, 1.0);
    rules.allowed = vec!["VEHICLE".into()];
    carrier.contain = Some(rules);
    crate::skirmish::tests::register_update(&mut carrier, "TransportContain");
    let kind = game.catalog.len();
    game.catalog.push(carrier);
    let transport = game.spawn(kind, 0, [450.0, 350.0], true);
    game.board(host, transport);
    assert_eq!(game.entities[host].contained_by(), Some(transport));
    assert!(game.entities[host].masked());
    assert!(game.entities[host].exists());
    game.update_hijacker(actor, 0);
    assert!(game.entities[actor].alive());
    assert!(game.entities[actor].masked());
    assert_eq!(game.entities[actor].driving_vehicle(), Some(host));
    game.remove_passenger(host, transport);
    assert!(!game.entities[host].masked());
    game.update_hijacker(actor, 0);
    assert_eq!(game.entities[actor].driving_vehicle(), Some(host));
}

#[test]
fn named_hijack_retires_previous_host_alias_but_unnamed_driver_preserves_it() {
    let (mut game, actor, host) = fixture(false);
    game.map_objects.insert("Old vehicle".into(), host);
    game.map_objects.insert("Named driver".into(), actor);
    game.hijack(actor, host).unwrap();
    game.hijacker_contact(actor, host);
    assert_eq!(game.named_map_object("Named driver"), Some(host));
    assert_eq!(game.named_map_object("Old vehicle"), None);
    assert!(game.named_map_object_destroyed("Old vehicle"));
    assert!(!game.named_map_object_destroyed("Named driver"));
    assert!(!game.named_map_object_destroyed("Never existed"));
    let (mut game, actor, host) = fixture(false);
    game.map_objects.insert("Old vehicle".into(), host);
    game.hijack(actor, host).unwrap();
    game.hijacker_contact(actor, host);
    assert_eq!(game.named_map_object("Old vehicle"), Some(host));
    assert!(!game.named_map_object_destroyed("Old vehicle"));
}

#[test]
fn hijack_event_retains_pre_defection_position_after_vehicle_moves() {
    let (mut game, actor, host) = fixture(true);
    let position = game.object_position(host);
    game.hijack(actor, host).unwrap();
    game.hijacker_contact(actor, host);
    game.entities[host].position[0] += 100.0;
    game.update_hijacker(actor, 0);
    assert_ne!(
        game.object_position(host).map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert_eq!(
        game.presentation.hijacker_events[0]
            .position
            .map(f32::to_bits),
        position.map(f32::to_bits)
    );
}

#[test]
fn authored_callbacks_keep_hidden_host_until_deletion_and_wake_inactive_instances() {
    let (mut game, actor, host) = fixture(true);
    let registration = game.catalog[5]
        .update_modules
        .iter()
        .position(|entry| matches!(entry, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "HijackerUpdate"))
        .unwrap();
    // The fieldless inactive update still returns UPDATE_SLEEP_NONE.
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.update_remaining_sleep(actor, registration), 1);
    game.hijack(actor, host).unwrap();
    assert!(game.hijacker_contact(actor, host));
    assert!(game.entities[actor].drawable_effectively_hidden());
    assert_ne!(game.entities[actor].object_status & (1 << 5), 0);
    game.entities[host].maintenance.masked = true;
    game.set_object_position(host, [420.0, 350.0, 0.0]);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.entities[actor].driving_vehicle(), Some(host));
    assert_eq!(
        game.object_position(actor).map(f32::to_bits),
        game.object_position(host).map(f32::to_bits)
    );
    game.destroy_maintenance_object(host);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.entities[actor].driving_vehicle(), Some(host));
    game.process_update_destruction();
    game.tick += 1;
    game.run_object_updates();
    assert!(game.entities[actor].driving_vehicle().is_none());
    assert!(!game.entities[actor].drawable_effectively_hidden());
    assert_eq!(game.entities[actor].object_status & (1 << 5), 0);
    assert_eq!(
        game.entities[actor].maintenance.instance_flags
            & crate::skirmish::maintenance::INSTANCE_UNSELECTABLE,
        0
    );
}

#[test]
fn effectively_dead_vehicle_with_positive_health_cannot_be_hijacked() {
    let (mut game, actor, host) = fixture(false);
    assert!(game.can_hijack(actor, host));
    game.hijack(actor, host).unwrap();
    game.entities[host].set_effectively_dead(true);
    assert!(game.entities[host].health > 0.0);
    assert!(!game.can_hijack(actor, host));
    assert!(!game.hijacker_contact(actor, host));
    assert_eq!(game.entities[host].owner, 1);
    assert!(game.presentation.hijacker_events.is_empty());
}

#[test]
fn vehicle_without_an_authored_ai_interface_cannot_be_hijacked() {
    let (mut game, actor, host) = fixture(false);
    let vehicle = game.entities[host].type_index;
    let unit = &mut game.catalog[vehicle];
    unit.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    unit.behavior_modules
        .retain(|module| module.family != "AIUpdateInterface");
    unit.update_modules.retain(|module| {
        !matches!(module,
        crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. }
            if family == "AIUpdateInterface")
    });
    let inert = game.spawn(vehicle, 1, [370.0, 300.0], true);
    game.update_visibility();
    assert!(game.entities[inert].ai.registration.is_none());
    assert!(game.entities[inert].alive());
    assert!(!game.can_hijack(actor, inert));
    assert!(game.hijack(actor, inert).is_err());
    assert_eq!(game.entities[inert].owner, 1);
}

#[test]
fn driver_queued_for_destruction_cannot_claim_a_second_vehicle() {
    let (mut game, actor, host) = fixture(false);
    let vehicle = game.entities[host].type_index;
    let second = game.spawn(vehicle, 1, [370.0, 300.0], true);
    game.update_visibility();
    assert!(game.can_hijack(actor, second));
    game.hijack(actor, host).unwrap();
    assert!(game.hijacker_contact(actor, host));
    assert!(game.entities[actor].alive());
    assert!(game.update_objects[game.entities[actor].update_owner].destroy_requested);
    assert!(!game.can_hijack(actor, second));
    assert!(game.hijack(actor, second).is_err());
    assert_eq!(game.entities[second].owner, 1);
    assert_eq!(game.presentation.hijacker_events.len(), 1);
}
