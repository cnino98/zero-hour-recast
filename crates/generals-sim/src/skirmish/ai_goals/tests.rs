use super::*;

#[test]
fn main_attack_goal_snapshots_target_origin_and_survives_internal_completion() {
    let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
    let target = game.spawn(game.entities[actor].type_index, 1, [600.0, 150.0], true);
    let original = game.object_position(target);
    game.attack(actor, target).unwrap();
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        original.map(f32::to_bits)
    );
    game.entities[target].position = [650.0, 180.0];
    game.update_order(actor);
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        original.map(f32::to_bits)
    );
    game.set_order(actor, Order::Idle);
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        original.map(f32::to_bits)
    );
    game.stop(actor).unwrap();
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
}

#[test]
fn attack_move_keeps_clicked_goal_during_nested_attack_but_guard_clears_it() {
    let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
    let goal = [600.0, 150.0];
    game.attack_move(actor, goal).unwrap();
    let snapshot = game.entities[actor].ai_goal_position();
    let target = game.spawn(game.entities[actor].type_index, 1, [350.0, 150.0], true);
    game.entities[actor].ai.ai_target = Some(target);
    game.update_attack_move(actor, goal);
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        snapshot.map(f32::to_bits)
    );
    game.guard(actor, [400.0, 300.0]).unwrap();
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
}

#[test]
fn waypoint_append_preserves_active_main_goal_until_next_follow_update() {
    let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
    let first = [300.0, 200.0];
    let second = [450.0, 250.0];
    game.queue_move_to(actor, first).unwrap();
    let snapshot = game.entities[actor].ai_goal_position();
    game.queue_move_to(actor, second).unwrap();
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        snapshot.map(f32::to_bits)
    );
    let (points, index) = game.entities[actor].waypoint_path().unwrap();
    assert_eq!(index, 0);
    assert_eq!(points.len(), 2);
    assert_eq!(
        points[0].map(f32::to_bits),
        [first[0], first[1], game.ground_height(first)].map(f32::to_bits)
    );
    game.update_native_follow_path(actor);
    // The ungrouped tank's active goal is the center of pathfinding cell (30, 20).
    let adjusted = [305.0, 205.0];
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        [adjusted[0], adjusted[1], game.ground_height(adjusted)].map(f32::to_bits)
    );
}
