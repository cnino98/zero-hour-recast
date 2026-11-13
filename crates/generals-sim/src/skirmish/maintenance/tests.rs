use super::super::{
    Faction, HoleExposure, Kind, RebuildRules,
    tests::{fixture_type, game},
};

#[test]
fn deleted_mobile_and_corpse_release_only_their_own_path_claims() {
    for dead in [false, true] {
        let (mut game, _) = crate::skirmish::containment::tests::setup(0.0, 0.0);
        let mover = game.spawn(5, 0, [305.0, 105.0], true);
        let removed = game.spawn(5, 0, [355.0, 205.0], true);
        game.refresh_occupancy();
        let cell = game
            .movement_cell(removed, game.entities[removed].position)
            .unwrap();
        assert_eq!(
            game.occupancy
                .position_owner_on(crate::grid::LayerCell { cell, layer: 1 }),
            Some(removed)
        );
        if dead {
            game.entities[removed].health = 0.0;
            game.finish_death(removed, None);
        }
        game.move_to(mover, [455.0, 105.0]).unwrap();
        game.process_path_service();
        let path = game.entities[mover].ai.path.raw.clone();
        let goal = game.entities[mover].ai.path_goal;
        assert!(!path.is_empty());
        game.destroy_maintenance_object(removed);
        game.process_update_destruction();
        assert!(game.entities[removed].masked());
        assert_eq!(
            game.occupancy
                .position_owner_on(crate::grid::LayerCell { cell, layer: 1 }),
            None
        );
        assert!(game.occupancy.goal_cell(removed).is_none());
        assert!(game.entities[mover].ai.path.raw == path);
        assert!(game.entities[mover].ai.path_goal == goal);
        assert!(game.entities[mover].ai.ground_path.is_some());
    }
}

#[test]
fn deleting_a_live_structure_removes_its_static_navigation_footprint() {
    let mut game = game();
    let building = game.spawn(3, 0, [355.0, 205.0], true);
    game.rebuild_navigation();
    let cell = game.cell_at(game.entities[building].position).unwrap();
    assert!(game.navigation.is_blocked(cell));
    game.destroy_maintenance_object(building);
    game.process_update_destruction();
    assert!(!game.navigation.is_blocked(cell));
    assert!(game.entities[building].masked());
}

#[test]
fn repairs_use_one_benefactor_and_cancel_without_charging_money() {
    let mut game = game();
    let worker_type = game.entities[1].type_index;
    game.catalog[worker_type].maintenance.repair_per_second = 0.02;
    game.entities[0].health = 500.0;
    let position = game.entities[1].position;
    let second = game.spawn(worker_type, 0, position, true);
    // Native Dozer task frame zero is the empty-task sentinel.
    game.tick = 1;
    game.repair(1, 0).unwrap();
    game.repair(second, 0).unwrap();
    let cash = game.players[0].cash;
    for _ in 0..300 {
        game.step();
        if game.entities[0].health > 500.0
            && game.entities[0].maintenance.healing_benefactor.is_some()
        {
            break;
        }
    }
    assert!(game.entities[0].health > 500.0);
    let benefactor = game.entities[0].maintenance.healing_benefactor.unwrap();
    assert!(benefactor == 1 || benefactor == second);
    let competitor = if benefactor == 1 { second } else { 1 };
    // Native internalTaskComplete clears the losing repair task without cancelling
    // its already-issued AI movement (unlike internalCancelTask).
    for _ in 0..5 {
        game.step();
    }
    assert!(!game.entities[competitor].has_pending_dozer_task());
    assert_eq!(game.entities[benefactor].repair_task_target(), Some(0));
    assert!(
        game.entities[benefactor]
            .drawable
            .model_conditions
            .constructing
    );
    assert_eq!(
        game.entities[0].maintenance.healing_benefactor,
        Some(benefactor)
    );
    assert_eq!(game.entities[competitor].repair_task_target(), None);
    assert_eq!(game.players[0].cash, cash);
    game.stop(benefactor).unwrap();
    let health = game.entities[0].health;
    for _ in 0..5 {
        game.step();
    }
    assert_eq!(game.entities[0].health.to_bits(), health.to_bits());
}

#[test]
fn sale_waits_for_scaffold_then_refunds_without_kill_credit() {
    let mut game = game();
    game.catalog[0].maintenance.sell_fraction = 0.5;
    let cash = game.players[0].cash;
    let cost = game.catalog[0].cost;
    assert_eq!(game.select_object(0, &[], 0, true), [0]);
    game.sell(0, 0).unwrap();
    assert!(game.selected_objects(0).is_empty());
    assert_eq!(game.entities[0].animation_duration(), Some((0, 45, false)));
    assert!(!game.entities[0].selectable(&game.catalog[game.entities[0].type_index]));
    assert!(game.sell(0, 0).is_err());
    for _ in 0..44 {
        game.update_sales();
        game.tick += 1;
    }
    assert_eq!(
        game.entities[0].construction_percent.to_bits(),
        99.9_f32.to_bits()
    );
    for _ in 0..135 {
        game.tick += 1;
        game.update_sales();
        game.process_update_destruction();
    }
    assert!(!game.entities[0].exists());
    assert_eq!(game.tick, 179);
    assert_eq!(game.players[0].cash, cash + cost / 2);
    assert_eq!(game.players[0].statistics.money_earned, 0);
    assert_eq!(game.players[0].statistics.buildings_lost, 0);
}

#[test]
fn destroyed_sale_has_no_refund() {
    let mut game = game();
    game.catalog[0].maintenance.sell_fraction = 0.5;
    let cash = game.players[0].cash;
    game.sell(0, 0).unwrap();
    game.destroy_maintenance_object(0);
    game.process_update_destruction();
    assert!(!game.entities[0].exists());
    game.tick = 200;
    game.update_sales();
    assert_eq!(game.players[0].cash, cash);
}

#[test]
fn sold_structure_retained_by_slow_death_still_finishes_sale() {
    use crate::skirmish::{DeathType, death_lifecycle};

    let mut game = game();
    let mut unit = fixture_type(Faction::Gla, Kind::Defense);
    unit.maintenance.refund = 500;
    unit.behavior_modules
        .retain(|module| module.family != "DestroyDie");
    unit.death_lifecycle = Some(death_lifecycle::Rules {
        on_die: Vec::new(),
        physics: None,
        slow: vec![death_lifecycle::SlowDeath {
            derived: death_lifecycle::Derived::None,
            tag: "Slow".into(),
            conditions: crate::skirmish::tests::death_conditions(),
            probability: 10,
            overkill_bonus: 0.0,
            sink_delay: [200, 0],
            destruction_delay: [240, 0],
            sink_rate: 0.0,
            fling: None,
            fx: std::array::from_fn(|_| Vec::new()),
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: std::array::from_fn(|_| Vec::new()),
        }],
        weapons: Vec::new(),
    });
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, 0, [350.0, 250.0], true);
    let cash = game.players[0].cash;
    game.sell(0, id).unwrap();
    game.tick = 44;
    game.kill_object(id, DeathType::Normal);
    game.process_update_destruction();
    assert!(game.entities[id].exists() && game.entities[id].corpse_present());
    for frame in 45..=179 {
        game.tick = frame;
        game.update_sales();
        game.process_update_destruction();
    }
    assert!(!game.entities[id].exists());
    assert_eq!(game.players[0].cash, cash + 500);
}

#[test]
fn hole_rebuilds_for_free_and_foundation_cancel_does_not_refund() {
    let mut game = game();
    // Normal rebuild timing needs surviving owned power after the command
    // center dies (Energy::getEnergySupplyRatio).
    let mut plant = fixture_type(Faction::Usa, Kind::PowerPlant);
    plant.energy = 5;
    let plant_type = game.catalog.len();
    game.catalog.push(plant);
    game.spawn(plant_type, 0, [155.0, 455.0], true);
    game.update_power();
    let mut hole = fixture_type(Faction::Gla, Kind::TechBuilding);
    hole.name = "Hole".into();
    game.catalog[1].name = "Worker".into();
    hole.maintenance.rebuild.push(RebuildRules {
        worker: "Worker".into(),
        respawn_frames: 2.0,
        regeneration: 0.005,
    });
    let hole_type = game.catalog.len();
    crate::skirmish::tests::register_update(&mut hole, "RebuildHoleBehavior");
    hole.behavior_modules.last_mut().unwrap().interfaces |= 2;
    game.catalog.push(hole);
    game.catalog[0].maintenance.expose_hole.push(HoleExposure {
        conditions: crate::skirmish::death_callbacks::Conditions {
            death_mask: u32::MAX,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
        },
        template: "Hole".into(),
        health: 500.0,
        transfer_attackers: true,
    });
    crate::skirmish::tests::register_behavior(&mut game.catalog[0], "RebuildHoleExposeDie", 2);
    let cash = game.players[0].cash;
    game.entities[0].health = 0.0;
    game.finish_death(0, None);
    let hole_id = game.entities.len() - 1;
    assert_eq!(game.entities[hole_id].type_index, hole_type);
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[hole_id].masked());
    game.tick += 1;
    game.run_object_updates();
    assert!(game.entities[hole_id].masked());
    let foundation = game.entities.len() - 1;
    game.cancel_construction(0, foundation).unwrap();
    game.destroy_maintenance_object(foundation);
    game.process_update_destruction();
    assert_eq!(game.players[0].cash, cash);
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[hole_id].masked());
    game.tick += 1;
    game.run_object_updates();
    let replacement = game.entities.len() - 1;
    assert!(replacement > foundation);
    for _ in 0..1500 {
        game.step();
        if !game.entities[hole_id].alive() {
            break;
        }
    }
    assert!(game.entities[replacement].complete());
    assert!(!game.entities[hole_id].alive());
    assert_eq!(game.players[0].cash, cash);
}

fn research(name: &str, player: bool) -> super::Research {
    super::Research {
        name: name.into(),
        cost: 100,
        frames: 2,
        player,
    }
}

#[test]
fn research_shares_production_queue_and_respects_player_and_object_scope() {
    let mut game = game();
    game.catalog[0].maintenance.research =
        vec![research("Capture", true), research("Radar", false)];
    let second = game.spawn(0, 0, [300.0, 300.0], true);
    let cash = game.players[0].cash;
    game.queue_upgrade(0, 0).unwrap();
    assert!(game.queue_upgrade(second, 0).is_err());
    game.queue_unit(0, 1).unwrap();
    assert_eq!(game.entities[0].queue.len(), 2);
    game.cancel_upgrade(0, 0, 0).unwrap();
    assert_eq!(game.players[0].cash, cash - game.catalog[1].cost);
    let id = game.entities[0].queue[0].id;
    game.cancel_production(0, 0, id).unwrap();
    assert_eq!(game.players[0].cash, cash);
    game.queue_upgrade(0, 0).unwrap();
    for _ in 0..2 {
        game.update_production(0);
    }
    assert!(game.has_upgrade(0, "Capture"));
    assert!(!game.has_upgrade(1, "Capture"));
    assert!(!game.can_research(second, 0));
    game.queue_upgrade(0, 1).unwrap();
    game.queue_upgrade(second, 1).unwrap();
    for _ in 0..2 {
        game.update_production(0);
    }
    assert!(game.object_has_upgrade(0, "Radar"));
    assert!(!game.object_has_upgrade(second, "Radar"));
    game.cancel_upgrade(0, second, 1).unwrap();
    assert!(!game.object_has_upgrade(second, "Radar"));
}

#[test]
fn research_callbacks_only_follow_completion_and_snapshot_producer_before_later_changes() {
    let mut game = game();
    game.catalog[0].maintenance.research = vec![research("Radar", false)];
    game.queue_upgrade(0, 0).unwrap();
    game.update_production(0);
    assert!(game.presentation.research_events.is_empty());
    game.cancel_upgrade(0, 0, 0).unwrap();
    assert!(game.presentation.research_events.is_empty());
    game.queue_upgrade(0, 0).unwrap();
    let position = game.object_position(0);
    game.update_production(0);
    game.update_production(0);
    game.entities[0].owner = 1;
    game.entities[0].position = [400.0; 2];
    assert_eq!(game.presentation.research_events.len(), 1);
    let event = &game.presentation.research_events[0];
    assert_eq!(event.producer, 0);
    assert_eq!(event.owner, 0);
    assert_eq!(event.type_index, 0);
    assert_eq!(event.upgrade, 0);
    assert_eq!(event.position.map(f32::to_bits), position.map(f32::to_bits));
    game.tick = event.frame + 2;
    game.clear_frame_events();
    assert!(game.presentation.research_events.is_empty());
    game.finish_research(0, 0);
    game.reset_slots(game.slots.clone()).unwrap();
    assert!(game.presentation.research_events.is_empty());
}

pub(super) fn capture_game() -> (super::Skirmish, usize, usize) {
    let mut game = game();
    game.catalog[0].maintenance.research = vec![research("Capture", true)];
    game.catalog[5].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    crate::skirmish::special_ability::capture_fixture::bind(
        &mut game.catalog[5],
        crate::skirmish::special_ability::PowerType::SpecialInfantryCaptureBuilding,
        [3, 4, 2, 2],
        true,
    );
    let mut oil = fixture_type(Faction::Civilian, Kind::TechBuilding);
    oil.name = "Oil".into();
    oil.kind_of.push("CAPTURABLE".into());
    oil.maintenance.kind_flags =
        super::MaintenanceRules::CAPTURABLE | super::MaintenanceRules::TECH_BUILDING;
    crate::skirmish::neutral_tech::tests::bind_tech(&mut oil);
    oil.maintenance.deposits.push(super::DepositRules {
        interval: 4,
        amount: 200,
        first_capture: 1000,
        actual_money: true,
        upgrades: Vec::new(),
    });
    crate::skirmish::tests::register_update(&mut oil, "AutoDepositUpdate");
    let oil_type = game.catalog.len();
    game.catalog.push(oil);
    let oil = game.spawn(oil_type, game.neutral_owner(), [300.0, 200.0], true);
    let infantry = game.spawn(5, 0, [335.0, 200.0], true);
    game.update_ground_contacts();
    game.update_visibility();
    (game, infantry, oil)
}

#[test]
fn authored_civilian_income_does_not_replace_the_fixed_neutral_owner() {
    let (mut game, _, oil) = capture_game();
    let template = game.entities[oil].type_index;
    game.layout
        .map_sides
        .push(crate::skirmish::map_scripts::SideDefinition {
            name: "PlyrCivilian".into(),
            faction: "FactionCivilian".into(),
            gatherers: [0; 3],
            human: false,
            template_money: 700,
            start_money: 0,
            allies: Vec::new(),
            enemies: Vec::new(),
        });
    game.reset_slots(game.slots.clone()).unwrap();
    let neutral = game.neutral_owner();
    let civilian = game.players.len() - 1;
    assert_ne!(neutral, civilian);
    game.spawn(template, neutral, [300.0, 200.0], true);
    game.spawn(template, civilian, [500.0, 200.0], true);
    game.tick = 4;
    game.run_object_updates();
    assert_eq!(game.players[neutral].cash, 0);
    assert_eq!(game.players[civilian].cash, 900);
    assert_eq!(game.players[civilian].statistics.money_earned, 200);
}

#[test]
fn capture_face_tracks_the_object_goal_and_unpacking_clears_it_immediately() {
    let (mut game, infantry, oil) = capture_game();
    game.players[0].upgrades.push("Capture".into());
    game.activate_upgrade_modules(infantry);
    game.tick = 2;
    game.capture(infantry, oil).unwrap();
    assert_eq!(
        game.entities[infantry].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    for _ in 0..600 {
        game.step();
        if matches!(
            game.entities[infantry].ai.command.main_state,
            crate::skirmish::ai_goals::MainState::FaceObject { .. }
        ) {
            break;
        }
    }
    assert!(matches!(
        game.entities[infantry].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::FaceObject { .. }
    ));
    assert_eq!(game.entities[infantry].ai.command.goal_object, Some(oil));
    assert_eq!(
        game.entities[infantry].ai_goal_position().map(f32::to_bits),
        game.object_position(oil).map(f32::to_bits)
    );
    for _ in 0..120 {
        game.step();
        if game.entities[infantry].capture_condition() == Some("UNPACKING") {
            break;
        }
    }
    assert_eq!(
        game.entities[infantry].capture_condition(),
        Some("UNPACKING")
    );
    assert_eq!(game.entities[infantry].ai.command.ability_goal, None);
    assert_eq!(
        game.entities[infantry].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
}

#[test]
fn capture_accepts_partial_visibility_but_rejects_the_remembered_building() {
    use crate::skirmish::visibility::{Status, VisibilityRules};
    let (mut game, infantry, oil) = capture_game();
    for unit in &mut game.catalog {
        unit.sight.clearing_range = 0.0;
        unit.sight.immobile = unit.kind.is_building();
    }
    game.catalog[5].sight.clearing_range = 100.0;
    let oil_type = game.entities[oil].type_index;
    game.catalog[oil_type].geometry.is_small = false;
    game.entities[infantry].position = [200.0, 200.0];
    game.players[0].upgrades.push("Capture".into());
    game.activate_upgrade_modules(infantry);
    game.tick = 2;
    game.configure_visibility(VisibilityRules {
        cell_size: 10.0,
        unlook_persist_frames: 30,
        use_shroud: true,
    })
    .unwrap();
    assert!(matches!(game.visibility_status(0, oil), Status::Partial));
    assert!(!game.visible_to(0, oil));
    assert!(game.can_capture(infantry, oil));
    game.capture(infantry, oil).unwrap();
    game.stop(infantry).unwrap();
    game.entities[infantry].position = [650.0, 500.0];
    game.observe_visibility(infantry);
    game.tick += 31;
    game.update_visibility();
    assert!(matches!(game.visibility_status(0, oil), Status::Fogged));
    assert!(!game.can_capture(infantry, oil));
    assert!(game.capture(infantry, oil).is_err());
}

#[test]
fn capture_approaches_the_ignored_building_from_outside_ability_range() {
    use crate::skirmish::{GroundAppearance, GroundRules};
    let (mut game, infantry, oil) = capture_game();
    let unit = &mut game.catalog[5];
    unit.geometry.major = 7.0;
    unit.geometry.minor = 7.0;
    unit.speed = 20.0;
    unit.locomotor.ground = Some(GroundRules {
        wander: None,
        appearance: GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        acceleration_damaged: 50.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        braking: 100.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 5.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    game.entities[infantry].position = [375.0, 200.0];
    game.rebuild_navigation();
    game.players[0].upgrades.push("Capture".into());
    game.activate_upgrade_modules(infantry);
    game.tick = 2;
    assert!(
        game.movement_path(
            infantry,
            game.entities[infantry].position,
            game.entities[oil].position
        )
        .is_none()
    );
    game.capture(infantry, oil).unwrap();
    // Initiation wakes SpecialAbilityUpdate; its next callback issues approach.
    game.step();
    assert!(
        game.movement_path(
            infantry,
            game.entities[infantry].position,
            game.entities[oil].position
        )
        .is_some()
    );
    for _ in 0..600 {
        game.step();
        if game.entities[oil].owner == 0 {
            break;
        }
    }
    assert_eq!(game.entities[oil].owner, 0);
    assert!(game.entities[infantry].position[0] < 340.0);
    assert!(game.entities[infantry].ai.ignored_producer.is_none());
}

#[test]
fn capture_requires_research_recharge_then_runs_unpack_prepare_and_pack() {
    let (mut game, infantry, oil) = capture_game();
    assert!(game.capture(infantry, oil).is_err());
    game.queue_upgrade(0, 0).unwrap();
    game.update_production(0);
    game.update_production(0);
    assert!(!game.capture_ready(infantry));
    game.tick = 2;
    assert!(game.capture_ready(infantry));
    let cash = game.players[0].cash;
    game.capture(infantry, oil).unwrap();
    let mut phases = Vec::new();
    for _ in 0..100 {
        game.step();
        if let Some(phase) = game.entities[infantry].capture_condition() {
            phases.push(phase);
        }
        if game.entities[oil].owner == 0 && game.entities[infantry].ai_idle() {
            break;
        }
    }
    assert_eq!(game.entities[oil].owner, 0);
    assert_eq!(
        phases.iter().filter(|&&phase| phase == "UNPACKING").count(),
        3
    );
    assert_eq!(
        phases
            .iter()
            .filter(|&&phase| phase == "RAISING_FLAG")
            .count(),
        4
    );
    assert_eq!(
        phases.iter().filter(|&&phase| phase == "PACKING").count(),
        2
    );
    assert_eq!(game.players[0].cash, cash + 1000);
    for _ in 0..4 {
        game.step();
    }
    assert_eq!(game.players[0].cash, cash + 1200);
    assert_eq!(game.players[0].statistics.money_earned, 1200);
}

#[test]
fn cancelled_capture_never_changes_owner_and_keeps_recharge() {
    let (mut game, infantry, oil) = capture_game();
    game.players[0].upgrades.push("Capture".into());
    game.activate_upgrade_modules(infantry);
    game.tick = 2;
    game.capture(infantry, oil).unwrap();
    for _ in 0..100 {
        game.step();
        if game.entities[infantry].capture_condition() == Some("RAISING_FLAG") {
            break;
        }
    }
    game.step();
    let ready = game.special_power_ready_frame(infantry, "InfantryCapture");
    game.stop(infantry).unwrap();
    for _ in 0..10 {
        game.step();
    }
    assert_eq!(game.entities[oil].owner, game.neutral_owner());
    assert!(game.entities[infantry].capture_condition().is_none());
    assert_eq!(
        game.special_power_ready_frame(infantry, "InfantryCapture"),
        ready
    );
}

#[test]
fn native_hole_template_health_is_not_rejected_by_a_scaffold_cap() {
    let mut game = game();
    // Installed GLAHole InitialHealth and MaxHealth are 9999999.9.
    game.catalog[0].max_health = "9999999.9".parse().unwrap();
    game.catalog[0].initial_health = "9999999.9".parse().unwrap();
    assert!(super::super::validate_catalog(&game.catalog, &game.rules).is_ok());
    game.catalog[0].max_health = f32::INFINITY;
    assert!(super::super::validate_catalog(&game.catalog, &game.rules).is_err());
}

#[test]
fn capture_faces_once_then_unpacks_and_retains_cooldown_on_cancellation() {
    let (mut game, infantry, oil) = capture_game();
    game.players[0].upgrades.push("Capture".into());
    game.activate_upgrade_modules(infantry);
    game.tick = 2;
    game.entities[infantry].position = [320.0, 200.0];
    game.entities[infantry].facing = 0.0;
    game.capture(infantry, oil).unwrap();
    game.step();
    assert!(matches!(
        game.entities[infantry].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::FaceObject { .. }
    ));
    let snapshot = game.entities[infantry].ai_goal_position();
    assert_eq!(
        snapshot.map(f32::to_bits),
        game.object_position(oil).map(f32::to_bits)
    );
    for _ in 0..120 {
        game.step();
        if game.entities[infantry].capture_condition() == Some("UNPACKING") {
            break;
        }
    }
    assert_eq!(
        game.entities[infantry].capture_condition(),
        Some("UNPACKING")
    );
    assert!(game.entities[infantry].ai_busy());
    assert!(game.capture_in_use(infantry));
    game.stop(infantry).unwrap();
    // The command changes AI source immediately; SpecialAbilityUpdate::update
    // observes it and runs onExit on its next real callback.
    assert!(game.capture_in_use(infantry));
    game.step();
    assert!(!game.capture_in_use(infantry));
    assert_eq!(game.capture_progress(infantry).to_bits(), 1.0_f32.to_bits());
    assert!(game.entities[infantry].capture_condition().is_none());
}

#[test]
fn capture_flash_cadence_accelerates_and_persists_across_commands() {
    let (mut game, infantry, oil) = capture_game();
    game.catalog[5].behavior.special_abilities[0]
        .timing
        .preparation = 120;
    game.players[0].upgrades.push("Capture".into());
    game.activate_upgrade_modules(infantry);
    game.tick = 2;
    game.entities[infantry].position = [320.0, 200.0];
    game.capture(infantry, oil).unwrap();
    for _ in 0..600 {
        if game.entities[infantry].capture_condition() == Some("RAISING_FLAG") {
            break;
        }
        game.tick += 1;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[infantry].capture_condition(),
        Some("RAISING_FLAG")
    );
    assert_eq!(game.capture_progress(infantry).to_bits(), 0.0_f32.to_bits());
    let started = game.tick;
    let mut ticks = Vec::new();
    for _ in 0..119 {
        game.tick += 1;
        game.run_object_updates();
        if game.presentation.ability_feedback.iter().any(|event| {
            event.frame == game.tick
                && matches!(
                    event.effect,
                    crate::skirmish::special_ability::FeedbackEffect::CaptureFlash
                )
        }) {
            ticks.push(game.tick - started);
        }
        assert_eq!(game.capture_progress(infantry).to_bits(), 0.0_f32.to_bits());
    }
    // Direct C++ continuePreparation recurrence, with float accumulation.
    assert_eq!(ticks, vec![38, 54, 66, 76, 85, 93, 100, 107, 114]);
    game.stop(infantry).unwrap();
    game.tick += 2;
    game.capture(infantry, oil).unwrap();
    for _ in 0..600 {
        if game.entities[infantry].capture_condition() == Some("RAISING_FLAG") {
            break;
        }
        game.tick += 1;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[infantry].capture_condition(),
        Some("RAISING_FLAG")
    );
    let restarted = game.tick;
    loop {
        game.tick += 1;
        game.run_object_updates();
        if game.presentation.ability_feedback.iter().any(|event| {
            event.frame == game.tick
                && matches!(
                    event.effect,
                    crate::skirmish::special_ability::FeedbackEffect::CaptureFlash
                )
        }) {
            assert!(game.tick - restarted < 38);
            break;
        }
        assert!(game.tick - restarted < 120);
    }
}

#[test]
fn capture_approach_uses_reduced_partition_range_and_native_terrain_los() {
    let (mut game, infantry, oil) = capture_game();
    game.players[0].upgrades.push("Capture".into());
    game.activate_upgrade_modules(infantry);
    game.tick = 2;
    game.entities[infantry].position = [333.0, 200.0];
    game.capture(infantry, oil).unwrap();
    game.tick += 1;
    game.update_ground_contacts();
    game.run_object_updates();
    assert!(!game.capture_in_use(infantry));
    game.entities[infantry].position = [332.0, 200.0];
    let mut heights = vec![0.0; 80 * 60];
    heights[20 * 80 + 32] = 100.0;
    game.set_projectile_terrain(80, 60, [0.0; 2], heights, 10.0)
        .unwrap();
    assert!(!game.ability_line_of_sight(infantry, oil));
    game.tick += 1;
    game.update_ground_contacts();
    game.run_object_updates();
    assert!(!game.capture_in_use(infantry));
    game.set_projectile_terrain(80, 60, [0.0; 2], vec![0.0; 80 * 60], 10.0)
        .unwrap();
    game.tick += 1;
    game.update_ground_contacts();
    game.run_object_updates();
    assert!(game.capture_in_use(infantry));
}

#[test]
fn capture_rechecks_native_sold_status_when_defection_triggers() {
    let (mut game, infantry, oil) = capture_game();
    game.transfer_maintenance_owner(oil, 1);
    game.players[0].upgrades.push("Capture".into());
    game.activate_upgrade_modules(infantry);
    game.tick = 2;
    game.entities[infantry].position = [320.0, 200.0];
    game.capture(infantry, oil).unwrap();
    for _ in 0..600 {
        if game.entities[infantry].capture_condition() == Some("RAISING_FLAG") {
            break;
        }
        game.tick += 1;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[infantry].capture_condition(),
        Some("RAISING_FLAG")
    );
    game.sell(1, oil).unwrap();
    // OBJECT_STATUS_SOLD is immediate. The draw condition waits for the sink.
    assert!(!game.entities[oil].sold_model());
    assert!(!game.can_capture(infantry, oil));
    for _ in 0..4 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert_eq!(game.entities[oil].owner, 1);
    assert_eq!(game.entities[infantry].capture_condition(), Some("PACKING"));
    assert!(
        !game
            .presentation
            .capture_events
            .iter()
            .any(|event| matches!(event.effect, super::CaptureEffect::Captured))
    );
}

#[test]
fn supply_lines_oil_boost_follows_current_owner_without_inflating_earned_statistics() {
    let (mut game, _, oil) = capture_game();
    let oil_type = game.entities[oil].type_index;
    game.catalog[oil_type]
        .maintenance
        .deposits
        .first_mut()
        .unwrap()
        .upgrades
        .push(super::DepositBoost {
            trigger: Some("Upgrade_AmericaSupplyLines".into()),
            amount: 20,
        });
    game.tick = 4;
    game.run_object_updates();
    game.players[0]
        .upgrades
        .push("Upgrade_AmericaSupplyLines".into());
    game.transfer_maintenance_owner(oil, 0);
    let cash = game.players[0].cash;
    let earned = game.players[0].statistics.money_earned;
    game.tick += 4;
    game.run_object_updates();
    assert_eq!(game.players[0].cash, cash + 220);
    assert_eq!(game.players[0].statistics.money_earned, earned + 200);
    game.transfer_maintenance_owner(oil, 1);
    let cash = game.players[1].cash;
    let earned = game.players[1].statistics.money_earned;
    game.tick += 4;
    game.run_object_updates();
    assert_eq!(game.players[1].cash, cash + 200);
    assert_eq!(game.players[1].statistics.money_earned, earned + 200);
}
