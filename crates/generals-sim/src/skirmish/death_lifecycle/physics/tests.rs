use crate::skirmish::{
    DeathType,
    death_lifecycle::tests::{fixture, kill},
};

#[test]
fn native_infantry_scrub_preserves_vertical_force_and_does_not_increase_xy_speed() {
    let (mut game, actor) = fixture(DeathType::Normal);
    kill(&mut game, actor, DeathType::Normal);
    let body = game.entities[actor]
        .death_lifecycle
        .as_mut()
        .unwrap()
        .body
        .as_mut()
        .unwrap();
    body.velocity = [3.0, 4.0, 0.0];
    body.motive_until = game.tick + 10;
    body.extra_friction = -0.1;
    game.update_dead_ground(actor);
    let body = game.entities[actor]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(
        body.velocity.map(f32::to_bits),
        [2.4, 3.2, 0.0].map(f32::to_bits)
    );
    assert_eq!(body.extra_friction.to_bits(), 0.0_f32.to_bits());
    game.tick += 1;
    let body = game.entities[actor]
        .death_lifecycle
        .as_mut()
        .unwrap()
        .body
        .as_mut()
        .unwrap();
    body.velocity = [3.0, 4.0, 10.0];
    game.update_dead_ground(actor);
    let body = game.entities[actor]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(
        body.velocity.map(f32::to_bits),
        [3.0, 4.0, 10.0].map(f32::to_bits)
    );
}

#[test]
fn original_handle_bounce_defers_ground_stiffness_impulse_until_next_physics_frame() {
    let (mut game, actor) = fixture(DeathType::Normal);
    kill(&mut game, actor, DeathType::Normal);
    let body = game.entities[actor]
        .death_lifecycle
        .as_mut()
        .unwrap()
        .body
        .as_mut()
        .unwrap();
    body.height = 0.5;
    body.velocity = [0.0, 0.0, -1.0];
    body.bouncing = true;
    body.falling = true;
    game.integrate_death_body(actor);
    let body = game.entities[actor]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(body.height.to_bits(), 0.0_f32.to_bits());
    assert_eq!(body.velocity[2].to_bits(), (-0.5_f32).to_bits());
    assert_eq!(body.acceleration[2].to_bits(), 1.0_f32.to_bits());
    game.integrate_death_body(actor);
    let body = game.entities[actor]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(body.acceleration[2].to_bits(), 0.0_f32.to_bits());
    assert!(!body.bouncing);
}

#[test]
fn held_sink_cannot_be_clipped_back_to_terrain_by_physics() {
    let (mut game, actor) = fixture(DeathType::Normal);
    kill(&mut game, actor, DeathType::Normal);
    game.tick += 90;
    game.update_deaths(game.entities.len());
    let before = game.object_position(actor)[2];
    assert!(before < 0.0);
    game.integrate_death_body(actor);
    assert_eq!(game.object_position(actor)[2].to_bits(), before.to_bits());
}

#[test]
fn dead_wheeled_vehicle_keeps_velocity_while_resetting_locomotor_friction() {
    let (mut game, actor) = fixture(DeathType::Normal);
    let unit = &mut game.catalog[game.entities[actor].type_index];
    unit.kind_of = vec!["VEHICLE".into()];
    unit.locomotor.ground.as_mut().unwrap().appearance =
        crate::skirmish::GroundAppearance::Wheels {
            min_turn_speed: 0.0,
            can_move_backwards: false,
        };
    kill(&mut game, actor, DeathType::Normal);
    let body = game.entities[actor]
        .death_lifecycle
        .as_mut()
        .unwrap()
        .body
        .as_mut()
        .unwrap();
    body.velocity = [3.0, 4.0, 2.0];
    body.extra_friction = -0.1;
    game.update_dead_ground(actor);
    let body = game.entities[actor]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(
        body.velocity.map(f32::to_bits),
        [3.0, 4.0, 2.0].map(f32::to_bits)
    );
    assert_eq!(body.extra_friction.to_bits(), 0.0_f32.to_bits());
}

#[test]
fn dead_body_uses_authored_stick_to_ground_and_airborne_friction_independently() {
    for (stick, airborne, expected_speed, expected_height) in [
        (false, false, 10.0_f32, 19.0_f32),
        (false, true, 8.5, 19.0),
        (true, false, 10.0, 0.0),
    ] {
        let (mut game, actor) = fixture(DeathType::Normal);
        let physics = game.catalog[game.entities[actor].type_index]
            .death_lifecycle
            .as_mut()
            .unwrap()
            .physics
            .as_mut()
            .unwrap();
        physics.stick_to_ground = stick;
        physics.airborne_friction = airborne;
        kill(&mut game, actor, DeathType::Normal);
        let body = game.entities[actor]
            .death_lifecycle
            .as_mut()
            .unwrap()
            .body
            .as_mut()
            .unwrap();
        body.height = 20.0;
        body.velocity = [10.0, 0.0, 0.0];
        body.acceleration = [0.0; 3];
        body.motive_until = 0;
        body.falling = false;
        game.entities[actor].facing = 0.0;
        game.integrate_death_body(actor);
        let body = game.entities[actor]
            .death_lifecycle
            .as_ref()
            .unwrap()
            .body
            .as_ref()
            .unwrap();
        assert_eq!(body.velocity[0].to_bits(), expected_speed.to_bits());
        assert_eq!(body.height.to_bits(), expected_height.to_bits());
    }
}
