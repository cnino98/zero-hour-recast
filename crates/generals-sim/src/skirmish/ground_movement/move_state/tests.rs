use crate::skirmish::{
    Order,
    ground_movement::{MoveResult, tests::moving_tank},
};

#[test]
fn move_waits_for_fifo_then_samples_its_force_in_locomotor_phase() {
    let (mut game, id) = moving_tank();
    game.move_to(id, [450.0, 150.0]).unwrap();
    game.tick = 1;
    assert!(matches!(
        game.update_primary_ground_move(id),
        MoveResult::Continue
    ));
    assert!(game.entities[id].ai.path_request.pending.is_some());
    assert!(game.entities[id].ai.ground_path.is_none());
    assert!(game.entities[id].ai.motion.command_frame.is_none());
    game.process_path_service();
    assert!(game.entities[id].ai.ground_path.is_some());
    assert!(matches!(
        game.update_primary_ground_move(id),
        MoveResult::Continue
    ));
    assert!(
        game.entities[id].ai.motion.climb_goal().is_none(),
        "state chooses a path, not a sampled force"
    );
    assert!(game.entities[id].ai.motion.command_frame.is_none());
    game.update_ground_locomotor(id);
    assert_eq!(game.entities[id].ai.motion.command_frame, Some(1));
    assert!(game.entities[id].ai.motion.acceleration[0] > 0.0);
}

#[test]
fn redirect_keeps_the_old_route_while_the_new_move_waits() {
    let (mut game, id) = moving_tank();
    game.move_to(id, [450.0, 150.0]).unwrap();
    game.tick = 1;
    game.process_path_service();
    game.update_order(id);
    game.update_ground_locomotor(id);
    let old_path: Vec<_> = game.entities[id]
        .ai
        .ground_path
        .as_ref()
        .unwrap()
        .positions()
        .map(|point| point.map(f32::to_bits))
        .collect();
    game.move_to(id, [400.0, 250.0]).unwrap();
    assert_eq!(
        game.entities[id]
            .ai
            .ground_path
            .as_ref()
            .unwrap()
            .positions()
            .map(|point| point.map(f32::to_bits))
            .collect::<Vec<_>>(),
        old_path
    );
    assert!(game.entities[id].ai.path_request.pending.is_some());
    assert!(matches!(
        game.update_primary_ground_move(id),
        MoveResult::Continue
    ));
    game.finish_completed_movement(id);
    assert!(
        game.entities[id].ai.ground_path.is_some(),
        "new onEnter cancels old onExit completion"
    );
    assert!(game.entities[id].ai.path_request.pending.is_some());
    game.process_path_service();
    assert!(game.entities[id].ai.path_request.pending.is_none());
    assert_ne!(
        game.entities[id]
            .ai
            .ground_path
            .as_ref()
            .unwrap()
            .positions()
            .map(|point| point.map(f32::to_bits))
            .collect::<Vec<_>>(),
        old_path
    );
}

#[test]
fn leaving_move_defers_path_and_waiting_cleanup_to_base_completion() {
    let (mut game, id) = moving_tank();
    game.move_to(id, [450.0, 150.0]).unwrap();
    game.stop(id).unwrap();
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(game.entities[id].ai.path_request.pending.is_some());
    game.finish_completed_movement(id);
    assert!(game.entities[id].ai.path_request.pending.is_none());
    assert!(game.entities[id].ai.ground_path.is_none());
    assert!(!game.entities[id].ai.motion.has_goal());
}

#[test]
fn moving_object_goal_repaths_after_strict_ten_frame_interval() {
    let (mut game, id) = moving_tank();
    let target = game.spawn(game.entities[id].type_index, 1, [450.0, 150.0], true);
    game.move_to(id, [450.0, 150.0]).unwrap();
    game.set_ai_goal_object(id, target);
    game.tick = 1;
    game.process_path_service();
    assert!(matches!(
        game.update_primary_ground_move(id),
        MoveResult::Continue
    ));
    game.set_object_position(target, [470.0, 150.0, 0.0]);
    game.tick = 11;
    assert!(matches!(
        game.update_primary_ground_move(id),
        MoveResult::Continue
    ));
    assert!(game.entities[id].ai.path_request.pending.is_none());
    game.tick = 12;
    assert!(matches!(
        game.update_primary_ground_move(id),
        MoveResult::Continue
    ));
    let request = game.entities[id].ai.path_request.pending.as_ref().unwrap();
    assert_eq!(
        request.goal.map(f32::to_bits),
        [470.0_f32, 150.0, 0.0].map(f32::to_bits)
    );
    assert!(game.entities[id].ai.ground_path.is_some());
}
