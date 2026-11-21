use super::*;
use crate::skirmish::tests::game;

fn fixture(effect: Effect, kind: &str) -> (Skirmish, usize, usize) {
    let mut game = game();
    game.catalog[5].behavior.sabotage = vec![Rules {
        effect,
        required: Vec::new(),
        forbidden: Vec::new(),
    }];
    let family = match &game.catalog[5].behavior.sabotage[0].effect {
        Effect::PowerPlant { .. } => "SabotagePowerPlantCrateCollide",
        Effect::Superweapon => "SabotageSuperweaponCrateCollide",
        Effect::CommandCenter => "SabotageCommandCenterCrateCollide",
        Effect::SupplyCenter { .. } => "SabotageSupplyCenterCrateCollide",
        Effect::MilitaryFactory { .. } => "SabotageMilitaryFactoryCrateCollide",
        Effect::FakeBuilding => "SabotageFakeBuildingCrateCollide",
        Effect::InternetCenter { .. } => "SabotageInternetCenterCrateCollide",
    };
    crate::skirmish::tests::register_behavior(&mut game.catalog[5], family, 0x10);
    game.catalog[0].kind_of.push(kind.into());
    let actor = game.spawn(5, 0, [300.0, 300.0], true);
    let target = game.spawn(0, 1, [350.0, 300.0], true);
    game.update_visibility();
    (game, actor, target)
}

#[test]
fn supply_theft_requires_enter_goal_caps_cash_and_destroys_without_kill_credit() {
    let (mut game, actor, target) =
        fixture(Effect::SupplyCenter { amount: 1000 }, "FS_SUPPLY_CENTER");
    game.players[1].cash = 700;
    let cash = game.players[0].cash;
    let earned = game.players[0].statistics.money_earned;
    let lost = game.players[0].statistics.units_lost;
    let destroyed = game.players[1].statistics.units_destroyed;
    assert!(!game.sabotage_contact(actor, target));
    assert_eq!(game.players[1].cash, 700);
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    assert_eq!(game.players[1].cash, 0);
    assert_eq!(game.players[0].cash, cash + 700);
    assert_eq!(game.players[0].statistics.money_earned, earned + 700);
    assert!(game.entities[actor].exists());
    assert!(game.update_objects[game.entities[actor].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.entities[actor].exists());
    assert!(game.entities[actor].masked());
    assert_eq!(game.players[0].statistics.units_lost, lost);
    assert_eq!(game.players[1].statistics.units_destroyed, destroyed);
    assert_eq!(game.presentation.sabotage_events[0].cash, 700);
    assert!(!game.sabotage_contact(actor, target));
}

#[test]
fn corrected_factory_gate_rejects_construction_sale_allies_and_carriers() {
    let (mut game, actor, target) = fixture(Effect::MilitaryFactory { frames: 900 }, "FS_AIRFIELD");
    assert!(game.can_sabotage(actor, target));
    game.entities[target].build_progress = 0.5;
    assert!(!game.can_sabotage(actor, target));
    game.entities[target].build_progress = 1.0;
    game.entities[target].owner = 0;
    assert!(!game.can_sabotage(actor, target));
    game.entities[target].owner = 1;
    game.catalog[0].kind_of.push("AIRCRAFT_CARRIER".into());
    assert!(!game.can_sabotage(actor, target));
    game.catalog[0].kind_of.pop();
    game.sabotage(actor, target).unwrap();
    let health = game.entities[target].health;
    assert!(game.sabotage_contact(actor, target));
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
    game.tick = 899;
    game.expire_hacked();
    assert!(game.entities[target].hacked());
    game.tick = 900;
    game.expire_hacked();
    assert!(!game.entities[target].hacked());
    let (mut game, actor, target) = fixture(Effect::MilitaryFactory { frames: 900 }, "FS_AIRFIELD");
    game.sell(1, target).unwrap();
    assert!(!game.can_sabotage(actor, target));
}

#[test]
fn power_sabotage_records_the_player_outage_deadline() {
    let (mut game, actor, target) = fixture(Effect::PowerPlant { frames: 900 }, "FS_POWER");
    game.catalog[0].kind_of.push("POWERED".into());
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    // Authored POWERED objects already belong to the player when sabotage sends its callback.
    let powered = game.spawn(0, 1, [400.0, 300.0], true);
    game.update_power();
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    assert_eq!(game.players[1].energy.sabotaged_until, game.tick + 900);
    assert_eq!(
        game.players[1].energy.consumption.to_bits(),
        0.0_f32.to_bits()
    );
    assert!(!game.has_operational_power(target));
    // The outage belongs to the victim's Energy, surviving plant destruction
    // and an ownership transfer of the original sabotage target.
    game.entities[target].health = 0.0;
    assert!(!game.has_operational_power(powered));
    game.entities[target].owner = 0;
    game.tick = 899;
    assert!(!game.has_operational_power(powered));
    game.tick = 900;
    game.update_power();
    assert!(!game.has_operational_power(powered));
    game.tick = 901;
    game.update_power();
    assert!(game.has_operational_power(powered));
    assert_eq!(game.players[1].energy.sabotaged_until, 0);
}

#[test]
fn player_outage_suppresses_upgrade_production_even_without_base_energy() {
    use crate::skirmish::PowerUpgrade;
    let (mut game, actor, _) = fixture(Effect::PowerPlant { frames: 900 }, "FS_POWER");
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    // Instantiate the upgrade-bearing target only after its authored module exists.
    let plants: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.type_index == 0 && unit.exists())
        .map(|(id, _)| id)
        .collect();
    for id in plants {
        game.destroy_maintenance_object(id);
    }
    game.process_update_destruction();
    game.catalog[0].maintenance.power_upgrade = Some(PowerUpgrade {
        trigger: "Upgrade_AmericaAdvancedControlRods".into(),
        bonus: 10,
    });
    crate::skirmish::tests::register_behavior(&mut game.catalog[0], "PowerPlantUpgrade", 0x80);
    game.catalog[0]
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers = vec!["Upgrade_AmericaAdvancedControlRods".into()];
    let target = game.spawn(0, 1, [350.0, 300.0], true);
    game.update_visibility();
    game.players[1]
        .upgrades
        .push("Upgrade_AmericaAdvancedControlRods".into());
    game.activate_upgrade_modules(target);
    game.update_power();
    assert_eq!(
        game.players[1].energy.production.to_bits(),
        10.0_f32.to_bits()
    );
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    assert_eq!(
        game.players[1].energy.production.to_bits(),
        0.0_f32.to_bits()
    );
    game.tick = 900;
    game.update_power();
    assert_eq!(
        game.players[1].energy.production.to_bits(),
        10.0_f32.to_bits()
    );
}

#[test]
fn energy_queries_recover_before_radar_and_powered_objects_without_adjustment() {
    let (mut game, actor, target) = fixture(Effect::PowerPlant { frames: 900 }, "FS_POWER");
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    game.catalog[0].energy = 10;
    game.catalog[3].kind_of.push("POWERED".into());
    crate::skirmish::tests::radar_upgrade(
        &mut game.catalog[3],
        "Upgrade_SabotageRadar",
        true,
        false,
    );
    let provider = game.spawn(3, 1, [500.0, 300.0], true);
    game.update_power();
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    let raw_production = game.players[1].energy.unsabotaged[0];
    game.tick = 899;
    game.update_power();
    assert_eq!(game.players[1].energy().0.to_bits(), 0.0_f32.to_bits());
    assert!(!game.has_radar(1));
    assert!(!game.has_operational_power(provider));
    game.tick = 900;
    game.update_power();
    assert_eq!(
        game.players[1].energy().0.to_bits(),
        raw_production.to_bits()
    );
    assert!(!game.has_radar(1));
    assert!(!game.has_operational_power(provider));
    // Unchanged periodic recomputation is not Energy::addProduction.
    game.update_power();
    assert!(!game.has_operational_power(provider));
    game.tick = 901;
    game.update_power();
    assert!(game.has_radar(1));
    assert!(game.has_operational_power(provider));
}

#[test]
fn paid_production_after_outage_uses_zero_supply_ratio_until_a_plant_returns() {
    let (mut game, actor, target) = fixture(Effect::PowerPlant { frames: 900 }, "FS_POWER");
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    game.catalog[0].energy = 10;
    let producer = game.spawn(3, 1, [500.0, 300.0], true);
    game.update_power();
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    let plants: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter_map(|(id, entity)| {
            (entity.owner == 1 && game.catalog[entity.type_index].energy > 0).then_some(id)
        })
        .collect();
    for plant in plants {
        game.destroy_maintenance_object(plant);
    }
    game.process_update_destruction();
    game.tick = 900;
    game.update_power();
    assert_eq!(game.players[1].energy().0.to_bits(), 0.0_f32.to_bits());
    assert_eq!(game.players[1].energy().1.to_bits(), 0.0_f32.to_bits());
    let cash = game.players[1].cash;
    let cost = game.build_cost(1, 5);
    game.train(producer, 5).unwrap();
    assert_eq!(game.players[1].cash, cash - cost);
    // Fixture build time is 150 frames. Source ratio zero gives shortage one,
    // pre-clamp speed zero, then the configured minimum speed 0.2: 750 frames.
    for _ in 0..749 {
        game.update_production(producer);
    }
    let queued = game.entities[producer].queue.front().unwrap();
    assert_eq!(queued.total_seconds.to_bits(), 25.0_f32.to_bits());
    assert_eq!(queued.elapsed_frames.to_bits(), 749.0_f32.to_bits());
    game.update_production(producer);
    assert!(game.entities[producer].queue.is_empty());
    game.spawn(0, 1, [600.0, 300.0], true);
    game.update_power();
    assert_eq!(game.players[1].energy().0.to_bits(), 10.0_f32.to_bits());
    let cash = game.players[1].cash;
    game.train(producer, 5).unwrap();
    assert_eq!(game.players[1].cash, cash - cost);
    for _ in 0..149 {
        game.update_production(producer);
    }
    let queued = game.entities[producer].queue.front().unwrap();
    assert_eq!(queued.total_seconds.to_bits(), 5.0_f32.to_bits());
    assert_eq!(queued.elapsed_frames.to_bits(), 149.0_f32.to_bits());
    game.update_production(producer);
    assert!(game.entities[producer].queue.is_empty());
}

#[test]
fn completed_power_plant_adjustment_clears_brownout_at_exact_deadline() {
    let (mut game, actor, target) = fixture(Effect::PowerPlant { frames: 900 }, "FS_POWER");
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    game.catalog[0].energy = 10;
    // Source positive plants remain enabled during the outage; a separate POWERED consumer is disabled.
    game.catalog[3].kind_of.push("POWERED".into());
    let powered = game.spawn(3, 1, [550.0, 300.0], true);
    let worker = game.spawn(1, 1, [440.0, 300.0], true);
    let plant = game.spawn(0, 1, [450.0, 300.0], false);
    game.entities[worker].ai.order = Order::Build(plant);
    game.entities[worker].ai.construction_dock = Some([440.0, 300.0]);
    game.entities[worker].ai.construction_phase = super::super::ConstructionPhase::Build;
    game.entities[plant].construction_percent = 99.99;
    game.entities[plant].build_progress = 0.9999;
    game.update_power();
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    game.tick = 900;
    game.update_power();
    assert!(!game.has_operational_power(powered));
    let production = game.players[1].energy().0;
    // The normal completion path performs the modeled source energy callback.
    game.update_construction(worker, plant);
    assert!(game.entities[plant].complete());
    assert_eq!(
        game.players[1].energy().0.to_bits(),
        (production + 10.0).to_bits()
    );
    assert!(game.has_operational_power(powered));
    assert_eq!(game.players[1].energy.sabotaged_until, 900);
}

#[test]
fn ordered_kind_masks_require_all_flags_and_shorter_disable_replaces_expiry() {
    let (mut game, actor, target) = fixture(Effect::MilitaryFactory { frames: 900 }, "FS_BARRACKS");
    game.catalog[5].behavior.sabotage[0].required =
        vec!["FS_BARRACKS".into(), "FS_AIRFIELD".into()];
    game.catalog[5].behavior.sabotage.push(Rules {
        effect: Effect::MilitaryFactory { frames: 60 },
        required: vec!["FS_BARRACKS".into()],
        forbidden: vec!["AIRCRAFT_CARRIER".into()],
    });
    game.set_hacked_until(target, crate::skirmish::ai_dispatch::native_frame(900));
    // BuildingPickup permits elevated Saboteurs; collision still decides contact.
    game.entities[actor].placed_height_offset = Some(20.0);
    assert!(game.can_sabotage(actor, target));
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    assert_eq!(game.presentation.sabotage_events[0].module, 1);
    assert_eq!(game.entities[target].hacked_until, game.tick + 60);
}

#[test]
fn internet_sabotage_disables_all_contained_units_without_harming_them() {
    let (mut game, actor, target) =
        fixture(Effect::InternetCenter { frames: 450 }, "FS_INTERNET_CENTER");
    let hacker = game.spawn(6, 1, [350.0, 300.0], true);
    game.entities[target].containment.passengers.push(hacker);
    game.entities[hacker].containment.carrier = Some(target);
    let health = game.entities[hacker].health;
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    assert_eq!(game.entities[target].hacked_until, game.tick + 450);
    assert_eq!(game.entities[hacker].hacked_until, game.tick + 450);
    assert_eq!(game.entities[hacker].health.to_bits(), health.to_bits());
}

#[test]
fn physical_collision_with_explicit_enter_goal_runs_the_crate() {
    let (mut game, actor, target) =
        fixture(Effect::SupplyCenter { amount: 1000 }, "FS_SUPPLY_CENTER");
    game.players[1].cash = 500;
    game.object_contact(actor, target, true);
    assert_eq!(game.players[1].cash, 500);
    assert!(game.entities[actor].alive());
    game.sabotage(actor, target).unwrap();
    game.object_contact(actor, target, true);
    assert_eq!(game.players[1].cash, 0);
    assert!(game.entities[actor].exists());
    assert!(game.update_objects[game.entities[actor].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.entities[actor].exists());
}

#[test]
fn command_center_resets_existing_shared_timers_but_creates_missing_timers_ready() {
    use crate::skirmish::{PowerDefinition, PowerEffect, ProgressionRules};
    let (mut game, actor, target) = fixture(Effect::CommandCenter, "COMMANDCENTER");
    game.catalog[0].progression.powers = vec!["Existing".into(), "Missing".into()];
    let power = |name: &str| PowerDefinition {
        name: name.into(),
        required_science: String::new(),
        reload_frames: 7200,
        effect: PowerEffect::CashHack {
            amount: 1000,
            upgrades: Vec::new(),
        },
    };
    game.progression = Some(ProgressionRules {
        multiplayer_colors: crate::skirmish::tests::original_multiplayer_colors(),
        ranks: Vec::new(),
        sciences: Vec::new(),
        factions: Vec::new(),
        powers: vec![power("Existing"), power("Missing"), power("Unrelated")],
    });
    game.players[1].power_ready_at = vec![Some(30), None, Some(400)];
    game.tick = 100;
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    assert_eq!(
        game.players[1].power_ready_at,
        [Some(7300), Some(100), Some(400)]
    );
}

#[test]
fn native_sabotage_restarts_each_target_module_without_triggering_or_unpausing_it() {
    use crate::skirmish::{special_power, tests::register_behavior};

    for (effect, kind) in [
        (Effect::Superweapon, "FS_SUPERWEAPON"),
        (Effect::CommandCenter, "COMMANDCENTER"),
    ] {
        let (mut game, actor, _) = fixture(effect, kind);
        for (name, shared, paused) in [
            ("TargetLocal", false, false),
            ("TargetPaused", false, true),
            ("TargetSharedExisting", true, false),
            ("TargetSharedMissing", true, false),
        ] {
            let unit = &mut game.catalog[0];
            unit.special_powers.push(special_power::Rules {
                name: name.into(),
                tag: name.into(),
                power_type: "SPECIAL_SCUD_STORM".into(),
                required_science: String::new(),
                view_range: 0.0,
                view_frames: 0,
                view_template: None,
                initiate_sound: String::new(),
                target_sound: String::new(),
                module_initiate_sound: String::new(),
                academy_classification: "ACT_NONE".into(),
                update_starts_attack: false,
                timer: special_power::Timer {
                    reload_frames: 9000,
                    shared,
                    public: true,
                },
                startup: special_power::Startup {
                    paused,
                    ready: false,
                },
                script_only: false,
            });
            register_behavior(unit, "OCLSpecialPower", 0x0100);
            unit.behavior_modules.last_mut().unwrap().tag = name.into();
        }
        let target = game.spawn(0, 1, [400.0, 300.0], true);
        let sibling = game.spawn(0, 1, [470.0, 300.0], true);
        let ally = game.spawn(0, 0, [400.0, 400.0], true);
        game.update_visibility();
        game.players[1]
            .special_power_ready
            .insert("TargetSharedExisting".into(), 30);
        game.players[1]
            .special_power_ready
            .remove("TargetSharedMissing");
        game.players[1]
            .special_power_ready
            .insert("Unrelated".into(), 777);
        game.players[0]
            .special_power_ready
            .insert("TargetSharedExisting".into(), 333);
        game.tick = 100;
        game.sabotage(actor, target).unwrap();
        assert!(game.sabotage_contact(actor, target));
        assert_eq!(game.special_power_remaining(target, "TargetLocal"), 9000);
        assert_eq!(game.special_power_remaining(sibling, "TargetLocal"), 8900);
        assert_eq!(game.special_power_remaining(ally, "TargetLocal"), 8900);
        assert!(!game.special_power_ready(target, "TargetPaused"));
        assert_eq!(
            game.players[1].special_power_ready["TargetSharedExisting"],
            9100
        );
        assert_eq!(
            game.players[1].special_power_ready["TargetSharedMissing"],
            100
        );
        assert_eq!(game.players[1].special_power_ready["Unrelated"], 777);
        assert_eq!(
            game.players[0].special_power_ready["TargetSharedExisting"],
            333
        );
        assert!(game.special_power_triggers.is_empty());
    }
}

#[test]
fn fake_building_takes_detonated_damage_with_saboteur_kill_attribution() {
    let (mut game, actor, target) = fixture(Effect::FakeBuilding, "FS_FAKE");
    let destroyed = game.players[0].statistics.buildings_destroyed;
    let lost = game.players[1].statistics.buildings_lost;
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    assert!(!game.entities[target].alive());
    assert!(matches!(
        game.entities[target].damage_state.death_type,
        DeathType::Detonated
    ));
    assert_eq!(
        game.entities[target].damage_state.last_attacker,
        Some(actor)
    );
    assert_eq!(
        game.players[0].statistics.buildings_destroyed,
        destroyed + 1
    );
    assert_eq!(game.players[1].statistics.buildings_lost, lost + 1);
}

#[test]
fn physical_internet_center_contact_reaches_sabotage_after_rejected_containment() {
    let (mut game, target) = super::super::containment::tests::setup(0.0, 1.0);
    let target_type = game.entities[target].type_index;
    game.catalog[target_type].kind = super::super::Kind::CommandCenter;
    game.catalog[target_type].kind_of = vec!["STRUCTURE".into(), "FS_INTERNET_CENTER".into()];
    game.catalog[5].behavior.sabotage = vec![Rules {
        effect: Effect::InternetCenter { frames: 450 },
        required: Vec::new(),
        forbidden: Vec::new(),
    }];
    crate::skirmish::tests::register_behavior(
        &mut game.catalog[5],
        "SabotageInternetCenterCrateCollide",
        0x10,
    );
    let position = game.entities[target].position;
    let hacker = game.spawn(5, 0, position, true);
    game.board(hacker, target);
    assert_eq!(game.entities[hacker].contained_by(), Some(target));
    let actor = game.spawn(5, 1, position, true);
    game.tick = 1;
    game.run_object_updates();
    game.update_visibility();
    game.enter(actor, target).unwrap();
    let health = game.entities[hacker].health;
    game.update_ground_contacts();
    assert_eq!(game.presentation.sabotage_events.len(), 1);
    assert!(game.entities[actor].exists());
    assert!(game.update_objects[game.entities[actor].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.entities[actor].exists());
    assert!(game.entities[actor].masked());
    assert_eq!(game.entities[target].hacked_until, game.tick + 450);
    assert_eq!(game.entities[hacker].hacked_until, game.tick + 450);
    assert_eq!(game.passengers(target), &[hacker]);
    assert_eq!(game.entities[hacker].health.to_bits(), health.to_bits());
}

#[test]
fn repeated_outage_refreshes_powered_objects_with_unchanged_energy_totals() {
    let (mut game, actor, target) = fixture(Effect::PowerPlant { frames: 900 }, "FS_POWER");
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    game.catalog[3].kind_of.push("POWERED".into());
    crate::skirmish::tests::radar_upgrade(
        &mut game.catalog[3],
        "Upgrade_RepeatedOutageRadar",
        true,
        true,
    );
    game.update_power();
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    let late = game.spawn(3, 0, [450.0, 300.0], true);
    game.transfer_maintenance_owner(late, 1);
    // Zero energy joins do not send Energy::addProduction/addConsumption.
    assert!(game.has_operational_power(late));
    assert!(game.has_radar(1));
    let next = game.spawn(5, 0, [300.0, 300.0], true);
    game.update_visibility();
    game.sabotage(next, target).unwrap();
    assert!(game.sabotage_contact(next, target));
    assert!(!game.has_operational_power(late));
    assert!(!game.has_radar(1));
    assert_eq!(game.players[1].energy().0.to_bits(), 0.0_f32.to_bits());
    assert_eq!(game.players[1].energy().1.to_bits(), 0.0_f32.to_bits());
}

#[test]
fn zero_duration_sabotage_still_sends_literal_brownout_despite_changed_energy() {
    let (mut game, actor, target) = fixture(Effect::PowerPlant { frames: 0 }, "FS_POWER");
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    game.catalog[3].kind_of.push("POWERED".into());
    crate::skirmish::tests::radar_upgrade(
        &mut game.catalog[3],
        "Upgrade_ZeroOutageRadar",
        true,
        false,
    );
    let provider = game.spawn(3, 1, [500.0, 300.0], true);
    game.update_power();
    assert!(game.has_radar(1));
    let plant = game.type_for(1, crate::skirmish::Kind::Tank).unwrap();
    game.catalog[plant].energy = 10;
    game.spawn(plant, 1, [600.0, 300.0], true);
    game.tick = 10;
    game.logic_update = true;
    game.sabotage(actor, target).unwrap();
    assert!(game.sabotage_contact(actor, target));
    // Energy::getProduction is already recovered at the exact deadline,
    // while the literal callback still disables the object and player radar.
    assert_eq!(game.players[1].energy().0.to_bits(), 10.0_f32.to_bits());
    assert!(!game.has_operational_power(provider));
    assert!(!game.has_radar(1));
    let edges: Vec<_> = game
        .presentation
        .radar_events
        .iter()
        .map(|event| event.online)
        .collect();
    assert_eq!(edges, [false]);
}

#[test]
fn authored_crate_order_skips_ineligible_modules_and_stops_repeated_pickup() {
    for first_is_valid in [false, true] {
        let (mut game, actor, target) =
            fixture(Effect::SupplyCenter { amount: 100 }, "FS_SUPPLY_CENTER");
        game.catalog[5].behavior.sabotage.insert(
            0,
            Rules {
                effect: Effect::PowerPlant { frames: 99 },
                required: Vec::new(),
                forbidden: Vec::new(),
            },
        );
        crate::skirmish::tests::register_behavior(
            &mut game.catalog[5],
            "SabotagePowerPlantCrateCollide",
            0x10,
        );
        let power = game.catalog[5].behavior_modules.pop().unwrap();
        let supply = game.catalog[5]
            .behavior_modules
            .iter()
            .position(|module| module.family == "SabotageSupplyCenterCrateCollide")
            .unwrap();
        game.catalog[5].behavior_modules.insert(supply, power);
        if first_is_valid {
            game.catalog[0].kind_of.push("FS_POWER".into());
        }
        let money = game.players[1].cash;
        game.sabotage(actor, target).unwrap();
        game.object_contact(actor, target, false);
        assert_eq!(game.presentation.sabotage_events.len(), 1);
        assert_eq!(
            game.presentation.sabotage_events[0].module,
            usize::from(!first_is_valid)
        );
        assert_eq!(
            game.players[1].cash,
            money - if first_is_valid { 0 } else { 100 }
        );
        assert!(game.update_objects[game.entities[actor].update_owner].destroy_requested);
        game.object_contact(actor, target, false);
        assert_eq!(game.presentation.sabotage_events.len(), 1);
    }
}
