use super::*;
use crate::skirmish::{ConditionalWeapon, Prefire, WeaponRules, WeaponTemplateSet, tests};

mod location;

fn fixture() -> (Skirmish, usize) {
    let mut game = tests::game();
    let unit = &mut game.catalog[1];
    unit.ai.acquisition = 0;
    unit.locomotor.ground = Some(crate::skirmish::GroundRules {
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
        forward_friction: 0.1,
        lateral_friction: 0.2,
        extra_friction: 0.0,
    });
    unit.weapon = WeaponRules::unarmed();
    unit.weapon_set_rules.templates.push(ConditionalWeapon {
        rules: WeaponRules {
            name: "WorkerMineDisarmingWeapon".into(),
            damage: 1.0,
            damage_type: DamageType::Disarm,
            range: 5.0,
            anti_mask: 0x10,
            auto_choose: u8::MAX,
            pre_attack_frames: 3,
            pre_attack_type: Prefire::Shot,
            continue_attack_range: 100.0,
            ..WeaponRules::unarmed()
        },
        flags: 16,
        slot: 0,
    });
    unit.weapon_set_rules.replacements = vec![
        WeaponTemplateSet {
            flags: 0,
            weapons: [None; 3],
            share_lock: false,
        },
        WeaponTemplateSet {
            flags: 16,
            weapons: [Some(7), None, None],
            share_lock: false,
        },
    ];
    // Object's helper prefix depends on every possible authored weapon set,
    // including this builder's conditional DISARM weapon.
    unit.update_modules = tests::fixture_update_helpers(true);
    let family = match unit.ai.update {
        crate::skirmish::UnitAiUpdate::Worker => "WorkerAIUpdate",
        crate::skirmish::UnitAiUpdate::Dozer => "DozerAIUpdate",
        _ => "AIUpdateInterface",
    };
    unit.behavior_modules
        .retain(|module| module.family != family);
    tests::register_update(unit, family);
    game.catalog[5].kind_of = vec!["DEMOTRAP".into()];
    game.catalog[5].weapon = WeaponRules::unarmed();
    let builder = game.spawn(1, 0, [300.0, 300.0], true);
    (game, builder)
}

#[test]
fn real_repair_approach_retains_clearance_but_work_is_unarmed_until_idle() {
    let (mut game, builder) = fixture();
    assert_eq!(
        game.entities[builder].active_weapon(&game.catalog[1]).name,
        "WorkerMineDisarmingWeapon"
    );
    game.catalog[1].maintenance.repair_per_second = 0.02;
    let structure = game.spawn(0, 0, [400.0, 300.0], true);
    game.change_body_health(structure, 500.0 - game.entities[structure].health);
    game.update_ground_contacts();
    game.update_visibility();
    // Dozer::findTask compares task frames strictly against initial frame zero.
    // A user command follows the first GameLogic update, not object creation.
    game.step();
    game.repair(builder, structure).unwrap();
    assert!(
        game.weapon_available(builder, 0),
        "approaching a task retains the detail"
    );
    for _ in 0..600 {
        game.step();
        if game.entities[structure].health > 500.0 {
            break;
        }
    }
    assert!(game.entities[structure].health > 500.0);
    assert!(!game.weapon_available(builder, 0));
    assert!(
        game.entities[builder]
            .active_weapon(&game.catalog[1])
            .name
            .is_empty()
    );
    game.stop(builder).unwrap();
    game.step();
    assert!(game.weapon_available(builder, 0));
}

#[test]
fn disarm_continuation_keeps_original_center_owner_and_destroys_without_death_credit() {
    let (mut game, builder) = fixture();
    let first = game.spawn(5, 1, [310.0, 300.0], true);
    let next = game.spawn(5, 1, [350.0, 300.0], true);
    let outside = game.spawn(5, 1, [440.0, 300.0], true);
    let friendly = game.spawn(5, 0, [311.0, 300.0], true);
    game.update_ground_contacts();
    game.attack(builder, first).unwrap();
    let losses = game.players[1].statistics.units_lost;
    for _ in 0..800 {
        game.step();
        if game.entities[next].masked() {
            break;
        }
    }
    assert!(game.entities[first].masked());
    assert!(game.entities[next].masked());
    assert!(
        game.entities[outside].alive(),
        "range stays centered on the first target"
    );
    assert!(game.entities[friendly].alive());
    assert_eq!(game.players[1].statistics.units_lost, losses);
    assert_eq!(game.entities[builder].experience, 0);
    assert!(
        game.presentation.object_fx_events.is_empty(),
        "custom destruction omits OnDie"
    );
}

#[test]
fn bored_command_has_its_own_source_mask_and_one_shot_limit() {
    let (mut game, builder) = fixture();
    game.catalog[1].weapon_set_rules.templates[0]
        .rules
        .auto_choose = Source::Dozer.mask();
    let first = game.spawn(5, 1, [310.0, 300.0], true);
    let next = game.spawn(5, 1, [320.0, 300.0], true);
    game.update_ground_contacts();
    assert!(!game.choose_weapon_from(builder, first, Source::Player.mask()));
    assert!(!game.choose_weapon_from(builder, first, Source::Ai.mask()));
    game.bored_disarm(builder, 150.0);
    assert!(game.entities[builder].ai.attack_context.source == Source::Dozer);
    for _ in 0..30 {
        game.step();
    }
    assert!(game.entities[first].masked());
    assert!(game.entities[next].alive());
}

#[test]
fn native_prefire_modes_use_actual_consecutive_victim_and_partial_clip() {
    let (mut game, builder) = fixture();
    game.catalog[1].weapon_set_rules.templates[0]
        .rules
        .pre_attack_type = Prefire::Attack;
    game.tick = 1;
    game.prefire_native_weapon(builder, Some(10));
    assert_eq!(
        game.entities[builder].weapon_set.weapons[0]
            .pre_attack
            .as_ref()
            .unwrap()
            .frames,
        3
    );
    assert!(matches!(
        game.entities[builder].weapon_status(0, &game.catalog[1], 4),
        Some(crate::skirmish::ai_dispatch::WeaponStatus::Ready)
    ));
    game.exit_attack_fire(builder);
    game.entities[builder].firing.record_target(Some(10), 4);
    game.tick = 5;
    game.prefire_native_weapon(builder, Some(10));
    assert!(
        game.entities[builder].weapon_set.weapons[0]
            .pre_attack
            .is_none()
    );
    game.tick = 6;
    game.prefire_native_weapon(builder, Some(11));
    assert_eq!(
        game.entities[builder].weapon_set.weapons[0]
            .pre_attack
            .as_ref()
            .unwrap()
            .start,
        6
    );
    game.exit_attack_fire(builder);
    game.catalog[1].weapon_set_rules.templates[0]
        .rules
        .pre_attack_type = Prefire::Clip;
    game.catalog[1].weapon_set_rules.templates[0]
        .rules
        .clip_size = 2;
    game.entities[builder].weapon_set.weapons[0].clip_remaining = 1;
    game.tick = 7;
    game.prefire_native_weapon(builder, Some(11));
    assert!(
        game.entities[builder].weapon_set.weapons[0]
            .pre_attack
            .is_none()
    );
    game.entities[builder].weapon_set.weapons[0].clip_remaining = 2;
    game.tick = 8;
    game.prefire_native_weapon(builder, Some(11));
    assert_eq!(
        game.entities[builder].weapon_set.weapons[0]
            .pre_attack
            .as_ref()
            .unwrap()
            .start,
        8
    );
}

#[test]
fn continuation_preserves_attack_move_and_hunt_outer_machines() {
    use crate::skirmish::ai_goals::{COMMAND_FROM_PLAYER, MainState};
    for hunt in [false, true] {
        let (mut game, builder) = fixture();
        let first = game.spawn(5, 1, [310.0, 300.0], true);
        let next = game.spawn(5, 1, [312.0, 300.0], true);
        game.update_ground_contacts();
        game.update_visibility();
        let point = [500.0, 300.0, 29.0];
        if hunt {
            game.native_hunt_from(builder, COMMAND_FROM_PLAYER);
        } else {
            game.native_attack_move_from(
                builder,
                point,
                i32::MAX.unsigned_abs(),
                COMMAND_FROM_PLAYER,
            );
            game.entities[builder].ai.mood.next_check = 0;
        }
        let mut continued = false;
        for _ in 0..200 {
            game.step();
            if game.entities[first].masked() && !game.entities[next].masked() {
                assert!(!matches!(game.entities[builder].ai.order, Order::Attack(_)));
                if hunt {
                    assert!(matches!(
                        game.entities[builder].ai.command.main_state,
                        MainState::Hunt
                    ));
                } else {
                    assert!(matches!(
                        game.entities[builder].ai.order,
                        Order::AttackMove(_)
                    ));
                    assert_eq!(game.entities[builder].ai.ai_target, Some(next));
                    assert_eq!(
                        game.entities[builder].ai_goal_position().map(f32::to_bits),
                        point.map(f32::to_bits)
                    );
                }
                continued = true;
            }
            if game.entities[next].masked() {
                break;
            }
        }
        assert!(
            continued,
            "the first DISARM shot must retain its parent machine"
        );
        assert!(game.entities[first].masked());
        assert!(game.entities[next].masked());
    }
}
