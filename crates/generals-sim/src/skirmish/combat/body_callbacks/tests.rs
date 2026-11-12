use crate::skirmish::{
    DamageType, DeathType, Faction, Kind, WeaponRules, death_weapons,
    fire::flammable,
    tests::{fixture_type, game, register_update},
};

fn death_weapon() -> death_weapons::Rules {
    death_weapons::Rules {
        weapon: Some(WeaponRules {
            name: "DeathCallbackProbe".into(),
            ..WeaponRules::unarmed()
        }),
        death_mask: u32::MAX,
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 0,
        removals: Vec::new(),
        upgrade_fx: String::new(),
        starts_active: true,
        triggers: Vec::new(),
        conflicts: Vec::new(),
        requires_all: false,
    }
}

#[test]
fn active_constructor_zero_health_and_later_zero_health_mutation_are_distinct() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::Effect);
    unit.initial_health = 0.0;
    unit.max_health = 100.0;
    unit.behavior.death_weapons.push(death_weapon());
    crate::skirmish::tests::register_behavior(&mut unit, "FireWeaponWhenDeadBehavior", 0x0082);
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let constructor_zero = game.spawn(kind, 0, [300.0; 2], true);
    assert!(!game.entities[constructor_zero].is_effectively_dead());
    assert_eq!(game.body_damage_state(constructor_zero), 3);
    game.kill_object(constructor_zero, DeathType::Normal);
    assert!(game.entities[constructor_zero].is_effectively_dead());
    assert_eq!(game.presentation.shots.len(), 1);
    let mutated = game.spawn(kind, 0, [400.0; 2], true);
    game.entities[mutated].refresh_body_health();
    game.kill_object(mutated, DeathType::Normal);
    assert_eq!(game.presentation.shots.len(), 1);
}

#[test]
fn inactive_body_ignores_damage_but_dispatches_unresistable_death_once() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::Effect);
    unit.body_kind = crate::skirmish::BodyKind::Inactive;
    unit.initial_health = 0.0;
    unit.max_health = 0.0;
    unit.behavior.death_weapons.push(death_weapon());
    crate::skirmish::tests::register_behavior(&mut unit, "FireWeaponWhenDeadBehavior", 0x0082);
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, 0, [300.0; 2], true);
    game.apply_damage(id, None, 1000.0, DamageType::Flame, DeathType::Burned);
    assert!(game.presentation.shots.is_empty());
    assert_eq!(game.body_damage_state(id), 0);
    game.apply_native_damage(id, None, 0.0, DeathType::Normal);
    assert_eq!(game.presentation.shots.len(), 1);
    game.kill_object(id, DeathType::Normal);
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(game.entities[id].health.to_bits(), 0.0_f32.to_bits());
}

#[test]
fn unclipped_lethal_flame_notifies_authored_instances_before_death() {
    let mut game = game();
    let mut unit = fixture_type(Faction::China, Kind::Vehicle);
    unit.initial_health = 5.0;
    unit.max_health = 100.0;
    for _ in 0..2 {
        unit.behavior.flammable.push(flammable::Rules {
            burned_delay: 0,
            duration: 60,
            damage_delay: 30,
            damage: 1.0,
            limit: 10.0,
            expiration: 60,
            sound: String::new(),
        });
        register_update(&mut unit, "FlammableUpdate");
    }
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, 0, [300.0; 2], true);
    game.apply_damage(id, None, 20.0, DamageType::Flame, DeathType::Burned);
    assert!(game.entities[id].aflame());
    assert_eq!(game.body_damage_state(id), 3);
    let sounds = game.take_burning_sound_events();
    assert_eq!(sounds.len(), 1);
    assert!(matches!(
        sounds[0],
        flammable::SoundEvent::Start { ordinal: 0, .. }
    ));
}

#[test]
fn immortal_floor_does_not_notify_flame_when_health_does_not_decrease() {
    let mut game = game();
    let mut unit = fixture_type(Faction::China, Kind::Vehicle);
    unit.initial_health = 1.0;
    unit.minimum_health = 1.0;
    unit.behavior.flammable.push(flammable::Rules {
        burned_delay: 0,
        duration: 60,
        damage_delay: 30,
        damage: 1.0,
        limit: 1.0,
        expiration: 60,
        sound: String::new(),
    });
    register_update(&mut unit, "FlammableUpdate");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, 0, [300.0; 2], true);
    game.apply_damage(id, None, 20.0, DamageType::Flame, DeathType::Burned);
    assert_eq!(game.entities[id].health.to_bits(), 1.0_f32.to_bits());
    assert!(!game.entities[id].aflame());
    game.kill_object(id, DeathType::Normal);
    assert!(!game.entities[id].is_effectively_dead());
}

#[test]
fn damage_state_thresholds_use_source_strict_comparisons_and_healing_refreshes_cache() {
    let mut game = game();
    let mut unit = fixture_type(Faction::China, Kind::Vehicle);
    unit.initial_health = 100.0;
    unit.max_health = 100.0;
    unit.body_damage_thresholds = [0.75, 0.25];
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, 0, [300.0; 2], true);
    assert_eq!(game.body_damage_state(id), 0);
    game.apply_native_damage(id, None, 25.0, DeathType::Normal);
    assert_eq!(game.body_damage_state(id), 1);
    game.apply_native_damage(id, None, 50.0, DeathType::Normal);
    assert_eq!(game.body_damage_state(id), 2);
    game.apply_healing(id, None, 1.0);
    assert_eq!(game.body_damage_state(id), 1);
}

#[test]
fn handled_damage_reuses_previous_health_until_an_actual_heal_changes_it() {
    let mut game = game();
    game.catalog[6].maintenance.regeneration = Some(crate::skirmish::Regeneration {
        percent_per_second: 0.1,
        delay: 9,
    });
    register_update(&mut game.catalog[6], "BaseRegenerateUpdate");
    let target = game.spawn(6, 0, [300.0, 200.0], true);
    let registration = game.catalog[6].update_modules.len() - 1;
    game.tick = 12;
    game.apply_weapon_hit(target, 2, 10.0, DamageType::SmallArms, DeathType::Normal);
    let health = game.entities[target].health;
    game.wake_update(target, registration, 7);
    game.apply_weapon_hit(target, 2, 10.0, DamageType::Status, DeathType::Normal);
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
    assert_eq!(game.update_remaining_sleep(target, registration), 9);
    game.apply_healing(target, None, 1.0);
    game.wake_update(target, registration, 7);
    game.apply_weapon_hit(target, 2, 10.0, DamageType::Status, DeathType::Normal);
    assert_eq!(game.update_remaining_sleep(target, registration), 7);
}

#[test]
fn healing_weapon_applies_armor_once_and_bypasses_damage_scalar() {
    let mut game = game();
    game.catalog[6].armor[DamageType::Healing as usize] = 0.5;
    let target = game.spawn(6, 0, [300.0, 200.0], true);
    game.entities[target].battle_bonuses.armor = 0.1;
    game.apply_native_damage(target, None, 20.0, DeathType::Normal);
    let before = game.entities[target].health;
    game.apply_weapon_hit(target, 2, 20.0, DamageType::Healing, DeathType::None);
    assert_eq!(
        game.entities[target].health.to_bits(),
        (before + 10.0).to_bits()
    );
}

#[test]
fn negative_armored_subdual_clamps_but_unresistable_preserves_raw_amount() {
    let mut game = game();
    game.catalog[6].maintenance.subdual = Some(crate::skirmish::microwave::SubdualRules {
        cap: 500.0,
        heal_rate: 3,
        heal_amount: 10.0,
    });
    let target = game.spawn(6, 0, [300.0, 200.0], true);
    game.apply_weapon_hit(target, 2, 20.0, DamageType::SubdualVehicle, DeathType::None);
    game.apply_weapon_hit(
        target,
        2,
        -10.0,
        DamageType::SubdualVehicle,
        DeathType::None,
    );
    assert_eq!(
        game.entities[target].subdual_damage().to_bits(),
        20.0_f32.to_bits()
    );
    game.apply_weapon_hit(
        target,
        2,
        -10.0,
        DamageType::SubdualUnresistable,
        DeathType::None,
    );
    assert_eq!(
        game.entities[target].subdual_damage().to_bits(),
        10.0_f32.to_bits()
    );
    game.catalog[6].armor[DamageType::Unresistable as usize] = 0.1;
    game.entities[target].battle_bonuses.armor = 0.1;
    let before = game.entities[target].health;
    game.apply_weapon_hit(target, 2, 20.0, DamageType::Unresistable, DeathType::Normal);
    assert_eq!(
        game.entities[target].health.to_bits(),
        (before - 20.0).to_bits()
    );
}

#[test]
fn damage_sounds_and_handled_damage_preserve_original_fear_rng_crossing() {
    let mut game = game();
    game.catalog[6].damaged_sound = "Damaged".into();
    game.catalog[6].really_damaged_sound = "ReallyDamaged".into();
    let target = game.spawn(6, 0, [300.0, 200.0], true);
    let original_random = game.random;
    game.apply_native_damage(target, None, 50.0, DeathType::Normal);
    assert_eq!(game.presentation.object_sounds[0].name, "Damaged");
    assert_eq!(game.random, original_random);
    let mut expected = game.random;
    crate::skirmish::startup::random_between(&mut expected, 99);
    game.apply_native_damage(target, None, 30.0, DeathType::Normal);
    assert_eq!(game.random, expected);
    crate::skirmish::startup::random_between(&mut expected, 99);
    game.apply_weapon_hit(target, 2, 10.0, DamageType::Status, DeathType::None);
    assert_eq!(game.random, expected);
    game.apply_native_damage(target, None, 10.0, DeathType::Normal);
    assert_eq!(game.presentation.object_sounds[1].name, "ReallyDamaged");
    assert_eq!(game.random, expected);
}

#[test]
fn handled_damage_checks_body_state_again_after_a_reaction_weapon_changes_health() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Usa, Kind::Infantry);
    unit.name = "ReactionInfantry".into();
    unit.really_damaged_sound = "ReallyDamaged".into();
    unit.behavior
        .damage_weapons
        .push(crate::skirmish::fire::when_damaged::Rules {
            reaction: [
                Some(WeaponRules {
                    name: "ReactionBlast".into(),
                    damage: 90.0,
                    damage_type: DamageType::Unresistable,
                    area: Some(crate::skirmish::AreaDamage {
                        at_source: true,
                        primary_radius: 20.0,
                        secondary_radius: 0.0,
                        secondary_damage: 0.0,
                        affects: 9,
                    }),
                    ..WeaponRules::unarmed()
                }),
                None,
                None,
                None,
            ],
            continuous: std::array::from_fn(|_| None),
            starts_active: true,
            damage_types: 1 << DamageType::Status as u32,
            amount: 0.0,
            triggers: Vec::new(),
            conflicts: Vec::new(),
            requires_all: false,
            removals: Vec::new(),
            upgrade_fx: String::new(),
        });
    register_update(&mut unit, "FireWeaponWhenDamagedBehavior");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let target = game.spawn(kind, 0, [300.0, 200.0], true);
    game.update_ground_contacts();
    game.apply_native_damage(target, None, 5.0, DeathType::Normal);
    game.apply_weapon_hit(target, 2, 1.0, DamageType::Status, DeathType::None);
    assert_eq!(game.entities[target].health.to_bits(), 5.0_f32.to_bits());
    // The inner reaction and outer handled-damage calls each observe the
    // NORMAL -> REALLYDAMAGED transition after their own onDamage callbacks.
    assert_eq!(
        game.presentation
            .object_sounds
            .iter()
            .filter(|sound| sound.name == "ReallyDamaged")
            .count(),
        2
    );
}
