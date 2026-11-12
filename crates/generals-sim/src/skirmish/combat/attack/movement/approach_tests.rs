use super::super::{Goal, Phase};
use crate::skirmish::Order;
use crate::skirmish::Skirmish;

fn approaching() -> (Skirmish, usize, usize) {
    let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
    let target = game.spawn(game.entities[actor].type_index, 1, [600.0, 150.0], true);
    game.tick = 100;
    game.attack(actor, target).unwrap();
    game.update_order(actor);
    assert!(game.entities[actor].ai.path_request.pending.is_some());
    game.process_path_service();
    assert!(game.entities[actor].ai.ground_path.is_some());
    game.update_order(actor);
    assert!(matches!(
        game.entities[actor].ai.attack.as_ref().unwrap().phase,
        Phase::Approach
    ));
    (game, actor, target)
}

#[test]
fn moving_target_keeps_route_until_native_interval_and_distance_tolerance() {
    let (mut game, actor, target) = approaching();
    let route = game.entities[actor].ai.path.raw.clone();
    for frame in 101_u16..=110 {
        game.tick = u64::from(frame);
        game.entities[target].position[0] = 600.0 + f32::from(frame - 100);
        game.update_order(actor);
        assert!(game.entities[actor].ai.path_request.pending.is_none());
        assert!(game.entities[actor].ai.motion.has_goal());
        assert!(game.entities[actor].ai.path.raw == route);
    }
    assert_eq!(
        game.entities[actor]
            .ai
            .attack
            .as_ref()
            .unwrap()
            .movement
            .approach_frame,
        110
    );
    // This larger change still waits for the interval started by the preceding
    // tolerance check, which did not request a new path.
    game.entities[target].position[0] = 700.0;
    game.tick = 119;
    game.update_order(actor);
    assert!(game.entities[actor].ai.path_request.pending.is_none());
    game.tick = 120;
    game.update_order(actor);
    assert!(game.entities[actor].ai.path_request.pending.is_some());
    let state = game.entities[actor].ai.attack.as_ref().unwrap();
    assert_eq!(state.movement.approach_frame, 120);
    assert_eq!(
        state.movement.previous_position.map(f32::to_bits),
        [700.0_f32, 150.0, 0.0].map(f32::to_bits)
    );
    game.tick = 121;
    game.entities[target].position[0] = 710.0;
    game.update_order(actor);
    assert_eq!(
        game.entities[actor]
            .ai
            .attack
            .as_ref()
            .unwrap()
            .movement
            .approach_frame,
        120
    );
}

#[test]
fn stuck_or_missing_route_bypasses_approach_interval_but_keeps_queue_throttle() {
    for stuck in [false, true] {
        let (mut game, actor, _) = approaching();
        if stuck {
            game.entities[actor].ai.blocking.stuck = true;
        } else {
            game.entities[actor].ai.path.clear();
            game.entities[actor].ai.ground_path = None;
        }
        game.tick = 101;
        game.update_order(actor);
        assert!(game.entities[actor].ai.path_request.pending.is_some());
        // The old path survives a stuck repath, but the throttle clears motion.
        assert!(!game.entities[actor].ai.motion.has_goal());
        assert_eq!(game.entities[actor].ai.ground_path.is_some(), stuck);
        assert_eq!(
            game.entities[actor]
                .ai
                .attack
                .as_ref()
                .unwrap()
                .movement
                .approach_frame,
            101
        );
        // The source requestAttackPath throttle waits sixty frames, then the
        // FIFO service consumes the request on the following logic frame.
        game.tick = 160;
        game.wake_path_request(actor);
        game.process_path_service();
        assert!(game.entities[actor].ai.path_request.pending.is_some());
        game.tick = 161;
        game.wake_path_request(actor);
        game.process_path_service();
        assert!(game.entities[actor].ai.path_request.pending.is_none());
    }
}

#[test]
fn replacement_command_and_approach_completion_discard_previous_victim_state() {
    let (mut game, actor, target) = approaching();
    game.stop(actor).unwrap();
    assert!(game.entities[actor].ai.attack.is_none());
    let second = game.spawn(game.entities[actor].type_index, 1, [650.0, 150.0], true);
    game.tick = 200;
    game.attack(actor, second).unwrap();
    game.update_order(actor);
    let state = game.entities[actor].ai.attack.as_ref().unwrap();
    assert!(matches!(state.phase, Phase::Approach));
    assert!(matches!(state.goal, Goal::Object(id) if id == second));
    assert_ne!(second, target);
    assert_eq!(
        state.movement.previous_position.map(f32::to_bits),
        game.object_position(second).map(f32::to_bits)
    );
    game.process_path_service();
    let endpoint = game.entities[actor]
        .ai
        .ground_path
        .as_ref()
        .unwrap()
        .positions()
        .last()
        .unwrap();
    game.set_object_position(
        actor,
        [endpoint[0], endpoint[1], game.ground_height(endpoint)],
    );
    game.tick += 1;
    game.update_order(actor);
    let state = game.entities[actor].ai.attack.as_ref().unwrap();
    assert!(!matches!(state.phase, Phase::Approach));
    assert!(matches!(game.entities[actor].ai.order, Order::Attack(id) if id == second));
}
