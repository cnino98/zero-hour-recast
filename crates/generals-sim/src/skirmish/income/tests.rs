use super::*;
use crate::skirmish::Kind;

mod capture;
mod contained;

pub(in crate::skirmish) fn bind_hacker(unit: &mut crate::skirmish::UnitType) {
    for module in &mut unit.behavior_modules {
        if module.family == "AIUpdateInterface" {
            module.family = "HackInternetAIUpdate".into();
        }
    }
    for module in &mut unit.update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } = module
            && family == "AIUpdateInterface"
        {
            *family = "HackInternetAIUpdate".into();
        }
    }
}

fn hacker() -> (Skirmish, usize) {
    let mut game = crate::skirmish::tests::game();
    let index = game.type_for(0, Kind::Infantry).unwrap();
    let unit = &mut game.catalog[index];
    unit.weapon.damage = 0.0;
    unit.behavior.hacking = Some(HackerRules {
        unpack: 2,
        pack: 2,
        variation: 0.0,
        delay: 2,
        contained_delay: 1,
        amounts: [5, 6, 8, 10],
        experience: 1,
    });
    bind_hacker(unit);
    unit.behavior
        .destroy_die
        .push(crate::skirmish::tests::death_conditions());
    crate::skirmish::tests::register_behavior(unit, "DestroyDie", 2);
    unit.veterancy.trainable = true;
    unit.veterancy.required = [0, 1, 2, 3];
    let id = game.spawn(index, 0, [350.0, 200.0], true);
    (game, id)
}

#[test]
fn original_counter_waits_one_more_update_then_deposits_before_promoting() {
    let (mut game, id) = hacker();
    let cash = game.players[0].cash;
    game.hack_internet(id).unwrap();
    assert_eq!(game.entities[id].hacking_condition(), Some("UNPACKING"));
    assert_eq!(game.entities[id].animation_duration(), Some((0, 2, false)));
    for _ in 0..2 {
        game.step();
    }
    assert_eq!(game.entities[id].hacking_condition(), Some("UNPACKING"));
    game.step();
    assert_eq!(game.entities[id].hacking_condition(), Some("FIRING_A"));
    assert!(game.entities[id].animation_duration().is_none());
    for _ in 0..2 {
        game.step();
    }
    assert_eq!(game.players[0].cash, cash);
    game.step();
    assert_eq!(game.players[0].cash, cash + 5);
    assert_eq!(game.players[0].statistics.money_earned, 5);
    assert_eq!(game.entities[id].veterancy, 1);
    for amount in [6, 8, 10] {
        let before = game.players[0].cash;
        for _ in 0..3 {
            game.step();
        }
        assert_eq!(game.players[0].cash, before + amount);
    }
    assert_eq!(game.entities[id].experience, 4);
}

#[test]
fn repeat_hack_preserves_source_rng_and_timers_but_unpack_can_be_cancelled() {
    let (mut game, id) = hacker();
    game.catalog[game.entities[id].type_index]
        .behavior
        .hacking
        .as_mut()
        .unwrap()
        .variation = 0.5;
    game.hack_internet(id).unwrap();
    let random = game.random;
    let duration = game.entities[id].animation_duration();
    game.hack_internet(id).unwrap();
    assert_eq!(game.random, random);
    assert_eq!(game.entities[id].animation_duration(), duration);
    game.stop(id).unwrap();
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert_eq!(game.entities[id].hacking_condition(), None);
    assert!(game.entities[id].animation_duration().is_none());
}

#[test]
fn newest_pending_move_waits_for_pack_and_stop_replaces_it_without_income() {
    let (mut game, id) = hacker();
    game.hack_internet(id).unwrap();
    for _ in 0..3 {
        game.step();
    }
    game.hack_internet(id).unwrap();
    assert_eq!(game.entities[id].hacking.remaining, 2);
    let origin = game.entities[id].position;
    game.move_to(id, [450.0, 200.0]).unwrap();
    assert_eq!(game.entities[id].hacking_condition(), Some("PACKING"));
    game.move_to(id, [500.0, 200.0]).unwrap();
    game.step();
    game.stop(id).unwrap();
    for _ in 0..3 {
        game.step();
    }
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        origin.map(f32::to_bits)
    );
    assert_eq!(game.players[0].statistics.money_earned, 0);
    game.hack_internet(id).unwrap();
    for _ in 0..3 {
        game.step();
    }
    game.attack_move(id, [450.0, 200.0]).unwrap();
    for _ in 0..3 {
        game.step();
    }
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        origin.map(f32::to_bits)
    );
    assert!(!game.entities[id].ai_idle());
    game.step();
    assert!(matches!(game.entities[id].ai.order, Order::AttackMove(_)));
}

#[test]
fn dead_hacker_never_deposits_and_missing_veteran_amount_falls_back() {
    let (mut game, id) = hacker();
    game.catalog[game.entities[id].type_index]
        .behavior
        .hacking
        .as_mut()
        .unwrap()
        .amounts = [5, 0, 0, 0];
    game.entities[id].veterancy = 3;
    game.hack_internet(id).unwrap();
    for _ in 0..6 {
        game.step();
    }
    assert_eq!(game.players[0].statistics.money_earned, 5);
    game.apply_native_damage(id, None, 10_000.0, crate::skirmish::DeathType::Normal);
    for _ in 0..12 {
        game.step();
    }
    assert_eq!(game.players[0].statistics.money_earned, 5);
    assert!(game.hack_internet(id).is_err());
}

#[test]
fn move_aside_command_waits_for_hacker_packing_and_reissues_after_idle() {
    let (mut game, id) = hacker();
    let index = game.entities[id].type_index;
    game.catalog[index].locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    let other = game.spawn(index, 0, [300.0, 200.0], true);
    game.entities[other].ai.ground_path = Some(crate::skirmish::path_following::Path::new(
        [[300.0, 200.0, 0.0], [500.0, 200.0, 0.0]]
            .into_iter()
            .map(|position| crate::skirmish::path_following::Node {
                position,
                layer: 1,
                previous_layer: 1,
                can_optimize: false,
            })
            .collect(),
    ));
    game.hack_internet(id).unwrap();
    for _ in 0..3 {
        game.step();
    }
    assert_eq!(game.entities[id].hacking_condition(), Some("FIRING_A"));
    game.move_away_from(id, other, 1);
    assert!(game.entities[id].ai.blocking.temporary_goal().is_none());
    assert!(
        matches!(game.entities[id].hacking.pending, Some(Command { source: COMMAND_FROM_AI, kind: CommandKind::MoveAside { other: target, depth: 1 } }) if target == other)
    );
    assert_eq!(game.entities[id].hacking_condition(), Some("PACKING"));
    for _ in 0..3 {
        game.update_hacking(id);
    }
    assert!(matches!(game.entities[id].hacking.phase, Phase::Idle));
    game.dispatch_hacker_pending(id);
    assert!(game.entities[id].hacking.pending.is_none());
    assert!(game.entities[id].ai.blocking.temporary_goal().is_some());
}

#[test]
fn script_face_replays_full_xyz_and_source_after_pack_finishes() {
    let (mut game, id) = hacker();
    game.hack_internet(id).unwrap();
    for _ in 0..3 {
        game.step();
    }
    let goal = [250.0, 300.0, 37.0];
    game.native_face_from(
        id,
        crate::skirmish::combat::attack::Goal::Position(goal),
        crate::skirmish::ai_goals::COMMAND_FROM_SCRIPT,
    );
    assert_eq!(game.entities[id].hacking_condition(), Some("PACKING"));
    assert_eq!(
        game.entities[id].ai.command.last_command_source,
        COMMAND_FROM_AI
    );
    for _ in 0..3 {
        game.step();
    }
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(!game.entities[id].ai_idle());
    game.step();
    assert!(matches!(
        game.entities[id].ai.command.main_state,
        MainState::FacePosition { .. }
    ));
    assert_eq!(
        game.entities[id].ai.command.last_command_source,
        crate::skirmish::ai_goals::COMMAND_FROM_SCRIPT
    );
    assert_eq!(
        game.entities[id].ai_goal_position().map(f32::to_bits),
        goal.map(f32::to_bits)
    );
    assert!(game.entities[id].hacking.pending.is_none());
}

#[test]
fn hacked_income_state_retains_counter_until_disabled_deadline_clears() {
    let (mut game, id) = hacker();
    game.hack_internet(id).unwrap();
    for _ in 0..3 {
        game.step();
    }
    let remaining = game.entities[id].hacking.remaining;
    let cash = game.players[0].cash;
    game.set_hacked_until(id, game.update_frame() + 20);
    for _ in 0..10 {
        game.step();
    }
    assert!(game.entities[id].hacked());
    assert_eq!(game.entities[id].hacking.remaining, remaining);
    assert_eq!(game.players[0].cash, cash);
    assert_eq!(game.entities[id].hacking_condition(), Some("FIRING_A"));
}

#[test]
fn hidden_hacker_payment_keeps_emission_visibility_after_reveal() {
    let (mut game, old) = hacker();
    let index = game.entities[old].type_index;
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    game.catalog[index].behavior.stealth = Some(crate::skirmish::stealth::Rules {
        module_order: 0,
        delay: 1,
        forbidden: 0,
        threshold_speed: 0.0,
        required_status: vec![],
        forbidden_status: vec![],
        hint_status: vec![],
        innate: true,
        use_rider: false,
        idle_enemies_on_reveal: false,
        wake: crate::skirmish::stealth::Wake::Automatic,
        upgrades: vec![],
        reveal_distance: 0.0,
        black_market_frames: 0,
        friendly_opacity: [0.5, 1.0],
        pulse_frames: 30,
        disguise: None,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[index], "StealthUpdate");
    let id = game.spawn(index, 1, [350.0, 200.0], true);
    game.native_hack_internet_from(id, COMMAND_FROM_AI);
    for _ in 0..6 {
        game.step();
    }
    assert!(game.entities[id].stealthed());
    assert!(!game.logically_visible(0, id));
    let event = game
        .presentation
        .hacking_events
        .iter()
        .find(|event| event.object == id && matches!(event.effect, Effect::Cash { .. }))
        .expect("scheduled hidden payment");
    let Effect::Cash {
        visible_players, ..
    } = event.effect
    else {
        unreachable!()
    };
    assert_eq!(visible_players & 1, 0);
    assert_ne!(visible_players & 2, 0);
    game.mark_detected(id, 10);
    game.update_stealth_module(id);
    assert!(game.logically_visible(0, id));
    let event = game
        .presentation
        .hacking_events
        .iter()
        .find(|event| event.object == id && matches!(event.effect, Effect::Cash { .. }))
        .unwrap();
    let Effect::Cash {
        visible_players, ..
    } = event.effect
    else {
        unreachable!()
    };
    assert_eq!(
        visible_players & 1,
        0,
        "later reveal cannot expose earlier cash text"
    );
}

#[test]
fn unpack_update_restores_flag_after_special_ability_cleanup() {
    let (mut game, id) = hacker();
    game.hack_internet(id).unwrap();
    game.ability_model_flags(id, 0, (1 << 36) | (1 << 73) | (1 << 74));
    assert!(
        !game.entities[id]
            .drawable
            .model_conditions
            .has_flag(1 << 74)
    );
    game.step();
    assert!(
        game.entities[id]
            .drawable
            .model_conditions
            .has_flag(1 << 74)
    );
}
