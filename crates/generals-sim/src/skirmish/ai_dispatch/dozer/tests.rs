use super::{BUILD, Phase, REPAIR};
use crate::skirmish::{
    Faction, Kind, Order, Skirmish, UnitAiUpdate,
    module_updates::ModuleRegistration,
    tests::{fixture_type, register_update},
};

fn native_dozer() -> (Skirmish, usize) {
    native_dozer_at(10, [100.0, 100.0])
}

fn native_dozer_at(frame: u64, position: [f32; 2]) -> (Skirmish, usize) {
    let (mut game, tank) = crate::skirmish::ground_movement::tests::moving_tank();
    let index = game.catalog.len();
    game.catalog.push(fixture_type(Faction::Gla, Kind::Builder));
    let ground = game.catalog[game.entities[tank].type_index]
        .locomotor
        .ground
        .take();
    let unit = &mut game.catalog[index];
    unit.ai.update = UnitAiUpdate::Dozer;
    unit.locomotor.ground = ground;
    unit.maintenance.repair_per_second = 0.3;
    unit.maintenance.idle_repair = Some(crate::skirmish::IdleRepair {
        frames: 10000.0,
        range: 100.0,
    });
    unit.update_modules.retain(|module| !matches!(module, ModuleRegistration::Behavior { family, .. } if matches!(family.as_str(), "AIUpdateInterface" | "WorkerAIUpdate" | "DozerAIUpdate")));
    unit.behavior_modules.retain(|module| {
        !matches!(
            module.family.as_str(),
            "AIUpdateInterface" | "WorkerAIUpdate" | "DozerAIUpdate"
        )
    });
    register_update(unit, "DozerAIUpdate");
    game.tick = frame;
    let id = game.spawn(index, 0, position, true);
    game.entities[id].ai.mood.next_check = u64::MAX;
    (game, id)
}

fn damaged_building(game: &mut Skirmish) -> usize {
    let index = game.type_for(0, Kind::Barracks).unwrap();
    let id = game.spawn(index, 0, [160.0, 100.0], true);
    game.change_body_health(id, -game.entities[id].max_health * 0.5);
    game.update_visibility();
    id
}

#[test]
fn authored_dozer_keeps_repair_task_through_real_pick_move_and_action_states() {
    let (mut game, id) = native_dozer();
    let target = damaged_building(&mut game);
    let initial = game.entities[target].health;
    game.repair(id, target).unwrap();
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    game.step();
    assert!(matches!(
        game.entities[id].ai.dozer.as_ref().unwrap().phase,
        Phase::Pick
    ));
    assert_eq!(game.entities[target].health.to_bits(), initial.to_bits());
    game.step();
    assert!(matches!(
        game.entities[id].ai.dozer.as_ref().unwrap().phase,
        Phase::Move
    ));
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
    game.step();
    assert!(matches!(
        game.entities[id].ai.dozer.as_ref().unwrap().phase,
        Phase::Action
    ));
    assert_eq!(game.entities[target].health.to_bits(), initial.to_bits());
    game.step();
    assert!(game.entities[target].health > initial);
    assert!(game.entities[id].drawable.model_conditions.constructing);
    assert_eq!(
        game.entities[target].maintenance.healing_benefactor,
        Some(id)
    );
    game.stop(id).unwrap();
    assert!(!game.entities[id].drawable.model_conditions.constructing);
    assert!(
        game.entities[id].ai.dozer.as_ref().unwrap().tasks[REPAIR]
            .target
            .is_none()
    );
    let health = game.entities[target].health;
    game.step();
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
}

#[test]
fn native_idle_worker_order_is_callback_order_and_busy_reentry_appends() {
    let (mut game, first) = native_dozer();
    let index = game.entities[first].type_index;
    let second = game.spawn(index, 0, [200.0, 100.0], true);
    game.update_dozer_module(second);
    game.update_dozer_module(first);
    assert_eq!(game.idle_workers, [(second, 0), (first, 0)]);
    game.move_to(second, [500.0, 100.0]).unwrap();
    assert_eq!(game.idle_workers, [(first, 0)]);
    game.stop(second).unwrap();
    game.update_dozer_module(second);
    assert_eq!(game.idle_workers, [(first, 0), (second, 0)]);
}

#[test]
fn same_frame_tasks_tie_by_native_slot_and_zero_frame_is_not_selected() {
    let (mut game, id) = native_dozer();
    let target = damaged_building(&mut game);
    game.tick = 0;
    game.new_dozer_task(id, REPAIR, target);
    assert_eq!(
        game.entities[id].ai.dozer.as_ref().unwrap().most_recent(),
        None
    );
    game.tick = 20;
    game.new_dozer_task(id, REPAIR, target);
    let build_type = game.type_for(0, Kind::Barracks).unwrap();
    let site = game.spawn(build_type, 0, [400.0, 200.0], false);
    game.new_dozer_task(id, BUILD, site);
    assert_eq!(
        game.entities[id].ai.dozer.as_ref().unwrap().most_recent(),
        Some(BUILD)
    );
    assert_eq!(game.entities[site].maintenance.builder, Some(id));
}

#[test]
fn only_emp_hacked_subdued_and_power_disable_remember_the_active_task() {
    for (mask, resumes) in [
        (1 << 2, true),
        (1 << 1, true),
        (1 << 6, true),
        (1 << 10, true),
        (1 << 3, false),
        (1 << 5, false),
    ] {
        let (mut game, id) = native_dozer();
        let target = damaged_building(&mut game);
        game.repair(id, target).unwrap();
        game.update_dozer_module(id);
        game.entities[id].disabled_status = mask;
        game.observe_spy_vision_disabled(id);
        assert!(
            game.entities[id].ai.dozer.as_ref().unwrap().tasks[REPAIR]
                .target
                .is_none()
        );
        assert!(matches!(game.entities[id].ai.order, Order::Idle));
        game.entities[id].disabled_status = 0;
        game.observe_spy_vision_disabled(id);
        assert_eq!(
            game.entities[id].ai.dozer.as_ref().unwrap().tasks[REPAIR]
                .target
                .is_some(),
            resumes
        );
    }
}

#[test]
fn a_later_builder_claim_prevents_two_native_tasks_from_constructing_the_same_site() {
    let (mut game, first) = native_dozer();
    let second = game.spawn(game.entities[first].type_index, 0, [120.0, 100.0], true);
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let site = game.spawn(building_type, 0, [180.0, 100.0], false);
    game.assist_build(first, site).unwrap();
    game.assist_build(second, site).unwrap();
    assert_eq!(game.entities[site].maintenance.builder, Some(second));
    for _ in 0..600 {
        game.step();
        if game.entities[site].construction_percent > 0.0 {
            break;
        }
    }
    assert!(game.entities[site].construction_percent > 0.0);
    assert!(
        game.entities[first].ai.dozer.as_ref().unwrap().tasks[BUILD]
            .target
            .is_none()
    );
    assert_eq!(game.entities[site].maintenance.builder, Some(second));
    assert_eq!(game.entities[site].producer, Some(second));
}

#[test]
fn native_build_completion_stops_the_site_sound_and_keeps_the_departure_move() {
    let (mut game, id) = native_dozer();
    let index = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[index].build_seconds = 2.0 / 30.0;
    game.catalog[index].maintenance.construction_sound = "BuildLoop".into();
    let site = game.spawn(index, 0, [180.0, 100.0], false);
    game.assist_build(id, site).unwrap();
    let docks = game.entities[id].ai.dozer.as_ref().unwrap().tasks[BUILD].docks;
    let action = docks[1].unwrap();
    let departure = docks[2].unwrap();
    assert!(((departure[0] - action[0]).hypot(departure[1] - action[1]) - 50.0).abs() < 0.000_01);
    assert_eq!(departure[2].to_bits(), action[2].to_bits());
    let mut sound_actions = Vec::new();
    for _ in 0..600 {
        game.step();
        for event in &game.presentation.object_sounds {
            if event.name == "BuildLoop" {
                assert_eq!(event.object, site);
                sound_actions.push(matches!(
                    event.action,
                    crate::skirmish::object_sounds::Action::Play
                ));
            }
        }
        if game.entities[site].complete() {
            break;
        }
    }
    assert!(game.entities[site].complete());
    assert_eq!(sound_actions, [true, false]);
    assert!(
        matches!(game.presentation.dozer_feedback.as_slice(), [super::Feedback::Construction {
        builder, object_type, owner: 0, ..
    }] if *builder == id && *object_type == index)
    );
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
    assert!(
        game.entities[id].ai.dozer.as_ref().unwrap().tasks[BUILD]
            .target
            .is_none()
    );
    assert!(!game.entities[id].drawable.model_conditions.constructing);
    let completion_position = game.entities[id].position;
    let goal = game.entities[id].ai_goal_position();
    assert_eq!(
        game.entities[id].ai.command.last_command_source,
        crate::skirmish::ai_goals::COMMAND_FROM_AI
    );
    for _ in 0..600 {
        game.step();
        if game.entities[id].ai_idle() {
            break;
        }
    }
    let final_position = game.entities[id].position;
    assert!(game.entities[id].ai_idle());
    assert!(
        (final_position[0] - completion_position[0])
            .hypot(final_position[1] - completion_position[1])
            > 30.0
    );
    assert!((final_position[0] - goal[0]).hypot(final_position[1] - goal[1]) < 10.0);
}

#[test]
fn completed_dozer_build_adjusts_an_end_dock_blocked_after_task_assignment() {
    let (mut game, id) = native_dozer();
    let index = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[index].build_seconds = 2.0 / 30.0;
    let site = game.spawn(index, 0, [180.0, 100.0], false);
    game.assist_build(id, site).unwrap();
    let departure = game.entities[id].ai.dozer.as_ref().unwrap().tasks[BUILD].docks[2].unwrap();
    for _ in 0..600 {
        game.step();
        if game.entities[site].construction_percent > 0.0 {
            break;
        }
    }
    assert!(game.entities[site].under_construction());
    assert!(game.entities[site].construction_percent > 0.0);
    let obstacle = game.spawn(index, 0, [departure[0], departure[1]], true);
    game.rebuild_navigation();
    for _ in 0..600 {
        game.step();
        if game.entities[site].complete() {
            break;
        }
    }
    assert!(
        game.entities[site].complete(),
        "construction stopped at {} percent after the end dock became obstructed",
        game.entities[site].construction_percent
    );
    let adjusted = game.entities[id].ai_goal_position();
    assert_ne!(adjusted.map(f32::to_bits), departure.map(f32::to_bits));
    assert!(game.entities[obstacle].alive());
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
    let cell = game.cell_at([adjusted[0], adjusted[1]]).unwrap();
    assert!(!game.navigation_for(id).is_blocked(cell));
    // AIUpdate::computePath may return findClosestPath's reachable endpoint;
    // AIInternalMoveToState tests the path end, not the requested command point.
    let mut path_end = None;
    for _ in 0..600 {
        if game.entities[id].ai.path_request.pending.is_none()
            && let Some(path) = &game.entities[id].ai.ground_path
        {
            path_end = path.last_position();
        }
        game.step();
        if game.entities[id].ai_idle() {
            break;
        }
    }
    assert!(game.entities[id].ai_idle());
    let destination = path_end.expect("resolved departure path");
    let point = game.entities[id].position;
    let close_enough = game.catalog[game.entities[id].type_index]
        .locomotor
        .ground
        .as_ref()
        .unwrap()
        .close_enough;
    assert!(
        (point[0] - destination[0]).hypot(point[1] - destination[1]) < close_enough,
        "command={adjusted:?} path_end={destination:?} final={point:?}"
    );
}

#[test]
fn construction_health_uses_active_body_previous_health_and_inactive_body_noop() {
    for inactive in [false, true] {
        let (mut game, id) = native_dozer();
        let index = game.type_for(0, Kind::Barracks).unwrap();
        game.catalog[index].build_seconds = 10.0 / 30.0;
        if inactive {
            game.catalog[index].body_kind = crate::skirmish::BodyKind::Inactive;
        }
        let site = game.spawn(index, 0, [180.0, 100.0], false);
        let before = game.entities[site].health;
        assert!(!game.construct_building_piece(id, site));
        if inactive {
            assert_eq!(game.entities[site].health.to_bits(), before.to_bits());
        } else {
            assert_eq!(
                game.entities[site].damage_state.previous_health.to_bits(),
                before.to_bits()
            );
            assert!(game.entities[site].health > before);
        }
        assert_eq!(game.entities[site].producer, Some(id));
    }
}

#[test]
fn busy_dozer_ignores_tank_yield_but_accepts_another_dozer() {
    for other_dozer in [false, true] {
        let (mut game, id) = native_dozer();
        let site_type = game.type_for(0, Kind::Barracks).unwrap();
        let site = game.spawn(site_type, 0, [400.0, 250.0], false);
        game.assist_build(id, site).unwrap();
        game.update_dozer_module(id);
        assert_eq!(
            game.entities[id].ai.dozer.as_ref().unwrap().current,
            Some(BUILD)
        );
        let other_type = if other_dozer {
            game.entities[id].type_index
        } else {
            game.type_for(0, Kind::Tank).unwrap()
        };
        let other = game.spawn(other_type, 0, [80.0, 100.0], true);
        game.move_to(other, [300.0, 100.0]).unwrap();
        game.process_path_service();
        game.entities[id].drawable.model_conditions.constructing = true;
        game.refresh_occupancy();
        game.move_away_from(id, other, 0);
        assert!(!game.entities[id].drawable.model_conditions.constructing);
        assert_eq!(
            game.entities[id].ai.blocking.temporary_goal().is_some(),
            other_dozer
        );
        assert_eq!(game.entities[id].build_task_target(), Some(site));
    }
}

#[test]
fn replacing_task_clears_old_common_goal_before_new_dock_movement() {
    let (mut game, id) = native_dozer();
    let first = damaged_building(&mut game);
    let index = game.entities[first].type_index;
    let second = game.spawn(index, 0, [240.0, 100.0], true);
    game.new_dozer_task(id, REPAIR, first);
    // A common AI target is independent of the Dozer's retained task target.
    game.set_ai_goal_object(id, first);
    game.new_dozer_task(id, REPAIR, second);
    assert_eq!(game.entities[id].repair_task_target(), Some(second));
    assert!(game.entities[id].ai.command.goal_object.is_none());
    assert!(game.entities[id].last_command_from_ai());
}

#[test]
fn hunt_cancels_the_real_dozer_task_only_for_a_player_command() {
    for source in [
        crate::skirmish::ai_goals::COMMAND_FROM_AI,
        1,
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
    ] {
        let (mut game, id) = native_dozer();
        let target = damaged_building(&mut game);
        game.repair(id, target).unwrap();
        game.step();
        game.step();
        assert_eq!(
            game.entities[id].ai.dozer.as_ref().unwrap().current,
            Some(REPAIR)
        );
        game.native_hunt_from(id, source);
        let current = game.entities[id].ai.dozer.as_ref().unwrap().current;
        if source == crate::skirmish::ai_goals::COMMAND_FROM_PLAYER {
            assert_eq!(current, None);
        } else {
            assert_eq!(current, Some(REPAIR));
        }
        assert!(!game.entities[id].drawable.model_conditions.constructing);
    }
}

#[test]
fn frame_zero_build_task_is_retained_until_a_positive_frame_command_reissues_it() {
    // AIIdleState centers ground actors during frame 0/1 independently of tasks.
    // Start on that native grid center so unchanged position isolates Build.
    let (mut game, id) = native_dozer_at(0, [105.0, 105.0]);
    let index = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[index].build_seconds = 2.0 / 30.0;
    let site = game.spawn(index, 0, [180.0, 100.0], false);
    game.assist_build(id, site).unwrap();
    assert_eq!(
        game.entities[id].ai.dozer.as_ref().unwrap().tasks[BUILD].frame,
        0
    );
    let position = game.entities[id].position;
    for _ in 0..3 {
        game.step();
        let state = game.entities[id].ai.dozer.as_ref().unwrap();
        assert_eq!(state.tasks[BUILD].target, Some(site));
        assert!(state.current.is_none());
        assert!(matches!(state.phase, Phase::Idle));
        assert_eq!(
            game.entities[site].construction_percent.to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            game.entities[id].position.map(f32::to_bits),
            position.map(f32::to_bits)
        );
    }
    // Reissue the real command at the actual current frame. The source task
    // selector can now choose it, and ordinary scheduled callbacks complete it.
    game.assist_build(id, site).unwrap();
    assert_eq!(
        game.entities[id].ai.dozer.as_ref().unwrap().tasks[BUILD].frame,
        3
    );
    game.step();
    assert!(matches!(
        game.entities[id].ai.dozer.as_ref().unwrap().phase,
        Phase::Pick
    ));
    for _ in 0..600 {
        game.step();
        if game.entities[site].complete() {
            break;
        }
    }
    assert!(game.entities[site].complete());
    assert!(
        game.entities[id].ai.dozer.as_ref().unwrap().tasks[BUILD]
            .target
            .is_none()
    );
}
