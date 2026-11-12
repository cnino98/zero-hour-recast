use super::{Mount, Phase, Rules, State, launch};
use crate::skirmish::{Kind, LaunchTransform, Order, tests::game};
use std::f32::consts::{FRAC_PI_2, PI};

#[test]
fn turning_crosses_wrap_using_shortest_arc_and_source_alignment_threshold() {
    let mut turret = State::new(None);
    turret.angle = PI - 0.1;
    assert!(!turret.turn(-PI + 0.1, 0.05, 0.035));
    for _ in 0..4 {
        turret.turn(-PI + 0.1, 0.05, 0.035);
    }
    assert!((turret.angle - (-PI + 0.1)).abs() < 0.000_001);
    assert!(turret.turn(-PI + 0.1, 0.05, 0.035));
    assert!(!turret.rotating);
}

#[test]
fn exact_rate_step_retains_rotate_condition_until_following_snap() {
    let mut turret = State::new(None);
    assert!(turret.turn(0.5, 0.5, 0.0));
    assert!(turret.rotating);
    assert!(turret.turn(0.5, 0.5, 0.0));
    assert!(!turret.rotating);
}

#[test]
fn attack_waits_for_turret_without_turning_chassis_and_stop_recenters_half_speed() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut rules = Rules::source_defaults();
    rules.turn_rate = 0.1;
    rules.recenter_frames = 3;
    rules.controlled_slots = 1;
    game.catalog[kind].turret = Some(rules);
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    game.entities[actor].facing = 0.0;
    let target_type = game.type_for(1, Kind::Barracks).unwrap();
    let target = game.spawn(target_type, 1, [250.0, 320.0], true);
    game.set_order(actor, Order::Attack(target));
    for tick in 1..17 {
        game.tick = tick;
        game.update_ai_module(actor);
        assert!(game.presentation.shots.is_empty());
        assert_eq!(game.entities[actor].facing.to_bits(), 0.0_f32.to_bits());
    }
    game.tick = 17;
    game.update_ai_module(actor);
    assert_eq!(game.presentation.shots.len(), 1);
    let aimed = game.entities[actor].ai.turret.angle;
    assert!((aimed - FRAC_PI_2).abs() < 0.035);
    game.set_order(actor, Order::Idle);
    for _ in 0..3 {
        game.tick += 1;
        game.update_idle_turret(actor);
    }
    assert_eq!(
        game.entities[actor].ai.turret.angle.to_bits(),
        aimed.to_bits()
    );
    game.tick += 1;
    game.update_idle_turret(actor);
    assert!((game.entities[actor].ai.turret.angle - (aimed - 0.05)).abs() < 0.000_001);
}

#[test]
fn fixed_turret_attack_turns_the_chassis_before_firing() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Treads,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.1,
        lateral_friction: 0.2,
        extra_friction: 0.0,
    });
    let mut rules = Rules::source_defaults();
    rules.controlled_slots = 1;
    rules.turn_rate = 0.0;
    game.catalog[kind].turret = Some(rules);
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    game.entities[actor].facing = 0.0;
    let target_type = game.type_for(1, Kind::Barracks).unwrap();
    let target = game.spawn(target_type, 1, [250.0, 320.0], true);
    game.set_order(actor, Order::Attack(target));
    game.tick = 1;
    game.update_ai_module(actor);
    assert!(game.presentation.shots.is_empty());
    assert!(game.entities[actor].facing > 0.0);
    assert!(matches!(game.entities[actor].ai.turret.phase, Phase::Aim));
    for tick in 2..60 {
        game.tick = tick;
        game.update_ai_module(actor);
        if !game.presentation.shots.is_empty() {
            break;
        }
    }
    assert_eq!(game.presentation.shots.len(), 1);
    assert!((game.entities[actor].facing - FRAC_PI_2).abs() < 0.035);
    assert_eq!(game.entities[actor].ai.turret.angle.to_bits(), 0);
}

#[test]
fn an_unmounted_current_slot_does_not_retarget_an_unrelated_turret() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut rules = Rules::source_defaults();
    rules.controlled_slots = 1;
    game.catalog[kind].turret = Some(rules);
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    game.entities[actor].weapon_set.current = 2;
    game.entities[actor].ai.turret.opportunity = Some((0, 0));
    game.set_attack_turret_targets(
        actor,
        crate::skirmish::combat::attack::Goal::Position([250.0, 320.0, 0.0]),
        false,
    );
    let turret = &game.entities[actor].ai.turret;
    assert_eq!(turret.opportunity, Some((0, 0)));
    assert!(turret.updated_at.is_none());
    assert!(matches!(turret.phase, Phase::Idle(_)));
}

#[test]
fn pitch_requires_alignment_and_uses_target_origin_minus_half_owner_height() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut rules = Rules::source_defaults();
    rules.turn_rate = 1.0;
    rules.controlled_slots = 1;
    rules.pitch_rate = 0.1;
    rules.allows_pitch = true;
    rules.natural_pitch = FRAC_PI_2;
    rules.minimum_pitch = -0.5;
    game.catalog[kind].geometry_bounds.max_height = 20.0;
    game.catalog[kind].turret = Some(rules);
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    game.entities[actor].facing = 0.0;
    let target_type = game.type_for(1, Kind::Barracks).unwrap();
    let target = game.spawn(target_type, 1, [300.0, 250.0], true);
    assert!(!game.rotate_native_turret(actor, 0, game.object_position(target)));
    for _ in 0..30 {
        game.tick += 1;
        game.rotate_native_turret(actor, 0, game.object_position(target));
    }
    let expected = (-10.0_f32 / 50.0_f32.hypot(10.0)).asin();
    assert!((game.entities[actor].ai.turret.pitch - expected).abs() < 0.000_001);
    assert!(game.rotate_native_turret(actor, 0, game.object_position(target)));
}

#[test]
fn projectile_launch_rotates_about_authored_pitch_then_yaw_mounts() {
    let mut state = State::new(None);
    state.angle = FRAC_PI_2;
    state.pitch = FRAC_PI_2;
    let mount = Mount {
        yaw: [1.0, 0.0, 2.0],
        pitch: [2.0, 0.0, 2.0],
    };
    let mut launch_point = LaunchTransform {
        position: [4.0, 0.0, 2.0],
        axes: LaunchTransform::IDENTITY.axes,
    };
    launch(&mut launch_point, &mount, &state);
    for (actual, expected) in launch_point.position.into_iter().zip([1.0, 1.0, 4.0]) {
        assert!((actual - expected).abs() < 0.000_001);
    }
    assert!((launch_point.axes[0][2] - 1.0).abs() < 0.000_001);
}

#[test]
fn idle_scan_has_source_half_speed_then_hold_and_recenter() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut rules = Rules::source_defaults();
    rules.turn_rate = 0.2;
    rules.idle_angles = [0.3; 2];
    rules.idle_intervals = [0; 2];
    rules.recenter_frames = 2;
    game.catalog[kind].turret = Some(rules);
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    game.tick += 1;
    game.update_idle_turret(actor);
    assert!(matches!(
        game.entities[actor].ai.turret.phase,
        Phase::Scan(_)
    ));
    game.tick += 1;
    game.update_idle_turret(actor);
    assert!((game.entities[actor].ai.turret.angle.abs() - 0.1).abs() < 0.000_001);
    for _ in 0..20 {
        game.tick += 1;
        game.update_idle_turret(actor);
    }
    assert!(game.entities[actor].ai.turret.angle.is_finite());
}

#[test]
fn sweep_admits_initial_fire_inside_its_strict_boundary_before_sweeping_is_enabled() {
    let mut rules = Rules::source_defaults();
    rules.turn_rate = 0.0;
    rules.fire_sweep[0] = 0.5;
    let mut turret = State::new(Some(&rules));
    turret.angle = 0.5;
    assert!(!turret.aim_yaw(0.0, &rules, 0, 1));
    turret.angle = 0.499;
    assert!(turret.aim_yaw(0.0, &rules, 0, 1));
    assert!(turret.sweep.positive);
    assert_eq!(turret.sweep.until, 0);
}

#[test]
fn sweep_uses_slot_speed_and_expires_exactly_three_frames_after_the_latest_shot() {
    let mut rules = Rules::source_defaults();
    rules.turn_rate = 0.1;
    rules.fire_sweep[1] = 0.5;
    rules.sweep_speed[1] = 0.5;
    let mut turret = State::new(Some(&rules));
    assert!(turret.aim_yaw(0.0, &rules, 1, 10));
    // Alignment flips direction even when sweep was not enabled yet.
    assert!(!turret.sweep.positive);
    turret.fired(10);
    assert!(turret.aim_yaw(0.0, &rules, 1, 11));
    assert_eq!(turret.angle.to_bits(), (-0.05_f32).to_bits());
    assert!(turret.aim_yaw(0.0, &rules, 1, 12));
    assert_eq!(turret.angle.to_bits(), (-0.1_f32).to_bits());
    assert!(turret.aim_yaw(0.0, &rules, 1, 13));
    assert_eq!(turret.angle.to_bits(), 0);
    assert!(turret.sweep.positive);
    turret.fired(13);
    turret.fired(14);
    assert!(turret.aim_yaw(0.0, &rules, 1, 16));
    assert_eq!(turret.angle.to_bits(), 0.05_f32.to_bits());
    assert!(turret.aim_yaw(0.0, &rules, 1, 17));
    assert_eq!(turret.angle.to_bits(), 0);
}

#[test]
fn sweep_reverses_at_each_endpoint_and_zero_sweep_slots_keep_ordinary_alignment() {
    let mut rules = Rules::source_defaults();
    rules.turn_rate = 0.25;
    rules.fire_sweep[0] = 0.5;
    let mut turret = State::new(Some(&rules));
    turret.fired(1);
    assert!(turret.aim_yaw(0.0, &rules, 0, 2));
    assert_eq!(turret.angle.to_bits(), 0.25_f32.to_bits());
    assert!(!turret.aim_yaw(0.0, &rules, 0, 3));
    assert!(!turret.sweep.positive);
    turret.fired(3);
    for frame in 4..7 {
        assert!(turret.aim_yaw(0.0, &rules, 0, frame));
        turret.fired(frame);
    }
    assert!(!turret.aim_yaw(0.0, &rules, 0, 7));
    assert!(turret.sweep.positive);
    assert!(!turret.aim_yaw(0.0, &rules, 2, 8));
    assert_eq!(turret.angle.to_bits(), (-0.25_f32).to_bits());
    assert!(turret.sweep.positive);
}

#[test]
fn aiming_uses_current_physical_weapon_slot_sweep_and_original_target_pitch() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut rules = Rules::source_defaults();
    rules.turn_rate = 0.1;
    rules.controlled_slots = 2;
    rules.fire_sweep[1] = 0.5;
    rules.sweep_speed[1] = 0.5;
    rules.allows_pitch = true;
    rules.pitch_rate = 1.0;
    rules.minimum_pitch = -1.0;
    game.catalog[kind].turret = Some(rules);
    game.catalog[kind].secondary_weapon = Some(
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Tank).weapon,
    );
    let id = game.spawn(kind, 0, [250.0, 250.0], true);
    game.entities[id].facing = 0.0;
    game.entities[id].weapon_set.current = 1;
    game.entities[id].ai.turret.fired(10);
    game.tick = 11;
    assert!(game.rotate_native_turret(id, 0, [300.0, 250.0, 0.0]));
    let actor = &game.entities[id];
    assert_eq!(actor.ai.turret.angle.to_bits(), 0.05_f32.to_bits());
    assert!(actor.ai.turret.pitch < 0.0);
    assert_eq!(actor.facing.to_bits(), 0);
}

#[test]
fn transfer_retargets_both_linked_turrets_without_reentering_aim() {
    use crate::skirmish::combat::attack::Goal;
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut primary = Rules::source_defaults();
    primary.controlled_slots = 1;
    let mut secondary = Rules::source_defaults();
    secondary.controlled_slots = 2;
    game.catalog[kind].turret = Some(primary);
    game.catalog[kind].secondary_turret = Some(secondary);
    game.catalog[kind].turrets_linked = true;
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    let old = game.spawn(kind, 1, [300.0, 250.0], true);
    let next = game.spawn(kind, 1, [310.0, 250.0], true);
    game.set_attack_turret_targets(actor, Goal::Object(old), false);
    let deadline = 123;
    game.entities[actor].ai.turret.sleep_until = deadline;
    game.entities[actor]
        .ai
        .secondary_turret
        .as_mut()
        .unwrap()
        .sleep_until = deadline;
    game.transfer_native_attack(actor, old, next);
    for turret in [
        &game.entities[actor].ai.turret,
        game.entities[actor].ai.secondary_turret.as_ref().unwrap(),
    ] {
        assert!(matches!(turret.target, Some(Goal::Object(target)) if target == next));
        assert!(matches!(turret.phase, Phase::Aim));
        assert!(turret.forced);
        assert_eq!(turret.sleep_until, deadline);
    }
}
