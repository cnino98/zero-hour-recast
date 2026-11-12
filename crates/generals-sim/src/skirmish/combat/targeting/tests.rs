use super::Result;
use crate::skirmish::{
    DamageType, DeathType, Kind, WeaponTemplateSet,
    tests::{fixture_type, game},
    weapons::ClipReload,
};

#[test]
fn portable_weapon_is_blocked_by_emp_and_no_attack_before_can_attack_override() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind]
        .kind_of
        .extend(["PORTABLE_STRUCTURE".into(), "CAN_ATTACK".into()]);
    let actor = game.spawn(kind, 0, [300.0, 300.0], true);
    assert!(game.object_can_attack(actor));
    game.entities[actor].disabled_status = 1;
    assert!(game.object_can_attack(actor));
    game.entities[actor].disabled_status = 1 << 2;
    assert!(!game.object_can_attack(actor));
    game.entities[actor].disabled_status = 0;
    game.entities[actor].object_status |= 1 << 6;
    assert!(!game.object_can_attack(actor));
}

#[test]
fn effective_death_flag_controls_continued_target_legality_even_with_positive_health() {
    let mut game = game();
    let actor = game.spawn(6, 0, [300.0, 300.0], true);
    let target = game.spawn(6, 1, [330.0, 300.0], true);
    let position = game.object_position(target);
    assert!(matches!(
        game.attack_target_result(actor, Some(target), position, false, true, None),
        Result::Possible
    ));
    game.entities[target].set_effectively_dead(true);
    assert!(game.entities[target].health > 0.0);
    assert!(matches!(
        game.attack_target_result(actor, Some(target), position, false, true, None),
        Result::NotPossible
    ));
    game.entities[target].set_effectively_dead(false);
    game.entities[actor].set_effectively_dead(true);
    assert!(matches!(
        game.attack_target_result(actor, Some(target), position, true, true, None),
        Result::NotPossible
    ));
}

#[test]
fn immobile_target_admission_checks_current_weapon_range_even_when_secondary_reaches() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].kind_of.push("IMMOBILE".into());
    game.catalog[kind].weapon.range = 20.0;
    let mut secondary = fixture_type(game.catalog[kind].faction, Kind::Tank).weapon;
    secondary.range = 150.0;
    game.catalog[kind].secondary_weapon = Some(secondary);
    let actor = game.spawn(kind, 0, [300.0, 300.0], true);
    let target = game.spawn(5, 1, [380.0, 300.0], true);
    let position = game.object_position(target);
    assert!(matches!(
        game.attack_target_result(actor, Some(target), position, false, false, Some(1)),
        Result::InvalidShot
    ));
    game.switch_weapon(actor, 1).unwrap();
    assert!(matches!(
        game.attack_target_result(actor, Some(target), position, false, false, None),
        Result::Possible
    ));
}

#[test]
fn estimator_uses_actual_empty_clip_and_retains_auto_reload_targetability() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].weapon.clip_size = 1;
    game.catalog[kind].weapon.reload = ClipReload::None;
    let actor = game.spawn(kind, 0, [300.0, 300.0], true);
    let target = game.spawn(5, 1, [330.0, 300.0], true);
    game.entities[target].health = 1000.0;
    game.entities[target].max_health = 1000.0;
    assert!(game.estimate_native_weapon_damage(actor, 0, target) > 0.0);
    game.fire_weapon(actor, target);
    assert_eq!(game.entities[actor].weapon_set.weapons[0].clip_remaining, 0);
    game.tick = game.entities[actor].weapon_set.weapons[0].possible_next_shot;
    assert_eq!(
        game.estimate_native_weapon_damage(actor, 0, target)
            .to_bits(),
        0.0_f32.to_bits()
    );
    game.catalog[kind].weapon.reload = ClipReload::Automatic;
    assert!(game.estimate_native_weapon_damage(actor, 0, target) > 0.0);
}

#[test]
fn disarm_and_shrubbery_estimates_preserve_native_special_case_precedence() {
    let mut game = game();
    let actor = game.spawn(6, 0, [300.0, 300.0], true);
    let target = game.spawn(5, 1, [330.0, 300.0], true);
    game.catalog[6].weapon.damage_type = DamageType::Disarm;
    game.catalog[6].weapon.damage = 0.0;
    assert_eq!(
        game.estimate_native_weapon_damage(actor, 0, target)
            .to_bits(),
        0.0_f32.to_bits()
    );
    game.catalog[5].kind_of.push("MINE".into());
    assert_eq!(
        game.estimate_native_weapon_damage(actor, 0, target)
            .to_bits(),
        1.0_f32.to_bits()
    );
    game.catalog[5].kind_of.push("SHRUBBERY".into());
    assert_eq!(
        game.estimate_native_weapon_damage(actor, 0, target)
            .to_bits(),
        0.0_f32.to_bits()
    );
    game.catalog[6].weapon.death_type = DeathType::Burned;
    assert_eq!(
        game.estimate_native_weapon_damage(actor, 0, target)
            .to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn container_default_fire_gate_is_distinct_from_the_carrier_id_query() {
    let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
    let kind = game.entities[carrier].type_index;
    game.catalog[kind].weapon_set_rules.replacements = vec![WeaponTemplateSet {
        flags: 0,
        weapons: [None; 3],
        share_lock: false,
    }];
    game.replace_weapon_set(carrier);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(rider, carrier);
    assert_eq!(game.passengers(carrier), &[rider]);
    assert!(game.passenger_can_fire(rider));
    // Object::isAbleToAttack passes the carrier's ID to TransportContain,
    // whose infantry check rejects this VEHICLE; WeaponSet passes INVALID_ID.
    assert!(!game.object_can_attack(carrier));
    let target = game.spawn(6, 1, [385.0, 255.0], true);
    let position = game.object_position(target);
    assert!(matches!(
        game.attack_target_result(carrier, Some(target), position, false, false, None),
        Result::Possible
    ));
    // OpenContain's constructor copies PassengersAllowedToFire into its
    // live upgradeable latch. A different authored value needs a new Object.
    game.catalog[kind].contain.as_mut().unwrap().weapons.fire = false;
    let closed = game.spawn(kind, 0, [355.0, 255.0], true);
    game.remove_passenger(rider, carrier);
    game.board(rider, closed);
    assert_eq!(game.entities[rider].contained_by(), Some(closed));
    assert!(!game.passenger_can_fire(rider));
    assert!(matches!(
        game.attack_target_result(closed, Some(target), position, false, false, None),
        Result::InvalidShot
    ));
}
