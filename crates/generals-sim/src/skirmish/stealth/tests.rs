use super::*;
use crate::skirmish::tests;
mod camo;
mod detector_feedback;

fn rules() -> Rules {
    Rules {
        module_order: 0,
        delay: 2,
        forbidden: MOVING,
        threshold_speed: 0.1,
        required_status: vec![],
        forbidden_status: vec![],
        hint_status: vec![],
        innate: true,
        use_rider: false,
        idle_enemies_on_reveal: false,
        wake: Wake::Automatic,
        upgrades: vec![],
        reveal_distance: 0.0,
        black_market_frames: 0,
        friendly_opacity: [0.5, 1.0],
        pulse_frames: 30,
    }
}

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    game.catalog[6].behavior.stealth = Some(rules());
    tests::register_update(&mut game.catalog[6], "StealthUpdate");
    let concealed = game.spawn(6, 1, [500.0, 300.0], true);
    let observer = game.spawn(6, 0, [400.0, 300.0], true);
    game.update_ground_contacts();
    (game, concealed, observer)
}

#[test]
fn motion_restarts_delay_and_hidden_objects_reject_enemy_weapons_but_remain_visible_to_allies() {
    let (mut game, target, observer) = fixture();
    game.tick = 2;
    game.run_object_updates();
    assert!(game.entities[target].stealthed());
    assert!(!game.logically_visible(0, target));
    assert!(game.logically_visible(1, target));
    assert!(!game.weapon_can_target(observer, target));
    game.entities[target].ai.motion.velocity = [0.11, 0.0];
    game.tick = 3;
    game.run_object_updates();
    assert!(!game.entities[target].stealthed());
    assert!(game.weapon_can_target(observer, target));
    game.entities[target].ai.motion.velocity = [0.1, 0.0];
    game.tick = 4;
    game.run_object_updates();
    assert!(!game.entities[target].stealthed());
    game.tick = 5;
    game.run_object_updates();
    assert!(game.entities[target].stealthed());
    game.defeated[0] = true;
    assert!(game.logically_visible(0, target));
}

#[test]
fn explicit_detector_duration_extends_but_default_reveal_overwrites_expiry() {
    let (mut game, target, observer) = fixture();
    game.tick = 2;
    game.run_object_updates();
    game.mark_detected(target, 10);
    assert!(!game.entities[target].detected());
    game.update_concealment(target);
    assert!(game.entities[target].detected());
    assert!(game.weapon_can_target(observer, target));
    game.tick = 3;
    game.mark_detected(target, 1);
    assert_eq!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .detected_until,
        12
    );
    game.mark_detected(target, 0);
    assert_eq!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .detected_until,
        5
    );
    game.tick = 5;
    game.update_concealment(target);
    assert!(!game.entities[target].detected());
}

#[test]
fn primary_only_forbidden_keeps_secondary_attack_hidden_and_checks_two_shot_frames() {
    let (mut game, _, observer) = fixture();
    game.catalog[6].behavior.stealth.as_mut().unwrap().forbidden = FIRING_PRIMARY;
    game.catalog[6].secondary_weapon = Some(
        tests::fixture_type(
            crate::skirmish::Faction::China,
            crate::skirmish::Kind::Infantry,
        )
        .weapon,
    );
    let target = game.spawn(6, 1, [450.0, 300.0], true);
    game.tick = 4;
    game.switch_weapon(target, 1).unwrap();
    game.attack(target, observer).unwrap();
    assert!(game.update_attack(target, observer));
    assert!(game.entities[target].attacking());
    game.fire_weapon(target, observer);
    assert_eq!(
        game.entities[target].weapon_set.weapons[1].last_shot,
        Some(4)
    );
    game.update_concealment(target);
    assert!(game.entities[target].stealthed());
    game.switch_weapon(target, 0).unwrap();
    game.fire_weapon(target, observer);
    assert_eq!(
        game.entities[target].weapon_set.weapons[0].last_shot,
        Some(4)
    );
    game.tick = 5;
    game.update_concealment(target);
    assert!(!game.entities[target].stealthed());
    game.tick = 6;
    game.update_concealment(target);
    assert!(!game.entities[target].stealthed());
    game.tick = 7;
    game.update_concealment(target);
    assert!(game.entities[target].stealthed());
}

#[test]
fn taking_damage_reads_the_retained_body_timestamp_including_source_priority() {
    let (mut game, target, observer) = fixture();
    game.catalog[6].behavior.stealth.as_mut().unwrap().forbidden = TAKING_DAMAGE;
    assert!(game.allowed_to_stealth(target));
    game.tick = 20;
    game.apply_hit(target, observer, 1.0);
    assert!(!game.allowed_to_stealth(target));
    game.tick = 21;
    let health = game.entities[target].health;
    game.apply_body_damage(
        target,
        None,
        1.0,
        crate::skirmish::DamageType::Water,
        crate::skirmish::DeathType::Normal,
        false,
    );
    assert!(game.entities[target].health < health);
    assert_eq!(game.entities[target].damage_state.retained_frame, 20);
    assert!(!game.allowed_to_stealth(target));
    game.tick = 22;
    game.apply_healing(target, None, 1.0);
    assert!(game.allowed_to_stealth(target));
    assert_eq!(game.entities[target].damage_state.retained_frame, 20);
}

#[test]
fn firing_conditions_use_native_zero_frame_wrap_and_allocated_weapon_slots() {
    let (mut game, target, observer) = fixture();
    game.catalog[6].behavior.stealth.as_mut().unwrap().forbidden = FIRING_PRIMARY;
    game.attack(target, observer).unwrap();
    assert!(game.update_attack(target, observer));
    assert!(game.entities[target].attacking());
    assert!(
        game.entities[target].weapon_set.weapons[0]
            .last_shot
            .is_none()
    );
    assert!(game.allowed_to_stealth(target));
    game.tick = 1;
    assert!(!game.allowed_to_stealth(target));
    game.catalog[6].behavior.stealth.as_mut().unwrap().forbidden = FIRING_TERTIARY;
    assert!(game.allocated_weapon(target, 2).is_none());
    assert!(game.allowed_to_stealth(target));
}

#[test]
fn detector_scan_uses_center_range_and_native_deferred_detection_and_disabled_wake() {
    let (mut game, target, _) = fixture();
    let mut unit = tests::fixture_type(crate::skirmish::Faction::Usa, crate::skirmish::Kind::Tank);
    unit.behavior.detectors.push(Detector {
        module_order: 1,
        rate: 15,
        range: 100.0,
        initially_disabled: true,
        required_kind: vec!["VEHICLE".into()],
        forbidden_kind: vec![],
        while_garrisoned: false,
        while_contained: false,
        feedback: None,
    });
    tests::register_update(&mut unit, "StealthDetectorUpdate");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let detector = game.spawn(kind, 0, [400.0, 300.0], true);
    game.update_ground_contacts();
    game.tick = 2;
    game.update_concealment(target);
    game.run_object_updates();
    assert_eq!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .detected_until,
        0
    );
    game.set_detection_enabled(detector, true);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .detected_until,
        19
    );
    assert!(!game.entities[target].detected());
    game.update_concealment(target);
    assert!(game.entities[target].detected());
    game.set_detection_enabled(detector, false);
    assert!(!game.entities[detector].behaviors.detection_enabled[0]);
    assert_eq!(
        game.update_remaining_sleep(
            detector,
            game.update_objects[game.entities[detector].update_owner]
                .handles
                .len()
                - 1
        ) + game.update_frame(),
        super::super::module_updates::FOREVER
    );
}

#[test]
fn temporary_grant_counts_actual_updates_and_player_command_revokes_it() {
    let (mut game, target, _) = fixture();
    game.receive_stealth_grant(target, false, 0);
    game.set_last_command_source(target, crate::skirmish::ai_goals::COMMAND_FROM_AI);
    game.receive_temporary_stealth(target, 3);
    assert!(game.entities[target].stealthed());
    game.update_concealment(target);
    assert!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .has(CAN_STEALTH)
    );
    game.tick = 100;
    game.update_concealment(target);
    assert!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .has(CAN_STEALTH)
    );
    game.update_concealment(target);
    assert!(
        !game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .has(CAN_STEALTH)
    );
    game.receive_temporary_stealth(target, 100);
    game.set_last_command_source(target, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
    game.update_concealment(target);
    assert!(
        !game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .has(CAN_STEALTH)
    );
}

#[test]
fn temporary_center_grant_preserves_permanent_stealth_and_permanent_upgrade_cancels_expiry() {
    let (mut game, target, _) = fixture();
    game.receive_temporary_stealth(target, 2);
    assert_eq!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .frames_granted,
        0
    );
    game.receive_stealth_grant(target, false, 0);
    game.receive_temporary_stealth(target, 2);
    game.grant_stealth(target);
    game.set_last_command_source(target, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
    game.update_concealment(target);
    assert!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .has(CAN_STEALTH)
    );
    assert_eq!(
        game.entities[target]
            .stealth
            .as_ref()
            .unwrap()
            .frames_granted,
        0
    );
}
