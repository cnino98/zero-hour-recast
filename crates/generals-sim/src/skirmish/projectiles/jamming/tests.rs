use super::*;
use crate::skirmish::{
    AreaDamage, DamageType, DeathType, WeaponRules,
    microwave::SubdualRules,
    point_defense::tests::{
        missile, missile_from_template, missile_rules, missile_targeted, missile_template,
    },
    tests::game,
};

fn jammer(game: &mut Skirmish) -> usize {
    game.catalog[6].weapon = WeaponRules {
        name: "ECMTankMissileJammer".into(),
        damage: 100.0,
        damage_type: DamageType::SubdualMissile,
        area: Some(AreaDamage {
            at_source: true,
            primary_radius: 150.0,
            secondary_radius: 0.0,
            secondary_damage: 0.0,
            affects: 2 | 4,
        }),
        ..WeaponRules::unarmed()
    };
    game.spawn(6, 0, [405.0, 305.0], true)
}

fn missile_body(game: &mut Skirmish, index: usize, armor: f32) {
    let kind = game.entities[game.projectiles[index].object.unwrap()].type_index;
    game.catalog[kind].armor[DamageType::SubdualMissile as usize] = armor;
    game.catalog[kind].maintenance.subdual = Some(SubdualRules {
        cap: 200.0,
        heal_rate: 3_000,
        heal_amount: 50.0,
    });
}

#[test]
fn ecm_scatter_draws_xy_once_and_retains_missile_owner_health_and_flight() {
    let mut game = game();
    let tank = jammer(&mut game);
    let target = game.spawn(6, 0, [520.0, 305.0], true);
    missile_targeted(
        &mut game,
        [430.0, 305.0, 10.0],
        1,
        1.0,
        Some(target),
        [405.0, 305.0, 0.0],
    );
    missile_body(&mut game, 0, 0.5);
    let before = game.random;
    game.deal_bound_weapon_damage(
        tank,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[tank].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[tank].weapon_slot(),
        },
        None,
        [405.0, 305.0, 0.0],
        1.0,
    );
    assert!(!game.projectiles[0].jammed(&game.entities));
    assert_eq!(game.random, before);
    let mut expected = before;
    let x = 520.0 + crate::skirmish::startup::random_real(&mut expected, -75.0, 75.0);
    let y = 305.0 + crate::skirmish::startup::random_real(&mut expected, -75.0, 75.0);
    game.deal_bound_weapon_damage(
        tank,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[tank].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[tank].weapon_slot(),
        },
        None,
        [405.0, 305.0, 0.0],
        1.0,
    );
    let projectile = &game.projectiles[0];
    assert!(projectile.jammed(&game.entities));
    assert_eq!(projectile.owner, 1);
    assert_eq!(
        game.entities[projectile.object.unwrap()].health.to_bits(),
        100.0_f32.to_bits()
    );
    assert_eq!(
        projectile.destination.map(f32::to_bits),
        [x, y].map(f32::to_bits)
    );
    assert!(projectile.target.is_none());
    assert_eq!(game.random, expected);
    let flight = game.entities[projectile.object.unwrap()]
        .projectile_flight
        .as_ref()
        .unwrap();
    assert!(matches!(flight.controls.first(), Some(Control::Missile(_))));
    assert_eq!(
        flight.motion.position.map(f32::to_bits),
        [430.0_f32, 305.0, 10.0].map(f32::to_bits)
    );
    game.deal_bound_weapon_damage(
        tank,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[tank].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[tank].weapon_slot(),
        },
        None,
        [405.0, 305.0, 0.0],
        1.0,
    );
    assert_eq!(game.random, expected);
    assert_eq!(
        game.projectiles[0].destination.map(f32::to_bits),
        [x, y].map(f32::to_bits)
    );
}

#[test]
fn jamming_uses_retained_dead_objects_but_not_destroyed_tombstones() {
    for (health, masked, destroyed, target_x) in [
        (100.0, true, false, 520.0),
        (0.0, false, false, 520.0),
        (0.0, true, false, 520.0),
        (0.0, true, true, 700.0),
    ] {
        let mut game = game();
        let tank = jammer(&mut game);
        let target = game.spawn(6, 0, [700.0, 305.0], true);
        missile_targeted(
            &mut game,
            [430.0, 305.0, 10.0],
            1,
            1.0,
            Some(target),
            [700.0, 305.0, 0.0],
        );
        missile_body(&mut game, 0, 1.0);
        // The retained move goal is its launch position; a live object lookup
        // sees its later position until the object is actually destroyed.
        game.entities[target].position = [520.0, 305.0];
        game.entities[target].health = health;
        game.entities[target].maintenance.masked = masked;
        if destroyed {
            game.destroy_maintenance_object(target);
            game.process_update_destruction();
        }
        let mut expected = game.random;
        let x = target_x + crate::skirmish::startup::random_real(&mut expected, -75.0, 75.0);
        let y = 305.0 + crate::skirmish::startup::random_real(&mut expected, -75.0, 75.0);
        game.deal_bound_weapon_damage(
            tank,
            crate::skirmish::WeaponBinding {
                type_index: game.entities[tank].type_index,
                weapon_index: 0,
                weapon_slot: game.entities[tank].weapon_slot(),
            },
            None,
            [405.0, 305.0, 0.0],
            1.0,
        );
        assert!(game.projectiles[0].jammed(&game.entities));
        assert_eq!(
            game.projectiles[0].destination.map(f32::to_bits),
            [x, y].map(f32::to_bits)
        );
        assert_eq!(game.random, expected);
    }
}

#[test]
fn coordinate_missile_ignores_recorded_victim_when_jammed() {
    let mut game = game();
    let tank = jammer(&mut game);
    missile(&mut game, [430.0, 305.0, 10.0], 1, 1.0);
    missile_body(&mut game, 0, 1.0);
    // A victim may still be recorded for collision/damage even when the
    // missile launch is a coordinate shot (TryToFollowTarget=false).
    let target = game.spawn(6, 0, [520.0, 405.0], true);
    game.projectiles[0].target = Some(target);
    let mut expected = game.random;
    let x = 405.0 + crate::skirmish::startup::random_real(&mut expected, -75.0, 75.0);
    let y = 305.0 + crate::skirmish::startup::random_real(&mut expected, -75.0, 75.0);
    game.deal_bound_weapon_damage(
        tank,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[tank].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[tank].weapon_slot(),
        },
        None,
        [405.0, 305.0, 0.0],
        1.0,
    );
    assert_eq!(
        game.projectiles[0].destination.map(f32::to_bits),
        [x, y].map(f32::to_bits)
    );
    assert_eq!(game.random, expected);
}

#[test]
fn jam_destination_uses_highest_layer_below_original_z_including_destroyed_deck() {
    for (height, destroyed, expected) in
        [(50.0, false, 40.0), (50.0, true, 40.0), (30.0, false, 0.0)]
    {
        let mut game = game();
        let tank = jammer(&mut game);
        for deck in [40.0, 60.0] {
            game.movement_terrain
                .add_bridge(
                    crate::terrain::BridgeGeometry {
                        from: [300.0, 305.0, deck],
                        to: [600.0, 305.0, deck],
                        corners: [
                            [300.0, 280.0, deck],
                            [300.0, 330.0, deck],
                            [600.0, 280.0, deck],
                            [600.0, 330.0, deck],
                        ],
                    },
                    |_| 0.0,
                )
                .unwrap();
        }
        if destroyed {
            game.movement_terrain.set_bridge_destroyed(2, true).unwrap();
        }
        let mut rules = missile_rules();
        rules.lock_distance = 0.0;
        rules.jam_scatter = 0.0;
        let template = missile_template(rules, 1.0);
        missile_from_template(
            &mut game,
            [430.0, 305.0, 10.0],
            1,
            None,
            [420.0, 305.0, height],
            template,
        );
        missile_body(&mut game, 0, 1.0);
        game.deal_bound_weapon_damage(
            tank,
            crate::skirmish::WeaponBinding {
                type_index: game.entities[tank].type_index,
                weapon_index: 0,
                weapon_slot: game.entities[tank].weapon_slot(),
            },
            None,
            [405.0, 305.0, 0.0],
            1.0,
        );
        let object = game.projectiles[0].object.unwrap();
        let Control::Missile(missile) = &game.entities[object]
            .projectile_flight
            .as_ref()
            .unwrap()
            .controls[0]
        else {
            panic!("missile")
        };
        assert_eq!(
            missile.jam_destination(None).map(f32::to_bits),
            [420.0_f32, 305.0, expected].map(f32::to_bits)
        );
    }
}

#[test]
fn ecm_cylinder_range_uses_original_center_and_max_axis_bounding_sphere() {
    let mut game = game();
    let tank = jammer(&mut game);
    for height in [151.0, 149.0] {
        let mut rules = missile_rules();
        rules.geometry.shape = crate::collision_geometry::Shape::Cylinder;
        rules.geometry.height = 20.0;
        let template = missile_template(rules, 1.0);
        missile_from_template(
            &mut game,
            [405.0, 305.0, height],
            1,
            None,
            [405.0, 305.0, 0.0],
            template,
        );
        let index = game.projectiles.len() - 1;
        missile_body(&mut game, index, 1.0);
    }
    game.deal_bound_weapon_damage(
        tank,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[tank].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[tank].weapon_slot(),
        },
        None,
        [405.0, 305.0, 0.0],
        1.0,
    );
    assert!(!game.projectiles[0].jammed(&game.entities));
    assert!(game.projectiles[1].jammed(&game.entities));
}

#[test]
fn ecm_radius_uses_strict_sphere_distance_and_does_not_jam_allies() {
    let mut game = game();
    let tank = jammer(&mut game);
    missile(&mut game, [430.0, 305.0, 0.0], 0, 1.0);
    missile(&mut game, [557.0, 305.0, 0.0], 1, 1.0);
    missile(&mut game, [556.5, 305.0, 0.0], 1, 1.0);
    for index in 0..3 {
        missile_body(&mut game, index, 1.0);
    }
    game.deal_bound_weapon_damage(
        tank,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[tank].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[tank].weapon_slot(),
        },
        None,
        [405.0, 305.0, 0.0],
        1.0,
    );
    assert!(!game.projectiles[0].jammed(&game.entities));
    assert!(!game.projectiles[1].jammed(&game.entities));
    assert!(game.projectiles[2].jammed(&game.entities));
}

#[test]
fn ecm_vehicle_subdual_stops_and_recovers_without_reducing_health() {
    let mut game = game();
    game.catalog[6].maintenance.subdual = Some(SubdualRules {
        cap: 200.0,
        heal_rate: 15,
        heal_amount: 50.0,
    });
    game.catalog[6].armor[DamageType::SubdualVehicle as usize] = 1.0;
    let source = game.spawn(6, 0, [405.0, 305.0], true);
    let target = game.spawn(6, 1, [445.0, 305.0], true);
    let health = game.entities[target].health;
    for _ in 0..5 {
        game.apply_weapon_hit(
            target,
            source,
            24.0,
            DamageType::SubdualVehicle,
            DeathType::Normal,
        );
    }
    assert!(game.entities[target].subdued());
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
    for _ in 0..15 {
        game.tick += 1;
        game.run_object_updates();
    }
    assert!(!game.entities[target].subdued());
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
}

#[test]
fn missile_splash_keeps_launch_owner_after_launcher_capture() {
    let mut game = game();
    let source = game.spawn(6, 0, [300.0, 200.0], true);
    let original_ally = game.spawn(5, 0, [430.0, 305.0], true);
    let original_enemy = game.spawn(5, 1, [440.0, 305.0], true);
    game.catalog[6].weapon = WeaponRules {
        damage: 20.0,
        damage_type: DamageType::Explosion,
        area: Some(AreaDamage {
            at_source: false,
            primary_radius: 100.0,
            secondary_radius: 0.0,
            secondary_damage: 0.0,
            affects: 2,
        }),
        ..WeaponRules::unarmed()
    };
    missile(&mut game, [435.0, 305.0, 10.0], 0, 1.0);
    let object = game.projectiles[0].object.unwrap();
    game.entities[source].owner = 1;
    game.deal_bound_weapon_damage_with_radius(
        &crate::skirmish::damage::Source::Projectile {
            launcher: source,
            object,
        },
        crate::skirmish::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        None,
        [435.0, 305.0, 0.0],
        1.0,
        1.0,
    );
    assert_eq!(
        game.entities[original_ally].health.to_bits(),
        100.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[original_enemy].health.to_bits(),
        80.0_f32.to_bits()
    );
}

#[test]
fn airborne_projectile_upgrade_uses_launch_owner_after_launcher_capture() {
    let mut game = game();
    let mut template = missile_template(missile_rules(), 1.0);
    template.maintenance.weapon_upgrade = Some(crate::skirmish::maintenance::WeaponUpgrade {
        trigger: "Upgrade_AmericaLaserMissiles".into(),
    });
    crate::skirmish::tests::register_behavior(&mut template, "WeaponBonusUpgrade", 0x80);
    template
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers
        .push("Upgrade_AmericaLaserMissiles".into());
    missile_from_template(
        &mut game,
        [430.0, 305.0, 10.0],
        1,
        None,
        [405.0, 305.0, 0.0],
        template,
    );
    let source = game.projectiles[0].source;
    let object = game.projectiles[0].object.unwrap();
    let kind = game.entities[source].type_index;
    let weapon = &mut game.catalog[kind].weapon;

    weapon.bonuses.template[5][crate::skirmish::containment::bonuses::DAMAGE] = 1.25;
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[0]).to_bits(),
        1.0_f32.to_bits()
    );
    game.entities[source].owner = 0;
    game.players[0]
        .upgrades
        .push("Upgrade_AmericaLaserMissiles".into());
    game.activate_upgrade_modules(object);
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[0]).to_bits(),
        1.0_f32.to_bits()
    );
    game.players[1]
        .upgrades
        .push("Upgrade_AmericaLaserMissiles".into());
    game.activate_upgrade_modules(object);
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[0]).to_bits(),
        1.25_f32.to_bits()
    );
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[0]).to_bits(),
        1.25_f32.to_bits()
    );
    assert!(!game.entities[source].weapon_upgrade_active());
}

#[test]
fn projectile_subdual_contract_rejects_nonfinite_armor_and_body_amounts() {
    let mut game = game();
    missile(&mut game, [430.0, 305.0, 10.0], 1, 1.0);
    missile_body(&mut game, 0, 1.0);
    let kind = game.entities[game.projectiles[0].object.unwrap()].type_index;
    assert!(crate::skirmish::validate_catalog(&game.catalog, &game.rules).is_ok());
    game.catalog[kind].armor[DamageType::SubdualMissile as usize] = f32::NAN;
    assert!(crate::skirmish::validate_catalog(&game.catalog, &game.rules).is_err());
    game.catalog[kind].armor[DamageType::SubdualMissile as usize] = 1.0;
    game.catalog[kind]
        .maintenance
        .subdual
        .as_mut()
        .unwrap()
        .heal_amount = f32::INFINITY;
    assert!(crate::skirmish::validate_catalog(&game.catalog, &game.rules).is_err());
}

#[test]
fn designated_victim_overrides_stale_impact_center_for_missile_subdual() {
    let mut game = game();
    let source = jammer(&mut game);
    let victim = game.spawn(6, 1, [620.0, 305.0], true);
    missile(&mut game, [640.0, 305.0, 10.0], 1, 1.0);
    missile_body(&mut game, 0, 1.0);
    missile(&mut game, [320.0, 305.0, 10.0], 1, 1.0);
    missile_body(&mut game, 1, 1.0);
    game.deal_bound_weapon_damage(
        source,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        Some(victim),
        [320.0, 305.0, 0.0],
        1.0,
    );
    assert!(game.projectiles[0].jammed(&game.entities));
    assert!(!game.projectiles[1].jammed(&game.entities));
}

#[test]
fn inflight_missile_capture_keeps_original_relations_and_upgrade_on_actual_detonation() {
    let mut game = game();
    let ally = game.spawn(5, 0, [410.0, 305.0], true);
    let enemy = game.spawn(5, 1, [420.0, 305.0], true);
    let mut rules = missile_rules();
    rules.fuel_lifetime = 1;
    rules.detonate_on_no_fuel = true;
    let mut template = missile_template(rules, 1.0);
    template.maintenance.weapon_upgrade = Some(crate::skirmish::maintenance::WeaponUpgrade {
        trigger: "Upgrade_AmericaLaserMissiles".into(),
    });
    crate::skirmish::tests::register_behavior(&mut template, "WeaponBonusUpgrade", 0x80);
    template
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers
        .push("Upgrade_AmericaLaserMissiles".into());
    missile_from_template(
        &mut game,
        [400.0, 305.0, 40.0],
        0,
        None,
        [405.0, 305.0, 0.0],
        template,
    );
    let object = game.projectiles[0].object.unwrap();
    let registration = game.catalog[game.entities[object].type_index].update_modules.iter().position(|entry| matches!(entry, crate::skirmish::module_updates::ModuleRegistration::Behavior {family, ..} if family == "MissileAIUpdate")).unwrap();
    let launcher = game.projectiles[0].source;
    let kind = game.entities[launcher].type_index;
    let weapon = &mut game.catalog[kind].weapon;
    weapon.damage = 20.0;
    weapon.damage_type = DamageType::Explosion;
    weapon.area = Some(AreaDamage {
        at_source: false,
        primary_radius: 100.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 2,
    });

    weapon.bonuses.template[5][crate::skirmish::containment::bonuses::DAMAGE] = 1.25;
    game.tick = 1;
    game.update_missile_module(object, registration);
    assert_eq!(game.projectiles.len(), 1);
    game.entities[launcher].owner = 1;
    game.players[0]
        .upgrades
        .push("Upgrade_AmericaLaserMissiles".into());
    game.activate_upgrade_modules(object);
    game.tick = 2;
    game.update_missile_module(object, registration);
    assert_eq!(game.entities[ally].health.to_bits(), 100.0_f32.to_bits());
    assert_eq!(game.entities[enemy].health.to_bits(), 75.0_f32.to_bits());
    assert_eq!(game.projectiles[0].owner, 0);
}
