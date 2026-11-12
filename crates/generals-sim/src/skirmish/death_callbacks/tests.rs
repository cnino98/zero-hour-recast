use crate::skirmish::{
    DeathType,
    tests::{death_conditions, death_fx, game, register_behavior},
};

#[test]
fn queued_destruction_changes_later_die_filters_without_skipping_callbacks() {
    let mut game = game();
    let unit = &mut game.catalog[5];
    unit.behavior.destroy_die.push(death_conditions());
    register_behavior(unit, "DestroyDie", 2);
    let mut effect = death_fx("AfterDestroy");
    effect.conditions.required_status = 1 << 1;
    unit.behavior.death_fx.push(effect);
    register_behavior(unit, "FXListDie", 0x82);
    let mut effect = death_fx("ExcludeDestroyed");
    effect.conditions.exempt_status = 1 << 1;
    unit.behavior.death_fx.push(effect);
    register_behavior(unit, "FXListDie", 0x82);
    let object = game.spawn(5, 0, [300.0, 300.0], true);
    game.kill_object(object, DeathType::Normal);
    assert!(game.entities[object].exists());
    assert_eq!(
        game.presentation
            .object_fx_events
            .iter()
            .map(|event| event.effect.as_str())
            .collect::<Vec<_>>(),
        ["AfterDestroy"]
    );
    game.process_update_destruction();
    assert!(!game.entities[object].exists());
}

#[test]
fn death_fx_retains_damage_dealer_pose_only_for_object_form() {
    let mut game = game();
    let unit = &mut game.catalog[5];
    unit.behavior.death_fx.push(death_fx("Object"));
    register_behavior(unit, "FXListDie", 0x82);
    let mut effect = death_fx("Position");
    effect.orient_to_object = false;
    unit.behavior.death_fx.push(effect);
    register_behavior(unit, "FXListDie", 0x82);
    let victim = game.spawn(5, 0, [300.0, 300.0], true);
    let attacker = game.spawn(6, 1, [400.0, 300.0], true);
    game.set_object_orientation(victim, std::f32::consts::FRAC_PI_2);
    let source_position = game.object_position(attacker);
    game.apply_native_damage(victim, Some(attacker), 1_000.0, DeathType::Normal);
    game.entities[attacker].position = [600.0, 300.0];
    assert_eq!(game.presentation.object_fx_events.len(), 2);
    let oriented = &game.presentation.object_fx_events[0];
    assert!(oriented.oriented_object);
    assert_eq!(
        oriented
            .secondary_position
            .map(|position| position.map(f32::to_bits)),
        Some(source_position.map(f32::to_bits))
    );
    assert_eq!(oriented.basis[1][0].to_bits(), 1.0_f32.to_bits());
    let position = &game.presentation.object_fx_events[1];
    assert!(!position.oriented_object);
    assert!(position.secondary_position.is_none());
    assert_eq!(
        position.basis.map(|row| row.map(f32::to_bits)),
        crate::skirmish::physical_ocl::transform::yaw(0.0).map(|row| row.map(f32::to_bits))
    );
}

#[test]
fn death_fx_upgrade_removal_resets_constructed_instances_then_player_upgrade_reactivates() {
    let mut game = game();
    let unit = &mut game.catalog[5];
    let mut first = death_fx("First");
    first.triggers.push("Pulse".into());
    unit.behavior.death_fx.push(first);
    register_behavior(unit, "FXListDie", 0x82);
    unit.behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers
        .push("Pulse".into());
    let mut second = death_fx("Second");
    second.removals.push("Pulse".into());
    unit.behavior.death_fx.push(second);
    register_behavior(unit, "FXListDie", 0x82);
    unit.behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .removals
        .push("Pulse".into());
    let id = game.spawn(5, 0, [300.0, 300.0], true);
    assert_eq!(game.entities[id].behaviors.death_fx, [false, true]);
    game.players[0].upgrades.push("Pulse".into());
    game.activate_upgrade_modules(id);
    assert_eq!(game.entities[id].behaviors.death_fx, [true, true]);
    game.remove_native_object_upgrade(id, "Pulse");
    assert_eq!(game.entities[id].behaviors.death_fx, [false, true]);
    assert!(game.has_upgrade(0, "Pulse"));
    game.activate_upgrade_modules(id);
    game.kill_object(id, DeathType::Normal);
    assert_eq!(
        game.presentation
            .object_fx_events
            .iter()
            .map(|event| event.effect.as_str())
            .collect::<Vec<_>>(),
        ["First", "Second"]
    );
}

#[test]
fn upgrade_removal_does_not_change_the_captured_key_mask_for_later_fx_modules() {
    let mut game = game();
    let unit = &mut game.catalog[5];
    unit.maintenance
        .research
        .push(crate::skirmish::maintenance::Research {
            name: "Pulse".into(),
            cost: 0,
            frames: 1,
            player: false,
        });
    for name in ["First", "Second"] {
        let mut rules = death_fx(name);
        rules.starts_active = false;
        rules.triggers.push("Pulse".into());
        if name == "First" {
            rules.removals.push("Pulse".into());
        }
        unit.behavior.death_fx.push(rules);
        register_behavior(unit, "FXListDie", 0x82);
        unit.behavior_modules
            .last_mut()
            .unwrap()
            .upgrade
            .triggers
            .push("Pulse".into());
    }
    unit.behavior_modules
        .iter_mut()
        .find(|module| module.family == "FXListDie")
        .unwrap()
        .upgrade
        .removals
        .push("Pulse".into());
    let id = game.spawn(5, 0, [300.0, 300.0], true);
    game.finish_research(id, 0);
    assert!(!game.object_has_upgrade(id, "Pulse"));
    assert_eq!(game.entities[id].behaviors.death_fx, [true, true]);
}

#[test]
fn projectile_death_fx_uses_canonical_object_rotation_before_deferred_destruction() {
    let mut game = game();
    let mut unit = crate::skirmish::point_defense::tests::missile_template(
        crate::skirmish::point_defense::tests::missile_rules(),
        1.0,
    );
    unit.behavior.death_fx.push(death_fx("PitchedMissile"));
    register_behavior(&mut unit, "FXListDie", 0x82);
    let serial = crate::skirmish::point_defense::tests::missile_from_template(
        &mut game,
        [355.0, 305.0, 10.0],
        1,
        None,
        [405.0, 305.0, 0.0],
        unit,
    );
    let index = game.projectile_index(serial).unwrap();
    let id = game.projectiles[index].object.unwrap();
    game.entities[id]
        .projectile_flight
        .as_mut()
        .unwrap()
        .motion
        .axes = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
    game.kill_object(id, DeathType::Normal);
    let event = game
        .presentation
        .object_fx_events
        .iter()
        .find(|event| event.effect == "PitchedMissile")
        .unwrap();
    assert_eq!(
        event.basis.map(|row| row.map(f32::to_bits)),
        [[0.0_f32, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]].map(|row| row.map(f32::to_bits))
    );
    assert!(event.oriented_object);
    assert!(game.entities[id].exists());
}

#[test]
fn crush_die_uses_nearest_native_point_and_publishes_body_draw_and_sound() {
    use crate::skirmish::{
        DamageType, Faction, Kind, death_callbacks::CrushRules, tests::fixture_type,
    };
    for (offset, front, back, sound) in [
        (5.0, true, false, "Front"),
        (-5.0, false, true, "Back"),
        (0.0, true, true, "Total"),
    ] {
        let mut game = game();
        let mut unit = fixture_type(Faction::Civilian, Kind::Tank);
        unit.geometry.major = 10.0;
        unit.behavior.crush_die.push(CrushRules {
            conditions: death_conditions(),
            sounds: ["Total".into(), "Back".into(), "Front".into()],
            percentages: [100; 3],
        });
        register_behavior(&mut unit, "CrushDie", 2);
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let victim = game.spawn(kind, 1, [300.0, 300.0], true);
        game.entities[victim].facing = 0.0;
        let crusher = game.spawn(6, 0, [300.0 + offset, 300.0], true);
        game.random = crate::skirmish::startup::seed_random(37);
        let mut expected_random = game.random;
        crate::skirmish::startup::random_between(&mut expected_random, 99);
        game.apply_weapon_hit(
            victim,
            crusher,
            999_999.0,
            DamageType::Crush,
            DeathType::Crushed,
        );
        assert!(!game.entities[victim].alive());
        assert_eq!(game.entities[victim].damage_state.front_crushed, front);
        assert_eq!(game.entities[victim].damage_state.back_crushed, back);
        let flags = game.model_flags(&game.entities[victim], [0.5, 0.1]);
        assert_eq!(flags.contains("FRONTCRUSHED"), front);
        assert_eq!(flags.contains("BACKCRUSHED"), back);
        assert_eq!(game.presentation.object_sounds.last().unwrap().name, sound);
        assert_eq!(
            game.presentation.object_sounds.last().unwrap().object,
            victim
        );
        assert_eq!(game.random, expected_random);
    }
}

#[test]
fn crush_die_zero_percent_still_draws_but_missing_sound_and_noncrush_do_not() {
    use crate::skirmish::{
        DamageType, Faction, Kind, death_callbacks::CrushRules, tests::fixture_type,
    };
    for (sound, damage, draws, crushed) in [
        ("CarAlarm", DamageType::Crush, true, true),
        ("", DamageType::Crush, false, true),
        ("CarAlarm", DamageType::Explosion, false, false),
    ] {
        let mut game = game();
        let mut unit = fixture_type(Faction::Civilian, Kind::Tank);
        unit.behavior.crush_die.push(CrushRules {
            conditions: death_conditions(),
            sounds: std::array::from_fn(|_| sound.into()),
            percentages: [0; 3],
        });
        register_behavior(&mut unit, "CrushDie", 2);
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let victim = game.spawn(kind, 1, [300.0, 300.0], true);
        let crusher = game.spawn(6, 0, [300.0, 300.0], true);
        let random = game.random;
        game.apply_weapon_hit(victim, crusher, 999_999.0, damage, DeathType::Crushed);
        assert_eq!(game.entities[victim].damage_state.front_crushed, crushed);
        assert_eq!(game.entities[victim].damage_state.back_crushed, crushed);
        assert_eq!(game.random != random, draws);
        assert!(game.presentation.object_sounds.is_empty());
    }
}

#[test]
fn upgrade_die_uses_original_producer_and_authored_filters_without_refunding() {
    let mut game = game();
    let unit = &mut game.catalog[5];
    let mut rejected = death_conditions();
    rejected.required_status = 1 << 1;
    unit.behavior.upgrade_die.push(super::UpgradeRules {
        conditions: rejected,
        upgrade: Some("Keep".into()),
    });
    register_behavior(unit, "UpgradeDie", 2);
    unit.behavior.upgrade_die.push(super::UpgradeRules {
        conditions: death_conditions(),
        upgrade: Some("Release".into()),
    });
    register_behavior(unit, "UpgradeDie", 2);
    let child = game.spawn(5, 1, [300.0, 300.0], true);
    let producer = 1;
    game.entities[child].producer = Some(producer);
    game.grant_native_object_upgrade(producer, "Keep");
    game.grant_native_object_upgrade(producer, "Release");
    let cash = game.players[0].cash;
    game.kill_object(child, DeathType::Normal);
    assert!(game.object_has_upgrade(producer, "Keep"));
    assert!(!game.object_has_upgrade(producer, "Release"));
    assert_eq!(game.players[0].cash, cash);
}
