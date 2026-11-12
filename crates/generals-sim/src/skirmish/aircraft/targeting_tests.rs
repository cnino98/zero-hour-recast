//! Weapon.cpp's object-to-position conversion during the Aurora miss deadline.
use super::{LocomotorSet, Rules};
use crate::{
    collision_geometry::Body,
    skirmish::{self, Kind, LaunchTransform, Skirmish, WeaponMotion},
};

fn scenario() -> (Skirmish, usize, usize) {
    let mut game = skirmish::tests::game();
    let source_type = game.type_for(0, Kind::Infantry).unwrap();
    let target_type = game.type_for(1, Kind::Tank).unwrap();
    let taxi = skirmish::tests::fixture_type(skirmish::Faction::Usa, Kind::Tank).locomotor;
    game.catalog[target_type].aircraft = Some(Rules {
        taxi: LocomotorSet {
            role: crate::skirmish::locomotor::set::TAXI,
            locomotor: taxi,
            speed: 50.0,
        },
        attacking: None,
        returning: None,
        needs_runway: true,
        keeps_space: true,
        helipad: false,
        takeoff_pause: 0,
        idle_return: 0,
        out_of_ammo_damage: 0.0,
        min_height: 0.0,
        parking_offset: 0.0,
        attack_locomotor_frames: 3,
        attackers_miss_frames: 60,
        sneaky_offset: -20.0,
        lockon: super::lockon::Rules::native(),
        sounds: super::Sounds {
            afterburner: String::new(),
            low_fuel: String::new(),
            wheel_screech: String::new(),
        },
    });
    for entry in &mut game.catalog[target_type].update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } = entry
            && family == "AIUpdateInterface"
        {
            *family = "JetAIUpdate".into();
        }
    }
    for entry in &mut game.catalog[target_type].behavior_modules {
        if entry.family == "AIUpdateInterface" {
            entry.family = "JetAIUpdate".into();
        }
    }
    game.catalog[target_type]
        .kind_of
        .extend(["AIRCRAFT".into(), "VEHICLE".into()]);
    game.catalog[target_type].locomotor.ground = None;
    game.catalog[target_type].locomotor.flight = Some(skirmish::flight::tests::rules(
        skirmish::flight::Appearance::Wings,
    ));
    game.catalog[target_type].locomotor.legal_surfaces = 8;
    crate::skirmish::aircraft_death::tests::install_physics(&mut game.catalog[target_type]);
    game.catalog[target_type].geometry_bounds.center_height = 12.0;
    let weapon = &mut game.catalog[source_type].weapon;
    weapon.range = 500.0;
    weapon.speed = 300.0;
    weapon.clip_size = 4;
    weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| vec![LaunchTransform::IDENTITY]),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: None,
        collide_mask: 0,
        shell_speed_scale: None,
    });
    let source = game.spawn(source_type, 0, [300.0, 100.0], true);
    let target = game.spawn(target_type, 1, [400.0, 100.0], true);
    game.set_object_position(source, [300.0, 100.0, 50.0]);
    game.set_object_position(target, [400.0, 100.0, 50.0]);
    game.set_object_orientation(target, 0.0);
    game.update_ground_contacts();
    game.tick = 100;
    game.attack(target, source).unwrap();
    assert!(game.entities[target].attacking());
    game.update_aircraft_attack_locomotor(target);
    // Isolate the tested weapon from the target's real Attack entry feedback.
    game.presentation.weapon_fx.clear();
    assert!(game.projectiles.is_empty());
    (game, source, target)
}

#[test]
fn aurora_direct_damage_uses_shifted_position_delay_and_does_not_reacquire() {
    let (mut game, source, target) = scenario();
    let health = game.entities[target].health;
    game.fire_weapon(source, target);
    assert_eq!(game.presentation.shots[0].target, None);
    assert_eq!(
        game.presentation.shots[0].target_position.map(f32::to_bits),
        [380.0_f32, 100.0, 50.0].map(f32::to_bits)
    );
    assert_eq!(game.projectiles[0].target, None);
    let Some(crate::skirmish::projectiles::DirectHit {
        deadline, position, ..
    }) = &game.projectiles[0].direct
    else {
        panic!("expected delayed direct weapon");
    };
    // Native fastceil adds 0x3f7fffff in binary32: exactly eight frames becomes nine.
    assert_eq!(*deadline, 109);
    assert_eq!(
        position.map(f32::to_bits),
        [380.0_f32, 100.0, 50.0].map(f32::to_bits)
    );
    // The stored attack remains a position even when the Aurora stops being sneaky.
    game.tick = 160;
    for _ in 0..9 {
        game.tick += 1;
        game.update_direct_weapon_damage();
    }
    assert!(game.projectiles.is_empty());
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
    game.fire_weapon(source, target);
    assert_eq!(game.projectiles[0].target, Some(target));
    for _ in 0..11 {
        game.tick += 1;
        game.update_direct_weapon_damage();
    }
    assert!(game.entities[target].health < health);
}

#[test]
fn aurora_immediate_direct_and_laser_fire_at_position_without_direct_hit() {
    for laser in [false, true] {
        let (mut game, source, target) = scenario();
        let kind = game.entities[source].type_index;
        game.catalog[kind].weapon.speed = 999_999.0;
        game.catalog[kind].weapon.laser = laser;
        let health = game.entities[target].health;
        game.fire_weapon(source, target);
        assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
        assert!(game.projectiles.is_empty());
        assert_eq!(game.presentation.shots[0].target, None);
        if laser {
            assert_eq!(game.presentation.laser_shots.len(), 1);
            assert_eq!(game.presentation.laser_shots[0].target, None);
            // createLaser never adds its grounded-object +10 to position shots.
            assert_eq!(
                game.presentation.laser_shots[0]
                    .target_position
                    .map(f32::to_bits),
                [380.0_f32, 100.0, 50.0].map(f32::to_bits)
            );
        }
    }
}

#[test]
fn aurora_converted_point_checks_range_before_fx_or_scatter_but_spends_round() {
    {
        let (mut game, source, target) = scenario();
        let kind = game.entities[source].type_index;
        game.catalog[kind].weapon.range = 80.0 - game.object_radius(source);
        game.fire_weapon(source, target);
        assert_eq!(game.presentation.shots.len(), 1);
        assert_eq!(
            game.presentation.shots[0].target_position[0].to_bits(),
            380.0_f32.to_bits()
        );
    }
    let (mut game, source, target) = scenario();
    let kind = game.entities[source].type_index;
    let radius = game.object_radius(source);
    game.catalog[kind].weapon.range = 80.0 - radius;
    game.catalog[kind].weapon.scatter = 10.0;
    game.catalog[kind].weapon.delay = 0.0;
    game.set_object_position(target, [401.0, 100.0, 50.0]);
    let seed = game.random;
    let first_fire_event = game.presentation.weapon_fx.len();
    game.fire_weapon(source, target);
    assert_eq!(game.random, seed);
    assert!(game.presentation.shots.is_empty() && game.projectiles.is_empty());
    // Object::fireCurrentWeapon calls FiringTracker even when the template rejects
    // the converted position. Only its ordinary firing sound survives this branch.
    assert!(
        matches!(&game.presentation.weapon_fx[first_fire_event..], [skirmish::WeaponFxEvent::FiringSound { object, .. }] if *object == source)
    );
    assert_eq!(
        game.entities[source].weapon_set.weapons[0].clip_remaining,
        3
    );
    game.catalog[kind].weapon.range = 500.0;
    game.catalog[kind].weapon.min_range = 80.0 - radius;
    game.set_object_position(target, [399.0, 100.0, 50.0]);
    game.fire_weapon(source, target);
    assert!(game.presentation.shots.is_empty());
    assert_eq!(
        game.entities[source].weapon_set.weapons[0].clip_remaining,
        2
    );
    game.set_object_position(target, [400.0, 100.0, 50.0]);
    game.fire_weapon(source, target);
    assert_eq!(game.presentation.shots.len(), 1);
    assert_ne!(game.random, seed);
}

#[test]
fn aurora_incidental_collision_misses_but_preexisting_intended_projectile_hits() {
    let (mut game, source, target) = scenario();
    let geometry = &game.catalog[game.entities[source].type_index].geometry;
    let body = Body {
        geometry,
        position: game.object_position(target),
        angle: 0.0,
    };
    assert!(!game.missile_contact(&body, target, source, None, u16::MAX));
    assert!(game.missile_contact(&body, target, source, Some(target), u16::MAX));
    game.tick = 160;
    let geometry = &game.catalog[game.entities[source].type_index].geometry;
    let body = Body {
        geometry,
        position: game.object_position(target),
        angle: 0.0,
    };
    assert!(game.missile_contact(&body, target, source, None, u16::MAX));
}

#[test]
fn aurora_scatter_keeps_fire_aim_and_direct_damage_separate_from_projectile_scatter() {
    let (mut game, source, target) = scenario();
    let kind = game.entities[source].type_index;
    game.catalog[kind].weapon.scatter = 10.0;
    game.catalog[kind].weapon.infantry_scatter = 50.0;
    // Even if an authored object has INFANTRY too, the converted victim is null.
    let victim_kind = game.entities[target].type_index;
    game.catalog[victim_kind].maintenance.healing_kind_flags |= 1;
    let mut seed = game.random;
    let radius = skirmish::startup::random_real(&mut seed, 0.0, 10.0);
    let angle = skirmish::startup::random_angle(&mut seed);
    let scattered = [380.0 + radius * angle.cos(), 100.0 + radius * angle.sin()];
    game.fire_weapon(source, target);
    assert_eq!(game.random, seed);
    assert_eq!(
        game.projectiles[0].destination.map(f32::to_bits),
        scattered.map(f32::to_bits)
    );
    let Some(crate::skirmish::projectiles::DirectHit { position, .. }) =
        &game.projectiles[0].direct
    else {
        panic!("expected direct weapon");
    };
    assert_eq!(
        position.map(f32::to_bits),
        [380.0_f32, 100.0, 50.0].map(f32::to_bits)
    );
    assert_eq!(
        game.presentation.shots[0].target_position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
}

#[test]
fn ordinary_direct_scatter_keeps_designated_damage_and_laser_uses_sampled_radius() {
    for radius in [0.0, 20.0] {
        let (mut game, source, target) = scenario();
        game.tick = 160;
        let kind = game.entities[source].type_index;
        game.catalog[kind].weapon.scatter = 10.0;
        let health = game.entities[target].health;
        game.fire_weapon(source, target);
        // Unlike a scattered physical projectile, direct damage keeps victimID.
        assert_eq!(game.projectiles[0].target, Some(target));
        for _ in 0..11 {
            game.tick += 1;
            game.update_direct_weapon_damage();
        }
        assert!(game.entities[target].health < health);
        game.catalog[kind].weapon.laser = true;
        if radius > 0.0 {
            game.catalog[kind].weapon.area = Some(skirmish::AreaDamage {
                at_source: false,
                primary_radius: radius,
                secondary_radius: radius,
                secondary_damage: 0.0,
                affects: 2,
            });
        }
        let before = game.entities[target].health;
        // The first direct hit can arrive before this allocated weapon is ready again.
        game.tick = game.entities[source].weapon_set.weapons[0].possible_next_shot;
        game.fire_weapon(source, target);
        if radius > 0.0 {
            assert_eq!(game.presentation.laser_shots[0].target, Some(target));
            assert!(game.entities[target].health < before);
        } else {
            assert_eq!(game.presentation.laser_shots[0].target, None);
            assert_eq!(
                game.presentation.laser_shots[0].target_position[2].to_bits(),
                0.0_f32.to_bits()
            );
            assert_eq!(game.entities[target].health.to_bits(), before.to_bits());
        }
    }
}

#[test]
fn aurora_position_damage_can_hit_another_object_without_blanket_immunity() {
    let (mut game, source, target) = scenario();
    let source_type = game.entities[source].type_index;
    let target_type = game.entities[target].type_index;
    game.catalog[target_type].geometry_bounds.sphere_radius = 1.0;
    game.catalog[source_type].geometry_bounds.sphere_radius = 1.0;
    game.catalog[source_type].weapon.area = Some(skirmish::AreaDamage {
        at_source: false,
        primary_radius: 5.0,
        secondary_radius: 5.0,
        secondary_damage: 0.0,
        affects: 2,
    });
    game.catalog[source_type].geometry_bounds.center_height = 0.0;
    let bystander = game.spawn(source_type, 1, [380.0, 100.0], true);
    game.set_object_position(bystander, [380.0, 100.0, 50.0]);
    game.update_ground_contacts();
    let health = game.entities[target].health;
    let bystander_health = game.entities[bystander].health;
    game.fire_weapon(source, target);
    game.set_object_position(target, [600.0, 100.0, 50.0]);
    for _ in 0..9 {
        game.tick += 1;
        game.update_direct_weapon_damage();
    }
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
    assert!(game.entities[bystander].health < bystander_health);
}

#[test]
fn explicit_scatter_pattern_precedes_and_bypasses_sneaky_object_offset() {
    let (game, _, target) = scenario();
    let point = [405.0, 115.0, 0.0];
    let aim = game.weapon_target(target, Some(point));
    assert_eq!(aim.object, None);
    assert_eq!(aim.position.map(f32::to_bits), point.map(f32::to_bits));
}

#[test]
fn aurora_missile_launch_has_no_victim_and_keeps_shifted_destination_after_expiry() {
    let (mut game, source, target) = scenario();
    let kind = game.entities[source].type_index;
    let rules = skirmish::projectiles::MissileRules {
        retirement: crate::skirmish::MissileRetirement {
            calls_kill: false,
            delay: 3,
        },
        jam_scatter: 75.0,
        initial_velocity: 5.0,
        ignition_delay: 0,
        fuel_lifetime: 90,
        initial_distance: 3.0,
        dive_distance: 0.0,
        lock_distance: 75.0,
        follow_target: true,
        use_weapon_speed: false,
        detonate_on_no_fuel: false,
        thrust: skirmish::projectiles::ThrustRules {
            max_speed: 7.5,
            min_speed: 4.0,
            acceleration: 0.75,
            braking: 0.0,
            turn_rate: 100.0 * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0)),
            max_thrust_angle: std::f32::consts::FRAC_PI_4,
            preferred_height: 0.0,
            height_damping: 1.0,
            close_enough: 1.0,
            airborne_motive: true,
        },
        physics: skirmish::projectiles::ProjectilePhysics {
            mass: 1.0,
            gravity: -64.0 / 900.0,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            extra_friction: 0.0,
            apply_friction_when_airborne: false,
        },
        geometry: crate::collision_geometry::Geometry {
            is_small: true,
            shape: crate::collision_geometry::Shape::Sphere,
            major: 2.0,
            minor: 2.0,
            height: 2.0,
        },
        clear: None,
    };
    let projectile_type = game.catalog.len();
    game.catalog
        .push(skirmish::point_defense::tests::missile_template(rules, 1.0));
    let motion = game.catalog[kind].weapon.motion.as_mut().unwrap();
    motion.projectile_type = Some(projectile_type);
    motion.collide_mask = u16::MAX;
    game.fire_weapon(source, target);
    assert_eq!(game.projectiles.len(), 1);
    assert_eq!(game.projectiles[0].target, None);
    assert_eq!(
        game.projectiles[0].destination.map(f32::to_bits),
        [380.0_f32, 100.0].map(f32::to_bits)
    );
    game.set_object_position(target, [500.0, 400.0, 50.0]);
    game.tick = 160;
    game.tick += 1;
    game.update_direct_weapon_damage();
    assert_eq!(game.projectiles[0].target, None);
    assert_eq!(
        game.projectiles[0].destination.map(f32::to_bits),
        [380.0_f32, 100.0].map(f32::to_bits)
    );
}

#[test]
fn laser_endpoint_raises_only_grounded_nonprojectile_objects() {
    for (height, projectile, raised) in [
        (0.0, false, true),
        (10.0, false, true),
        (11.0, false, false),
        (0.0, true, false),
    ] {
        let (mut game, source, target) = scenario();
        game.tick = 160;
        let kind = game.entities[source].type_index;
        game.catalog[kind].weapon.laser = true;
        let kind = game.entities[target].type_index;
        game.catalog[kind].locomotor.airborne_targeting_height = 10.0;
        game.set_object_position(target, [400.0, 100.0, height]);
        if projectile {
            game.catalog[kind].kind_of.push("PROJECTILE".into());
        }
        let position = game.object_position(target);
        game.fire_weapon(source, target);
        assert_eq!(game.presentation.laser_shots[0].target, Some(target));
        assert_eq!(
            game.presentation.laser_shots[0].target_position[2].to_bits(),
            (position[2] + if raised { 10.0 } else { 0.0 }).to_bits()
        );
    }
}
