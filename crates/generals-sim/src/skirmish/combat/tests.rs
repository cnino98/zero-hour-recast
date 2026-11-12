use super::*;
use crate::skirmish::{CELL_SIZE, locomotor};

use crate::skirmish::{Kind, LaunchTransform, WeaponMotion};

#[test]
fn lethal_damage_retains_preferred_damage_and_the_incoming_death_type() {
    use crate::skirmish::{DamageType, DeathType};
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let source = game.spawn(tank, 0, [300.0, 300.0], true);
    let victim = game.spawn(tank, 1, [500.0, 300.0], true);
    game.catalog[tank].armor[DamageType::Water as usize] = 0.5;
    let health = game.entities[victim].health;
    game.apply_weapon_hit(victim, source, 20.0, DamageType::Water, DeathType::Flooded);
    assert_eq!(
        game.entities[victim].health.to_bits(),
        (health - 10.0).to_bits()
    );
    assert!(matches!(
        game.entities[victim].damage_state.damage_type,
        DamageType::Water
    ));
    game.apply_weapon_hit(
        victim,
        source,
        100_000.0,
        DamageType::Water,
        DeathType::Normal,
    );
    assert!(!game.entities[victim].alive());
    assert!(matches!(
        game.entities[victim].damage_state.damage_type,
        DamageType::Water
    ));
    assert!(matches!(
        game.entities[victim].damage_state.death_type,
        DeathType::Normal
    ));
    game.apply_native_damage(victim, None, 100_000.0, DeathType::Flooded);
    assert!(matches!(
        game.entities[victim].damage_state.damage_type,
        DamageType::Water
    ));
    let victim = game.spawn(tank, 1, [600.0, 300.0], true);
    game.apply_weapon_hit(victim, source, 1.0, DamageType::Water, DeathType::Flooded);
    game.apply_native_damage(victim, None, 100_000.0, DeathType::Flooded);
    assert!(matches!(
        game.entities[victim].damage_state.damage_type,
        DamageType::Water
    ));
    assert!(matches!(
        game.entities[victim].damage_state.death_type,
        DeathType::Flooded
    ));
    let victim = game.spawn(tank, 1, [650.0, 300.0], true);
    game.apply_weapon_hit(victim, source, 1.0, DamageType::Water, DeathType::Flooded);
    game.tick += 2;
    game.apply_native_damage(victim, None, 100_000.0, DeathType::Flooded);
    assert!(matches!(
        game.entities[victim].damage_state.damage_type,
        DamageType::Unresistable
    ));
}

fn ranger_at_attack_boundary(position: [f32; 2]) -> (Skirmish, usize, usize) {
    let mut game = crate::skirmish::tests::game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let unit = &mut game.catalog[infantry];
    unit.weapon.range = 100.0;
    unit.geometry.major = 7.0;
    unit.geometry.minor = 7.0;
    unit.speed = 20.0;
    unit.locomotor.turn_rate = 500.0 * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0));
    unit.locomotor.ground = Some(locomotor::GroundRules {
        wander: None,
        appearance: locomotor::GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        acceleration_damaged: 50.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        braking: 100.0 * ((1.0 / 30.0) * (1.0 / 30.0)),
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 5.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    let barracks = game.type_for(1, Kind::Barracks).unwrap();
    let target = &mut game.catalog[barracks];
    target.geometry.shape = crate::collision_geometry::Shape::Box;
    target.geometry.major = 55.0;
    target.geometry.minor = 45.0;
    let actor = game.spawn(infantry, 0, position, true);
    let target = game.spawn(barracks, 1, [395.0, 245.0], true);
    game.rebuild_navigation();
    (game, actor, target)
}

#[test]
fn attack_approach_does_not_stop_just_outside_the_weapon_range() {
    // Installed Ranger/Barracks stall, translated by400 units onto this map.
    let (mut game, actor, target) = ranger_at_attack_boundary([420.527_7, 421.458_6]);
    let range = game.entities[actor].weapon_range(
        &game.catalog[game.entities[actor].type_index],
        game.passenger_bonus_flags(actor),
    ) + game.object_radius(actor)
        + game.object_radius(target);
    let before = distance_squared(
        game.entities[actor].position,
        game.entities[target].position,
    );
    assert!(before > range * range);
    assert!(before.sqrt() < range + 1.0);
    let health = game.entities[target].health;
    game.attack(actor, target).unwrap();
    for _ in 0..180 {
        game.step();
        if game.entities[target].health < health {
            break;
        }
    }
    assert!(game.entities[target].health < health);
    assert!(
        distance_squared(
            game.entities[actor].position,
            game.entities[target].position
        ) <= range * range
    );
}

#[test]
fn a_valid_firing_position_does_not_require_the_attack_goal_margin() {
    let (mut game, actor, target) = ranger_at_attack_boundary([395.0, 422.0]);
    let position = game.entities[actor].position;
    let range = game.entities[actor].weapon_range(
        &game.catalog[game.entities[actor].type_index],
        game.passenger_bonus_flags(actor),
    ) + game.object_radius(actor)
        + game.object_radius(target);
    let margin = CELL_SIZE * 0.25;
    assert!(177.0 < range && 177.0 > range - margin);
    game.attack(actor, target).unwrap();
    for _ in 0..30 {
        game.step();
    }
    assert_eq!(
        game.entities[actor].position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert!(game.entities[target].health < game.entities[target].max_health);
}

#[test]
fn minimum_weapon_range_moves_away_before_firing() {
    let (mut game, actor, target) = ranger_at_attack_boundary([395.0, 345.0]);
    let kind = game.entities[actor].type_index;
    game.catalog[kind].weapon.min_range = 30.0;
    assert!(!game.within_attack_range(actor, target));
    let initial = game.entities[actor].position;
    let health = game.entities[target].health;
    game.attack(actor, target).unwrap();
    for _ in 0..240 {
        game.step();
        if game.entities[target].health < health {
            break;
        }
    }
    assert!(game.entities[target].health < health);
    assert!(game.within_attack_range(actor, target));
    assert!(distance_squared(initial, game.entities[actor].position) > 25.0);
}

#[test]
fn exhausted_attack_search_uses_the_original_ordinary_path_fallback() {
    let (mut game, actor, target) = ranger_at_attack_boundary([15.0, 15.0]);
    game.grid = crate::grid::Grid::new(4000, 3, []).unwrap();
    game.entities[target].position = [39_900.0, 15.0];
    game.configure_movement_terrain(crate::terrain::TerrainMap::classify(4000, 3, [], []).unwrap())
        .unwrap();
    game.refresh_occupancy();
    // The native attack search cannot span this strip within 2500 allocations.
    // Ordinary findPath can, so the attack state must retain its target.
    let unit = &game.catalog[game.entities[actor].type_index];
    let (radius, centered) = crate::skirmish::startup::radius_and_center(
        &unit.footprint(game.entities[actor].position, 0.0),
    );
    let unavailable = game.unavailable_goals(actor);
    let query = crate::grid::Attack {
        from: game.object_position(actor),
        target: game.entities[target].position,
        range_targets: None,
        radius,
        centered,
        weapon_range: [unit.weapon.min_range, unit.weapon.range],
        geometry_radius: game.object_radius(actor) + game.object_radius(target),
        distance: (game.entities[actor].weapon_range(
            &game.catalog[game.entities[actor].type_index],
            game.passenger_bonus_flags(actor),
        ) + game.object_radius(actor)
            + game.object_radius(target)),
        vehicle: false,
        unavailable: &unavailable,
    };
    assert!(
        game.navigation_for(actor)
            .attack_search_counted(&query, &game.route_units(actor), |_| true)
            .route
            .is_none()
    );
    game.attack(actor, target).unwrap();
    let start = game.entities[actor].position;
    for frame in 1..=30 {
        game.tick = frame;
        game.process_path_service();
        game.update_order(actor);
        game.finish_completed_movement(actor);
        game.wake_path_request(actor);
        game.update_ground_locomotor(actor);
        let entity = &mut game.entities[actor];
        locomotor::integrate_ground(entity, &game.catalog[entity.type_index], frame);
    }
    assert!(matches!(game.entities[actor].ai.order, Order::Attack(id) if id == target));
    assert_eq!(game.entities[actor].ai.ignored_producer, Some(target));
    assert!(game.entities[actor].ai.path_goal.unwrap().x > 3900);
    assert!(game.entities[actor].position[0] > start[0]);
    assert!(game.entities[actor].ai.ground_path.is_some());
}

#[test]
fn masked_objects_cannot_be_commanded_or_acquired_as_attack_targets() {
    let mut game = crate::skirmish::tests::game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let actor = game.spawn(infantry, 0, [300.0, 100.0], true);
    let target = game.spawn(infantry, 1, [400.0, 100.0], true);
    game.update_visibility();
    assert!(game.visible_to(0, target));
    game.entities[actor].ai.mood.next_check = 0;
    game.entities[target].maintenance.masked = true;
    assert!(game.attack(actor, target).is_err());
    assert!(!game.update_attack(actor, target));
    game.acquire_target(actor);
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
    assert!(game.presentation.shots.is_empty());
    game.entities[target].maintenance.masked = false;
    game.entities[actor].ai.mood.next_check = 0;
    game.acquire_target(actor);
    assert!(matches!(game.entities[actor].ai.order, Order::Attack(id) if id == target));
    assert!(game.attack(actor, target).is_ok());
}

#[test]
fn immediate_kill_keeps_the_firing_ranks_snapshot_before_promotion() {
    let mut game = crate::skirmish::tests::game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let unit = &mut game.catalog[infantry];
    unit.weapon.speed = 999_999.0;
    unit.veterancy.trainable = true;
    unit.veterancy.required = [0, 1, 2, 3];
    unit.veterancy.experience_value = [1; 4];
    unit.weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| vec![LaunchTransform::IDENTITY]),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: None,
        collide_mask: 0,
        shell_speed_scale: None,
    });
    let source = game.spawn(infantry, 0, [300.0, 100.0], true);
    let victim = game.spawn(infantry, 1, [400.0, 100.0], true);
    game.entities[victim].health = 1.0;
    game.fire_weapon(source, victim);
    assert!(!game.entities[victim].alive());
    assert_eq!(game.entities[source].veterancy, 1);
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(game.presentation.shots[0].veterancy, 0);
    assert_eq!(game.presentation.shots[0].target, Some(victim));
    assert!(game.projectiles.is_empty());
}

#[test]
fn shot_aim_is_original_target_center_before_infantry_scatter() {
    for physical in [false, true] {
        let mut game = crate::skirmish::tests::game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let infantry = game.type_for(0, Kind::Infantry).unwrap();
        game.catalog[tank].weapon.infantry_scatter = 10.0;
        if physical {
            game.projectile_terrain = Some(
                crate::skirmish::projectiles::Terrain::new(
                    64,
                    64,
                    [0.0; 2],
                    vec![0.0; 4096],
                    40.0,
                    [600.0; 2],
                )
                .unwrap(),
            );
            let kind = game.catalog.len();
            game.catalog
                .push(crate::skirmish::projectiles::fixtures::shell_template(
                    300, false,
                ));
            game.catalog[tank].weapon.motion = Some(crate::skirmish::WeaponMotion {
                turret_mounts: None,
                launches: std::array::from_fn(|_| vec![LaunchTransform::IDENTITY]),
                damage_thresholds: [0.5, 0.1],
                shots_per_barrel: 1,
                projectile_type: Some(kind),
                collide_mask: 0,
                shell_speed_scale: None,
            });
        }
        let source = game.spawn(tank, 0, [300.0, 100.0], true);
        let victim = game.spawn(infantry, 1, [400.0, 100.0], true);
        let center = game.target_center(victim);
        let seed = game.random;
        let (scattered, _) = game.weapon_destination(tank, 0, victim);
        game.random = seed;
        game.fire_weapon(source, victim);
        assert_eq!(
            game.presentation.shots[0].target_position.map(f32::to_bits),
            center.map(f32::to_bits)
        );
        assert_ne!(
            game.presentation.shots[0].target_position.map(f32::to_bits),
            scattered.map(f32::to_bits)
        );
        // Weapon.cpp: direct delayed damage retains victimID; a physical PUI
        // receives no tracked victim after a positive radius scatter.
        assert_eq!(
            game.projectiles[0].target,
            if physical { None } else { Some(victim) }
        );
        assert_ne!(game.random, seed);
    }
}

fn moving_launcher(projectile_type: usize) -> crate::skirmish::UnitType {
    let mut launcher = crate::skirmish::tests::fixture_type(
        crate::skirmish::Faction::Gla,
        crate::skirmish::Kind::Infantry,
    );
    launcher.name = "MovingLauncher".into();
    launcher.weapon.speed = 600.0;
    launcher.weapon.motion = Some(crate::skirmish::WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| {
            vec![LaunchTransform {
                position: [0.0, 0.0, 10.0],
                ..LaunchTransform::IDENTITY
            }]
        }),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: Some(projectile_type),
        collide_mask: 4,
        shell_speed_scale: None,
    });
    launcher.live_physics = Some(crate::skirmish::physical_ocl::LiveRules {
        motion: crate::skirmish::physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 50.0,
            gravity: -0.071,
            stiffness: 0.5,
            forward_friction: 0.0,
            lateral_friction: 0.0,
            aerodynamic_friction: 0.0,
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
        min_fall_speed: 0.0,
        fall_damage_factor: 1.0,
    });
    crate::skirmish::tests::register_update(&mut launcher, "PhysicsBehavior");
    launcher
}

#[test]
fn ordinary_moving_launcher_transfers_velocity_without_consuming_the_birth_impulse() {
    let mut game = crate::skirmish::tests::game();
    let target = game.spawn(6, 0, [405.0, 305.0], true);
    let projectile_type = game.catalog.len();
    game.catalog
        .push(crate::skirmish::point_defense::tests::missile_template(
            crate::skirmish::point_defense::tests::missile_rules(),
            1.0,
        ));
    let launcher = moving_launcher(projectile_type);
    let launcher_type = game.catalog.len();
    game.catalog.push(launcher);
    let source = game.spawn(launcher_type, 1, [355.0, 305.0], true);
    game.entities[source].facing = 0.0;
    game.entities[source]
        .live_physics
        .as_mut()
        .unwrap()
        .motion
        .velocity = [2.0, -1.0, 0.0];
    assert!(game.registered_physics(source));
    assert_eq!(
        game.entities[source].physics_velocity().map(f32::to_bits),
        [2.0_f32, -1.0, 0.0].map(f32::to_bits)
    );
    let victim = game.weapon_target(target, None);
    game.tick = 10;
    game.create_weapon_projectile(
        source,
        &victim,
        crate::skirmish::WeaponBinding {
            type_index: launcher_type,
            weapon_index: 0,
            weapon_slot: 0,
        },
        1.0,
        0,
    );
    assert_eq!(game.projectiles.len(), 1);
    let object = game.projectiles[0]
        .object
        .expect("canonical Missile Object");
    assert_eq!(
        game.entities[object]
            .projectile_velocity()
            .unwrap()
            .map(f32::to_bits),
        [2.0_f32, -1.0, 0.0].map(f32::to_bits)
    );
    // Physics constructor wakes at frame+1. Running the real heap on the birth
    // frame must preserve both launch position and the pending initial impulse.
    game.run_object_updates();
    assert_eq!(
        game.object_position(object).map(f32::to_bits),
        [355.0_f32, 305.0, 10.0].map(f32::to_bits)
    );
    game.tick += 1;
    game.update_projectile_physics(object);
    assert_eq!(
        game.entities[object]
            .projectile_velocity()
            .unwrap()
            .map(f32::to_bits),
        [7.0_f32, -1.0, -0.071].map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(object).map(f32::to_bits),
        [362.0_f32, 304.0, 9.929].map(f32::to_bits)
    );
}
