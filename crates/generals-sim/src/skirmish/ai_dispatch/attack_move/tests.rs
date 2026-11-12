use crate::skirmish::{
    Order,
    ai_goals::{COMMAND_FROM_AI, COMMAND_FROM_PLAYER, COMMAND_FROM_SCRIPT},
    startup,
};

#[test]
fn script_attack_move_preserves_xyz_and_player_clipping_preserves_z() {
    let (mut game, id) = crate::skirmish::ground_movement::tests::moving_tank();
    let point = [-50.0, -70.0, 43.0];
    game.native_attack_move_from(id, point, 7, COMMAND_FROM_SCRIPT);
    assert_eq!(
        game.entities[id].ai_goal_position().map(f32::to_bits),
        point.map(f32::to_bits)
    );
    assert_eq!(
        game.entities[id].ai.command.last_command_source,
        COMMAND_FROM_SCRIPT
    );
    assert!(matches!(game.entities[id].ai.order, Order::AttackMove(_)));
    assert!(game.entities[id].ai.path_request.pending.is_some());
    assert_eq!(
        game.entities[id].weapon_set.weapons[game.entities[id].weapon_slot()].max_shots,
        Some(7)
    );
    game.native_attack_move_from(id, point, 3, COMMAND_FROM_PLAYER);
    let actual = game.entities[id].ai_goal_position();
    let fudge = game.layout.partition_cell_size * 0.5;
    assert_eq!(
        actual.map(f32::to_bits),
        [
            game.playable_extent[0][0] + fudge,
            game.playable_extent[0][1] + fudge,
            43.0
        ]
        .map(f32::to_bits)
    );
}

#[test]
fn stopping_attack_move_exits_child_once_and_cancels_queued_movement() {
    let (mut game, id) = crate::skirmish::ground_movement::tests::moving_tank();
    let mut expected = game.random;
    let _ = startup::random_between(&mut expected, 60); // nested Idle onEnter
    game.native_attack_move_from(id, [500.0, 150.0, 0.0], 7, COMMAND_FROM_AI);
    assert_eq!(game.random, expected);
    assert!(game.entities[id].ai.path_request.pending.is_some());
    let _ = startup::random_between(&mut expected, 60); // child Idle onExit
    let _ = startup::random_between(&mut expected, 60); // primary Idle onEnter
    game.ai_idle_from_ai(id);
    assert_eq!(game.random, expected);
    assert!(game.entities[id].ai_idle());
    assert!(game.entities[id].ai.path_request.pending.is_none());
    assert!(game.entities[id].ai.attack.is_none());
    assert!(game.entities[id].ai.ai_target.is_none());
    assert!(!game.entities[id].ai.attack_move.active);
}

#[test]
fn blocked_attack_move_waits_three_seconds_before_retrying() {
    let (mut game, id) = crate::skirmish::ground_movement::tests::moving_tank();
    // Leave the distant destination clear: blocking every cell instead makes
    // adjustDestination snap the goal back beside the actor before pathfinding.
    let cells = (0..game.grid.height())
        .flat_map(|y| (32..35).map(move |x| crate::grid::Cell { x, y }))
        .collect::<Vec<_>>();
    game.navigation = game.grid.with_obstacles(cells);
    game.native_attack_move_from(id, [650.0, 150.0, 0.0], 7, COMMAND_FROM_AI);
    game.entities[id].ai.mood.next_check = u64::MAX;
    for _ in 0..1200 {
        game.step();
        if game.entities[id].ai.attack_move.sleep_until > 0 {
            break;
        }
    }
    let deadline = game.entities[id].ai.attack_move.sleep_until;
    assert!(
        deadline > 0,
        "position={:?}, active={}",
        game.entities[id].position,
        game.entities[id].ai.attack_move.active
    );
    assert_eq!(game.entities[id].ai.attack_move.retries, 4);
    game.tick = u64::from(deadline - 1);
    game.update_attack_move(id, [650.0, 150.0]);
    assert!(game.entities[id].ai.path_request.pending.is_none());
    assert_eq!(game.entities[id].ai.attack_move.retries, 4);
    game.tick += 1;
    game.update_attack_move(id, [650.0, 150.0]);
    assert!(game.entities[id].ai.path_request.pending.is_some());
    assert!(matches!(game.entities[id].ai.order, Order::AttackMove(_)));
}
