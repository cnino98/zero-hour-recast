use super::*;
use crate::skirmish::MaintenanceRules;
use crate::skirmish::{
    AutoHeal, Faction, Kind,
    tests::{fixture_type, game},
};

pub(in crate::skirmish) fn bind_tech(unit: &mut crate::skirmish::UnitType) {
    crate::skirmish::tests::register_update(unit, "TechBuildingBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces |= 2;
    unit.neutral_tech.tech_buildings.push(TechBuildingRules {
        pulse_fx: None,
        pulse_frames: 0,
    });
}

fn bind_cost_modifier(unit: &mut crate::skirmish::UnitType) {
    use crate::skirmish::tests::register_behavior;
    register_behavior(unit, "GrantUpgradeCreate", 8);
    unit.behavior
        .radar_grants
        .push(crate::skirmish::radar_update::Grant {
            name: Some("Upgrade_CostReduction".into()),
            academy_radar: false,
            player: false,
            on_created: true,
        });
    register_behavior(unit, "CostModifierUpgrade", 0x80);
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers =
        vec!["Upgrade_CostReduction".into()];
}

#[test]
fn identical_refineries_share_discount_until_last_owner_provider_is_lost() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let refinery_type = game.catalog.len();
    let mut refinery = fixture_type(Faction::Civilian, Kind::TechBuilding);
    refinery.neutral_tech.cost_modifier = Some(ProductionCostModifier {
        kind_mask: 2,
        percentage: -0.1,
    });
    refinery.maintenance.kind_flags |= MaintenanceRules::TECH_BUILDING;
    crate::skirmish::neutral_tech::tests::bind_tech(&mut refinery);
    bind_cost_modifier(&mut refinery);
    game.catalog.push(refinery);
    let first = game.spawn(refinery_type, 0, [350.0, 100.0], true);
    let second = game.spawn(refinery_type, 0, [500.0, 100.0], true);
    assert!((game.production_cost_multiplier(0, tank) - 0.9).abs() < f32::EPSILON);
    assert!((game.production_cost_multiplier(0, infantry) - 1.0).abs() < f32::EPSILON);
    game.transfer_maintenance_owner(first, 1);
    assert!((game.production_cost_multiplier(0, tank) - 0.9).abs() < f32::EPSILON);
    assert!((game.production_cost_multiplier(1, tank) - 0.9).abs() < f32::EPSILON);
    game.kill_object(second, crate::skirmish::DeathType::Normal);
    assert_eq!(game.entities[second].owner, game.neutral_owner());
    assert!((game.production_cost_multiplier(0, tank) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn hospital_whole_player_heals_remote_infantry_stacks_and_excludes_other_owners() {
    let mut game = game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let hospital_type = game.catalog.len();
    let mut hospital = fixture_type(Faction::Civilian, Kind::TechBuilding);
    hospital.maintenance.auto_heal.push(AutoHeal {
        damage_delay: 0,
        starts_active: true,
        triggers: Vec::new(),
        conflicts: Vec::new(),
        requires_all: false,
        amount: 2.0,
        delay: 30,
        targets: crate::skirmish::AutoHealTargets {
            radius: 0.0,
            affects_whole_player: true,
            kinds: Some(vec!["INFANTRY".into()]),
            forbidden: Vec::new(),
            skip_self: false,
        },
        single_burst: false,
    });
    crate::skirmish::tests::register_update(&mut hospital, "AutoHealBehavior");
    game.catalog.push(hospital);
    let first = game.spawn(hospital_type, 0, [300.0, 100.0], true);
    let second = game.spawn(hospital_type, 0, [500.0, 100.0], true);
    // Distant but inside the 800-by-600 playable map; whole-player scans reject off-map objects.
    let remote = game.spawn(infantry, 0, [740.0, 500.0], true);
    let enemy = game.spawn(infantry, 1, [300.0, 100.0], true);
    let vehicle = game.spawn(tank, 0, [310.0, 100.0], true);
    let held = game.spawn(infantry, 0, [315.0, 100.0], true);
    game.entities[held].position = [-100.0, -100.0];
    for id in [remote, enemy, vehicle, held] {
        game.entities[id].health = 50.0;
    }
    game.wake_family_update(first, "AutoHealBehavior", 0, 1);
    game.wake_family_update(second, "AutoHealBehavior", 0, 1);
    game.slots[0].team = Some(0);
    game.slots[1].team = Some(0);
    game.tick = 1;
    game.run_object_updates();
    assert!((game.entities[remote].health - 54.0).abs() < f32::EPSILON);
    for id in [enemy, vehicle, held] {
        assert!((game.entities[id].health - 50.0).abs() < f32::EPSILON);
    }
    assert_eq!(game.entities[remote].maintenance.healing_benefactor, None);
    game.transfer_maintenance_owner(first, 1);
    game.entities[second].health = 0.0;
    game.entities[second].set_effectively_dead(true);
    game.tick = 31;
    game.run_object_updates();
    assert!((game.entities[remote].health - 54.0).abs() < f32::EPSILON);
    assert!((game.entities[enemy].health - 52.0).abs() < f32::EPSILON);
}

#[test]
fn refinery_loss_changes_refund_to_cancellation_time_source_price() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let factory_type = game.type_for(0, Kind::Factory).unwrap();
    let factory = game.spawn(factory_type, 0, [350.0, 100.0], true);
    let refinery_type = game.catalog.len();
    let mut refinery = fixture_type(Faction::Civilian, Kind::TechBuilding);
    refinery.neutral_tech.cost_modifier = Some(ProductionCostModifier {
        kind_mask: 2,
        percentage: -0.1,
    });
    refinery.maintenance.kind_flags |= MaintenanceRules::TECH_BUILDING;
    crate::skirmish::neutral_tech::tests::bind_tech(&mut refinery);
    bind_cost_modifier(&mut refinery);
    game.catalog.push(refinery);
    let refinery = game.spawn(refinery_type, 0, [500.0, 100.0], true);
    let initial_cash = game.players[0].cash;
    let base_price = game.catalog[tank].cost;
    let discounted = game.build_cost(0, tank);
    assert_eq!(discounted, base_price * 9 / 10);
    game.train(factory, tank).unwrap();
    assert_eq!(game.players[0].cash, initial_cash - discounted);
    let production = game.entities[factory].queue[0].id;
    game.transfer_maintenance_owner(refinery, 1);
    game.cancel_production(0, factory, production).unwrap();
    assert_eq!(game.players[0].cash, initial_cash - discounted + base_price);
    assert!(game.entities[factory].queue.is_empty());
    game.destroy_maintenance_object(refinery);
    assert!((game.production_cost_multiplier(1, tank) - 1.0).abs() < f32::EPSILON);
    game.destroy_maintenance_object(refinery);
    assert!((game.production_cost_multiplier(1, tank) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn refinery_create_grant_waits_for_build_complete_callback() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let kind = game.catalog.len();
    let mut refinery = fixture_type(Faction::Civilian, Kind::TechBuilding);
    refinery.neutral_tech.cost_modifier = Some(ProductionCostModifier {
        kind_mask: 2,
        percentage: -0.1,
    });
    bind_cost_modifier(&mut refinery);
    game.catalog.push(refinery);
    let refinery = game.spawn(kind, 0, [500.0, 100.0], false);
    assert!((game.production_cost_multiplier(0, tank) - 1.0).abs() < f32::EPSILON);
    game.entities[refinery].build_progress = 1.0;
    // The actual dozer completion clears UNDER_CONSTRUCTION before CREATE callbacks.
    game.entities[refinery].object_status &= !(1 << 3);
    game.native_build_complete(refinery);
    game.native_build_complete(refinery);
    assert!((game.production_cost_multiplier(0, tank) - 0.9).abs() < f32::EPSILON);
    game.destroy_maintenance_object(refinery);
    assert!((game.production_cost_multiplier(0, tank) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn repair_bay_heals_owner_vehicles_globally_excludes_aircraft_and_moves_with_capture() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let bay_type = game.catalog.len();
    let mut bay = fixture_type(Faction::Civilian, Kind::TechBuilding);
    bay.maintenance.auto_heal.push(AutoHeal {
        damage_delay: 0,
        starts_active: true,
        triggers: Vec::new(),
        conflicts: Vec::new(),
        requires_all: false,
        amount: 2.0,
        delay: 30,
        targets: crate::skirmish::AutoHealTargets {
            radius: 0.0,
            affects_whole_player: true,
            kinds: Some(vec!["VEHICLE".into()]),
            forbidden: vec!["AIRCRAFT".into()],
            skip_self: false,
        },
        single_burst: false,
    });
    crate::skirmish::tests::register_update(&mut bay, "AutoHealBehavior");
    game.catalog.push(bay);
    let bay = game.spawn(bay_type, 0, [350.0, 100.0], true);
    game.catalog[tank].maintenance.healing_kind_flags = 2;
    let ground = game.spawn(tank, 0, [740.0, 500.0], true);
    let enemy = game.spawn(tank, 1, [700.0, 500.0], true);
    let air_type = game.catalog.len();
    let mut air = fixture_type(Faction::Usa, Kind::Tank);
    air.kind_of.push("AIRCRAFT".into());
    game.catalog.push(air);
    let air = game.spawn(air_type, 0, [690.0, 500.0], true);
    for id in [ground, enemy, air] {
        game.entities[id].health = 50.0;
    }
    game.wake_family_update(bay, "AutoHealBehavior", 0, 1);
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[ground].health.to_bits(), 52.0_f32.to_bits());
    assert_eq!(game.entities[enemy].health.to_bits(), 50.0_f32.to_bits());
    assert_eq!(game.entities[air].health.to_bits(), 50.0_f32.to_bits());
    assert_eq!(game.entities[ground].maintenance.healing_benefactor, None);
    game.transfer_maintenance_owner(bay, 1);
    game.tick = 31;
    game.run_object_updates();
    assert_eq!(game.entities[ground].health.to_bits(), 52.0_f32.to_bits());
    assert_eq!(game.entities[enemy].health.to_bits(), 52.0_f32.to_bits());
}

fn pulse_fixture() -> (Skirmish, usize) {
    use crate::skirmish::{FactionProgression, ProgressionRules, Rank};
    let mut game = game();
    game.configure_progression(ProgressionRules {
        multiplayer_colors: crate::skirmish::tests::original_multiplayer_colors(),
        ranks: vec![Rank {
            skill_points: 0,
            science_points: 0,
            sciences: Vec::new(),
        }],
        sciences: Vec::new(),
        powers: Vec::new(),
        factions: vec![
            FactionProgression {
                side: "GLA".into(),
                preferred_color: 0xff00_ff00,
                faction: Faction::Gla,
                playable_side: true,
                intrinsic_points: 0,
                intrinsic_sciences: Vec::new(),
            },
            FactionProgression {
                side: "Civilian".into(),
                preferred_color: 0xffff_ffff,
                faction: Faction::Civilian,
                playable_side: false,
                intrinsic_points: 0,
                intrinsic_sciences: Vec::new(),
            },
        ],
    })
    .unwrap();
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    bind_tech(&mut unit);
    bind_tech(&mut unit);
    unit.neutral_tech.tech_buildings[0] = TechBuildingRules {
        pulse_fx: Some("PulseFirst".into()),
        pulse_frames: 3,
    };
    unit.neutral_tech.tech_buildings[1] = TechBuildingRules {
        pulse_fx: Some("PulseSecond".into()),
        pulse_frames: 5,
    };
    let index = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(index, game.neutral_owner(), [310.0, 300.0], true);
    (game, id)
}

#[test]
fn tech_constructor_evaluates_once_and_capture_wakes_each_authored_pulse() {
    use crate::skirmish::module_updates::{FOREVER, ModuleRegistration};
    let (mut game, id) = pulse_fixture();
    let registrations:Vec<_>=game.catalog[game.entities[id].type_index].update_modules.iter().enumerate()
        .filter_map(|(index,module)|matches!(module,ModuleRegistration::Behavior{family,..} if family=="TechBuildingBehavior").then_some(index)).collect();
    for &index in &registrations {
        assert_eq!(game.update_remaining_sleep(id, index), 1);
    }
    game.tick = 1;
    game.run_object_updates();
    assert!(
        !game
            .model_flags(&game.entities[id], [0.5, 0.1])
            .contains("CAPTURED")
    );
    assert!(game.presentation.object_fx_events.is_empty());
    for &index in &registrations {
        assert_eq!(
            game.update_remaining_sleep(id, index) + game.update_frame(),
            FOREVER
        );
    }
    game.transfer_maintenance_owner(id, 0);
    assert!(
        !game
            .model_flags(&game.entities[id], [0.5, 0.1])
            .contains("CAPTURED")
    );
    game.tick = 2;
    game.run_object_updates();
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("CAPTURED")
    );
    let effects: Vec<_> = game
        .presentation
        .object_fx_events
        .iter()
        .map(|event| event.effect.as_str())
        .collect();
    assert_eq!(effects, ["PulseFirst", "PulseSecond"]);
    game.presentation.object_fx_events.clear();
    game.tick = 5;
    game.run_object_updates();
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    assert_eq!(game.presentation.object_fx_events[0].effect, "PulseFirst");
    game.transfer_maintenance_owner(id, game.neutral_owner());
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("CAPTURED")
    );
    game.tick = 6;
    game.run_object_updates();
    assert!(
        !game
            .model_flags(&game.entities[id], [0.5, 0.1])
            .contains("CAPTURED")
    );
}

#[test]
fn tech_uses_template_playability_and_does_not_require_a_deposit_or_tech_kind() {
    let (mut game, id) = pulse_fixture();
    game.transfer_maintenance_owner(id, 0);
    let index = game.entities[id].type_index;
    game.catalog[index].maintenance.kind_flags &= !MaintenanceRules::TECH_BUILDING;
    game.defeated[0] = true;
    game.tick = 1;
    game.run_object_updates();
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("CAPTURED")
    );
    game.kill_object(id, crate::skirmish::DeathType::Normal);
    assert_eq!(game.entities[id].owner, game.neutral_owner());
    assert!(
        !game
            .model_flags(&game.entities[id], [0.5, 0.1])
            .contains("CAPTURED")
    );
}

#[test]
fn tech_capture_model_and_pulse_read_actual_player_identity() {
    let (mut game, id) = pulse_fixture();
    game.transfer_maintenance_owner(id, 0);
    game.players[0].playable_side = false;
    game.tick = 2;
    game.run_object_updates();
    assert!(
        !game.entities[id]
            .drawable
            .model_conditions
            .has_flag(ModelConditions::CAPTURED)
    );
    assert!(game.presentation.object_fx_events.is_empty());
    game.players[0].playable_side = true;
    assert_eq!(game.update_tech_building(id, 0), 3);
    assert!(
        game.entities[id]
            .drawable
            .model_conditions
            .has_flag(ModelConditions::CAPTURED)
    );
    assert_eq!(game.presentation.object_fx_events[0].effect, "PulseFirst");
}

#[test]
fn blocked_cost_upgrade_does_not_transfer_a_discount_it_never_applied() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let mut refinery = fixture_type(Faction::Civilian, Kind::TechBuilding);
    refinery.neutral_tech.cost_modifier = Some(ProductionCostModifier {
        kind_mask: 2,
        percentage: -0.1,
    });
    bind_cost_modifier(&mut refinery);
    refinery
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .conflicts = vec!["Upgrade_DiscountBlocked".into()];
    game.players[0]
        .upgrades
        .push("Upgrade_DiscountBlocked".into());
    let kind = game.catalog.len();
    game.catalog.push(refinery);
    let id = game.spawn(kind, 0, [500.0, 100.0], true);
    assert!(!game.native_upgrade_executed(id, "CostModifierUpgrade", 0));
    assert_eq!(
        game.production_cost_multiplier(0, tank).to_bits(),
        1.0_f32.to_bits()
    );
    game.transfer_maintenance_owner(id, 1);
    assert_eq!(
        game.production_cost_multiplier(0, tank).to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        game.production_cost_multiplier(1, tank).to_bits(),
        1.0_f32.to_bits()
    );
    assert!(!game.native_upgrade_executed(id, "CostModifierUpgrade", 0));
    // Object::giveUpgrade reevaluates its modules even if this object mask was
    // already present; capture alone never requests updateUpgradeModules.
    game.grant_native_object_upgrade(id, "Upgrade_CostReduction");
    assert_eq!(
        game.production_cost_multiplier(1, tank).to_bits(),
        0.9_f32.to_bits()
    );
    assert!(game.native_upgrade_executed(id, "CostModifierUpgrade", 0));
    game.destroy_maintenance_object(id);
    assert!(!game.native_upgrade_executed(id, "CostModifierUpgrade", 0));
    assert_eq!(
        game.production_cost_multiplier(1, tank).to_bits(),
        1.0_f32.to_bits()
    );
    game.destroy_maintenance_object(id);
    assert_eq!(
        game.production_cost_multiplier(1, tank).to_bits(),
        1.0_f32.to_bits()
    );
}
