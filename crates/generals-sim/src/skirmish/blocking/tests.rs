use super::*;
use crate::skirmish::{GroundAppearance, GroundRules};

fn game() -> Skirmish {
    let mut game = crate::skirmish::tests::game();
    for id in 0..game.entities.len() {
        game.destroy_maintenance_object(id);
    }
    game.process_update_destruction();
    for kind in [Kind::Infantry, Kind::Tank] {
        let index = game.type_for(0, kind).unwrap();
        game.catalog[index].locomotor.ground = Some(GroundRules {
            wander: None,
            appearance: if kind == Kind::Infantry {
                GroundAppearance::Legs
            } else {
                GroundAppearance::Treads
            },
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
    }
    game
}

#[test]
fn blocked_cap_recovers_and_contacts_are_consumed_once_per_locomotor() {
    let mut state = Blocking::new();
    state.blocked = true;
    state.max_speed = 0.5;
    assert!(state.begin_locomotor());
    let (speed, blocked) = state.limit_speed(1.0);
    assert!(blocked);
    assert_eq!(speed.to_bits(), (0.5_f32 * 0.95).to_bits());
    state.finish_locomotor(blocked);
    assert!(!state.begin_locomotor());
    let (recovered, blocked) = state.limit_speed(1.0);
    assert!(!blocked);
    assert_eq!(recovered.to_bits(), (speed * 1.05).to_bits());
    state.finish_locomotor(blocked);
    assert_eq!(state.frames, 0);
    assert_eq!(state.max_speed.to_bits(), FAST_AS_POSSIBLE.to_bits());
}

#[test]
fn moving_blocker_only_spaces_members_of_the_same_nonempty_formation() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let rear = game.spawn(tank, 0, [105.0, 105.0], true);
    let front = game.spawn(tank, 0, [115.0, 105.0], true);
    game.entities[rear].facing = 0.0;
    game.entities[front].facing = 0.0;
    game.entities[rear].ai.blocking.max_speed = 10.0;
    game.entities[front].ai.motion.velocity = [2.0, 0.0];
    let speed =
        |game: &Skirmish| policy::max_blocked_speed(&game.entities[rear], &game.entities[front]);
    assert_eq!(speed(&game).to_bits(), 2.0_f32.to_bits());
    game.entities[rear].ai.command.formation_id = Some(1);
    game.entities[front].ai.command.formation_id = Some(2);
    assert_eq!(speed(&game).to_bits(), 2.0_f32.to_bits());
    game.entities[front].ai.command.formation_id = Some(1);
    assert_eq!(speed(&game).to_bits(), (2.0_f32 * 0.55).to_bits());
}

#[test]
fn yielding_keeps_parent_order_and_new_command_clears_temporary_state() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let mover = game.spawn(tank, 0, [105.0, 105.0], true);
    let idle = game.spawn(infantry, 0, [155.0, 105.0], true);
    game.move_to(mover, [355.0, 105.0]).unwrap();
    game.process_path_service();
    game.refresh_occupancy();
    game.move_away_from(idle, mover, 0);
    assert!(game.entities[idle].ai.blocking.temporary_goal().is_some());
    assert!(matches!(game.entities[idle].ai.order, Order::Idle));
    let yield_goal = game.entities[idle].ai.blocking.temporary_goal().unwrap();
    assert!((yield_goal[1] - 105.0).abs() > 7.5);
    game.stop(idle).unwrap();
    assert!(game.entities[idle].ai.blocking.temporary_goal().is_none());
    assert!(game.entities[idle].ai.path.raw.is_empty());
    assert!(!game.entities[idle].ai.blocking.path_through_units);
}

#[test]
fn idle_builder_waits_for_temporary_yield_before_starting_automatic_repair() {
    let mut game = game();
    let builder = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[builder].kind = Kind::Builder;
    game.catalog[builder].maintenance.repair_per_second = 0.02;
    game.catalog[builder].maintenance.idle_repair = Some(crate::skirmish::IdleRepair {
        frames: 150.0,
        range: 150.0,
    });
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let mover = game.spawn(tank, 0, [105.0, 105.0], true);
    let idle = game.spawn(builder, 0, [155.0, 105.0], true);
    let building = game.spawn(0, 0, [155.0, 235.0], true);
    game.entities[building].health = 500.0;
    game.update_ground_contacts();
    game.move_to(mover, [355.0, 105.0]).unwrap();
    game.process_path_service();
    game.refresh_occupancy();
    game.move_away_from(idle, mover, 0);
    let yielding = game.entities[idle].ai.blocking.temporary_goal().unwrap();
    game.tick = 151;
    game.update_idle_repair(idle);
    assert!(matches!(game.entities[idle].ai.order, Order::Idle));
    assert_eq!(
        game.entities[idle].ai.blocking.temporary_goal(),
        Some(yielding)
    );
    game.stop(idle).unwrap();
    game.tick = 301;
    game.update_idle_repair(idle);
    assert!(matches!(game.entities[idle].ai.order, Order::Idle));
    game.tick += 1;
    game.update_idle_repair(idle);
    assert!(matches!(game.entities[idle].ai.order, Order::Repair(id) if id == building));
}

#[test]
fn repeat_blocker_ignores_contacts_for_exactly_sixty_frames() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let first = game.spawn(tank, 0, [105.0, 105.0], true);
    let second = game.spawn(tank, 0, [155.0, 105.0], true);
    game.entities[first].ai.blocking.temporary = Some(TemporaryState::Movement(MoveAside {
        kind: TemporaryKind::OutOfWay,
        parent_goal: None,
        goal: [105.0, 155.0],
        until: 300,
    }));
    game.entities[first].ai.blocking.blockers[0] = Some(second);
    game.entities[first].ai.blocking.blocked = true;
    game.tick = 17;
    game.move_away_from(first, second, 0);
    assert_eq!(game.entities[first].ai.blocking.ignore_until, 77);
    assert!(game.entities[first].ai.blocking.temporary_goal().is_some());
}

#[test]
fn temporary_deadline_is_strict_and_original_command_remains_available() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [105.0, 105.0], true);
    game.move_to(id, [355.0, 105.0]).unwrap();
    game.process_path_service();
    game.entities[id].ai.blocking.temporary = Some(TemporaryState::Movement(MoveAside {
        kind: TemporaryKind::OutOfWay,
        parent_goal: Some([355.0, 105.0]),
        goal: [355.0, 105.0],
        until: 20,
    }));
    game.tick = 20;
    assert!(game.update_move_aside(id));
    game.tick = 21;
    assert!(!game.update_move_aside(id));
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
    assert!(game.entities[id].ai.blocking.temporary_goal().is_none());
}

#[test]
fn native_idle_query_observes_the_parent_even_during_temporary_motion() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [105.0, 105.0], true);
    let actor = &mut game.entities[id];
    actor.ai.motion.mark_angle(0);
    actor.ai.blocking.temporary = Some(TemporaryState::Movement(MoveAside {
        kind: TemporaryKind::OutOfWay,
        parent_goal: None,
        goal: [155.0, 105.0],
        until: 300,
    }));
    assert!(!moving(actor));
    actor.ai.order = Order::Move([355.0, 105.0]);
    assert!(moving(actor));
}

#[test]
fn resumed_parent_starting_move_cancels_old_temporary_completion() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [105.0, 105.0], true);
    game.move_to(id, [355.0, 105.0]).unwrap();
    game.process_path_service();
    game.update_order(id);
    game.entities[id].ai.ignored_producer = Some(999);
    game.entities[id].ai.blocking.temporary = Some(TemporaryState::Movement(MoveAside {
        kind: TemporaryKind::OutOfWay,
        parent_goal: Some([355.0, 105.0]),
        goal: [105.0, 155.0],
        until: 300,
    }));
    game.clear_temporary(id);
    game.update_order(id);
    game.finish_completed_movement(id);
    let actor = &mut game.entities[id];
    assert!(matches!(actor.ai.order, Order::Move(_)));
    assert!(actor.ai.path.raw.is_empty());
    assert!(actor.ai.motion.take_request().is_none());
    assert!(!actor.ai.motion.has_goal());
    assert_eq!(actor.ai.ignored_producer, Some(999));
    assert!(actor.ai.path_request.pending.is_some());
    game.tick += 1;
    game.process_path_service();
    game.update_order(id);
    assert!(game.entities[id].ai.motion.follows_path());
    assert!(game.entities[id].ai.ground_path.is_some());
    let before_motion = game.entities[id].position;
    game.update_ground_locomotor(id);
    assert_eq!(game.entities[id].ai.motion.command_frame, Some(game.tick));
    game.update_physics(game.entities.len());
    assert!(
        game.entities[id]
            .ai
            .motion
            .velocity
            .iter()
            .any(|speed| *speed != 0.0)
    );
    assert!(game.entities[id].position[0] > before_motion[0]);
    assert_eq!(
        game.entities[id].position[1].to_bits(),
        before_motion[1].to_bits()
    );
    // A later genuine parent onExit still performs every completion cleanup.
    game.finish_ground_path(id);
    game.finish_completed_movement(id);
    let actor = &mut game.entities[id];
    assert!(actor.ai.path.raw.is_empty());
    assert!(actor.ai.path_request.pending.is_none());
    assert!(actor.ai.ignored_producer.is_none());
    assert!(!actor.ai.motion.has_goal());
}

#[test]
fn a_new_move_state_cancels_pending_completion_before_locomotor_dispatch() {
    let (mut game, id) = crate::skirmish::ground_movement::tests::moving_tank();
    game.set_object_position(id, [105.0, 105.0, 0.0]);
    game.move_to(id, [255.0, 105.0]).unwrap();
    game.entities[id].ai.motion.end_move();
    game.move_to(id, [355.0, 105.0]).unwrap();
    game.process_path_service();
    game.update_order(id);
    game.finish_completed_movement(id);
    assert!(!game.entities[id].ai.path.raw.is_empty());
    // INITIAL follows the retained path; doLocomotor samples its force later.
    assert!(game.entities[id].ai.motion.follows_path());
    game.sample_ground_path_force(id);
    assert!(game.entities[id].ai.motion.take_request().is_some());
}

#[test]
fn construction_temporary_move_aliases_current_move_state_and_preserves_xyz() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [105.0, 105.0], true);
    game.materialize_selection_group(0, &[id]);
    game.move_to(id, [405.0, 105.0]).unwrap();
    let destination = [205.25, 205.75, 17.0];
    game.construction_move(id, destination);
    assert!(game.entities[id].ai.blocking.temporary_move_to());
    assert_eq!(
        game.entities[id].ai_goal_position().map(f32::to_bits),
        destination.map(f32::to_bits)
    );
    assert_eq!(
        game.entities[id]
            .ai
            .command
            .move_to
            .position
            .map(f32::to_bits),
        destination.map(f32::to_bits)
    );
    assert!(matches!(game.entities[id].ai.order, Order::Move([x, y])
        if x.to_bits() == destination[0].to_bits() && y.to_bits() == destination[1].to_bits()));
    game.clear_temporary(id);
    assert!(game.entities[id].ai.blocking.temporary_goal().is_none());
    assert_eq!(
        game.entities[id]
            .ai
            .command
            .move_to
            .position
            .map(f32::to_bits),
        destination.map(f32::to_bits)
    );
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
}

#[test]
fn construction_move_retains_attack_target_and_machine_goal_object_wins() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let attacker = game.spawn(tank, 0, [105.0, 105.0], true);
    let target = game.spawn(tank, 1, [405.0, 305.0], true);
    game.attack(attacker, target).unwrap();
    game.set_ai_goal_object(attacker, target);
    let displacement = [205.0, 205.0, 37.0];
    game.construction_move(attacker, displacement);
    assert!(matches!(game.entities[attacker].ai.order, Order::Attack(id) if id == target));
    assert_eq!(game.entities[attacker].ai.command.goal_object, Some(target));
    assert_eq!(
        game.entities[attacker].ai_goal_position().map(f32::to_bits),
        displacement.map(f32::to_bits)
    );
    let requested = game.entities[attacker]
        .ai
        .blocking
        .temporary_world_goal()
        .unwrap();
    assert!(requested[0] > 350.0 && requested[1] > 250.0);
    game.clear_temporary(attacker);
    assert!(matches!(game.entities[attacker].ai.order, Order::Attack(id) if id == target));
}

#[test]
fn construction_temporary_deadline_is_strict_and_expiry_resumes_parent() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [105.0, 105.0], true);
    game.set_order(id, Order::Guard([105.0, 105.0]));
    game.construction_move(id, [405.0, 305.0, 0.0]);
    game.entities[id].ai.motion.mark_angle(game.tick);
    let until = game.tick + 600;
    // Keep the real request pending to isolate the source timeout from arrival.
    game.tick = until;
    assert!(game.update_move_aside(id));
    game.tick += 1;
    assert!(!game.update_move_aside(id));
    assert!(game.entities[id].ai.blocking.temporary_goal().is_none());
    assert!(game.entities[id].ai.motion.has_goal());
    assert!(matches!(game.entities[id].ai.order, Order::Guard(_)));
}

#[test]
fn failed_construction_path_releases_temporary_state_without_erasing_parent() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [105.0, 105.0], true);
    game.set_order(id, Order::Guard([105.0, 105.0]));
    game.construction_move(id, [405.0, 305.0, 0.0]);
    game.entities[id].ai.motion.mark_angle(game.tick);
    game.entities[id].ai.path_request.cancel();
    game.entities[id].ai.path_request.failed = true;
    assert!(!game.update_move_aside(id));
    assert!(matches!(game.entities[id].ai.order, Order::Guard(_)));
    assert!(game.entities[id].ai.blocking.temporary_goal().is_none());
    assert!(game.entities[id].ai.motion.has_goal());
}

#[test]
fn dead_ai_exits_construction_temporary_state_in_its_dead_state_transition() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [105.0, 105.0], true);
    game.set_order(id, Order::Guard([105.0, 105.0]));
    game.construction_move(id, [405.0, 305.0, 0.0]);
    assert!(game.entities[id].ai.blocking.temporary_move_to());
    game.entities[id].health = 0.0;
    game.mark_ai_dead(id);
    game.update_ai_module(id);
    assert!(game.entities[id].ai.blocking.temporary_goal().is_none());
    assert!(matches!(
        game.entities[id].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::Dead
    ));
}

#[test]
fn temporary_move_to_ignored_goal_object_keeps_original_xyz_and_removes_goal_claim() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [105.0, 105.0], true);
    let target = game.spawn(tank, 1, [403.25, 303.75], true);
    game.attack(id, target).unwrap();
    game.set_ai_goal_object(id, target);
    game.entities[id].ai.ignored_producer = Some(target);
    let exact = game.object_position(target);
    game.construction_move(id, [205.0, 205.0, 37.0]);
    assert_eq!(
        game.entities[id].ai.blocking.temporary_world_goal(),
        Some(exact)
    );
    assert!(!game.move_destination_adjusts(id));
    game.process_path_service();
    assert!(game.occupancy.goal_cell(id).is_none());
    assert_eq!(game.entities[id].ai.path.exact_endpoint, Some(exact));
}

#[test]
fn snap_closest_uses_native_x_outer_neighbor_order_and_fresh_ground_height() {
    let mut game = game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let id = game.spawn(infantry, 0, [105.0, 105.0], true);
    game.navigation = game
        .navigation
        .with_obstacles([crate::Cell { x: 30, y: 20 }]);
    let goal = game.snap_closest_goal(id, [305.25, 205.75, 137.0]);
    assert_eq!(
        goal.map(f32::to_bits),
        [295.0_f32, 195.0, 0.0].map(f32::to_bits)
    );
    let valid = game.snap_closest_goal(id, [405.25, 205.75, 137.0]);
    assert_eq!(
        valid.map(f32::to_bits),
        [405.0_f32, 205.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn successful_temporary_move_clears_only_an_adjusted_locomotor_goal() {
    for adjusts in [true, false] {
        let mut game = game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let id = game.spawn(tank, 0, [105.0, 105.0], true);
        game.set_order(id, Order::Guard([105.0, 105.0]));
        if !adjusts {
            let target = game.spawn(tank, 1, [405.0, 305.0], true);
            game.set_ai_goal_object(id, target);
            game.entities[id].ai.ignored_producer = Some(target);
        }
        game.construction_move(id, [405.0, 305.0, 0.0]);
        game.process_path_service();
        let endpoint = game.entities[id]
            .ai
            .ground_path
            .as_ref()
            .unwrap()
            .positions()
            .last()
            .unwrap();
        game.entities[id].position = endpoint;
        game.entities[id].ai.motion.mark_angle(game.tick);
        assert!(game.entities[id].ai.motion.has_goal());
        assert!(!game.update_move_aside(id));
        assert!(game.entities[id].ai.blocking.temporary_goal().is_none());
        assert_eq!(game.entities[id].ai.motion.has_goal(), !adjusts);
    }
}

#[test]
fn stationary_ai_dead_infantry_only_bounces_for_squish_capable_ground_contacts() {
    let mut game = game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[infantry].kind_of.push("INFANTRY".into());
    game.catalog[infantry].crush.squish_module = Some(0);
    game.catalog[tank].crush.crusher_level = 1;
    let victim = game.spawn(infantry, 0, [105.0, 105.0], true);
    let crusher = game.spawn(tank, 1, [155.0, 105.0], true);
    assert!(!game.process_ground_contact(victim, crusher));
    game.mark_ai_dead(victim);
    assert!(game.process_ground_contact(victim, crusher));
    game.catalog[infantry].crush.squish_module = None;
    assert!(!game.process_ground_contact(victim, crusher));
    game.catalog[infantry].crush.squish_module = Some(0);
    game.entities[crusher].disabled_status |= 1 << 3;
    assert!(!game.process_ground_contact(victim, crusher));
}

#[test]
fn primary_panic_bounces_blocked_infantry_without_marking_it_blocked() {
    let mut game = game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let actor = game.spawn(infantry, 0, [105.0, 105.0], true);
    let blocker = game.spawn(tank, 0, [110.0, 105.0], true);
    let graph = vec![crate::skirmish::AiWaypoint {
        id: 1,
        position: [405.0, 105.0],
        links: Vec::new(),
        labels: Vec::new(),
        bidirectional: false,
    }];
    game.tick = 10;
    game.start_waypoint_wander(actor, &graph, 0, true);
    assert!(game.process_ground_contact(actor, blocker));
    assert!(!game.entities[actor].ai.blocking.blocked);
    game.start_waypoint_wander(actor, &graph, 0, false);
    assert!(!game.process_ground_contact(actor, blocker));
    assert!(game.entities[actor].ai.blocking.blocked);
}
