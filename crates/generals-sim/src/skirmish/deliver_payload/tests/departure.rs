//! Isolated native `HeadOffMap` callback contracts; installed maps use scheduling.
use super::*;

fn departing() -> (Skirmish, usize) {
    let (mut game, _, plane, _, _) = create();
    game.set_object_transform(
        plane,
        [355.0, 255.0, 100.0],
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    );
    game.enter_delivery_phase(plane, Phase::HeadOffMap);
    assert_eq!(
        game.delivery_mission(plane)
            .exit_direction
            .map(f32::to_bits),
        [1.0_f32, 0.0, 0.0].map(f32::to_bits)
    );
    (game, plane)
}

#[test]
fn on_map_deflection_requires_turning_and_strict_departure_dot_threshold() {
    let below = f32::from_bits(0.3_f32.to_bits() - 1);
    for (turning, dot, killed) in [
        (false, below, false),
        (true, 0.3, false),
        (true, below, true),
    ] {
        let (mut game, plane) = departing();
        let square = dot * dot;
        let side = (1.0 - square).sqrt();
        let basis = [[dot, -side, 0.0], [side, dot, 0.0], [0.0, 0.0, 1.0]];
        game.set_object_transform(plane, [355.0, 255.0, 100.0], basis);
        game.entities[plane].physics_control.turning = turning;
        let health = game.entities[plane].health;
        game.update_delivery_state(plane);
        if killed {
            assert!(game.entities[plane].health <= 0.0);
        } else {
            assert_eq!(game.entities[plane].health.to_bits(), health.to_bits());
        }
    }
}

#[test]
fn offmap_cleanup_wins_before_turning_deflection_kill() {
    let (mut game, plane) = departing();
    game.set_object_transform(
        plane,
        [355.0, -100.0, 100.0],
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
    );
    game.entities[plane].physics_control.turning = true;
    let health = game.entities[plane].health;
    game.update_delivery_state(plane);
    game.process_update_destruction();
    assert!(!game.entities[plane].exists());
    assert_eq!(game.entities[plane].health.to_bits(), health.to_bits());
}
