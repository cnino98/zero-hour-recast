use super::{
    GroundAppearance, GroundGoal, GroundRules, forward_speed, integrate_ground, maintain_ground,
    motive_force, move_ground,
};
use crate::skirmish::{Kind, Skirmish};

fn setup_catalog(appearance: GroundAppearance) -> (Skirmish, usize) {
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].locomotor.ground = Some(GroundRules {
        wander: None,
        appearance,
        is_dozer: false,
        downhill_only: false,
        acceleration: 15.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        acceleration_damaged: 15.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        braking: 50.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    (game, tank)
}

fn setup(appearance: GroundAppearance) -> (Skirmish, usize) {
    let (mut game, tank) = setup_catalog(appearance);
    let id = game.spawn(tank, 0, [100.0, 100.0], true);
    game.entities[id].facing = 0.0;
    (game, id)
}

fn goal(position: [f32; 2], distance: f32) -> GroundGoal {
    GroundGoal {
        height: 0.0,
        position,
        distance,
        desired_speed: 1.0,
        no_slow_down: false,
        ultra_accurate: false,
    }
}

#[test]
fn overlord_acceleration_is_integrated_after_the_locomotor_force() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    move_ground(
        actor,
        unit,
        &goal([500.0, 100.0], 400.0),
        50.0,
        1,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    assert_eq!(actor.position[0].to_bits(), 100.0_f32.to_bits());
    assert_eq!(actor.ai.motion.velocity[0].to_bits(), 0.0_f32.to_bits());
    integrate_ground(actor, unit, 1);
    assert!((actor.ai.motion.velocity[0] - 0.016_666_668).abs() < 0.000_000_01);
    let first_position = actor.position[0];
    move_ground(
        actor,
        unit,
        &goal([500.0, 100.0], 399.0),
        50.0,
        2,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    integrate_ground(actor, unit, 2);
    assert!(actor.position[0] > first_position);
    assert!((actor.ai.motion.velocity[0] - 0.033_333_335).abs() < 0.000_000_02);
}

#[test]
fn walking_accelerates_while_still_turning_toward_a_nearby_angle() {
    let (mut game, id) = setup(GroundAppearance::Legs);
    let unit = &mut game.catalog[game.entities[id].type_index];
    unit.locomotor.turn_rate = 500.0 * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0));
    unit.locomotor.ground.as_mut().unwrap().acceleration = 100.0 / 900.0;
    let actor = &mut game.entities[id];
    move_ground(
        actor,
        unit,
        &goal([273.205_08, 200.0], 200.0),
        50.0,
        1,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    integrate_ground(actor, unit, 1);
    assert!(actor.facing > 0.28 && actor.facing < 0.30);
    assert!(actor.ai.motion.velocity[0] > 0.10 && actor.ai.motion.velocity[1] > 0.03);
    assert!(actor.position[0] > 100.0 && actor.position[1] > 100.0);
}

#[test]
fn braking_uses_previous_status_then_minimum_position_step() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &mut game.catalog[game.entities[id].type_index];
    unit.locomotor.ground.as_mut().unwrap().braking = 1000.0 / 900.0;
    let actor = &mut game.entities[id];
    actor.ai.motion.velocity = [1.0, 0.0];
    move_ground(
        actor,
        unit,
        &goal([100.5, 100.0], 0.5),
        50.0,
        1,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    integrate_ground(actor, unit, 1);
    assert_eq!(actor.position[0].to_bits(), 100.0_f32.to_bits());
    assert_eq!(actor.ai.motion.velocity[0].to_bits(), 0.0_f32.to_bits());
    move_ground(
        actor,
        unit,
        &goal([100.5, 100.0], 0.5),
        50.0,
        2,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    integrate_ground(actor, unit, 2);
    assert!((actor.position[0] - 100.333_336).abs() < 0.000_01);
    let remaining = 100.5 - actor.position[0];
    move_ground(
        actor,
        unit,
        &goal([100.5, 100.0], remaining),
        50.0,
        3,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    integrate_ground(actor, unit, 3);
    assert_eq!(actor.position[0].to_bits(), 100.5_f32.to_bits());
}

#[test]
fn driven_stop_scrubs_velocity_and_does_not_coast() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    move_ground(
        actor,
        unit,
        &goal([500.0, 100.0], 400.0),
        50.0,
        1,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    integrate_ground(actor, unit, 1);
    let position = actor.position;
    maintain_ground(actor, 2);
    integrate_ground(actor, unit, 2);
    assert_eq!(
        actor.ai.motion.velocity.map(f32::to_bits),
        [0.0_f32.to_bits(); 2]
    );
    assert_eq!(actor.position.map(f32::to_bits), position.map(f32::to_bits));
}

#[test]
fn forward_friction_begins_when_the_ten_frame_motive_window_expires() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    let rules = unit.locomotor.ground.as_ref().unwrap();
    motive_force(actor, rules.mass, [0.0; 2], 0);
    actor.ai.motion.velocity = [2.0, 1.0];
    integrate_ground(actor, unit, 1);
    assert_eq!(actor.ai.motion.velocity[0].to_bits(), 2.0_f32.to_bits());
    assert!((actor.ai.motion.velocity[1] - 0.85).abs() < 0.000_001);
    actor.ai.motion.velocity = [2.0, 1.0];
    integrate_ground(actor, unit, 10);
    assert!((actor.ai.motion.velocity[0] - 1.7).abs() < 0.000_001);
    assert!((actor.ai.motion.velocity[1] - 0.85).abs() < 0.000_001);
}

#[test]
fn source_forward_speed_preserves_its_non_dot_product_diagonal_result() {
    let (mut game, id) = setup(GroundAppearance::Legs);
    let actor = &mut game.entities[id];
    actor.facing = std::f32::consts::FRAC_PI_4;
    actor.ai.motion.velocity = [1.0, 1.0];
    assert!((forward_speed(actor) - 1.0).abs() < 0.000_001);
    actor.ai.motion.velocity = [-1.0, -1.0];
    assert!((forward_speed(actor) + 1.0).abs() < 0.000_001);
}

#[test]
fn aligned_attacker_stops_driven_motion_while_firing_and_reloading() {
    use crate::skirmish::physical_ocl::{CollisionRules, LiveRules, MotionRules};

    let (mut game, tank) = setup_catalog(GroundAppearance::Treads);
    // INITIAL AI must scrub the moving body's velocity before NORMAL Physics.
    game.catalog[tank].live_physics = Some(LiveRules {
        motion: MotionRules {
            center_of_mass_offset: 0.0,
            mass: 50.0,
            gravity: -0.1,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 1.0,
        kill_when_resting: false,
        collision: CollisionRules {
            stiffness: 0.5,
            rubble_height: 0.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.0; 3],
        min_fall_speed: 99.0,
        fall_damage_factor: 1.0,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[tank], "PhysicsBehavior");
    let id = game.spawn(tank, 0, [100.0, 100.0], true);
    game.entities[id].ai.mood.next_check = u64::MAX;
    game.move_to(id, [500.0, 100.0]).unwrap();
    for _ in 0..10 {
        game.step();
    }
    assert!(game.entities[id].ai.motion.velocity[0] > 0.0);
    let enemy_type = game.type_for(1, Kind::Tank).unwrap();
    let enemy = game.spawn(enemy_type, 1, [220.0, 100.0], true);
    game.entities[enemy].ai.mood.next_check = u64::MAX;
    let stopped = game.entities[id].position;
    game.attack(id, enemy).unwrap();
    game.step();
    assert!(game.presentation.shots.is_empty());
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        stopped.map(f32::to_bits)
    );
    game.step();
    assert!(game.presentation.shots.iter().any(|shot| shot.source == id));
    assert_eq!(
        game.entities[id].ai.motion.velocity.map(f32::to_bits),
        [0; 2]
    );
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        stopped.map(f32::to_bits)
    );
    assert_ne!(game.entities[id].ai.motion.command_frame, Some(game.tick));
    for _ in 0..5 {
        game.step();
    }
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        stopped.map(f32::to_bits)
    );
}

#[test]
fn exact_aim_tolerance_neither_rotates_nor_fires() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    // Initial map Idle snaps to the claimed cell during frames zero and one.
    game.tick = 10;
    let type_index = game.entities[id].type_index;
    game.catalog[type_index].weapon.aim_delta = std::f32::consts::FRAC_PI_2;
    let enemy_type = game.type_for(1, Kind::Tank).unwrap();
    let enemy = game.spawn(enemy_type, 1, [100.0, 200.0], true);
    game.entities[enemy].ai.mood.next_check = u64::MAX;
    game.attack(id, enemy).unwrap();
    game.step();
    assert!(game.presentation.shots.is_empty());
    assert_eq!(game.entities[id].facing.to_bits(), 0.0_f32.to_bits());
    assert_ne!(game.entities[id].ai.motion.command_frame, Some(game.tick));
    game.catalog[type_index].weapon.aim_delta =
        f32::from_bits(std::f32::consts::FRAC_PI_2.to_bits() - 1);
    game.step();
    assert!(game.presentation.shots.is_empty());
    assert!(game.entities[id].facing > 0.0);
    assert_eq!(game.entities[id].ai.motion.command_frame, Some(game.tick));
}

#[test]
fn invalid_terrain_applies_native_escape_force_before_the_move_goal() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    actor.position = [-1.0, 100.0];
    actor.ai.motion.velocity = [-4.0, 0.0];
    let mut probes = Vec::new();
    move_ground(
        actor,
        unit,
        &goal([-100.0, 100.0], 99.0),
        50.0,
        1,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |point| {
            probes.push(point);
            point[0] >= 0.0
        },
    );
    assert_eq!(probes.len(), 10);
    assert_eq!(
        probes[1].map(f32::to_bits),
        [(-11.0_f32).to_bits(), 90.0_f32.to_bits()]
    );
    assert_eq!(
        probes[9].map(f32::to_bits),
        [9.0_f32.to_bits(), 110.0_f32.to_bits()]
    );
    assert_eq!(actor.facing.to_bits(), 0.0_f32.to_bits());
    // The native square-root counter-impulse reduces -4 to -2, then the
    // three invalid cells on the left add .6 world units/frame of velocity.
    integrate_ground(actor, unit, 1);
    assert!((actor.ai.motion.velocity[0] + 1.4).abs() < 0.000_001);
    assert!((actor.position[0] + 2.4).abs() < 0.000_001);
}

#[test]
fn terrain_escape_preserves_dozer_exception_and_strict_outgoing_threshold() {
    use super::terrain_correction::fix_invalid_position;
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &mut game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    actor.position = [-1.0, 100.0];
    let rules = unit.locomotor.ground.as_mut().unwrap();
    rules.is_dozer = true;
    let mut probes = 0;
    assert!(!fix_invalid_position(
        actor,
        rules,
        rules.mass,
        1,
        &mut |_| {
            probes += 1;
            false
        }
    ));
    assert_eq!(probes, 0);
    rules.is_dozer = false;
    actor.ai.motion.velocity = [f32::from_bits(0.25_f32.to_bits() + 1), 0.0];
    assert!(!fix_invalid_position(
        actor,
        rules,
        rules.mass,
        1,
        &mut |point| point[0] >= 0.0
    ));
    assert_eq!(actor.ai.motion.acceleration.map(f32::to_bits), [0; 2]);
    actor.ai.motion.velocity = [0.25, 0.0];
    assert!(fix_invalid_position(
        actor,
        rules,
        rules.mass,
        1,
        &mut |point| point[0] >= 0.0
    ));
    assert!((actor.ai.motion.acceleration[0] - 0.6).abs() < 0.000_001);
}

#[test]
fn blocked_motion_scrubs_the_velocity_vector_and_still_turns_without_accelerating() {
    for appearance in [
        GroundAppearance::Legs,
        GroundAppearance::Treads,
        GroundAppearance::Wheels {
            min_turn_speed: 0.5,
            can_move_backwards: false,
        },
    ] {
        let (mut game, id) = setup(appearance);
        let unit = &mut game.catalog[game.entities[id].type_index];
        unit.locomotor.turn_rate = 0.1;
        let actor = &mut game.entities[id];
        actor.ai.motion.velocity = [3.0, 4.0];
        actor.ai.blocking.frames = 4;
        actor.ai.blocking.blocked = true;
        actor.ai.blocking.max_speed = 0.5;
        move_ground(
            actor,
            unit,
            &goal([100.0, 500.0], 400.0),
            50.0,
            1,
            &crate::skirmish::physical_ocl::transform::TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0,
            },
            |_| true,
        );

        // Extracted AIUpdate::doLocomotor's .95 cap followed by native
        // PhysicsBehavior::scrubVelocity2D produced these exact f32 bits.
        assert_eq!(
            actor.ai.motion.velocity.map(f32::to_bits),
            [1_049_750_405, 1_052_938_076]
        );
        assert_eq!(actor.ai.motion.acceleration.map(f32::to_bits), [0; 2]);
        assert_eq!(actor.position.map(f32::to_bits), [100.0_f32.to_bits(); 2]);
        assert_eq!(actor.facing.to_bits(), 0.1_f32.to_bits());
        // Locomotor.cpp:1050 keeps blocked true while it is still rotating.
        assert_eq!(actor.ai.blocking.frames, 5);
        assert_eq!(
            actor.ai.blocking.max_speed.to_bits(),
            999_999.0_f32.to_bits()
        );
    }
}

#[test]
fn repeated_contacts_decay_the_bump_limit_and_release_resumes_acceleration() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    actor.ai.motion.velocity = [1.0, 0.0];
    for (frame, expected) in [(1, 1_056_125_747), (2, 1_055_328_829)] {
        actor.ai.blocking.blocked = true;
        actor.ai.blocking.max_speed = 0.5;
        move_ground(
            actor,
            unit,
            &goal([500.0, 100.0], 400.0),
            50.0,
            frame,
            &crate::skirmish::physical_ocl::transform::TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0,
            },
            |_| true,
        );
        // Source cap values are .475 then .45124999, not the unchanged
        // blocker speed. Alignment clears blocked after the scrub.
        assert_eq!(actor.ai.motion.velocity[0].to_bits(), expected);
        assert_eq!(actor.ai.motion.acceleration.map(f32::to_bits), [0; 2]);
        assert_eq!(actor.ai.blocking.frames, 1);
        integrate_ground(actor, unit, frame);
    }
    let previous = actor.ai.motion.velocity[0];
    move_ground(
        actor,
        unit,
        &goal([500.0, 100.0], 400.0),
        50.0,
        3,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    integrate_ground(actor, unit, 3);
    assert_eq!(actor.ai.blocking.frames, 0);
    assert!(actor.ai.motion.velocity[0] > previous);
    assert!(actor.ai.motion.velocity[0] < 0.474);
}

#[test]
fn a_contact_cap_above_current_speed_allows_normal_acceleration() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    actor.ai.motion.velocity = [0.1, 0.0];
    actor.ai.blocking.frames = 4;
    actor.ai.blocking.blocked = true;
    actor.ai.blocking.max_speed = 0.5;
    move_ground(
        actor,
        unit,
        &goal([500.0, 100.0], 400.0),
        50.0,
        1,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    // Locomotor.cpp:1037 clears blocked when the capped requested speed
    // exceeds actual velocity; it must not freeze a slower following unit.
    assert_eq!(actor.ai.motion.velocity[0].to_bits(), 0.1_f32.to_bits());
    assert_eq!(actor.ai.blocking.frames, 1);
    integrate_ground(actor, unit, 1);
    assert!(actor.ai.motion.velocity[0] > 0.1 && actor.ai.motion.velocity[0] < 0.475);
}

#[test]
fn submillimeter_blocked_speed_stops_without_suppressing_the_turn() {
    let (mut game, id) = setup(GroundAppearance::Treads);
    let unit = &mut game.catalog[game.entities[id].type_index];
    unit.locomotor.turn_rate = 0.1;
    let actor = &mut game.entities[id];
    actor.ai.motion.velocity = [0.01, -0.02];
    actor.ai.blocking.blocked = true;
    actor.ai.blocking.max_speed = 0.0005;
    move_ground(
        actor,
        unit,
        &goal([100.0, 500.0], 400.0),
        50.0,
        1,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    assert_eq!(actor.ai.motion.velocity.map(f32::to_bits), [0; 2]);
    assert_eq!(actor.facing.to_bits(), 0.1_f32.to_bits());
    integrate_ground(actor, unit, 1);
    assert_eq!(actor.position.map(f32::to_bits), [100.0_f32.to_bits(); 2]);
}

#[test]
fn movement_force_uses_runtime_mass_with_native_binary32_rounding() {
    for appearance in [
        GroundAppearance::Legs,
        GroundAppearance::Treads,
        GroundAppearance::Climber,
        GroundAppearance::Wheels {
            min_turn_speed: 0.1,
            can_move_backwards: false,
        },
    ] {
        let (mut game, id) = setup(appearance);
        let unit = &game.catalog[game.entities[id].type_index];
        let actor = &mut game.entities[id];
        move_ground(
            actor,
            unit,
            &goal([500.0, 100.0], 400.0),
            7.0,
            1,
            &crate::skirmish::physical_ocl::transform::TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0,
            },
            |_| true,
        );
        // Native (mass * acceleration) * (1 / mass), with the actual loaded
        // mass 7, differs by one binary32 unit from authored empty mass 50.
        assert_eq!(actor.ai.motion.acceleration[0].to_bits(), 0x3c88_888b);
    }
}
