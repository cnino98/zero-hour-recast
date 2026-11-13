use super::*;
use crate::skirmish::DeathType;
use crate::skirmish::{AreaDamage, DamageType, Kind, WeaponRules, death_lifecycle};

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = crate::skirmish::tests::game();
    let unit = &mut game.catalog[5];
    unit.kind = Kind::Defense;
    unit.kind_of = vec!["STRUCTURE".into(), "DEMOTRAP".into()];
    unit.demo_trap = vec![Rules {
        slots: [0, 1, 2],
        proximity_default: true,
        friendly_detonation: true,
        detonate_when_killed: true,
        range: 40.0,
        scan_frames: 0,
        ignore_kind: vec!["PROJECTILE".into(), "UNATTACKABLE".into()],
        detonation_weapon: None,
    }];
    crate::skirmish::tests::register_update(unit, "DemoTrapUpdate");
    let dummy = || WeaponRules {
        name: "DummyWeapon".into(),
        auto_choose: 0,
        ..WeaponRules::unarmed()
    };
    unit.weapon = dummy();
    unit.secondary_weapon = Some(dummy());
    unit.tertiary_weapon = Some(dummy());
    unit.behavior.instant_death = vec![crate::skirmish::instant_death::Rules {
        death_mask: u32::MAX,
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 1 << 3,
        effects: Vec::new(),
        objects: Vec::new(),
        weapons: Vec::new(),
    }];
    crate::skirmish::tests::register_behavior(unit, "InstantDeathBehavior", 2);
    unit.death_lifecycle = Some(death_lifecycle::Rules {
        on_die: Vec::new(),
        physics: None,
        slow: vec![death_lifecycle::SlowDeath {
            derived: crate::skirmish::death_lifecycle::Derived::None,
            tag: "Slow".into(),
            overkill_bonus: 0.0,
            conditions: crate::skirmish::death_callbacks::Conditions {
                death_mask: u32::MAX,
                veterancy_mask: 15,
                exempt_status: 1 << 3,
                required_status: 0,
            },
            probability: 10,
            sink_delay: [0, 0],
            destruction_delay: [30, 0],
            sink_rate: 0.0,
            fling: None,
            fx: [vec!["FX_GLADemoTrapWarning".into()], vec![], vec![]],
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: [vec![], vec![], vec![0]],
        }],
        weapons: vec![WeaponRules {
            name: "DemoTrapDetonationWeapon".into(),
            damage: 600.0,
            damage_type: DamageType::Explosion,
            death_type: DeathType::Exploded,
            area: Some(AreaDamage {
                at_source: false,
                primary_radius: 25.0,
                secondary_radius: 50.0,
                secondary_damage: 400.0,
                affects: 15,
            }),
            ..WeaponRules::unarmed()
        }],
    });
    let trap = game.spawn(5, 0, [300.0, 300.0], true);
    let victim = game.spawn(6, 1, [320.0, 300.0], true);
    game.entities[victim].health = 2_000.0;
    game.update_ground_contacts();
    (game, trap, victim)
}

fn advance(game: &mut Skirmish) {
    game.tick += 1;
    game.run_object_updates();
}

#[test]
fn selling_suppresses_proximity_detonation_before_the_trap_sinks() {
    let (mut game, trap, victim) = fixture();
    let health = game.entities[victim].health;
    game.sell(0, trap).unwrap();
    for _ in 0..31 {
        advance(&mut game);
    }
    assert!(!game.entities[trap].sold_model());
    assert!(game.entities[trap].alive());
    assert_eq!(game.entities[victim].health.to_bits(), health.to_bits());
    assert!(game.presentation.object_fx_events.is_empty());
}

#[test]
fn proximity_warns_then_fires_exactly_once_after_thirty_frames_without_physics() {
    let (mut game, trap, victim) = fixture();
    assert_eq!(game.entities[trap].weapon_slot(), 1);
    assert_eq!(game.entities[trap].veterancy, 0);
    assert!(matches!(
        game.entities[trap].weapon_set.lock,
        weapons::Lock::Temporary
    ));
    advance(&mut game);
    assert!(!game.entities[trap].alive());
    assert!(game.entities[trap].corpse_present());
    assert!(game.entities[trap].corpse_height().is_none());
    assert_eq!(
        game.presentation.object_fx_events[0].effect,
        "FX_GLADemoTrapWarning"
    );
    assert!(game.presentation.shots.is_empty());
    game.tick = 30;
    game.update_deaths(game.entities.len());
    assert_eq!(
        game.entities[victim].health.to_bits(),
        2_000.0_f32.to_bits()
    );
    game.tick = 31;
    game.update_deaths(game.entities.len());
    assert_eq!(
        game.entities[victim].health.to_bits(),
        1_400.0_f32.to_bits()
    );
    assert_eq!(game.presentation.shots.len(), 1);
    assert!(game.entities[trap].masked());
    game.tick = 31;
    advance(&mut game);
    game.update_deaths(game.entities.len());
    assert_eq!(game.presentation.shots.len(), 1);
}

#[test]
fn manual_mode_allows_dummy_slot_selection_and_detonate_bypasses_scan_cooldown() {
    let (mut game, trap, _) = fixture();
    game.switch_weapon(trap, 2).unwrap();
    advance(&mut game);
    assert!(game.entities[trap].alive());
    game.entities[trap]
        .behaviors
        .demo_trap
        .first_mut()
        .unwrap()
        .scan_frames = 99;
    game.switch_weapon(trap, 0).unwrap();
    advance(&mut game);
    assert!(!game.entities[trap].alive());
    assert_eq!(game.presentation.object_fx_events.len(), 1);
}

#[test]
fn unfinished_and_disarmed_traps_never_execute_warning_or_final_blast() {
    let (mut game, trap, victim) = fixture();
    game.entities[trap].build_progress = 0.5;
    advance(&mut game);
    assert!(game.entities[trap].alive());
    game.apply_native_damage(trap, Some(victim), 9_999.0, DeathType::Normal);
    assert!(game.update_objects[game.entities[trap].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(game.entities[trap].masked());
    game.tick = 30;
    game.update_deaths(game.entities.len());
    assert!(game.presentation.object_fx_events.is_empty());
    assert!(game.presentation.shots.is_empty());

    let (mut game, trap, dozer) = fixture();
    game.catalog[6].weapon.damage_type = DamageType::Disarm;
    game.fire_weapon(dozer, trap);
    assert!(game.update_objects[game.entities[trap].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(game.entities[trap].masked());
    assert!(!game.entities[trap].corpse_present());
    assert!(game.presentation.object_fx_events.is_empty());
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(
        game.presentation.shots[0].source_position.map(f32::to_bits),
        game.object_position(trap).map(f32::to_bits)
    );
    game.tick = 31;
    advance(&mut game);
    game.update_deaths(game.entities.len());
    assert_eq!(game.presentation.shots.len(), 1);
}

#[test]
fn proximity_ignores_airborne_projectiles_and_attacking_disarm_dozer() {
    let (mut game, trap, victim) = fixture();
    game.set_object_position(victim, [320.0, 300.0, 0.01]);
    game.tick += 1;
    game.update_demo_trap(trap, 0);
    assert!(game.entities[trap].alive());
    game.set_object_position(victim, [320.0, 300.0, 0.0]);
    game.catalog[6].kind_of.push("PROJECTILE".into());
    game.tick += 1;
    game.update_demo_trap(trap, 0);
    assert!(game.entities[trap].alive());
    game.catalog[6].kind_of = vec!["DOZER".into()];
    game.catalog[6].weapon.damage_type = DamageType::Disarm;
    // Enter the real Attack owner before the trap's isolated NORMAL callback.
    game.attack(victim, trap).unwrap();
    assert!(game.entities[victim].attacking());
    game.tick += 1;
    game.update_demo_trap(trap, 0);
    assert!(game.entities[trap].alive());
    // Merely retaining the disarm weapon does not protect an idle dozer.
    game.stop(victim).unwrap();
    game.tick += 1;
    game.update_demo_trap(trap, 0);
    assert!(!game.entities[trap].alive());
}

#[test]
fn scan_clock_expires_in_manual_mode_and_proximity_uses_center_distance() {
    let (mut game, trap, victim) = fixture();
    game.catalog[5].demo_trap.first_mut().unwrap().scan_frames = 3;
    game.entities[victim].position = [341.0, 300.0];
    game.update_ground_contacts();
    advance(&mut game);
    assert!(game.entities[trap].alive());
    game.switch_weapon(trap, 2).unwrap();
    for _ in 0..10 {
        advance(&mut game);
    }
    assert_eq!(
        game.entities[trap]
            .behaviors
            .demo_trap
            .first()
            .unwrap()
            .scan_frames,
        0
    );
    // The real preceding Idle callback snaps the actor to its navigation cell.
    // Place this target relative to that current center, not the birth position.
    let center = game.entities[trap].position;
    game.entities[victim].position = [center[0] + 40.0, center[1]];
    assert_eq!(
        distance_squared(center, game.entities[victim].position).to_bits(),
        1600.0_f32.to_bits()
    );
    game.update_ground_contacts();
    game.switch_weapon(trap, 1).unwrap();
    advance(&mut game);
    // PartitionManager's admission is strict even though DemoTrap's inner
    // redundant distance comparison permits equality.
    assert!(game.entities[trap].alive());
    game.entities[victim].position = [center[0] + 39.0, center[1]];
    game.update_ground_contacts();
    for _ in 0..4 {
        advance(&mut game);
    }
    assert!(!game.entities[trap].alive());
}

#[test]
fn killing_a_trap_during_its_warning_does_not_repeat_the_death_callbacks() {
    let (mut game, trap, victim) = fixture();
    game.apply_native_damage(trap, Some(victim), 9_999.0, DeathType::Normal);
    advance(&mut game);
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    game.tick = 15;
    game.apply_native_damage(trap, Some(victim), 9_999.0, DeathType::Normal);
    advance(&mut game);
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    game.tick = 30;
    game.update_deaths(game.entities.len());
    assert_eq!(game.presentation.shots.len(), 1);
}

#[test]
fn authored_instances_keep_independent_scan_clocks_and_last_created_mode() {
    let (mut game, _, victim) = fixture();
    game.catalog[5].demo_trap[0].scan_frames = 3;
    game.catalog[5].demo_trap.push(Rules {
        slots: [0, 1, 2],
        proximity_default: false,
        friendly_detonation: true,
        detonate_when_killed: true,
        range: 40.0,
        scan_frames: 5,
        ignore_kind: Vec::new(),
        detonation_weapon: None,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[5], "DemoTrapUpdate");
    let trap = game.spawn(5, 0, [600.0, 600.0], true);
    assert_eq!(game.entities[trap].behaviors.demo_trap.len(), 2);
    assert_eq!(game.entities[trap].weapon_slot(), 2);
    assert_eq!(game.entities[trap].veterancy, 0);
    assert!(game.entities[trap].weapon_set.forced_veteran);
    game.switch_weapon(trap, 1).unwrap();
    advance(&mut game);
    assert_eq!(game.entities[trap].behaviors.demo_trap[0].scan_frames, 3);
    assert_eq!(game.entities[trap].behaviors.demo_trap[1].scan_frames, 5);
    advance(&mut game);
    assert_eq!(game.entities[trap].behaviors.demo_trap[0].scan_frames, 2);
    assert_eq!(game.entities[trap].behaviors.demo_trap[1].scan_frames, 4);
    assert!(game.entities[victim].alive());
}

#[test]
fn explicit_detonation_weapon_uses_full_payload_before_native_kill() {
    let (mut game, trap, victim) = fixture();
    game.catalog[5].demo_trap[0].detonation_weapon = Some(WeaponRules {
        name: "ImmediateTrapWeapon".into(),
        damage: 125.0,
        damage_type: DamageType::Explosion,
        area: Some(AreaDamage {
            at_source: false,
            primary_radius: 30.0,
            secondary_radius: 30.0,
            secondary_damage: 125.0,
            affects: 15,
        }),
        ..WeaponRules::unarmed()
    });
    game.switch_weapon(trap, 0).unwrap();
    advance(&mut game);
    assert_eq!(game.entities[victim].health.to_bits(), 1875.0_f32.to_bits());
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(
        game.presentation.shots[0].weapon_index,
        game.catalog[5].demo_weapon_start()
    );
    assert!(!game.entities[trap].alive());
    assert!(game.entities[trap].behaviors.demo_trap[0].detonated);
    advance(&mut game);
    assert_eq!(game.presentation.shots.len(), 1);
}
