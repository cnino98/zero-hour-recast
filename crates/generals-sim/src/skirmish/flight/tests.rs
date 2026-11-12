use super::{Appearance, Height, Rules};
use crate::skirmish::{Kind, tests::game};

pub(in crate::skirmish) fn rules(appearance: Appearance) -> Rules {
    Rules {
        draw: super::draw::Rules {
            stiffness: [0.1; 2],
            damping: [0.9; 2],
            axial_damping: 1.0,
            velocity: [0.0; 3],
            acceleration: [0.0; 2],
            pitch_limits: [0.0; 2],
            rudder: [0.0; 2],
            elevator: [0.0; 2],
        },
        appearance,
        height: Height::Surface,
        acceleration: 60.0 / 900.0,
        acceleration_damaged: 30.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        close_3d: false,
        preferred_height: 100.0,
        lift: 120.0 / 900.0,
        lift_damaged: 80.0 / 900.0,
        damping: 1.0,
        speed_limit_z: 999_999.0,
        gravity: -1.0 / 30.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        aerodynamic_friction: 0.0,
        extra_friction: 0.0,
        friction_2d: true,
        airborne_force: true,
        slide_frames: 3.0,
        circling_radius: 0.0,
    }
}

fn setup(appearance: Appearance) -> (crate::skirmish::Skirmish, usize) {
    let mut game = game();
    let index = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[index];
    unit.locomotor.ground = None;
    unit.locomotor.flight = Some(rules(appearance));
    unit.movement_surface = crate::skirmish::MovementSurface::Air;
    unit.locomotor.legal_surfaces = 8;
    unit.preferred_height = 100.0;
    let id = game.spawn(index, 0, [100.0, 100.0], true);
    game.entities[id].facing = 0.0;
    (game, id)
}

#[test]
fn hover_accelerates_using_the_direction_before_rotating_and_physics_runs_later() {
    let (mut game, id) = setup(Appearance::Hover);
    assert!(!game.travel_flight(id, [100.0, 400.0], 0.1));
    game.update_flight(id);
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        [100.0_f32.to_bits(); 2]
    );
    let state = game.entities[id].flight.as_ref().unwrap();
    assert!(game.entities[id].facing > 0.0);
    assert_eq!(state.velocity.map(f32::to_bits), [0; 3]);
    assert!(state.acceleration[0] > 0.06 && state.acceleration[0] < 0.07);
    assert_eq!(state.acceleration[1].to_bits(), 0);
    game.integrate_flight(id);
    assert!(game.entities[id].position[0] > 100.06);
    assert_eq!(game.entities[id].position[1].to_bits(), 100.0_f32.to_bits());
    assert_eq!(game.entities[id].flight_height(), Some(100.0));
}

#[test]
fn stopped_hover_brakes_without_teleporting_or_zeroing_its_velocity() {
    let (mut game, id) = setup(Appearance::Hover);
    let state = game.entities[id].flight.as_mut().unwrap();
    state.velocity[0] = 1.0;
    state.motive_until = 10;
    game.entities[id].ai.motion.velocity[0] = 1.0;
    game.entities[id].ai.motion.motive_until = 10;
    game.update_flight(id);
    game.integrate_flight(id);
    let actor = &game.entities[id];
    assert!(actor.position[0] > 100.8 && actor.position[0] < 101.0);
    assert!(actor.flight.as_ref().unwrap().velocity[0] > 0.8);
}

#[test]
fn native_inch_forward_hover_preserves_nonzero_minimum_and_strict_idle_force_gate() {
    for (speed, brakes) in [(0.0, false), (1.0, false), (2.0, true)] {
        let (mut game, id) = setup(Appearance::Hover);
        let index = game.entities[id].type_index;
        let rules = game.catalog[index].locomotor.flight.as_mut().unwrap();
        rules.min_speed = 25.0 * (1.0 / 30.0);
        super::validate(rules).unwrap();
        let force = rules.mass * -rules.braking;
        let expected = if brakes {
            force * (1.0 / rules.mass)
        } else {
            0.0
        };
        let state = game.entities[id].flight.as_mut().unwrap();
        state.velocity[0] = speed;
        state.motive_until = 10;
        game.entities[id].ai.motion.velocity[0] = speed;
        game.entities[id].ai.motion.motive_until = 10;
        game.update_flight(id);
        assert_eq!(
            game.entities[id].flight.as_ref().unwrap().acceleration[0].to_bits(),
            expected.to_bits()
        );
    }
}

#[test]
fn idle_wings_keep_turning_and_flying_instead_of_stopping_like_a_hover() {
    let (mut game, id) = setup(Appearance::Wings);
    let index = game.entities[id].type_index;
    game.catalog[index]
        .locomotor
        .flight
        .as_mut()
        .unwrap()
        .min_speed = 1.0;
    let state = game.entities[id].flight.as_mut().unwrap();
    state.velocity[0] = 1.0;
    state.motive_until = 10;
    game.entities[id].ai.motion.velocity[0] = 1.0;
    game.entities[id].ai.motion.motive_until = 10;
    game.update_flight(id);
    game.integrate_flight(id);
    assert!(game.entities[id].facing > 0.0);
    assert!(game.entities[id].position[0] > 100.9);
}

#[test]
fn moving_aircraft_aim_keeps_explicit_target_and_turns_only_in_locomotor_phase() {
    for (offset, fires) in [(4.0, true), (40.0, false)] {
        let (mut game, id) = setup(Appearance::Wings);
        let index = game.entities[id].type_index;
        let unit = &mut game.catalog[index];
        unit.locomotor.flight.as_mut().unwrap().min_speed = 1.0;
        unit.locomotor.turn_rate = 0.005;
        unit.weapon.range = 1000.0;
        unit.weapon.aim_delta = 0.035;
        let target_type = game.type_for(1, Kind::Tank).unwrap();
        let target = game.spawn(target_type, 1, [300.0, 100.0 + offset], true);
        let target_position = game.object_position(target);
        assert!(game.within_attack_range(id, target));
        game.attack(id, target).unwrap();
        assert!(game.update_attack(id, target));
        assert!(game.presentation.shots.is_empty());
        // Aim succeeds into Fire; firing occurs in the following state update.
        assert!(game.update_attack(id, target));
        // Two-degree aiming tolerance is independent of this frame's much
        // smaller turn rate. Aiming must not rotate the aircraft a second time.
        assert_eq!(
            game.presentation.shots.iter().any(|shot| shot.source == id),
            fires
        );
        assert_eq!(game.entities[id].facing.to_bits(), 0.0_f32.to_bits());
        let goal = game.entities[id]
            .flight
            .as_ref()
            .unwrap()
            .goal
            .as_ref()
            .unwrap();
        assert_eq!(
            goal.position.map(f32::to_bits),
            target_position.map(f32::to_bits)
        );
        assert_eq!(goal.distance.to_bits(), 0.0_f32.to_bits());
        game.update_flight(id);
        assert!(game.entities[id].facing > 0.0);
    }
}

#[test]
fn dying_jet_negative_maximum_lift_keeps_native_ordered_bounds() {
    let gravity = -1.0_f32 / 30.0;
    let gross = -gravity * (1.0 - 1.1);
    assert!(gross < 0.0);
    for precise in [false, true] {
        for velocity in [-2.0, 0.0, 2.0] {
            let actual =
                super::physics::lift(100.0, velocity, 120.0, gross, gravity, 999.0, precise);
            let expected = if precise { 3.0 * gross } else { gross };
            assert_eq!(actual.to_bits(), expected.to_bits());
            assert!(actual + gravity < gravity);
        }
    }
}

#[test]
fn precision_lift_retains_native_negative_force_and_downward_speed_limit_branch() {
    let rules = rules(Appearance::Hover);
    let ordinary = super::physics::lift(
        100.0,
        0.0,
        90.0,
        rules.lift,
        rules.gravity,
        rules.speed_limit_z,
        false,
    );
    let precise = super::physics::lift(
        100.0,
        0.0,
        90.0,
        rules.lift,
        rules.gravity,
        rules.speed_limit_z,
        true,
    );
    assert_eq!(ordinary.to_bits(), 0);
    assert_eq!(precise.to_bits(), (-rules.lift).to_bits());
}

#[test]
fn native_flight_projection_accepts_crossed_endpoint_and_a_new_approach_gets_a_new_path() {
    let (mut game, id) = setup(Appearance::Hover);
    assert!(!game.travel_flight(id, [200.0, 100.0], 0.1));
    game.entities[id].position = [201.0, 120.0];
    assert!(game.travel_flight(id, [200.0, 100.0], 0.1));
    // An attack still outside its weapon range must be able to re-approach.
    assert!(!game.travel_flight(id, [200.0, 100.0], 0.1));
}

#[test]
fn precise_repair_goal_requires_vertical_arrival() {
    let (mut game, id) = setup(Appearance::Hover);
    let index = game.entities[id].type_index;
    game.catalog[index]
        .locomotor
        .flight
        .as_mut()
        .unwrap()
        .close_3d = true;
    game.set_flight_precision(id, true, true);
    game.construction_move(id, [100.0, 100.0, 25.0]);
    assert_eq!(
        game.entities[id].ai.command.last_command_source,
        crate::skirmish::ai_goals::COMMAND_FROM_AI
    );
    assert_eq!(
        game.entities[id]
            .ai
            .ground_path
            .as_ref()
            .unwrap()
            .last_position()
            .unwrap()
            .map(f32::to_bits),
        [100.0_f32, 100.0, 25.0].map(f32::to_bits)
    );
    assert!(!game.travel_flight(id, [100.0, 100.0], 0.1));
    game.set_flight_position(id, [100.0, 100.0, 25.0]);
    assert!(game.travel_flight(id, [100.0, 100.0], 0.1));
}

#[test]
fn unchanged_panic_selection_keeps_the_existing_hover_reference() {
    let (mut game, id) = setup(Appearance::Hover);
    game.update_flight(id);
    let point = game.entities[id].flight.as_ref().unwrap().maintain;
    game.entities[id].position[0] += 1.0;
    game.set_flight_locomotor(id, false);
    game.update_flight(id);
    assert_eq!(game.entities[id].flight.as_ref().unwrap().maintain, point);
}

#[test]
fn short_projected_path_uses_actual_distance_before_decelerating() {
    let (mut game, id) = setup(Appearance::Hover);
    game.entities[id].ai.motion.velocity[0] = 1.0;
    game.entities[id].flight.as_mut().unwrap().goal = Some(super::Goal {
        position: [200.0, 100.0, 100.0],
        distance: 0.25,
        speed: 2.0,
        no_slow_down: false,
    });
    game.update_flight(id);
    let state = game.entities[id].flight.as_ref().unwrap();
    assert!(state.flag(super::BRAKING));
    assert!(state.acceleration[0] > 0.0);
}

#[test]
fn terrain_collision_samples_the_new_horizontal_position() {
    let (mut game, id) = setup(Appearance::Hover);
    game.set_projectile_terrain(
        5,
        5,
        [90.0, 90.0],
        [0.0, 0.0, 50.0, 50.0, 50.0].repeat(5),
        10.0,
    )
    .unwrap();
    let state = game.entities[id].flight.as_mut().unwrap();
    state.height = 1.0;
    state.velocity[0] = 2.0;
    state.motive_until = 10;
    game.integrate_flight(id);
    let actor = &game.entities[id];
    let surface = game.ground_height(actor.position);
    assert!(surface > 1.0);
    assert_eq!(actor.flight_height().unwrap().to_bits(), surface.to_bits());
}

#[test]
fn braking_release_uses_damage_condition_maximum_even_for_a_slower_requested_goal() {
    let (mut game, id) = setup(Appearance::Hover);
    let index = game.entities[id].type_index;
    game.catalog[index].speed = 120.0;
    game.catalog[index].locomotor.speed_damaged = 60.0;
    game.catalog[index]
        .locomotor
        .flight
        .as_mut()
        .unwrap()
        .braking = 0.1;
    for (health, retained) in [(1.0, true), (0.1, false)] {
        let actor = &mut game.entities[id];
        actor.health = actor.max_health * health;
        let state = actor.flight.as_mut().unwrap();
        state.set(super::BRAKING, true);
        state.goal = Some(super::Goal {
            position: [130.0, 100.0, 100.0],
            distance: 30.0,
            speed: 1.0,
            no_slow_down: false,
        });
        game.update_flight(id);
        assert_eq!(
            game.entities[id]
                .flight
                .as_ref()
                .unwrap()
                .flag(super::BRAKING),
            retained
        );
    }
}

#[test]
fn airborne_friction_uses_the_strict_native_height_boundary() {
    let (mut game, id) = setup(Appearance::Hover);
    let index = game.entities[id].type_index;
    let rules = game.catalog[index].locomotor.flight.as_mut().unwrap();
    rules.friction_2d = false;
    let threshold = -9.0 * rules.gravity;
    let state = game.entities[id].flight.as_mut().unwrap();
    state.height = threshold;
    state.velocity[0] = 1.0;
    game.integrate_flight(id);
    let state = game.entities[id].flight.as_mut().unwrap();
    assert!(state.velocity[0] < 1.0);
    state.height = threshold + 0.001;
    state.velocity[0] = 1.0;
    game.integrate_flight(id);
    assert_eq!(
        game.entities[id].flight.as_ref().unwrap().velocity[0].to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn changing_from_taxi_to_hover_retains_the_same_object_physics() {
    let (mut game, id) = setup(Appearance::Hover);
    game.tick = 50;
    let actor = &mut game.entities[id];
    actor.ai.motion.velocity = [1.0, 0.0];
    actor.ai.motion.motive_until = 70;
    actor.ai.motion.set_object_braking(true);
    game.update_flight(id);
    let actor = &game.entities[id];
    let flight = actor.flight.as_ref().unwrap();
    assert!(flight.acceleration[0] < 0.0);
    assert!(flight.flag(super::OBJECT_BRAKING));
    assert_eq!(flight.motive_until, 60);
    assert_eq!(actor.ai.motion.motive_until, 60);
    game.integrate_flight(id);
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        [100.0_f32.to_bits(); 2]
    );
    assert!(game.entities[id].ai.motion.velocity[0] > 0.8);
}

#[test]
fn grounded_wing_shock_hands_off_full_motion_once_and_keeps_ai_lift() {
    use crate::skirmish::physical_ocl::{LiveRules, MotionRules};
    let (mut game, id) = setup(Appearance::Wings);
    let kind = game.entities[id].type_index;
    let gravity = game.catalog[kind]
        .locomotor
        .flight
        .as_ref()
        .unwrap()
        .gravity;
    game.catalog[kind].live_physics = Some(LiveRules {
        motion: MotionRules {
            center_of_mass_offset: 0.0,
            mass: 50.0,
            gravity,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.0,
        kill_when_resting: false,
        collision: crate::skirmish::physical_ocl::CollisionRules {
            stiffness: 0.5,
            rubble_height: 0.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.0; 3],
        min_fall_speed: 99.0,
        fall_damage_factor: 1.0,
    });
    game.set_flight_position(id, [200.0, 200.0, 0.0]);
    let actor = &mut game.entities[id];
    actor.ai.motion.velocity = [2.0, 0.0];
    actor.ai.motion.motive_until = game.tick + 10;
    let state = actor.flight.as_mut().unwrap();
    state.velocity = [2.0, 0.0, 0.25];
    state.acceleration = [0.1, 0.0, 0.4];
    state.motive_until = game.tick + 10;
    game.apply_damage_shock(id, [5.0, 0.0, 0.0], 50.0, 50.0, 1.0);
    game.integrate_flight(id);
    assert_eq!(
        game.entities[id].flight.as_ref().unwrap().height.to_bits(),
        0.0_f32.to_bits()
    );
    game.integrate_live_physics(id);
    let expected = 0.25_f32 + (1.0 + 0.4 + gravity);
    assert!((game.entities[id].physics_velocity()[2] - expected).abs() < 0.000_001);
    let first = game.object_position(id);
    game.integrate_live_physics(id);
    assert_eq!(
        game.object_position(id).map(f32::to_bits),
        first.map(f32::to_bits)
    );
    assert!(
        (game.entities[id]
            .flight
            .as_ref()
            .unwrap()
            .previous_acceleration[2]
            - 1.4)
            .abs()
            < 0.000_001
    );
    game.tick += 1;
    game.update_flight(id);
    assert!(
        game.entities[id]
            .aircraft_physics_input(&game.catalog[kind])
            .unwrap()
            .acceleration[2]
            > 0.0
    );
    game.integrate_flight(id);
    game.integrate_live_physics(id);
    assert!(game.object_position(id)[2] > first[2]);
}

#[test]
fn recreated_flight_locomotor_resets_caps_and_precision_but_keeps_command_and_physics() {
    let (mut game, id) = setup(Appearance::Wings);
    let state = game.entities[id].flight.as_mut().unwrap();
    state.velocity = [3.0, -2.0, 1.0];
    state.acceleration = [0.25, 0.5, -0.5];
    state.motive_until = 123;
    state.goal = Some(super::Goal {
        position: [200.0, 300.0, 40.0],
        distance: 75.0,
        speed: 2.0,
        no_slow_down: true,
    });
    state.goal_z = Some(40.0);
    state.maintain = Some([100.0, 100.0, 20.0]);
    state.flags = super::OBJECT_BRAKING | super::BRAKING | super::PRECISE_Z | super::ULTRA_ACCURATE;
    state.speed_limit = 4.0;
    state.lift_limit = 3.0;
    state.braking_limit = 2.0;
    state.turn_limit = 1.0;
    game.initialize_locomotor_set(id);
    let state = game.entities[id].flight.as_ref().unwrap();
    assert_eq!(
        state.velocity.map(f32::to_bits),
        [3.0_f32, -2.0, 1.0].map(f32::to_bits)
    );
    assert_eq!(
        state.acceleration.map(f32::to_bits),
        [0.25_f32, 0.5, -0.5].map(f32::to_bits)
    );
    assert_eq!(state.motive_until, 123);
    let goal = state.goal.as_ref().unwrap();
    assert_eq!(
        goal.position.map(f32::to_bits),
        [200.0_f32, 300.0, 40.0].map(f32::to_bits)
    );
    assert_eq!(goal.distance.to_bits(), 75.0_f32.to_bits());
    assert_eq!(goal.speed.to_bits(), 2.0_f32.to_bits());
    assert!(!goal.no_slow_down);
    assert_eq!(state.goal_z.map(f32::to_bits), Some(40.0_f32.to_bits()));
    assert_eq!(state.flags, super::OBJECT_BRAKING);
    assert!(state.maintain.is_none());
    assert_eq!(
        [
            state.speed_limit,
            state.lift_limit,
            state.braking_limit,
            state.turn_limit
        ]
        .map(f32::to_bits),
        [99_999.0_f32.to_bits(); 4]
    );
}

#[test]
fn projectile_approach_sets_and_clears_precise_height_without_changing_ultra_accuracy() {
    let mut game = crate::skirmish::tests::game();
    let kind = game.type_for(0, crate::skirmish::Kind::Tank).unwrap();
    game.catalog[kind].locomotor.flight = Some(rules(super::Appearance::Hover));
    game.catalog[kind].locomotor.legal_surfaces = 8;
    game.catalog[kind].kind_of.push("PROJECTILE".into());
    game.catalog[kind].location_weapons = 1;
    game.catalog[kind].weapon.range = 100.0;
    let actor = game.spawn(kind, 0, [200.0, 200.0], true);
    game.set_flight_position(actor, [200.0, 200.0, 100.0]);
    game.set_flight_precision(actor, false, true);
    game.fire_weapon_at(actor, 0, [700.0, 200.0]).unwrap();
    let flight = game.entities[actor].flight.as_ref().unwrap();
    assert!(flight.flag(super::PRECISE_Z));
    assert!(flight.flag(super::ULTRA_ACCURATE));
    game.stop(actor).unwrap();
    let flight = game.entities[actor].flight.as_ref().unwrap();
    assert!(!flight.flag(super::PRECISE_Z));
    assert!(flight.flag(super::ULTRA_ACCURATE));
}

#[test]
fn hover_angle_goal_overrides_travel_and_persists_until_cleared() {
    let (mut game, id) = setup(Appearance::Hover);
    let index = game.entities[id].type_index;
    game.catalog[index].locomotor.turn_rate = 0.1;
    game.entities[id].flight.as_mut().unwrap().turn_limit = 0.04;
    assert!(!game.travel_flight(id, [100.0, 400.0], 0.1));
    let position = game.entities[id].position;
    game.entities[id]
        .ai
        .motion
        .request_angle(std::f32::consts::FRAC_PI_2);
    game.update_flight(id);
    assert_eq!(game.entities[id].facing.to_bits(), 0.04_f32.to_bits());
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    let flight = game.entities[id].flight.as_ref().unwrap();
    assert_eq!(flight.acceleration[0].to_bits(), 0);
    assert_eq!(flight.acceleration[1].to_bits(), 0);
    assert!(flight.goal.is_none() && flight.maintain.is_none());
    assert!(game.entities[id].physics_control.turning);
    // Native ANGLE remains a locomotor goal between AI updates.
    game.update_flight(id);
    assert_eq!(game.entities[id].facing.to_bits(), 0.08_f32.to_bits());
    game.entities[id].ai.motion.clear_goal();
    game.update_flight(id);
    assert_eq!(game.entities[id].facing.to_bits(), 0.08_f32.to_bits());
    assert!(
        game.entities[id]
            .flight
            .as_ref()
            .unwrap()
            .maintain
            .is_some()
    );
}

#[test]
fn damaged_ultra_accurate_flight_angle_preserves_rate_cap_and_height_force() {
    let (mut game, id) = setup(Appearance::Hover);
    let index = game.entities[id].type_index;
    game.catalog[index].locomotor.turn_rate_damaged = 0.03;
    let preferred = game.catalog[index].locomotor.flight.as_mut().unwrap();
    preferred.height = Height::Surface;
    preferred.preferred_height = 200.0;
    game.entities[id].health = 1.0;
    let flight = game.entities[id].flight.as_mut().unwrap();
    flight.turn_limit = 0.02;
    flight.set(super::ULTRA_ACCURATE, true);
    game.entities[id]
        .ai
        .motion
        .request_angle(std::f32::consts::FRAC_PI_2);
    game.update_flight(id);
    assert_eq!(game.entities[id].facing.to_bits(), 0.04_f32.to_bits());
    let flight = game.entities[id].flight.as_ref().unwrap();
    assert_eq!(flight.acceleration[0].to_bits(), 0);
    assert_eq!(flight.acceleration[1].to_bits(), 0);
    assert!(flight.acceleration[2] > 0.0);
}

#[test]
fn positive_minimum_flight_angle_moves_without_stopping_to_rotate() {
    let (mut game, id) = setup(Appearance::Wings);
    let index = game.entities[id].type_index;
    game.catalog[index]
        .locomotor
        .flight
        .as_mut()
        .unwrap()
        .min_speed = 1.0;
    game.entities[id].ai.motion.request_angle(0.3);
    game.update_flight(id);
    let flight = game.entities[id].flight.as_ref().unwrap();
    assert!(game.entities[id].facing > 0.0);
    assert!(flight.acceleration[0] > 0.0);
    assert!(flight.goal.is_none() && flight.maintain.is_none());
    assert!(game.entities[id].ai.motion.angle_goal().is_some());
}

#[test]
fn flight_angle_minimum_speed_is_limited_by_current_maximum() {
    let (mut game, id) = setup(Appearance::Wings);
    let index = game.entities[id].type_index;
    game.catalog[index]
        .locomotor
        .flight
        .as_mut()
        .unwrap()
        .min_speed = 5.0;
    game.entities[id].flight.as_mut().unwrap().speed_limit = 1.0;
    game.entities[id].ai.motion.velocity = [2.0, 0.0];
    game.entities[id].ai.motion.request_angle(0.0);
    game.update_flight(id);
    // Native desired-speed clamping decelerates the object already above its
    // current cap; requesting the uncapped minimum would accelerate it instead.
    assert!(game.entities[id].flight.as_ref().unwrap().acceleration[0] < 0.0);
}

#[test]
fn flight_angle_uses_native_pivot_and_disables_it_while_braking() {
    for braking in [false, true] {
        let (mut game, id) = setup(Appearance::Hover);
        let index = game.entities[id].type_index;
        game.catalog[index].locomotor.turn_pivot_offset = 0.5;
        game.entities[id]
            .flight
            .as_mut()
            .unwrap()
            .set(super::BRAKING, braking);
        let position = game.entities[id].position;
        game.entities[id].ai.motion.request_angle(0.5);
        game.update_flight(id);
        assert!(game.entities[id].facing > 0.0);
        let moved = game.entities[id].position.map(f32::to_bits) != position.map(f32::to_bits);
        assert_eq!(moved, !braking);
        let state = game.entities[id].flight.as_ref().unwrap();
        assert_eq!(state.acceleration[0].to_bits(), 0);
        assert_eq!(state.acceleration[1].to_bits(), 0);
    }
}
