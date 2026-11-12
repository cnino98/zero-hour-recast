//! Authored body flags and source player/AI/forced relationship differences.
use crate::skirmish::{DamageType, DeathType, Kind, Order, tests::game};

#[test]
fn player_neutral_targetability_does_not_allow_allies_or_expand_auto_acquisition() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let actor = game.spawn(tank, 0, [300.0, 300.0], true);
    let neutral = game.spawn(tank, game.neutral_owner(), [330.0, 300.0], true);
    assert!(!game.player_attack_relationship(actor, neutral));
    assert!(game.attack(actor, neutral).is_err());
    game.set_map_body_controls(neutral, true, false);
    assert!(game.player_attack_relationship(actor, neutral));
    assert!(game.mood_target(actor, false, true).is_none());
    game.attack(actor, neutral).unwrap();
    assert!(!game.entities[actor].ai.command.from_ai);
    assert!(game.update_attack(actor, neutral));
    game.set_map_body_controls(neutral, false, false);
    assert!(!game.update_attack(actor, neutral));
    game.force_attack(actor, neutral).unwrap();
    assert!(game.entities[actor].ai.attack_context.forced);
    assert!(game.update_attack(actor, neutral));
    assert!(game.attack(actor, neutral).is_err());
    assert!(game.entities[actor].ai.attack_context.forced);
    assert!(game.update_attack(actor, neutral));
    game.entities[neutral].owner = 0;
    game.set_map_body_controls(neutral, true, false);
    assert!(!game.player_attack_relationship(actor, neutral));
    assert!(game.attack(actor, neutral).is_err());
    assert!(game.force_attack_target_reachable(actor, neutral));
    game.force_attack(actor, neutral).unwrap();
    assert!(game.update_attack(actor, neutral));
    assert!(!game.player_attack_target_reachable(actor, neutral));
    game.attack_from_ai(actor, neutral).unwrap();
    assert!(game.entities[actor].ai.command.from_ai);
    assert!(!game.entities[actor].ai.attack_context.forced);
    assert!(game.update_attack(actor, neutral));
    assert!(game.force_attack(actor, actor).is_err());
}

#[test]
fn indestructible_gate_precedes_damage_records_death_and_experience_but_allows_healing() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let source = game.spawn(tank, 0, [300.0, 300.0], true);
    let victim = game.spawn(tank, 1, [330.0, 300.0], true);
    let health = game.entities[victim].health;
    game.set_map_body_controls(victim, false, true);
    game.apply_damage(
        victim,
        Some(source),
        100_000.0,
        DamageType::Water,
        DeathType::Flooded,
    );
    assert_eq!(game.entities[victim].health.to_bits(), health.to_bits());
    assert!(game.entities[victim].last_attacker().is_none());
    assert_eq!(game.entities[source].experience, 0);
    assert!(matches!(game.entities[victim].ai.order, Order::Idle));
    game.damage_body(
        victim,
        Some(source),
        100_000.0,
        DamageType::Flame,
        DeathType::Burned,
    );
    assert_eq!(game.entities[victim].health.to_bits(), health.to_bits());
    game.entities[victim].health -= 10.0;
    game.apply_healing(victim, None, 5.0);
    assert_eq!(
        game.entities[victim].health.to_bits(),
        (health - 5.0).to_bits()
    );
    game.set_map_body_controls(victim, false, false);
    game.apply_native_damage(victim, Some(source), 100_000.0, DeathType::Normal);
    assert!(!game.entities[victim].alive());
}

#[test]
fn script_targetability_and_force_attack_cannot_override_unattackable_kind() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let actor = game.spawn(tank, 0, [300.0, 300.0], true);
    let mut unit =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Civilian, Kind::Tank);
    unit.kind_of.push("UNATTACKABLE".into());
    let index = game.catalog.len();
    game.catalog.push(unit);
    let target = game.spawn(index, game.neutral_owner(), [330.0, 300.0], true);
    game.set_map_body_controls(target, true, false);
    assert!(game.player_attack_relationship(actor, target));
    assert!(!game.weapon_target_reachable(actor, target));
    assert!(!game.force_attack_target_reachable(actor, target));
    assert!(game.attack(actor, target).is_err());
    assert!(game.force_attack(actor, target).is_err());
    assert!(game.attack_from_ai(actor, target).is_err());
}
