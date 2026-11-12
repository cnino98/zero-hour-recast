use super::*;
use crate::grid::{Cell, Grid};
use crate::skirmish::{GroundAppearance, GroundRules, Kind as UnitKind};

fn game() -> Skirmish {
    let mut game = crate::skirmish::tests::game();
    game.clear_match_state();
    game.initialize_map_side_players().unwrap();
    let tank = game.type_for(0, UnitKind::Tank).unwrap();
    game.catalog[tank].locomotor.ground = Some(GroundRules {
        wander: None,
        appearance: GroundAppearance::Treads,
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
    game
}

fn tank(game: &mut Skirmish, position: [f32; 2]) -> usize {
    let index = game.type_for(0, UnitKind::Tank).unwrap();
    game.spawn(index, 0, position, true)
}

fn request(goal: [f32; 2], kind: Kind) -> Request {
    Request {
        goal: [goal[0], goal[1], 0.0],
        range: 0.1,
        kind,
    }
}

#[test]
fn native_ring_keeps_one_empty_slot_and_deduplicates_across_wrap() {
    let mut queue = Service::new();
    for id in 0..511 {
        assert!(queue.queue(id));
    }
    assert!(!queue.queue(511));
    assert!(queue.queue(37));
    for id in 0..500 {
        assert_eq!(queue.entries[queue.head].take(), Some(id));
        queue.head = (queue.head + 1) % 512;
    }
    for id in 511..1011 {
        assert!(queue.queue(id));
    }
    assert!(queue.queue(600));
    assert!(!queue.queue(1011));
    for id in 500..1011 {
        assert_eq!(queue.entries[queue.head].take(), Some(id));
        queue.head = (queue.head + 1) % 512;
    }
    assert_eq!(queue.head, queue.tail);
}

#[test]
fn ground_command_waits_for_service_and_replacement_keeps_original_fifo_slot() {
    let mut game = game();
    let first = tank(&mut game, [105.0, 105.0]);
    let second = tank(&mut game, [105.0, 155.0]);
    game.begin_move_world(first, [305.0, 105.0, game.ground_height([305.0, 105.0])])
        .unwrap();
    game.begin_move_world(second, [305.0, 155.0, game.ground_height([305.0, 155.0])])
        .unwrap();
    game.begin_move_world(first, [405.0, 105.0, game.ground_height([405.0, 105.0])])
        .unwrap();
    assert!(game.entities[first].ai.path.raw.is_empty());
    assert!(game.entities[first].ai.path_request.pending.is_some());
    assert_eq!(game.path_service.tail, 2);
    assert_eq!(game.path_service.entries[0], Some(first));
    game.tick = 1;
    game.process_path_service();
    assert!(game.entities[first].ai.path_request.pending.is_none());
    assert!(game.entities[first].ai.ground_path.is_some());
    assert_eq!(game.entities[first].ai.path_endpoint, Some([405.0, 105.0]));
    assert!(game.entities[second].ai.ground_path.is_some());
}

#[test]
fn canceled_and_deleted_entries_are_consumed_without_resurrecting_commands() {
    let mut game = game();
    let canceled = tank(&mut game, [105.0, 105.0]);
    let dead = tank(&mut game, [105.0, 155.0]);
    game.begin_move_world(canceled, [305.0, 105.0, game.ground_height([305.0, 105.0])])
        .unwrap();
    game.begin_move_world(dead, [305.0, 155.0, game.ground_height([305.0, 155.0])])
        .unwrap();
    game.stop(canceled).unwrap();
    game.finish_completed_movement(canceled);
    game.entities[dead].health = 0.0;
    game.finish_death(dead, None);
    game.destroy_maintenance_object(dead);
    assert!(game.entities[dead].exists());
    game.process_update_destruction();
    assert!(!game.entities[dead].exists());
    assert_eq!(game.path_service.tail, 2);
    game.process_path_service();
    assert_eq!(game.path_service.head, game.path_service.tail);
    assert_eq!(game.path_service.cumulative_cells, 0);
    for id in [canceled, dead] {
        assert!(game.entities[id].ai.path_request.pending.is_none());
        assert!(game.entities[id].ai.path.raw.is_empty());
        if id == canceled {
            assert!(matches!(game.entities[id].ai.order, Order::Idle));
        } else {
            assert!(!game.entities[id].exists());
            assert!(
                game.update_objects[game.entities[id].update_owner]
                    .handles
                    .is_empty()
            );
        }
    }
}

#[test]
fn recent_move_and_attack_requests_observe_their_distinct_queue_deadlines() {
    for (kind, delay) in [(Kind::Move, 30), (Kind::Tighten, 60), (Kind::Attack(0), 60)] {
        let mut game = game();
        let actor = tank(&mut game, [105.0, 105.0]);
        game.tick = 10;
        game.entities[actor].ai.blocking.path_frame = 9;
        game.request_ground_path(actor, request([305.0, 105.0], kind));
        assert_eq!(game.path_service.head, game.path_service.tail);
        assert_eq!(
            game.entities[actor].ai.path_request.queue_at,
            Some(10 + delay)
        );
        game.tick = 9 + u64::from(delay);
        game.wake_path_request(actor);
        assert_eq!(game.path_service.head, game.path_service.tail);
        game.tick += 1;
        game.wake_path_request(actor);
        assert_eq!(game.path_service.tail, 1);
        assert!(game.entities[actor].ai.path_request.queue_at.is_none());
        game.wake_path_request(actor);
        assert_eq!(game.path_service.tail, 1);
    }
}

#[test]
fn exactly_three_frames_old_path_is_eligible_and_cancel_clears_delayed_wake() {
    let mut game = game();
    let actor = tank(&mut game, [105.0, 105.0]);
    game.tick = 10;
    game.entities[actor].ai.blocking.path_frame = 7;
    game.request_ground_path(actor, request([305.0, 105.0], Kind::Move));
    assert_eq!(game.path_service.tail, 1);
    game.entities[actor].ai.blocking.path_frame = 10;
    game.request_ground_path(actor, request([405.0, 105.0], Kind::Move));
    game.stop(actor).unwrap();
    game.tick = 40;
    game.wake_path_request(actor);
    game.process_path_service();
    assert_eq!(game.path_service.head, game.path_service.tail);
    assert!(game.entities[actor].ai.ground_path.is_none());
}

#[test]
fn complete_search_can_exceed_frame_budget_and_leaves_next_object_waiting() {
    let mut game = game();
    game.grid = Grid::new(200, 200, (0..200).map(|y| Cell { x: 100, y })).unwrap();
    game.navigation = game.grid.clone();
    let first = tank(&mut game, [105.0, 1005.0]);
    let second = tank(&mut game, [105.0, 105.0]);
    game.request_ground_path(first, request([1905.0, 1005.0], Kind::Tighten));
    game.request_ground_path(second, request([305.0, 105.0], Kind::Move));
    game.process_path_service();
    assert!(game.path_service.cumulative_cells > 5000);
    assert!(game.entities[first].ai.ground_path.is_some());
    assert!(game.entities[first].ai.path_request.pending.is_none());
    assert!(game.entities[second].ai.path_request.pending.is_some());
    assert!(game.entities[second].ai.ground_path.is_none());
    assert_eq!(game.path_service.head, 1);
    game.process_path_service();
    assert!(game.path_service.cumulative_cells < 5000);
    assert!(game.entities[second].ai.ground_path.is_some());
    assert_eq!(game.path_service.head, game.path_service.tail);
}

#[test]
fn failed_patch_queries_spend_budget_and_retry_after_source_delay() {
    let mut game = game();
    game.grid = Grid::new(160, 160, []).unwrap();
    game.navigation = game.grid.clone();
    for _ in 0..4 {
        let id = tank(&mut game, [55.0, 55.0]);
        game.entities[id].ai.path.raw = [Cell { x: 5, y: 5 }, Cell { x: 150, y: 150 }]
            .map(|cell| crate::grid::LayerCell { cell, layer: 1 })
            .to_vec();
        game.entities[id].ai.path_goal = Some(Cell { x: 150, y: 150 });
        game.entities[id].ai.path_endpoint = Some([1505.0, 1505.0]);
        game.entities[id].ai.blocking.stuck = true;
        game.entities[id].ai.order = Order::Move([1505.0, 1505.0]);
        game.entities[id].ai.command.move_to.position = [1505.0, 1505.0, 0.0];
        game.request_ground_path(id, request([1505.0, 1505.0], Kind::Recovery));
    }
    game.process_path_service();
    assert!(game.path_service.cumulative_cells >= 5000);
    assert!(game.path_service.head < game.path_service.tail);
    let serviced = game.path_service.head;
    for id in 0..serviced {
        assert!(game.entities[id].ai.ground_path.is_none());
        assert_eq!(game.entities[id].ai.blocking.retry_at, Some(30));
        assert!(!game.entities[id].ai.path_request.failed);
    }
    assert!(game.entities[serviced].ai.path_request.pending.is_some());
    let tail = game.path_service.tail;
    game.tick = 29;
    game.wake_path_request(0);
    game.update_order(0);
    assert!(matches!(game.entities[0].ai.order, Order::Move(_)));
    assert!(game.entities[0].ai.path_request.pending.is_some());
    assert_eq!(game.path_service.tail, tail);
    game.tick = 30;
    game.wake_path_request(0);
    game.update_order(0);
    assert!(matches!(game.entities[0].ai.order, Order::Move(_)));
    assert!(game.entities[0].ai.path_request.pending.is_some());
    assert_eq!(game.path_service.tail, tail + 1);
    // Let the other original requests drain before observing this retry.
    for _ in 0..4 {
        game.process_path_service();
        if game.entities[0].ai.path_request.pending.is_none() {
            break;
        }
    }
    assert!(game.entities[0].ai.path_request.pending.is_none());
    assert!(game.entities[0].ai.ground_path.is_some());
    game.update_order(0);
    assert!(matches!(game.entities[0].ai.order, Order::Move(_)));
    assert!(!game.entities[0].ai.command.move_to.waiting);
}

#[test]
fn zone_recalculation_skips_one_service_frame_and_reset_discards_old_ids() {
    let mut game = game();
    let actor = tank(&mut game, [105.0, 105.0]);
    game.begin_move_world(actor, [305.0, 105.0, game.ground_height([305.0, 105.0])])
        .unwrap();
    game.path_service.zone_recalculate_at = Some(game.tick);
    game.process_path_service();
    assert!(game.entities[actor].ai.path_request.pending.is_some());
    assert_eq!(game.path_service.head, 0);
    game.process_path_service();
    assert!(game.entities[actor].ai.ground_path.is_some());
    game.begin_move_world(actor, [405.0, 105.0, game.ground_height([405.0, 105.0])])
        .unwrap();
    game.reset_sandbox(crate::skirmish::Faction::Gla).unwrap();
    assert_eq!(game.path_service.head, game.path_service.tail);
    assert!(
        game.entities
            .iter()
            .all(|actor| actor.ai.path_request.pending.is_none())
    );
}

#[test]
fn tighten_callback_does_not_change_compute_path_timestamp() {
    let mut game = game();
    let actor = tank(&mut game, [105.0, 105.0]);
    game.tick = 10;
    game.entities[actor].ai.blocking.path_frame = 3;
    game.request_ground_path(actor, request([305.0, 105.0], Kind::Tighten));
    game.process_path_service();
    assert!(game.entities[actor].ai.ground_path.is_some());
    assert_eq!(game.entities[actor].ai.blocking.path_frame, 3);
}

#[test]
fn waiting_recovery_keeps_native_locomotor_path_but_recent_attack_stops_it() {
    let mut game = game();
    let actor = tank(&mut game, [105.0, 105.0]);
    game.begin_move_world(actor, [405.0, 105.0, game.ground_height([405.0, 105.0])])
        .unwrap();
    game.tick = 10;
    game.process_path_service();
    assert!(!game.travel_ground(actor, [405.0, 105.0], 0.1, None));
    game.update_ground_locomotor(actor);
    game.request_ground_path(actor, request([405.0, 105.0], Kind::Recovery));
    assert!(game.entities[actor].ai.path_request.queue_at.is_some());
    assert!(!game.travel_ground(actor, [405.0, 105.0], 0.1, None));
    // The state retains POSITION_ON_PATH; force sampling belongs to the later
    // doLocomotor phase and is no longer an eager state-update side effect.
    assert!(game.entities[actor].ai.motion.follows_path());
    game.request_ground_path(actor, request([405.0, 105.0], Kind::Attack(actor)));
    assert!(!game.travel_ground(actor, [405.0, 105.0], 0.1, None));
    assert!(!game.entities[actor].ai.motion.follows_path());
    assert!(game.entities[actor].ai.motion.take_request().is_none());
}

#[test]
fn full_native_queue_services_real_ground_units_fairly_with_canceled_and_deleted_slots() {
    let mut game = game();
    game.grid = Grid::new(160, 160, []).unwrap();
    game.navigation = game.grid.clone();
    let mut canceled = Vec::new();
    for index in 0..511_u16 {
        let position = [
            55.0 + f32::from(index % 32) * 30.0,
            55.0 + f32::from(index / 32) * 30.0,
        ];
        let id = tank(&mut game, position);
        let point = [position[0] + 350.0, position[1] + 600.0];
        game.begin_move_world(id, [point[0], point[1], game.ground_height(point)])
            .unwrap();
        assert_eq!(game.path_service.entries[usize::from(index)], Some(id));
    }
    assert_eq!(game.path_service.tail, 511);
    for id in 0..511 {
        if id % 17 == 0 {
            game.stop(id).unwrap();
            game.finish_completed_movement(id);
            canceled.push(id);
        } else if id % 19 == 0 {
            game.entities[id].health = 0.0;
            game.finish_death(id, None);
            game.destroy_maintenance_object(id);
            assert!(game.entities[id].exists());
            game.process_update_destruction();
            assert!(!game.entities[id].exists());
            canceled.push(id);
        }
    }
    game.refresh_occupancy();
    let mut frames = 0;
    let mut previous = 0;
    while game.path_service.head != game.path_service.tail {
        frames += 1;
        assert!(frames <= 511, "nonempty native queue must make progress");
        game.tick += 1;
        game.process_path_service();
        let head = game.path_service.head;
        assert!(
            head > previous,
            "FIFO head must progress without skipping live requests"
        );
        for id in previous..head {
            assert!(game.entities[id].ai.path_request.pending.is_none());
        }
        for id in head..511 {
            assert_eq!(
                game.entities[id].ai.path_request.pending.is_none(),
                canceled.contains(&id)
            );
        }
        if head != game.path_service.tail {
            assert!(game.path_service.cumulative_cells >= 5000);
        }
        previous = head;
    }
    assert!(frames > 1, "real search work must exercise frame budgeting");
    for id in 0..511 {
        if canceled.contains(&id) {
            assert!(game.entities[id].ai.ground_path.is_none());
            if id % 17 == 0 {
                assert!(matches!(game.entities[id].ai.order, Order::Idle));
            } else {
                assert!(!game.entities[id].exists());
                assert!(
                    game.update_objects[game.entities[id].update_owner]
                        .handles
                        .is_empty()
                );
            }
        } else {
            assert!(game.entities[id].ai.ground_path.is_some());
            assert!(!game.entities[id].ai.path_request.failed);
        }
    }
}

#[test]
fn factory_callback_yields_stationary_allies_across_owners_but_respects_ignore_and_busy() {
    let mut game = game();
    game.slots[0].team = Some(1);
    game.slots[1].team = Some(1);
    let mover = tank(&mut game, [105.0, 105.0]);
    let ignored = tank(&mut game, [155.0, 105.0]);
    let busy = tank(&mut game, [205.0, 105.0]);
    let ally = tank(&mut game, [255.0, 105.0]);
    game.entities[ally].owner = 1;
    let other = tank(&mut game, [305.0, 105.0]);
    game.entities[other].owner = 2;
    let moving = tank(&mut game, [355.0, 105.0]);
    game.begin_move_world(moving, [355.0, 255.0, game.ground_height([355.0, 255.0])])
        .unwrap();
    game.ai_busy_from_ai(busy);
    game.entities[mover].ai.order = Order::Exit([455.0, 105.0, 0.0]);
    game.entities[mover].ai.waypoints.exit_quick_pending = true;
    game.entities[mover].ai.ignored_producer = Some(ignored);
    game.request_ground_path(mover, request([455.0, 105.0], Kind::Move));
    game.refresh_occupancy();
    let random = game.random;
    let moving_cells = game
        .compute_queued_path(moving, &request([355.0, 255.0], Kind::Move))
        .cells;
    game.process_path_service();
    assert_eq!(game.random, random);
    assert!(game.entities[ally].ai.blocking.temporary_goal().is_some());
    assert!(game.entities[ally].ai.ground_path.is_some());
    assert!(game.path_service.cumulative_cells > moving_cells);
    for id in [ignored, busy, other, moving] {
        assert!(game.entities[id].ai.blocking.temporary_goal().is_none());
    }
    assert!(!game.entities[mover].ai.waypoints.exit_quick_pending);
}

#[test]
fn closest_route_completion_retains_actual_endpoint_separate_from_requested_goal() {
    let mut game = game();
    game.grid = Grid::new(40, 40, (0..40).map(|y| Cell { x: 20, y })).unwrap();
    game.navigation = game.grid.clone();
    let actor = tank(&mut game, [105.0, 105.0]);
    let goal = [305.0, 105.0];
    game.set_order(actor, Order::Move(goal));
    game.request_ground_path(actor, request(goal, Kind::Move));
    game.process_path_service();
    let result = crate::skirmish::diagnostics::ground_route_result(&game, actor).unwrap();
    let endpoint = result
        .endpoint
        .expect("unreachable requested zone still has a source closest route");
    assert_eq!(result.requested.map(f32::to_bits), goal.map(f32::to_bits));
    assert!(endpoint[0] < 200.0);
    assert!((endpoint[0] - goal[0]).abs() > 40.0);
    assert!(!game.entities[actor].ai.path_request.failed);
    // onExit marks movement complete. The base callback destroys its path
    // after updating the main machine, without erasing the query result.
    game.set_order(actor, Order::Idle);
    assert!(game.entities[actor].ai.ground_path.is_some());
    game.finish_completed_movement(actor);
    let result = crate::skirmish::diagnostics::ground_route_result(&game, actor).unwrap();
    assert_eq!(result.endpoint, Some(endpoint));
    assert!(game.entities[actor].ai.ground_path.is_none());
}

#[test]
fn no_route_failure_cannot_be_reported_as_closest_route_completion() {
    let mut game = game();
    game.grid = Grid::new(200, 200, []).unwrap();
    game.navigation = game.grid.clone();
    let unit = game.type_for(0, UnitKind::Tank).unwrap();
    game.catalog[unit].geometry.major = 25.0;
    game.catalog[unit].geometry.minor = 25.0;
    // 1200 disjoint five-by-five position footprints exhaust native INFOs;
    // the requested cell lies in the unallocated lower quarter of the map.
    for y in 0..30_u16 {
        for x in 0..40_u16 {
            let offset = [f32::from(x) * 50.0, f32::from(y) * 50.0];
            tank(&mut game, [25.0 + offset[0], 25.0 + offset[1]]);
        }
    }
    game.refresh_occupancy();
    let actor = 0;
    let goal = [1905.0, 1805.0];
    assert_eq!(game.route_units(actor).info_available, 0);
    game.request_ground_path(actor, request(goal, Kind::Move));
    game.process_path_service();
    let result = crate::skirmish::diagnostics::ground_route_result(&game, actor).unwrap();
    assert_eq!(result.requested.map(f32::to_bits), goal.map(f32::to_bits));
    assert!(result.endpoint.is_none());
    assert!(game.entities[actor].ai.path_request.failed);
    assert!(game.entities[actor].ai.ground_path.is_none());
}

#[test]
fn waypoint_append_while_first_move_is_waiting_retains_both_original_world_goals() {
    let mut game = game();
    let actor = tank(&mut game, [105.0, 105.0]);
    game.materialize_selection_group(0, &[actor]);
    let first = [305.0, 105.0, 17.0];
    let second = [405.0, 155.0, game.ground_height([405.0, 155.0])];
    game.move_to_world(actor, first).unwrap();
    assert!(game.entities[actor].ai.path_request.pending.is_some());
    game.queue_move_to_world(actor, second).unwrap();
    assert_eq!(game.entities[actor].ai.waypoints.points, [first, second]);
    assert_eq!(game.path_service.cumulative_cells, 0);
    let pending = game.entities[actor]
        .ai
        .path_request
        .pending
        .as_ref()
        .unwrap();
    assert_eq!(pending.goal.map(f32::to_bits), first.map(f32::to_bits));
    game.process_path_service();
    assert!(game.entities[actor].ai.ground_path.is_some());
    assert_eq!(
        game.entities[actor]
            .ai
            .path
            .exact_endpoint
            .unwrap()
            .map(f32::to_bits),
        first.map(f32::to_bits)
    );
    assert_eq!(game.entities[actor].ai.path.raw.last().unwrap().layer, 1);
    assert_eq!(
        game.entities[actor].ai.path_endpoint,
        Some([first[0], first[1]])
    );
}

#[test]
fn attack_callback_that_enters_range_succeeds_without_path_or_timestamp_change() {
    let mut game = game();
    let actor = tank(&mut game, [105.0, 105.0]);
    let target = tank(&mut game, [405.0, 105.0]);
    game.tick = 100;
    game.entities[actor].ai.blocking.path_frame = 17;
    game.entities[actor].ai.order = Order::Attack(target);
    game.request_ground_path(actor, request([405.0, 105.0], Kind::Attack(target)));
    game.entities[target].position = [115.0, 105.0];
    game.process_path_service();
    assert!(game.entities[actor].ai.path.raw.is_empty());
    assert!(game.entities[actor].ai.ground_path.is_none());
    assert!(!game.entities[actor].ai.path_request.failed);
    assert_eq!(game.entities[actor].ai.blocking.path_frame, 17);
    assert_eq!(game.path_service.cumulative_cells, 0);
    assert!(matches!(game.entities[actor].ai.order,Order::Attack(id) if id==target));
}

#[test]
fn contact_callback_preserves_exact_requested_tail_and_locomotor_no_slow_down() {
    let mut game = game();
    let index = game.type_for(0, UnitKind::Tank).unwrap();
    game.catalog[index].weapon.range = 5.0;
    let actor = tank(&mut game, [105.0, 105.0]);
    let target = tank(&mut game, [405.0, 105.0]);
    game.tick = 100;
    game.entities[actor].ai.order = Order::Attack(target);
    game.entities[actor].ai.ignored_producer = Some(target);
    game.request_ground_path(actor, request([405.25, 105.25], Kind::Attack(target)));
    game.process_path_service();
    assert_eq!(
        game.entities[actor].ai.path.exact_endpoint,
        Some([405.25, 105.25, 0.0])
    );
    assert!(game.entities[actor].ai.motion.no_slow_down());
    assert!(!game.entities[actor].ai.path_request.failed);
}

#[test]
fn native_zone_notifications_coalesce_without_delaying_the_first_deadline() {
    let mut service = super::Service::new();
    service.mark_zones_dirty(100);
    assert_eq!(service.zone_recalculate_at, Some(400));
    service.mark_zones_dirty(200);
    assert_eq!(service.zone_recalculate_at, Some(400));
    let mut initial = super::Service::new();
    initial.mark_zones_dirty(0);
    initial.mark_zones_dirty(1);
    assert_eq!(initial.zone_recalculate_at, Some(2));
}

#[test]
fn native_queue_lookup_services_retained_objects_but_skips_actual_deletion() {
    let mut game = game();
    let retained = tank(&mut game, [105.0, 105.0]);
    let deleted = tank(&mut game, [105.0, 155.0]);
    game.begin_move_world(retained, [305.0, 105.0, game.ground_height([305.0, 105.0])])
        .unwrap();
    game.begin_move_world(deleted, [305.0, 155.0, game.ground_height([305.0, 155.0])])
        .unwrap();
    // This is the source callback boundary before AI's next dead-state
    // transition. Body health and GameLogic object lookup are independent.
    game.entities[retained].health = 0.0;
    game.entities[deleted].maintenance.deleted = true;
    game.process_path_service();
    assert_eq!(
        game.entities[retained].ai.path_request.last_goal,
        Some([305.0, 105.0])
    );
    assert!(game.entities[retained].ai.path_request.pending.is_none());
    assert!(game.entities[deleted].ai.path_request.pending.is_some());
    assert!(game.entities[deleted].ai.path_request.last_goal.is_none());
    assert_eq!(game.path_service.head, game.path_service.tail);
}

#[test]
fn dead_ai_move_exit_preserves_waiting_snapshot_until_the_following_update() {
    let mut game = game();
    let actor = tank(&mut game, [105.0, 105.0]);
    game.begin_move_world(actor, [305.0, 105.0, game.ground_height([305.0, 105.0])])
        .unwrap();
    game.entities[actor].health = 0.0;
    game.mark_ai_dead(actor);
    game.update_ai_module(actor);
    assert!(game.entities[actor].ai.path_request.pending.is_some());
    game.process_path_service();
    assert_eq!(
        game.entities[actor].ai.path_request.last_goal,
        Some([305.0, 105.0])
    );
    assert!(!game.entities[actor].ai.path.raw.is_empty());
    game.mark_ai_dead(actor);
    game.update_ai_module(actor);
    assert!(game.entities[actor].ai.path_request.pending.is_none());
    assert!(game.entities[actor].ai.path.raw.is_empty());
    assert!(game.entities[actor].ai.ground_path.is_none());
}

#[test]
fn delayed_path_request_uses_native_frame_after_rollover_and_zero_sentinel() {
    let mut game = game();
    let id = tank(&mut game, [105.0, 105.0]);
    let epoch = u64::from(u32::MAX) + 1;
    game.tick = epoch + 10;
    game.entities[id].ai.blocking.path_frame = game.tick;
    game.request_ground_path(id, request([305.0, 105.0], Kind::Move));
    assert_eq!(game.entities[id].ai.path_request.queue_at, Some(40));
    game.wake_path_request(id);
    assert_eq!(game.path_service.head, game.path_service.tail);
    game.tick = epoch + 39;
    game.wake_path_request(id);
    assert_eq!(game.path_service.head, game.path_service.tail);
    game.tick = epoch + 40;
    game.wake_path_request(id);
    game.process_path_service();
    assert!(game.entities[id].ai.path_request.pending.is_none());
    assert!(game.entities[id].ai.ground_path.is_some());

    game.tick = epoch * 2 - 30;
    game.entities[id].ai.blocking.path_frame = game.tick;
    game.request_ground_path(id, request([405.0, 105.0], Kind::Move));
    assert_eq!(game.entities[id].ai.path_request.queue_at, None);
    game.tick = epoch * 2;
    game.wake_path_request(id);
    assert_eq!(game.path_service.head, game.path_service.tail);
}

#[test]
fn failed_blocked_patch_keeps_position_until_delayed_callback() {
    let mut game = game();
    let id = tank(&mut game, [103.0, 107.0]);
    game.begin_move_world(id, [305.0, 105.0, game.ground_height([305.0, 105.0])])
        .unwrap();
    game.process_path_service();
    game.entities[id].ai.blocking.stuck = true;
    let position = game.entities[id].position;
    game.receive_ground_path(id, request([305.0, 105.0], Kind::Recovery), None);
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert!(game.entities[id].ai.ground_path.is_none());
    assert!(game.entities[id].ai.path_request.pending.is_some());
    game.tick = 29;
    game.update_order(id);
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
}

#[test]
fn failed_nonblocked_repath_discards_old_route_only_when_callback_starts() {
    let mut game = game();
    game.grid = Grid::new(200, 200, []).unwrap();
    game.navigation = game.grid.clone();
    let kind = game.type_for(0, UnitKind::Tank).unwrap();
    game.catalog[kind].geometry.major = 25.0;
    game.catalog[kind].geometry.minor = 25.0;
    let id = tank(&mut game, [25.0, 25.0]);
    game.begin_move_world(id, [305.0, 25.0, game.ground_height([305.0, 25.0])])
        .unwrap();
    game.process_path_service();
    assert!(game.entities[id].ai.ground_path.is_some());
    // Match the retained INFO exhaustion fixture: a full terrain barrier can
    // still produce a native closest path, so it does not force query failure.
    for y in 0..30_u16 {
        for x in 0..40_u16 {
            if x != 0 || y != 0 {
                tank(&mut game, [f32::from(25 + x * 50), f32::from(25 + y * 50)]);
            }
        }
    }
    game.refresh_occupancy();
    assert_eq!(game.route_units(id).info_available, 0);
    game.tick = 10;
    game.request_ground_path(id, request([1905.0, 1805.0], Kind::Move));
    assert!(game.entities[id].ai.ground_path.is_some());
    assert!(!game.entities[id].ai.path.raw.is_empty());
    game.process_path_service();
    assert!(game.entities[id].ai.path.raw.is_empty());
    assert!(game.entities[id].ai.ground_path.is_none());
    assert!(game.entities[id].ai.path_request.pending.is_none());
    assert!(game.entities[id].ai.path_request.failed);
    game.update_order(id);
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
}

#[test]
fn intermediate_bridge_exit_uses_current_layer_quick_path_without_search_budget() {
    use crate::terrain::{BridgeGeometry, TerrainMap};
    for intermediate in [true, false] {
        let mut game = game();
        let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
        let layer = terrain
            .add_bridge(
                BridgeGeometry {
                    from: [400.0, 205.0, 100.0],
                    to: [550.0, 205.0, 100.0],
                    corners: [
                        [400.0, 255.0, 100.0],
                        [400.0, 155.0, 100.0],
                        [550.0, 255.0, 100.0],
                        [550.0, 155.0, 100.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        game.configure_movement_terrain(terrain).unwrap();
        let id = tank(&mut game, [455.0, 205.0]);
        // Place the fixture on the actual deck, before issuing an ordinary path command.
        game.entities[id].layer = layer;
        game.set_object_position(id, [455.0, 205.0, 100.0]);
        let goal = [605.0, 205.0, 0.0];
        assert_eq!(
            game.movement_terrain.get_layer_for_destination(goal, 0.0),
            1
        );
        assert!(game.ground_line_passable_on(id, [455.0, 205.0], [605.0, 205.0], layer));
        let mut points = vec![goal];
        if intermediate {
            points.push([705.0, 205.0, 0.0]);
        }
        game.native_follow_path_from(
            id,
            points,
            None,
            crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
            false,
        );
        assert!(game.entities[id].ai.path_request.pending.is_some());
        assert_eq!(game.entities[id].ai.path_request.final_goal, !intermediate);
        game.process_path_service();
        assert!(game.entities[id].ai.path_request.pending.is_none());
        assert!(game.entities[id].ai.ground_path.is_some());
        if intermediate {
            // AIUpdate::computeQuickPath keeps the source layer on the first node,
            // ground on the last, and the destination Z on both positions.
            assert_eq!(game.path_service.cumulative_cells, 0);
            let path = &game.entities[id].ai.path;
            assert_eq!(path.raw.len(), 2);
            assert_eq!(path.raw[0].layer, layer);
            assert_eq!(path.raw[1].layer, 1);
            assert_eq!(
                path.quick_start.map(|point| point.map(f32::to_bits)),
                Some([455.0_f32, 205.0, 0.0].map(f32::to_bits))
            );
            assert_eq!(
                path.exact_endpoint.map(|point| point.map(f32::to_bits)),
                Some(goal.map(f32::to_bits))
            );
        } else {
            assert!(game.path_service.cumulative_cells > 0);
            assert!(game.entities[id].ai.path.quick_start.is_none());
        }
    }
}
