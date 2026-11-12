use super::*;
use crate::skirmish::{DamageType, DeathType, WeaponRules, tests::game};

#[test]
fn projectile_subdual_uses_body_threshold_without_disabling_object_updates() {
    let mut game = game();
    game.catalog[5].kind_of = vec!["PROJECTILE".into()];
    game.catalog[5].maintenance.subdual = Some(SubdualRules {
        cap: 150.0,
        heal_rate: 3,
        heal_amount: 10.0,
    });
    let id = game.spawn(5, 0, [300.0, 300.0], true);
    game.apply_subdual_damage(id, 110.0);
    assert_eq!(
        game.entities[id].subdual_damage().to_bits(),
        110.0_f32.to_bits()
    );
    assert!(!game.entities[id].subdued());
    assert!(game.entities[id].gaining_subdual_damage());
    for frame in 1..=6 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[id].subdual_damage().to_bits(),
        90.0_f32.to_bits()
    );
    assert!(!game.entities[id].subdued());
    assert!(!game.entities[id].gaining_subdual_damage());
    assert_eq!(game.entities[id].health.to_bits(), 100.0_f32.to_bits());
}

#[test]
fn inactive_indestructible_and_effectively_dead_bodies_reject_subdual_before_notification() {
    let mut game = game();
    game.catalog[5].maintenance.subdual = Some(SubdualRules {
        cap: 150.0,
        heal_rate: 3,
        heal_amount: 10.0,
    });
    for condition in 0..3 {
        let id = game.spawn(5, 0, [300.0, 300.0], true);
        match condition {
            0 => game.set_map_body_controls(id, true, true),
            1 => game.entities[id].set_effectively_dead(true),
            _ => {
                game.catalog[5].body_kind = crate::skirmish::BodyKind::Inactive;
            }
        }
        game.apply_weapon_hit(id, 2, 150.0, DamageType::SubdualMissile, DeathType::Normal);
        assert_eq!(
            game.entities[id].subdual_damage().to_bits(),
            0.0_f32.to_bits()
        );
        assert!(!game.entities[id].gaining_subdual_damage());
        assert!(!game.entities[id].subdued());
        assert_eq!(game.entities[id].maintenance.microwave.heal_countdown, 0);
    }
}

fn emitter() -> crate::skirmish::fire::weapon_update::Rules {
    crate::skirmish::fire::weapon_update::Rules {
        weapon: Some(crate::skirmish::WeaponRules {
            name: "MicrowaveTankEmitterWeapon".into(),
            damage: 8.0,
            delay: 0.25,
            damage_type: DamageType::Microwave,
            death_type: DeathType::Burned,
            area: Some(crate::skirmish::AreaDamage {
                at_source: true,
                primary_radius: 100.0,
                secondary_radius: 0.0,
                secondary_damage: 0.0,
                affects: 2 | 64,
            }),
            ..crate::skirmish::WeaponRules::unarmed()
        }),
        initial_delay: 0,
        exclusive_delay: 30,
    }
}

fn register_emitter(game: &mut crate::skirmish::Skirmish, type_index: usize) {
    game.catalog[type_index].behavior.fire_weapons = vec![emitter()];
    crate::skirmish::tests::register_update(&mut game.catalog[type_index], "FireWeaponUpdate");
}

#[test]
fn original_emitter_starts_at_thirty_fires_every_eight_and_only_real_weapons_suppress_it() {
    let mut game = game();
    register_emitter(&mut game, 6);
    let tank = game.spawn(6, 0, [300.0, 200.0], true);
    let target = game.spawn(5, 1, [340.0, 200.0], true);
    let ally = game.spawn(5, 0, [340.0, 220.0], true);
    game.update_ground_contacts();
    for _ in 0..29 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert!(game.presentation.shots.is_empty());
    game.tick = 30;
    game.run_object_updates();
    assert_eq!(game.presentation.shots.len(), 1);
    game.tick = 37;
    game.run_object_updates();
    assert_eq!(game.presentation.shots.len(), 1);
    game.tick = 38;
    game.run_object_updates();
    assert_eq!(game.presentation.shots.len(), 2);
    assert_eq!(game.entities[target].health.to_bits(), 84.0_f32.to_bits());
    assert_eq!(game.entities[ally].health.to_bits(), 100.0_f32.to_bits());
    game.tick = 45;
    game.fire_weapon(tank, target);
    let before = game.presentation.shots.len();
    game.tick = 74;
    game.run_object_updates();
    assert_eq!(game.presentation.shots.len(), before);
    game.tick = 75;
    game.run_object_updates();
    assert_eq!(game.presentation.shots.len(), before + 1);
    game.set_hacked_until(tank, crate::skirmish::ai_dispatch::native_frame(100));
    game.tick = 83;
    game.run_object_updates();
    assert_eq!(game.presentation.shots.len(), before + 1);
    game.clear_hacked_disable(tank);
    game.tick = 100;
    game.run_object_updates();
    assert_eq!(game.presentation.shots.len(), before + 2);
}

#[test]
fn emitter_airborne_filter_uses_physical_gravity_threshold_with_exact_boundary() {
    for (height, hit) in [(9.0, true), (9.25, false)] {
        let mut game = game();
        game.gravity = -1.0;
        register_emitter(&mut game, 6);
        game.catalog[5].preferred_height = height;
        game.catalog[5].locomotor.airborne_targeting_height = 1000.0;
        game.spawn(6, 0, [300.0, 200.0], true);
        let target = game.spawn(5, 1, [340.0, 200.0], true);
        game.update_ground_contacts();
        game.tick = 30;
        game.run_object_updates();
        assert_eq!(
            game.entities[target].health.to_bits(),
            if hit { 92.0_f32 } else { 100.0_f32 }.to_bits()
        );
    }
}

#[test]
fn subdual_reactor_edge_removes_only_positive_energy_and_restores_on_healing() {
    let mut game = game();
    game.catalog[3].energy = 10;
    game.catalog[4].energy = -2;
    game.catalog[3].maintenance.subdual = Some(SubdualRules {
        cap: 2000.0,
        heal_rate: 15,
        heal_amount: 100.0,
    });
    let reactor = game.spawn(3, 0, [300.0, 200.0], true);
    game.spawn(4, 0, [500.0, 200.0], true);
    game.update_power();
    assert_eq!(game.players[0].power, 8);
    game.apply_subdual_damage(reactor, 1000.0);
    assert!(game.entities[reactor].subdued());
    assert_eq!(game.players[0].power, -2);
    for _ in 0..15 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert!(!game.entities[reactor].subdued());
    assert_eq!(game.players[0].power, 8);
}

#[test]
fn subdual_disables_existing_satellite_spy_and_permanent_vision_returns_after_recovery() {
    let mut game = game();
    game.catalog[3]
        .behavior
        .spy_vision
        .push(crate::skirmish::spy_vision::Rules {
            needs_upgrade: true,
            self_powered: true,
            trigger: "Satellite".into(),
            duration: 0,
            interval: 0,
            command_centers_only: false,
        });
    game.catalog[3].maintenance.subdual = Some(SubdualRules {
        cap: 2000.0,
        heal_rate: 15,
        heal_amount: 100.0,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[3], "SpyVisionUpdate");
    game.catalog[3]
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers
        .push("Satellite".into());
    game.players[0].upgrades.push("Satellite".into());
    let target = game.spawn(5, 1, [500.0, 400.0], true);
    let spy = game.spawn(3, 0, [300.0, 200.0], true);
    assert_ne!(game.entities[target].vision_spy_mask() & 1, 0);
    game.apply_subdual_damage(spy, 1000.0);
    assert_eq!(game.entities[target].vision_spy_mask() & 1, 0);
    for _ in 0..16 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert!(!game.entities[spy].subdued());
    assert_ne!(game.entities[target].vision_spy_mask() & 1, 0);
}

#[test]
fn direct_clear_chooses_secondary_floors_raw_count_and_preserves_immune_garrison_health() {
    for immune in [false, true] {
        let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 0.5);
        let kind = game.entities[carrier].type_index;
        game.catalog[kind].contain.as_mut().unwrap().garrison =
            Some(crate::skirmish::containment::GarrisonRules {
                allow_enemies: false,
                allow_neutral: true,
                immune_to_clear: immune,
                heal_frames: None,
                stations: None,
            });
        game.entities[carrier].owner = 1;
        game.catalog[5].veterancy.experience_value = [25; 4];
        game.catalog[6].veterancy.trainable = true;
        let first = game.spawn(5, 1, [300.0, 200.0], true);
        let second = game.spawn(5, 1, [300.0, 220.0], true);
        game.board(first, carrier);
        game.board(second, carrier);
        game.catalog[6].weapon.damage_type = DamageType::SubdualBuilding;
        game.catalog[6].secondary_weapon = Some(WeaponRules {
            damage: 1.9,
            damage_type: DamageType::KillGarrisoned,
            range: 125.0,
            auto_choose: 7,
            ..WeaponRules::unarmed()
        });
        let tank = game.spawn(6, 0, [300.0, 300.0], true);
        assert_eq!(game.choose_weapon(tank, carrier), !immune);
        if !immune {
            assert_eq!(game.entities[tank].weapon_slot(), 1);
        }
        let health = game.entities[carrier].health;
        game.apply_weapon_hit(
            carrier,
            tank,
            0.9,
            DamageType::KillGarrisoned,
            DeathType::Normal,
        );
        assert!(game.entities[first].alive() && game.entities[second].alive());
        game.apply_weapon_hit(
            carrier,
            tank,
            1.9,
            DamageType::KillGarrisoned,
            DeathType::Normal,
        );
        assert_eq!(game.entities[first].alive(), immune);
        assert!(game.entities[second].alive());
        assert_eq!(game.entities[carrier].health.to_bits(), health.to_bits());
        assert_eq!(
            game.players[0].statistics.units_destroyed,
            u32::from(!immune)
        );
        assert_eq!(game.entities[tank].experience, if immune { 0 } else { 25 });
    }
}

#[test]
fn held_microwave_heals_subdual_but_emits_only_after_ordinary_container_exit() {
    let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 0.5);
    let kind = game.entities[carrier].type_index;
    game.catalog[kind]
        .contain
        .as_mut()
        .unwrap()
        .allowed
        .push("VEHICLE".into());
    game.catalog[6].weapon.damage = 0.0;
    register_emitter(&mut game, 6);
    game.catalog[6].maintenance.subdual = Some(SubdualRules {
        cap: 200.0,
        heal_rate: 1,
        heal_amount: 50.0,
    });
    let tank = game.spawn(6, 0, [300.0, 250.0], true);
    assert!(game.can_enter(tank, carrier));
    game.board(tank, carrier);
    game.tick = 30;
    game.apply_subdual_damage(tank, 100.0);
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[tank].subdued());
    assert_eq!(
        game.entities[tank].subdual_damage().to_bits(),
        50.0_f32.to_bits()
    );
    assert!(game.presentation.shots.is_empty());
    game.exit_container(tank).unwrap();
    for _ in 0..120 {
        game.step();
        if game.entities[tank].contained_by().is_none() {
            break;
        }
        assert!(
            game.presentation
                .shots
                .iter()
                .all(|shot| shot.source != tank)
        );
    }
    assert!(game.entities[tank].contained_by().is_none());
    let mut emitted = false;
    for _ in 0..9 {
        game.step();
        emitted |= game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == tank);
    }
    assert!(emitted);
}

#[test]
fn subdual_suppresses_all_radar_providers_and_recovery_restores_them() {
    for disable_proof in [false, true] {
        let mut game = game();
        crate::skirmish::tests::radar_upgrade(
            &mut game.catalog[3],
            "Upgrade_SubdualRadar",
            true,
            disable_proof,
        );
        game.catalog[3].maintenance.subdual = Some(SubdualRules {
            cap: 2000.0,
            heal_rate: 1,
            heal_amount: 100.0,
        });
        let provider = game.spawn(3, 0, [300.0, 200.0], true);
        assert!(game.has_radar(0));
        game.apply_subdual_damage(provider, 1000.0);
        // DisableProof bypasses player radar disable, not Object::onDisabledEdge.
        assert!(!game.has_radar(0));
        game.tick += 1;
        game.run_object_updates();
        assert!(game.has_radar(0));
    }
}

#[test]
fn subdual_caps_without_health_damage_resets_healing_and_recovers_while_disabled() {
    let mut game = game();
    game.catalog[6].maintenance.subdual = Some(SubdualRules {
        cap: 200.0,
        heal_rate: 15,
        heal_amount: 50.0,
    });
    let target = game.spawn(6, 1, [400.0, 300.0], true);
    let health = game.entities[target].health;
    game.apply_weapon_hit(
        target,
        0,
        250.0,
        DamageType::SubdualBuilding,
        DeathType::Normal,
    );
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
    assert_eq!(
        game.entities[target].subdual_damage().to_bits(),
        200.0_f32.to_bits()
    );
    assert!(game.entities[target].subdued());
    for _ in 0..14 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[target].subdual_damage().to_bits(),
        200.0_f32.to_bits()
    );
    game.apply_subdual_damage(target, 1.0);
    for _ in 0..15 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[target].subdual_damage().to_bits(),
        150.0_f32.to_bits()
    );
    assert!(game.entities[target].subdued());
    for _ in 0..30 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert!(!game.entities[target].subdued());
    for _ in 0..15 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[target].subdual_damage().to_bits(),
        0.0_f32.to_bits()
    );
}

#[test]
fn subdual_uses_armor_without_battle_plan_scalar_and_ignores_bodies_without_cap() {
    let mut game = game();
    let first = game.spawn(6, 0, [300.0, 200.0], true);
    game.catalog[6].maintenance.subdual = Some(SubdualRules {
        cap: 200.0,
        heal_rate: 15,
        heal_amount: 50.0,
    });
    game.catalog[6].armor[DamageType::SubdualBuilding as usize] = 0.5;
    game.entities[first].battle_bonuses.armor = 0.8;
    game.apply_weapon_hit(
        first,
        0,
        50.0,
        DamageType::SubdualBuilding,
        DeathType::Normal,
    );
    assert_eq!(
        game.entities[first].subdual_damage().to_bits(),
        25.0_f32.to_bits()
    );
    let second = game.spawn(5, 0, [400.0, 200.0], true);
    let health = game.entities[second].health;
    game.apply_weapon_hit(
        second,
        0,
        1000.0,
        DamageType::SubdualBuilding,
        DeathType::Normal,
    );
    assert_eq!(
        game.entities[second].subdual_damage().to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(game.entities[second].health.to_bits(), health.to_bits());
}
