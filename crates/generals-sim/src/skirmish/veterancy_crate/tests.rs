use super::*;
use crate::skirmish::{Faction, Kind, tests, veterancy::StartingVeterancy};

mod pilot_find;

fn fixture() -> (Skirmish, usize, usize) {
    fixture_with_scan(0, None)
}

fn fixture_with_scan(
    owner: usize,
    scan: Option<crate::skirmish::pilot_find_vehicle::Rules>,
) -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    if owner != 0 {
        game.slots[owner].difficulty = Some(crate::skirmish::OpponentDifficulty::Hard);
    }
    let mut pilot = tests::fixture_type(Faction::Gla, Kind::Infantry);
    pilot.name = "AmericaInfantryPilot".into();
    // The original Pilot retains idle acquisition but has no WeaponSet.
    pilot.weapon = crate::skirmish::WeaponRules::unarmed();
    pilot.locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 5.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    pilot.kind_of.push("NO_GARRISON".into());
    pilot.veterancy.trainable = true;
    pilot.veterancy.required = [0, 1, 2, 3];
    pilot.veterancy.starting.push(StartingVeterancy {
        science: None,
        level: 1,
    });
    tests::register_behavior(&mut pilot, "VeterancyGainCreate", 0x8);
    tests::register_behavior(&mut pilot, "VeterancyCrateCollide", 0x10);
    pilot.behavior.veterancy_crates.push(Rules {
        restrictions: Restrictions {
            required: vec!["VEHICLE".into()],
            forbidden: vec!["DOZER".into()],
            forbid_owner: false,
            building_pickup: false,
            human_only: false,
            science: None,
        },
        adds_owner_veterancy: true,
        is_pilot: true,
        allow_multi_pickup: false,
        execute_fx: String::new(),
    });
    pilot.behavior.death_fx.push(tests::death_fx("PilotDie"));
    tests::register_behavior(&mut pilot, "FXListDie", 0x82);
    if let Some(scan) = scan {
        pilot.behavior.pilot_find_vehicle.push(scan);
        tests::register_update(&mut pilot, "PilotFindVehicleUpdate");
    }
    let kind = game.catalog.len();
    game.catalog.push(pilot);
    let target = &mut game.catalog[6];
    target.veterancy.trainable = true;
    target.veterancy.required = [0, 100, 200, 300];
    target
        .maintenance
        .experience_upgrades
        .push(crate::skirmish::ExperienceUpgrade {
            trigger: "Training".into(),
            scalar: 1.0,
        });
    tests::register_behavior(target, "ExperienceScalarUpgrade", 0x80);
    target.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Training".into()];
    let pilot = game.spawn(kind, owner, [300.0, 200.0], true);
    let vehicle = game.spawn(6, owner, [360.0, 200.0], true);
    game.grant_native_object_upgrade(vehicle, "Training");
    game.update_ground_contacts();
    game.update_visibility();
    (game, pilot, vehicle)
}

#[test]
fn enter_contact_promotes_without_recipient_bonus_transfers_name_and_deletes_selected_pilot() {
    let (mut game, pilot, vehicle) = fixture();
    assert_eq!(game.entities[pilot].veterancy, 1);
    assert!(
        game.presentation
            .promotion_events
            .iter()
            .all(|event| event.object != pilot)
    );
    assert_eq!(
        game.entities[vehicle].experience_scalar().to_bits(),
        2.0_f32.to_bits()
    );
    game.map_objects.insert("Ace".into(), pilot);
    game.map_objects.insert("OldTank".into(), vehicle);
    assert_eq!(game.create_selection(0, &[], &[pilot], true), [pilot]);
    // Incidental collisions do not collect this crate, even though it is valid.
    game.object_collide(pilot, Some(vehicle));
    assert_eq!(game.entities[vehicle].experience, 0);
    assert!(game.entities[pilot].exists());
    game.enter(pilot, vehicle).unwrap();
    assert_eq!(game.entities[pilot].ai.command.goal_object, Some(vehicle));
    game.object_collide(pilot, Some(vehicle));
    assert_eq!(game.entities[vehicle].experience, 100);
    assert_eq!(game.entities[vehicle].veterancy, 1);
    assert_eq!(game.entities[vehicle].owner, 0);
    assert_eq!(game.named_map_object("Ace"), Some(vehicle));
    assert!(game.named_map_object("OldTank").is_none());
    assert!(game.named_map_object_destroyed("OldTank"));
    assert!(game.update_objects[game.entities[pilot].update_owner].destroy_requested);
    game.object_collide(pilot, Some(vehicle));
    assert_eq!(game.entities[vehicle].experience, 100);
    assert!(
        game.presentation
            .object_fx_events
            .iter()
            .all(|event| event.effect != "PilotDie")
    );
    game.step();
    assert!(
        !game.entities[pilot].exists(),
        "Pilot position {:?}, target {:?}, enter {}, goal {:?}",
        game.entities[pilot].position,
        game.entities[vehicle].position,
        matches!(
            game.entities[pilot].ai.order,
            crate::skirmish::Order::Enter(_)
        ),
        game.entities[pilot].ai.command.goal_object,
    );
    assert!(game.selected_objects(0).is_empty());
    assert!(game.deselected_objects.contains(&pilot));
    assert_eq!(
        game.presentation
            .promotion_events
            .iter()
            .filter(|event| event.object == vehicle)
            .count(),
        1
    );
}

#[test]
fn scheduled_enter_reaches_real_partition_contact_without_container_capacity() {
    let (mut game, pilot, vehicle) = fixture();
    assert!(game.container_rules(vehicle).is_none());
    game.enter(pilot, vehicle).unwrap();
    for _ in 0..180 {
        game.step();
        if !game.entities[pilot].exists() {
            break;
        }
    }
    assert!(!game.entities[pilot].exists());
    assert_eq!(game.entities[vehicle].veterancy, 1);
    assert_eq!(game.entities[vehicle].experience, 100);
    assert!(game.entities[vehicle].contained_by().is_none());
}

#[test]
fn pilot_requires_same_owner_ground_locomotor_nonheroic_trainable_vehicle() {
    let (mut game, pilot, vehicle) = fixture();
    assert!(game.can_enter(pilot, vehicle));
    game.slots[0].team = Some(0);
    game.slots[1].team = Some(0);
    let allied = game.spawn(6, 1, [400.0, 200.0], true);
    assert!(game.allied(0, 1));
    assert!(!game.can_enter(pilot, allied));
    game.set_veterancy(vehicle, 3);
    assert!(!game.can_enter(pilot, vehicle));
    let mut air = tests::fixture_type(Faction::Gla, Kind::Tank);
    air.veterancy.trainable = true;
    air.veterancy.required = [0, 100, 200, 300];
    air.locomotor.legal_surfaces = 8;
    let kind = game.catalog.len();
    game.catalog.push(air);
    let grounded_aircraft = game.spawn(kind, 0, [450.0, 200.0], true);
    assert_eq!(
        game.height_above_ground(grounded_aircraft).to_bits(),
        0.0_f32.to_bits()
    );
    assert!(!game.can_enter(pilot, grounded_aircraft));
    let dozer = game.spawn(1, 0, [500.0, 200.0], true);
    assert!(!game.can_enter(pilot, dozer));
}

#[test]
fn non_scaled_gain_still_scales_each_experience_sink_edge() {
    let (mut game, _, child) = fixture();
    let parent = game.spawn(6, 0, [420.0, 200.0], true);
    game.grant_native_object_upgrade(parent, "Training");
    game.entities[child].set_experience_sink(Some(parent));
    game.gain_experience_levels(child, 1, false);
    assert_eq!(game.entities[child].experience, 0);
    assert_eq!(game.entities[parent].experience, 200);
    assert_eq!(game.entities[parent].veterancy, 2);
}

#[test]
fn crate_height_and_significant_recipient_height_use_native_strict_boundaries() {
    let (mut game, pilot, vehicle) = fixture();
    let [x, y, _] = game.object_position(vehicle);
    let floor = game.ground_height([x, y]);
    let threshold = -9.0 * game.gravity;
    game.set_object_position(vehicle, [x, y, floor + threshold]);
    assert!(game.can_enter(pilot, vehicle));
    game.set_object_position(vehicle, [x, y, floor + threshold + 1.0]);
    assert!(!game.can_enter(pilot, vehicle));
    game.set_object_position(vehicle, [x, y, floor]);
    let [x, y, _] = game.object_position(pilot);
    game.set_object_position(pilot, [x, y, game.ground_height([x, y]) + 1.0]);
    assert!(!game.can_enter(pilot, vehicle));
}

#[test]
fn configured_pickup_fx_uses_recipient_object_before_direct_deletion() {
    let mut game = tests::game();
    let unit = &mut game.catalog[5];
    unit.behavior.veterancy_crates.push(Rules {
        restrictions: Restrictions {
            required: vec!["VEHICLE".into()],
            forbidden: Vec::new(),
            forbid_owner: false,
            building_pickup: false,
            human_only: false,
            science: None,
        },
        adds_owner_veterancy: false,
        is_pilot: true,
        allow_multi_pickup: false,
        execute_fx: "Pickup".into(),
    });
    tests::register_behavior(unit, "VeterancyCrateCollide", 0x10);
    game.catalog[6].veterancy.trainable = true;
    game.catalog[6].veterancy.required = [0, 100, 200, 300];
    let pilot = game.spawn(5, 0, [300.0, 200.0], true);
    let vehicle = game.spawn(6, 0, [360.0, 200.0], true);
    game.update_visibility();
    game.enter(pilot, vehicle).unwrap();
    game.object_collide(pilot, Some(vehicle));
    let event = game.presentation.object_fx_events.last().unwrap();
    assert_eq!(event.effect, "Pickup");
    assert_eq!(event.object, vehicle);
    assert_eq!(
        event.position.map(f32::to_bits),
        game.object_position(vehicle).map(f32::to_bits)
    );
    assert_eq!(event.owner, 0);
    assert!(game.update_objects[game.entities[pilot].update_owner].destroy_requested);
}
