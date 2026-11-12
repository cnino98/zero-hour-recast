use super::*;
use crate::skirmish::{
    Faction, Kind,
    tests::{fixture_type, game},
};

fn actor(game: &mut Skirmish, flags: &[&str]) -> usize {
    let mut unit = fixture_type(Faction::Usa, Kind::Tank);
    unit.kind_of = flags.iter().map(|name| (*name).into()).collect();
    let kind = game.catalog.len();
    game.catalog.push(unit);
    game.spawn(kind, 0, [200.0, 200.0], true)
}

#[test]
fn anonymous_water_preserves_recent_tank_record_but_replaces_after_two_frames() {
    let mut game = game();
    let target = actor(&mut game, &["STRUCTURE"]);
    let tank = actor(&mut game, &["VEHICLE"]);
    game.tick = 10;
    game.record_body_damage(target, Some(tank), DamageType::Explosion);
    game.entities[target].damage_state.last_attacker = None;
    for frame in [10, 11] {
        game.tick = frame;
        game.record_body_damage(target, None, DamageType::Water);
        let damage = &game.entities[target].damage_state;
        assert_eq!(damage.retained_source, Some(tank));
        assert_eq!(damage.last_attacker, None);
        assert_eq!(damage.current_source, None);
        assert!(matches!(damage.damage_type, DamageType::Explosion));
        assert_eq!(damage.retained_frame, 10);
    }
    game.tick = 12;
    game.record_body_damage(target, None, DamageType::Water);
    assert!(matches!(
        game.entities[target].damage_state.damage_type,
        DamageType::Water
    ));
    assert_eq!(game.entities[target].damage_state.retained_source, None);
}

#[test]
fn native_source_priority_is_explicit_kind_bits_not_generic_structure_or_owner() {
    let mut game = game();
    let target = actor(&mut game, &["STRUCTURE"]);
    let tank = actor(&mut game, &["VEHICLE"]);
    let civilian = actor(&mut game, &["STRUCTURE", "FS_POWER"]);
    let factory = actor(&mut game, &["FS_WARFACTORY"]);
    game.tick = 20;
    game.record_body_damage(target, Some(tank), DamageType::Explosion);
    game.record_body_damage(target, Some(civilian), DamageType::Water);
    assert_eq!(
        game.entities[target].damage_state.retained_source,
        Some(tank)
    );
    assert_eq!(
        game.entities[target].damage_state.current_source,
        Some(civilian)
    );
    game.record_body_damage(target, Some(factory), DamageType::Water);
    assert_eq!(
        game.entities[target].damage_state.retained_source,
        Some(factory)
    );
    assert!(matches!(
        game.entities[target].damage_state.damage_type,
        DamageType::Water
    ));
}

#[test]
fn removed_source_is_null_but_retained_dead_source_still_blocks_lower_priority() {
    let mut game = game();
    let target = actor(&mut game, &["STRUCTURE"]);
    let tank = actor(&mut game, &["VEHICLE"]);
    let debris = actor(&mut game, &["PROJECTILE"]);
    game.tick = 30;
    game.record_body_damage(target, Some(tank), DamageType::Explosion);
    game.entities[tank].health = 0.0;
    game.record_body_damage(target, Some(debris), DamageType::Water);
    assert_eq!(
        game.entities[target].damage_state.retained_source,
        Some(tank)
    );
    game.destroy_maintenance_object(tank);
    game.process_update_destruction();
    game.record_body_damage(target, Some(debris), DamageType::Water);
    assert_eq!(
        game.entities[target].damage_state.retained_source,
        Some(debris)
    );
    assert_eq!(
        game.entities[target].damage_state.last_attacker,
        Some(debris)
    );
}

#[test]
fn native_invalid_timestamp_and_frame_wrap_keep_unsigned_comparison() {
    let mut game = game();
    let target = actor(&mut game, &["STRUCTURE"]);
    game.tick = 0;
    game.record_body_damage(target, None, DamageType::Water);
    assert!(matches!(
        game.entities[target].damage_state.damage_type,
        DamageType::Unresistable
    ));
    game.tick = 1;
    game.record_body_damage(target, None, DamageType::Water);
    assert!(matches!(
        game.entities[target].damage_state.damage_type,
        DamageType::Water
    ));
    let tank = actor(&mut game, &["INFANTRY"]);
    game.tick = u64::from(u32::MAX);
    game.record_body_damage(target, Some(tank), DamageType::Explosion);
    game.tick += 1;
    game.record_body_damage(target, None, DamageType::Water);
    assert_eq!(
        game.entities[target].damage_state.retained_source,
        Some(tank)
    );
}

#[test]
fn healing_replaces_source_and_type_without_refreshing_native_damage_timestamp() {
    let mut game = game();
    let target = actor(&mut game, &["STRUCTURE"]);
    let tank = actor(&mut game, &["VEHICLE"]);
    let repairer = actor(&mut game, &["DOZER"]);
    game.tick = 10;
    game.record_body_damage(target, Some(tank), DamageType::Water);
    game.entities[target].damage_state.last_attacker = None;
    game.tick = 100;
    game.record_body_healing(target, Some(repairer), DamageType::Unresistable);
    let damage = &game.entities[target].damage_state;
    assert_eq!(damage.retained_frame, 10);
    assert_eq!(damage.retained_source, Some(repairer));
    assert_eq!(damage.last_attacker, Some(repairer));
    assert!(matches!(damage.damage_type, DamageType::Unresistable));
    game.record_body_damage(target, None, DamageType::Water);
    assert_eq!(game.entities[target].damage_state.retained_source, None);
    assert!(matches!(
        game.entities[target].damage_state.damage_type,
        DamageType::Water
    ));
}

#[test]
fn healing_uses_armor_and_clamp_and_only_actual_health_gain_cures_poison() {
    let mut game = game();
    let kind = game.catalog.len();
    game.catalog.push(fixture_type(Faction::Usa, Kind::Tank));
    game.catalog[kind].armor[DamageType::Healing as usize] = 0.5;
    game.catalog[kind].behavior.poisoned = vec![crate::skirmish::hazards::PoisonRules {
        interval: 6,
        duration: 18,
    }];
    game.catalog[kind].update_modules.push(
        crate::skirmish::module_updates::ModuleRegistration::Behavior {
            family: "PoisonedBehavior".into(),
            tag: "PoisonRegression".into(),
            source: crate::skirmish::module_updates::ModuleSource {
                path: "PoisonedBehavior.cpp".into(),
                origin: "source regression".into(),
                line: 1,
            },
        },
    );
    crate::skirmish::tests::poison_damage_fixture(&mut game, kind);
    let target = game.spawn(kind, 0, [200.0, 200.0], true);
    game.entities[target].battle_bonuses.armor = 0.25;
    let maximum = game.entities[target].max_health;
    game.entities[target].health = maximum - 20.0;
    game.poison_damage_received(
        target,
        0,
        crate::skirmish::hazards::PoisonSource::Object(target),
        2.0,
        crate::skirmish::DeathType::Poisoned,
    );
    game.tick = 40;
    game.apply_healing(target, None, 12.0);
    assert_eq!(
        game.entities[target].health.to_bits(),
        (maximum - 14.0).to_bits()
    );
    assert!(!game.entities[target].poisoned());
    assert!(matches!(
        game.entities[target].damage_state.damage_type,
        DamageType::Healing
    ));
    game.apply_healing(target, None, 1000.0);
    assert_eq!(game.entities[target].health.to_bits(), maximum.to_bits());
    game.poison_damage_received(
        target,
        0,
        crate::skirmish::hazards::PoisonSource::Object(target),
        2.0,
        crate::skirmish::DeathType::Poisoned,
    );
    game.apply_healing(target, Some(target), 12.0);
    assert!(game.entities[target].poisoned());
    assert_eq!(
        game.entities[target].damage_state.retained_source,
        Some(target)
    );
}

#[test]
fn nonpositive_or_armor_blocked_healing_leaves_health_and_retained_damage_untouched() {
    let mut game = game();
    let target = actor(&mut game, &["VEHICLE"]);
    let kind = game.entities[target].type_index;
    game.entities[target].health = 20.0;
    game.tick = 10;
    game.record_body_damage(target, Some(target), DamageType::Water);
    for amount in [0.0, -10.0, f32::NAN] {
        game.apply_healing(target, None, amount);
        assert_eq!(game.entities[target].health.to_bits(), 20.0_f32.to_bits());
        assert!(matches!(
            game.entities[target].damage_state.damage_type,
            DamageType::Water
        ));
    }
    game.catalog[kind].armor[DamageType::Healing as usize] = 0.0;
    game.apply_healing(target, None, 100.0);
    assert_eq!(game.entities[target].health.to_bits(), 20.0_f32.to_bits());
    assert!(matches!(
        game.entities[target].damage_state.damage_type,
        DamageType::Water
    ));
}

#[test]
fn dead_body_rejects_healing_except_original_bridge_kind_flags() {
    let mut game = game();
    for flags in [&["STRUCTURE"][..], &["BRIDGE"][..], &["BRIDGE_TOWER"][..]] {
        let target = actor(&mut game, flags);
        let health = game.entities[target].health;
        game.change_body_health(target, -health);
        assert!(game.entities[target].is_effectively_dead());
        game.apply_healing(target, None, 10.0);
        let expected = if flags == ["STRUCTURE"] {
            0.0_f32
        } else {
            10.0
        };
        assert_eq!(game.entities[target].health.to_bits(), expected.to_bits());
    }
}
