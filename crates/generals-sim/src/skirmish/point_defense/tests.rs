use super::*;
use crate::skirmish::{FlightRules, LaunchTransform, WeaponMotion, tests::game};

fn defense() -> Rules {
    Rules {
        weapon: WeaponRules {
            name: "Defense".into(),
            laser: true,
            anti_mask: 3,
            damage: 100.0,
            range: 65.0,
            delay: 1.0,
            damage_type: DamageType::Laser,
            death_type: crate::skirmish::DeathType::Lasered,
            ..WeaponRules::unarmed()
        },
        primary: vec!["SMALL_MISSILE".into()],
        secondary: vec!["INFANTRY".into()],
        scan_frames: 15,
        scan_range: 120.0,
        predict_velocity: 3.0,
    }
}

fn setup() -> (Skirmish, usize) {
    setup_modules(1)
}

fn setup_modules(count: usize) -> (Skirmish, usize) {
    let mut game = game();
    for _ in 0..count {
        game.catalog[6].behavior.point_defense.push(defense());
        crate::skirmish::tests::register_update(&mut game.catalog[6], "PointDefenseLaserUpdate");
    }
    let tank = game.spawn(6, 0, [405.0, 305.0], true);
    game.update_ground_contacts();
    game.tick = 1;
    (game, tank)
}

fn defense_frame(game: &mut Skirmish) {
    game.run_object_updates();
    game.process_update_destruction();
    game.tick += 1;
}

fn missile_object(game: &Skirmish, serial: u64) -> usize {
    game.projectiles[game.projectile_index(serial).unwrap()]
        .object
        .unwrap()
}

pub(in crate::skirmish) fn missile(
    game: &mut Skirmish,
    position: [f32; 3],
    owner: usize,
    armor: f32,
) -> u64 {
    missile_targeted(game, position, owner, armor, None, [405.0, 305.0, 0.0])
}

pub(in crate::skirmish) fn missile_targeted(
    game: &mut Skirmish,
    position: [f32; 3],
    owner: usize,
    armor: f32,
    target: Option<usize>,
    destination: [f32; 3],
) -> u64 {
    let projectile = missile_template(missile_rules(), armor);
    missile_from_template(game, position, owner, target, destination, projectile)
}

pub(in crate::skirmish) fn missile_template(
    rules: crate::skirmish::MissileRules,
    armor: f32,
) -> crate::skirmish::UnitType {
    use crate::skirmish::{
        Faction, Kind, UnitAiUpdate,
        tests::{fixture_type, register_update},
    };
    let mut projectile = fixture_type(Faction::Gla, Kind::Effect);
    // This authored object has MissileAIUpdate, replacing the generic fixture AI.
    projectile.update_modules.retain(|entry| !matches!(entry, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "AIUpdateInterface"));
    projectile
        .behavior_modules
        .retain(|entry| entry.family != "AIUpdateInterface");
    projectile.name = "FixtureMissile".into();
    projectile.kind_of = vec!["PROJECTILE".into(), "SMALL_MISSILE".into()];
    projectile.selectable = false;
    projectile.ai.update = UnitAiUpdate::Ordinary;
    projectile.locomotor.wander_length = Some(1.0);
    projectile.locomotor.legal_surfaces = 4;
    projectile.geometry = crate::collision_geometry::Geometry {
        is_small: rules.geometry.is_small,
        shape: rules.geometry.shape,
        major: rules.geometry.major,
        minor: rules.geometry.minor,
        height: rules.geometry.height,
    };
    projectile.geometry_bounds = missile_bounds(&rules.geometry);
    projectile.live_physics = Some(crate::skirmish::physical_ocl::LiveRules {
        motion: crate::skirmish::physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: rules.physics.mass,
            gravity: rules.physics.gravity,
            stiffness: 0.5,
            forward_friction: rules.physics.forward_friction,
            lateral_friction: rules.physics.lateral_friction,
            aerodynamic_friction: rules.physics.aerodynamic_friction,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.0,
        kill_when_resting: false,
        collision: crate::skirmish::physical_ocl::CollisionRules {
            stiffness: 0.5,
            rubble_height: 0.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.05, 0.025, 0.025],
        min_fall_speed: (2.0 * rules.physics.gravity * 40.0).abs().sqrt(),
        fall_damage_factor: 1.0,
    });
    projectile.projectile_rules = vec![FlightRules::Missile(rules)];
    projectile.max_health = 100.0;
    projectile.initial_health = 100.0;
    projectile.armor[DamageType::Laser as usize] = armor;
    projectile
        .behavior
        .instant_death
        .push(crate::skirmish::instant_death::Rules {
            death_mask: u32::MAX,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
            effects: Vec::new(),
            objects: Vec::new(),
            weapons: Vec::new(),
        });
    register_update(&mut projectile, "MissileAIUpdate");
    register_update(&mut projectile, "PhysicsBehavior");
    projectile.behavior_modules.last_mut().unwrap().interfaces = 0x11;
    projectile
        .behavior_modules
        .push(crate::skirmish::module_updates::BehaviorRegistration {
            family: "InstantDeathBehavior".into(),
            tag: "Death".into(),
            interfaces: 2,
            upgrade: crate::skirmish::module_updates::UpgradeData::none(),
            source: crate::skirmish::module_updates::ModuleSource {
                path: "fixture.ini".into(),
                origin: "native projectile fixture".into(),
                line: 1,
            },
        });
    projectile
}

fn missile_bounds(
    geometry: &crate::collision_geometry::Geometry,
) -> crate::skirmish::GeometryBounds {
    let half_height = geometry.height * 0.5;
    let (center_height, sphere_radius, max_height) = match geometry.shape {
        crate::collision_geometry::Shape::Sphere => (0.0, geometry.major, geometry.major),
        crate::collision_geometry::Shape::Cylinder => (
            half_height,
            half_height.max(geometry.major),
            geometry.height,
        ),
        crate::collision_geometry::Shape::Box => {
            let squared = [geometry.major, geometry.minor, half_height].map(|value| value * value);
            (
                half_height,
                ((squared[0] + squared[1]) + squared[2]).sqrt(),
                geometry.height,
            )
        }
    };
    crate::skirmish::GeometryBounds {
        center_height,
        sphere_radius,
        max_height,
    }
}

pub(in crate::skirmish) fn missile_from_template(
    game: &mut Skirmish,
    position: [f32; 3],
    owner: usize,
    target: Option<usize>,
    destination: [f32; 3],
    projectile: crate::skirmish::UnitType,
) -> u64 {
    use crate::skirmish::{Faction, Kind, tests::fixture_type};
    let kind = game.catalog.len();
    game.catalog.push(projectile);
    let mut unit = fixture_type(Faction::Gla, Kind::Infantry);
    unit.name = format!("FixtureLauncher{kind}");
    unit.weapon.speed = 600.0;
    unit.weapon.damage = 500.0;
    unit.weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| {
            vec![LaunchTransform {
                position: [position[0] - 800.0, position[1] - 500.0, position[2]],
                axes: LaunchTransform::IDENTITY.axes,
            }]
        }),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: Some(kind),
        collide_mask: 4,
        shell_speed_scale: None,
    });
    game.catalog.push(unit);
    let source = game.spawn(game.catalog.len() - 1, owner, [800.0, 500.0], true);
    let target = crate::skirmish::damage::WeaponTarget {
        object: target,
        position: destination,
        projectile_position: destination,
        scatter_radius: 0.0,
    };
    game.create_weapon_projectile(
        source,
        &target,
        crate::skirmish::firing::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: 0,
        },
        1.0,
        0,
    );
    let serial = game.projectile_serial;
    game.update_ground_contacts();
    serial
}

pub(in crate::skirmish) fn missile_rules() -> crate::skirmish::MissileRules {
    crate::skirmish::MissileRules {
        jam_scatter: 75.0,
        retirement: crate::skirmish::MissileRetirement {
            calls_kill: false,
            delay: 3,
        },
        initial_velocity: 5.0,
        ignition_delay: 0,
        fuel_lifetime: 90,
        initial_distance: 3.0,
        dive_distance: 0.0,
        lock_distance: 75.0,
        follow_target: true,
        use_weapon_speed: false,
        detonate_on_no_fuel: false,
        thrust: crate::skirmish::ThrustRules {
            max_speed: 7.5,
            min_speed: 4.0,
            acceleration: 0.75,
            braking: 0.0,
            turn_rate: 0.058,
            max_thrust_angle: 0.785,
            preferred_height: 0.0,
            height_damping: 1.0,
            close_enough: 1.0,
            airborne_motive: true,
        },
        physics: crate::skirmish::ProjectilePhysics {
            mass: 1.0,
            gravity: -0.071,
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
    }
}

fn missile_with_death_effects() -> crate::skirmish::UnitType {
    use crate::skirmish::{DeathType, instant_death, tests::register_behavior};
    let mut unit = missile_template(missile_rules(), 1.0);
    unit.behavior.instant_death.clear();
    unit.behavior_modules
        .retain(|module| module.family != "InstantDeathBehavior");
    let detonated = 1 << DeathType::Detonated as u32;
    let lasered = 1 << DeathType::Lasered as u32;
    for (death_mask, effect) in [
        (detonated, "Detonation"),
        (lasered, "Lasered"),
        (!(detonated | lasered), "OtherDeath"),
    ] {
        unit.behavior.instant_death.push(instant_death::Rules {
            death_mask,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
            effects: vec![Some(effect.into())],
            objects: Vec::new(),
            weapons: Vec::new(),
        });
        register_behavior(&mut unit, "InstantDeathBehavior", 2);
    }
    unit
}

#[test]
fn missile_priority_interception_has_no_payload_damage_or_normal_detonation() {
    let (mut game, tank) = setup();
    let infantry = game.spawn(5, 1, [415.0, 305.0], true);
    let health = game.entities[tank].health;
    let serial = missile_from_template(
        &mut game,
        [455.0, 305.0, 10.0],
        1,
        None,
        [405.0, 305.0, 0.0],
        missile_with_death_effects(),
    );
    let object = missile_object(&game, serial);
    game.update_ground_contacts();
    defense_frame(&mut game);
    assert!(game.projectiles.is_empty());
    assert_eq!(game.entities[tank].health.to_bits(), health.to_bits());
    assert_eq!(
        game.entities[infantry].health.to_bits(),
        100.0_f32.to_bits()
    );
    assert_eq!(
        game.presentation.laser_shots[0]
            .target_position
            .map(f32::to_bits),
        [455.0_f32, 305.0, 10.0].map(f32::to_bits)
    );
    assert!(!game.entities[object].exists());
    let effects: Vec<_> = game
        .presentation
        .weapon_fx
        .iter()
        .filter_map(|event| match event {
            crate::skirmish::WeaponFxEvent::Laser(index)
                if game.presentation.laser_shots[*index].target == Some(object) =>
            {
                Some("Laser")
            }
            crate::skirmish::WeaponFxEvent::ObjectFx(index)
                if game.presentation.object_fx_events[*index].object == object =>
            {
                Some(game.presentation.object_fx_events[*index].effect.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(effects, ["Laser", "Lasered"]);
    assert!(!game.presentation.weapon_fx.iter().any(|event| matches!(event,
        crate::skirmish::WeaponFxEvent::Projectile(fx) if matches!(fx.stage, crate::skirmish::ProjectileFxStage::Detonation))));
}

#[test]
fn secondary_in_range_beats_primary_outside_and_friends_are_excluded() {
    let (mut game, tank) = setup();
    let infantry = game.spawn(5, 1, [445.0, 305.0], true);
    missile(&mut game, [500.0, 305.0, 10.0], 1, 1.0);
    missile(&mut game, [410.0, 305.0, 10.0], 0, 1.0);
    game.update_ground_contacts();
    defense_frame(&mut game);
    assert!(!game.entities[infantry].alive());
    assert_eq!(game.projectiles.len(), 2);
    assert_eq!(game.presentation.laser_shots[0].target, Some(infantry));
    assert_eq!(
        game.presentation.laser_shots[0].target_position[2].to_bits(),
        10.0_f32.to_bits()
    );
    assert_eq!(game.entities[tank].weapon_set.current, 0);
    assert!(
        game.entities[tank].weapon_set.weapons[0]
            .last_shot
            .is_none()
    );
}

#[test]
fn two_modules_damage_independently_and_keep_surviving_projectile_indices_valid() {
    let (mut game, first) = setup_modules(2);
    let stationary = |armor| {
        let mut rules = missile_rules();
        rules.initial_velocity = 0.0;
        rules.physics.gravity = 0.0;
        rules.thrust.min_speed = 0.0;
        rules.thrust.max_speed = 0.0;
        rules.thrust.acceleration = 0.0;
        missile_template(rules, armor)
    };
    let doomed = missile_from_template(
        &mut game,
        [445.0, 305.0, 10.0],
        1,
        None,
        [405.0, 305.0, 0.0],
        stationary(1.0),
    );
    let survives = missile_from_template(
        &mut game,
        [450.0, 305.0, 10.0],
        1,
        None,
        [405.0, 305.0, 0.0],
        stationary(0.5),
    );
    game.run_object_updates();
    assert_eq!(game.presentation.laser_shots.len(), 1);
    let doomed_object = missile_object(&game, doomed);
    assert!(game.entities[doomed_object].exists());
    assert!(game.entities[doomed_object].is_effectively_dead());
    // Both scans select the closest retained Object. The second module sees
    // its dead body before frame-end destruction and therefore does not fire.
    assert_eq!(game.entities[first].behaviors.point_defense[1].reload, 0);
    game.process_update_destruction();
    game.tick += 1;
    assert!(!game.projectiles.iter().any(|p| p.id == doomed));
    let remaining = game.projectiles.iter().find(|p| p.id == survives).unwrap();
    assert_eq!(
        game.entities[remaining.object.unwrap()].health.to_bits(),
        100.0_f32.to_bits()
    );
    assert_eq!(game.entities[first].behaviors.point_defense[0].reload, 30);
    for _ in 0..4 {
        defense_frame(&mut game);
        if game.presentation.laser_shots.len() == 2 {
            break;
        }
    }
    assert_eq!(game.presentation.laser_shots.len(), 2);
    let remaining = game.projectiles.iter().find(|p| p.id == survives).unwrap();
    assert_eq!(
        game.entities[remaining.object.unwrap()].health.to_bits(),
        50.0_f32.to_bits()
    );
    for _ in 0..31 {
        defense_frame(&mut game);
        if game.projectiles.is_empty() {
            break;
        }
    }
    assert!(game.projectiles.is_empty());
    assert_eq!(game.presentation.laser_shots.len(), 3);
}

#[test]
fn paint_is_timed_status_and_rate_condition_changes_only_after_an_actual_shot() {
    let mut game = game();
    let target = game.spawn(6, 1, [405.0, 305.0], true);
    let source = game.spawn(6, 0, [355.0, 305.0], true);
    game.catalog[6].weapon.damage_type = DamageType::Status;
    game.catalog[6].weapon.damage = 200.0;
    game.catalog[6].weapon.bonuses.global[22][3] = 1.5;
    game.deal_bound_weapon_damage(
        source,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        Some(target),
        game.object_position(target),
        1.0,
    );
    assert!(game.entities[target].painted());
    assert_eq!(game.entities[target].health.to_bits(), 100.0_f32.to_bits());
    assert!(!game.entities[source].firing.painted_target);
    game.entities[source].spend_weapon_round(&game.catalog[6], game.tick, 0);
    game.entities[source].update_painted_target(&game.catalog[6], true, game.tick, 0);
    assert_eq!(
        game.entities[source].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        40.0_f32.to_bits()
    );
    game.entities[source].weapon_set.weapons[0].cooldown = 7.0;
    game.entities[source].update_painted_target(&game.catalog[6], true, game.tick, 0);
    assert_eq!(
        game.entities[source].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        7.0_f32.to_bits()
    );
    game.tick = 5;
    game.run_object_updates();
    assert!(game.entities[target].painted());
    game.deal_bound_weapon_damage(
        source,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        Some(target),
        game.object_position(target),
        1.0,
    );
    game.tick = 10;
    game.run_object_updates();
    assert!(game.entities[target].painted());
    game.tick = 11;
    game.run_object_updates();
    assert!(!game.entities[target].painted());
    assert!(game.entities[source].firing.painted_target);
    game.entities[source].update_painted_target(&game.catalog[6], false, game.tick, 0);
    assert_eq!(
        game.entities[source].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        60.0_f32.to_bits()
    );
}

#[test]
fn disabled_and_dead_defenders_do_not_advance_their_independent_module_timers() {
    let (mut game, tank) = setup();
    game.entities[tank].behaviors.point_defense[0].reload = 30;
    game.entities[tank].behaviors.point_defense[0].scan = 15;
    game.entities[tank].battle_bonuses.paralyzed_until = 20;
    defense_frame(&mut game);
    assert_eq!(game.entities[tank].behaviors.point_defense[0].scan, 15);
    assert_eq!(game.entities[tank].behaviors.point_defense[0].reload, 30);
    game.entities[tank].battle_bonuses.paralyzed_until = 0;
    game.kill_object(tank, crate::skirmish::DeathType::Normal);
    defense_frame(&mut game);
    assert_eq!(game.entities[tank].behaviors.point_defense[0].scan, 15);
}

#[test]
fn actual_painter_fire_sets_status_before_the_firing_tracker_bonus_callback() {
    let mut game = game();
    game.catalog[6].weapon.laser = true;
    game.catalog[6].weapon.damage_type = DamageType::Status;
    game.catalog[6].weapon.damage = 200.0;
    game.catalog[6].weapon.aim_delta = std::f32::consts::PI;
    game.catalog[6].weapon.bonuses.global[22][3] = 1.5;
    let source = game.spawn(6, 0, [355.0, 305.0], true);
    let target = game.spawn(6, 1, [405.0, 305.0], true);
    // Native Aim entry, Aim update/Fire entry, then the first Fire update.
    assert!(game.update_attack(source, target));
    assert!(game.presentation.shots.is_empty());
    game.tick += 1;
    assert!(game.update_attack(source, target));
    assert!(game.presentation.shots.is_empty());
    game.tick += 1;
    assert!(game.update_attack(source, target));
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(game.presentation.laser_shots.len(), 1);
    assert!(game.entities[target].painted());
    assert!(game.entities[source].firing.painted_target);
    assert_eq!(game.entities[target].health.to_bits(), 100.0_f32.to_bits());
    assert_eq!(
        game.entities[source].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        40.0_f32.to_bits()
    );
}

#[test]
fn boundary_target_rescan_consumes_only_native_random_delay_and_retains_cooldown() {
    let (mut game, tank) = setup();
    missile(&mut game, [470.0, 305.0, 10.0], 1, 0.0);
    game.update_ground_contacts();
    game.scan_point_defense(tank, 0);
    assert!(game.entities[tank].behaviors.point_defense[0].in_range);
    // Acquisition <= range marks it in range, while firing tests strict <.
    // An outgoing target forces the native 0..3 rescan even when on the boundary.
    let mut expected = game.random;
    let delay = super::super::startup::random_value(&mut expected) % 4;
    game.entities[tank].behaviors.point_defense[0].reload = 2;
    game.fire_point_defense(tank, 0);
    assert_eq!(game.random, expected);
    assert_eq!(
        game.entities[tank].behaviors.point_defense[0].scan,
        if delay == 0 {
            15
        } else {
            i32::try_from(delay).unwrap()
        }
    );
    assert_eq!(game.entities[tank].behaviors.point_defense[0].reload, 1);
    assert!(game.presentation.laser_shots.is_empty());
}

#[test]
fn hidden_retiring_missile_remains_a_live_defense_target_until_destroyed() {
    let (mut game, _) = setup();
    let mut rules = missile_rules();
    rules.fuel_lifetime = 1;
    rules.detonate_on_no_fuel = true;
    let serial = missile_from_template(
        &mut game,
        [445.0, 305.0, 10.0],
        1,
        None,
        [800.0, 305.0, 0.0],
        missile_template(rules, 1.0),
    );
    let object = missile_object(&game, serial);
    let registration = game.catalog[game.entities[object].type_index]
        .update_modules
        .iter()
        .position(|module| matches!(module, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "MissileAIUpdate"))
        .unwrap();
    game.tick += 1;
    game.update_missile_module(object, registration);
    game.tick += 1;
    game.update_missile_module(object, registration);
    assert_eq!(game.projectiles.len(), 1);
    assert!(
        game.projectiles[0]
            .render_transform(&game.entities)
            .is_none()
    );
    assert!(
        game.projectiles[0]
            .effect_transform(&game.entities)
            .is_some()
    );
    defense_frame(&mut game);
    assert!(game.projectiles.is_empty());
    assert_eq!(game.presentation.laser_shots.len(), 1);
}

#[test]
fn zero_scan_module_pauses_reload_without_a_target_then_resumes_when_one_arrives() {
    let (mut game, tank) = setup();
    game.catalog[6].behavior.point_defense[0].scan_frames = 0;
    game.catalog[6].behavior.point_defense[0].secondary.clear();
    game.entities[tank].behaviors.point_defense[0].reload = 2;
    for _ in 0..20 {
        defense_frame(&mut game);
    }
    assert_eq!(game.entities[tank].behaviors.point_defense[0].reload, 2);
    missile(&mut game, [450.0, 305.0, 10.0], 1, 1.0);
    defense_frame(&mut game);
    defense_frame(&mut game);
    assert_eq!(game.projectiles.len(), 1);
    defense_frame(&mut game);
    assert!(game.projectiles.is_empty());
}

#[test]
fn equal_distance_missiles_keep_native_reversed_outward_cell_order() {
    let (mut game, tank) = setup();
    let right = missile(&mut game, [445.0, 305.0, 10.0], 1, 1.0);
    let upper = missile(&mut game, [405.0, 345.0, 10.0], 1, 1.0);
    game.update_ground_contacts();
    game.scan_point_defense(tank, 0);
    assert!(
        game.entities[tank].behaviors.point_defense[0].target == Some(missile_object(&game, upper))
    );
    assert_ne!(upper, right);
}

#[test]
fn normal_weapon_launch_uses_authored_initial_projectile_health() {
    let mut game = game();
    missile(&mut game, [455.0, 305.0, 10.0], 1, 1.0);
    let source = game.projectiles[0].source;
    let unit = game.entities[source].type_index;
    let object = game.projectiles[0].object.unwrap();
    game.destroy_maintenance_object(object);
    game.process_update_destruction();
    game.entities[source].position = [355.0, 305.0];
    game.entities[source].facing = 0.0;
    game.catalog[unit].weapon.aim_delta = std::f32::consts::PI;
    let kind = game.catalog[unit]
        .weapon
        .motion
        .as_ref()
        .unwrap()
        .projectile_type
        .unwrap();
    game.catalog[kind].initial_health = 50.0;
    let target = game.spawn(6, 0, [405.0, 305.0], true);
    // Native Aim entry, Aim update/Fire entry, then the first Fire update.
    assert!(game.update_attack(source, target));
    assert!(game.presentation.shots.is_empty());
    game.tick += 1;
    assert!(game.update_attack(source, target));
    assert!(game.presentation.shots.is_empty());
    game.tick += 1;
    assert!(game.update_attack(source, target));
    assert_eq!(game.projectiles.len(), 1);
    assert_eq!(
        game.entities[game.projectiles[0].object.unwrap()]
            .health
            .to_bits(),
        50.0_f32.to_bits()
    );
}

#[test]
fn original_defender_birth_wake_waits_until_the_following_frame() {
    let (mut game, old) = setup();
    game.kill_object(old, crate::skirmish::DeathType::Normal);
    missile(&mut game, [450.0, 305.0, 10.0], 1, 1.0);
    let defender = game.spawn(6, 0, [405.0, 305.0], true);
    defense_frame(&mut game);
    assert!(game.presentation.laser_shots.is_empty());
    assert_eq!(game.entities[defender].behaviors.point_defense[0].scan, 0);
    game.tick += 1;
    defense_frame(&mut game);
    assert!(game.projectiles.is_empty());
    assert_eq!(game.presentation.laser_shots[0].source, defender);
}

#[test]
fn weapon_birth_keeps_queued_physics_and_enters_defense_partition_at_frame_end() {
    let (mut game, defender) = setup();
    game.catalog[6].behavior.point_defense[0].scan_frames = 0;
    game.catalog[6].behavior.point_defense[0].secondary.clear();
    missile(&mut game, [445.0, 305.0, 10.0], 1, 1.0);
    let source = game.projectiles[0].source;
    let object = game.projectiles[0].object.unwrap();
    game.destroy_maintenance_object(object);
    game.process_update_destruction();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    let weapon = &mut game.catalog[game.entities[source].type_index].weapon;
    weapon.aim_delta = std::f32::consts::PI;
    weapon.motion.as_mut().unwrap().launches = std::array::from_fn(|_| {
        vec![LaunchTransform {
            position: [0.0, 0.0, 10.0],
            ..LaunchTransform::IDENTITY
        }]
    });
    game.entities[source].position = [445.0, 305.0];
    game.entities[source].facing = std::f32::consts::PI;
    game.attack(source, defender).unwrap();
    game.step();
    assert!(game.projectiles.is_empty());
    assert!(game.presentation.shots.is_empty());
    game.step();
    assert_eq!(game.projectiles.len(), 1);
    assert!(game.presentation.laser_shots.is_empty());
    assert_eq!(
        game.projectiles[0]
            .effect_transform(&game.entities)
            .unwrap()
            .0
            .map(f32::to_bits),
        [445.0_f32, 305.0, 10.0].map(f32::to_bits)
    );
    let born = game.tick;
    game.step();
    assert!(game.projectiles.is_empty());
    assert_eq!(game.presentation.laser_shots.len(), 1);
    assert_eq!(game.presentation.laser_shots[0].frame, born + 1);
    assert!(game.presentation.laser_shots[0].target_position[0] < 445.0);
}

#[test]
fn defense_uses_current_physics_position_when_missiles_cross_firing_range() {
    for (start, intercepted) in [(337.0, true), (467.0, false)] {
        let (mut game, defender) = setup();
        let serial = missile(&mut game, [start, 305.0, 10.0], 1, 1.0);
        for unit in &mut game.catalog {
            unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
        }
        let object = missile_object(&game, serial);
        game.tick += 1;
        game.update_projectile_physics(object);
        game.update_ground_contacts();
        game.update_point_defense(defender, 0);
        game.process_update_destruction();
        assert_eq!(game.projectiles.is_empty(), intercepted);
        assert_eq!(!game.presentation.laser_shots.is_empty(), intercepted);
        if intercepted {
            assert!(game.presentation.laser_shots[0].target_position[0] > 340.0);
        } else {
            assert!(game.projectiles[0].position[0] > 470.0);
        }
    }
}

#[test]
fn pdl_only_busts_occupants_after_missile_enters_self_killing_delay() {
    use crate::skirmish::{
        bunker_buster,
        containment::{GarrisonRules, tests::setup as container},
    };
    for after_impact in [false, true] {
        let (mut game, carrier) = container(0.0, 1.0);
        let carrier_type = game.entities[carrier].type_index;
        game.catalog[carrier_type]
            .contain
            .as_mut()
            .unwrap()
            .garrison = Some(GarrisonRules {
            allow_enemies: false,
            allow_neutral: false,
            immune_to_clear: false,
            heal_frames: None,
            stations: None,
        });
        game.entities[carrier].health = 10_000.0;
        game.entities[carrier].max_health = 10_000.0;
        let rider = game.spawn(5, 0, game.entities[carrier].position, true);
        game.entities[rider].containment.carrier = Some(carrier);
        game.entities[carrier].containment.passengers.push(rider);
        let position = game.object_position(carrier);
        let serial = missile_from_template(
            &mut game,
            [position[0] + 30.0, position[1], position[2] + 10.0],
            1,
            Some(carrier),
            position,
            bunker_buster::tests::template(None),
        );
        let object = missile_object(&game, serial);
        game.tick = 1;
        game.run_object_updates();
        assert!(
            !game.entities[object]
                .projectile_flight
                .as_ref()
                .unwrap()
                .killing_self()
        );
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
        if after_impact {
            assert!(game.projectile_contact(object, Some(carrier)));
            assert!(
                game.entities[object]
                    .projectile_flight
                    .as_ref()
                    .unwrap()
                    .killing_self()
            );
        }
        game.catalog[6].behavior.point_defense.push(defense());
        crate::skirmish::tests::register_update(&mut game.catalog[6], "PointDefenseLaserUpdate");
        let defender = game.spawn(6, 0, [position[0] + 30.0, position[1]], true);
        game.update_ground_contacts();
        game.tick = 2;
        defense_frame(&mut game);
        assert!(!game.entities[object].exists());
        assert_eq!(
            game.presentation.laser_shots.last().unwrap().source,
            defender
        );
        assert_eq!(
            game.entities[rider].contained_by(),
            if after_impact { None } else { Some(carrier) }
        );
        assert_eq!(game.entities[rider].alive(), !after_impact);
    }
}

#[test]
fn laser_object_lifetime_uses_logic_rng_once_and_fixed_bounds_consume_none() {
    let (mut game, source) = setup();
    let binding = crate::skirmish::WeaponBinding {
        type_index: game.entities[source].type_index,
        weapon_index: 7,
        weapon_slot: 2,
    };
    game.catalog[6].behavior.point_defense[0]
        .weapon
        .laser_lifetime = Some([7, 20]);
    let target = missile(&mut game, [430.0, 305.0, 10.0], 1, 1.0);
    // Projectile construction precedes this weapon's laser object constructor.
    let mut expected = game.random;
    let duration = 7 + crate::skirmish::startup::random_between(&mut expected, 13);
    let object = missile_object(&game, target);
    game.shoot_point_defense(source, 0, object);
    assert_eq!(game.random, expected);
    assert_eq!(
        game.presentation.laser_shots.last().unwrap().lifetime,
        Some(duration)
    );
    assert_eq!(
        game.presentation.laser_shots.last().unwrap().lifetime,
        Some(duration)
    );
    game.catalog[6].behavior.point_defense[0]
        .weapon
        .laser_lifetime = Some([4, 4]);
    assert_eq!(game.laser_lifetime(binding), Some(4));
    assert_eq!(game.random, expected);
    game.catalog[6].behavior.point_defense[0]
        .weapon
        .laser_lifetime = Some([0, 0]);
    assert_eq!(game.laser_lifetime(binding), Some(1));
    assert_eq!(game.random, expected);
}
