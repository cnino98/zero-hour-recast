use super::*;
use crate::skirmish::{Kind, Order, WeaponTemplateSet, ground_movement, tests};

#[test]
fn ground_query_keeps_not_possible_distinct_from_invalid_shot_and_mobile_range() {
    let (mut game, actor) = ground_movement::tests::moving_tank();
    let kind = game.entities[actor].type_index;
    game.catalog[kind].weapon.range = 20.0;
    let far = [500.0, 150.0, 0.0];
    assert!(matches!(
        game.force_attack_result(actor, None, far),
        CanAttackResult::PossibleAfterMoving
    ));
    game.catalog[kind].kind_of.push("IMMOBILE".into());
    assert!(matches!(
        game.force_attack_result(actor, None, far),
        CanAttackResult::InvalidShot
    ));
    game.entities[actor].object_status |= 1 << 6; // Actual NO_ATTACK object status.
    assert!(matches!(
        game.force_attack_result(actor, None, far),
        CanAttackResult::NotPossible
    ));
}

#[test]
fn player_query_uses_explicit_source_even_after_an_ai_command() {
    let (mut game, actor) = ground_movement::tests::moving_tank();
    let target = game.spawn(5, 0, [330.0, 150.0], true);
    game.set_last_command_source(actor, crate::skirmish::ai_goals::COMMAND_FROM_AI);
    assert!(matches!(
        game.player_attack_result(actor, target),
        CanAttackResult::NotPossible
    ));
    assert!(matches!(
        game.force_attack_result(actor, Some(target), game.object_position(target)),
        CanAttackResult::Possible
    ));
    assert!(
        game.entities[actor].last_command_from_ai(),
        "a GUI query must not become an AI command producer"
    );
}

fn dual_weapons() -> (Skirmish, usize) {
    let mut game = tests::game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].secondary_weapon =
        Some(tests::fixture_type(game.catalog[kind].faction, Kind::Tank).weapon);
    game.catalog[kind].weapon_set_rules.replacements = vec![WeaponTemplateSet {
        flags: 0,
        weapons: [Some(0), Some(1), None],
        share_lock: false,
    }];
    game.catalog[kind].location_weapons = 0b10;
    let actor = game.spawn(kind, 0, [300.0, 150.0], true);
    (game, actor)
}

#[test]
fn ground_command_keeps_xyz_unlimited_shots_and_permanent_secondary_lock() {
    let (mut game, actor) = dual_weapons();
    game.switch_weapon(actor, 1).unwrap();
    game.move_to(actor, [450.0, 150.0]).unwrap();
    let goal = [380.0, 170.0, 35.0];
    game.group_force_attack_position(&[actor], goal).unwrap();
    assert!(matches!(
        game.entities[actor].ai.order,
        Order::FireAt { .. }
    ));
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        goal.map(f32::to_bits)
    );
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    assert!(matches!(
        game.entities[actor].weapon_set.lock,
        Lock::Permanent
    ));
    assert_eq!(
        game.entities[actor].weapon_set.weapons[1].max_shots,
        Some(NO_MAX_SHOTS_LIMIT)
    );
    assert!(!game.entities[actor].last_command_from_ai());
}

#[test]
fn active_group_temporarily_selects_primary_and_stop_releases_location_lock() {
    let (mut game, actor) = dual_weapons();
    game.fire_weapon_at(actor, 1, [370.0, 150.0]).unwrap();
    assert!(matches!(
        game.entities[actor].weapon_set.lock,
        Lock::Temporary
    ));
    game.move_to(actor, [450.0, 150.0]).unwrap();
    game.group_force_attack_position(&[actor], [380.0, 150.0, 0.0])
        .unwrap();
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    assert!(matches!(game.entities[actor].weapon_set.lock, Lock::None));
    assert_eq!(
        game.entities[actor].weapon_set.weapons[0].max_shots,
        Some(NO_MAX_SHOTS_LIMIT)
    );
    game.fire_weapon_at(actor, 1, [370.0, 150.0]).unwrap();
    game.stop(actor).unwrap();
    assert!(game.entities[actor].ai_idle());
    // Native privateIdle exits the finite location command and releases its
    // temporary lock; the next ground command uses ordinary weapon selection.
    assert!(matches!(game.entities[actor].weapon_set.lock, Lock::None));
    game.group_force_attack_position(&[actor], [380.0, 150.0, 0.0])
        .unwrap();
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    assert!(matches!(game.entities[actor].weapon_set.lock, Lock::None));
    assert!(matches!(
        game.entities[actor].ai.order,
        Order::FireAt { .. }
    ));
}
