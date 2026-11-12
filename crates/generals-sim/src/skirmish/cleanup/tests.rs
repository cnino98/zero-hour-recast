use super::*;
use crate::{
    collision_geometry::{Geometry, Shape},
    skirmish::{hazards, tests::game},
};

fn field() -> hazards::Rules {
    hazards::Rules {
        name: "PoisonFieldMedium".into(),
        damage: 2.0,
        radius: 80.0,
        interval: 15,
        airborne_height: 0.64,
        lifetime: 900,
        health: 120.0,
        geometry_radius: 40.0,
        geometry: Geometry {
            shape: Shape::Cylinder,
            major: 40.0,
            minor: 1.0,
            height: 1.0,
            is_small: false,
        },
        core_radius: 10.0,
        core_damage: 200.0,
        death_type: crate::skirmish::DeathType::PoisonedBeta,
        fire_fx: String::new(),
        death_fx: String::new(),
        ambient: String::new(),
    }
}

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = game();
    let mut cleaner_unit = crate::skirmish::tests::fixture_type(
        crate::skirmish::Faction::Usa,
        crate::skirmish::Kind::Vehicle,
    );
    cleaner_unit.name = "NativeAmbulanceFixture".into();
    cleaner_unit.behavior.cleanup = vec![Rules {
        slot: 0,
        scan_frames: 30,
        scan_range: 101.0,
    }];
    cleaner_unit.behavior.cleanup_area = vec![AreaRules {
        range: 300.0,
        power: "SpecialAbilityAmbulanceCleanupArea".into(),
    }];
    cleaner_unit.weapon.range = 100.0;
    cleaner_unit.weapon.damage = 100.0;
    cleaner_unit.weapon.damage_type = crate::skirmish::DamageType::HazardCleanup;
    cleaner_unit.weapon.area = Some(crate::skirmish::AreaDamage {
        at_source: false,
        primary_radius: 50.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 7,
    });
    crate::skirmish::tests::register_update(&mut cleaner_unit, "CleanupHazardUpdate");
    let cleaner_type = game.catalog.len();
    game.catalog.push(cleaner_unit);
    game.catalog[6].weapon.hazard = Some(field());
    crate::skirmish::tests::native_hazard_fixture(&mut game, 6, hazards::Origin::Weapon(0));
    let cleaner = game.spawn(cleaner_type, 0, [300.0, 300.0], true);
    let source = game.spawn(6, 1, [600.0, 300.0], true);
    (game, cleaner, source)
}

#[test]
fn passive_cleanup_chooses_nearest_field_and_preserves_scan_counter() {
    let (mut game, cleaner, source) = fixture();
    game.create_weapon_hazard(source, 0, 1, [390.0, 300.0, 0.0], 1);
    game.create_weapon_hazard(source, 0, 1, [340.0, 300.0, 0.0], 1);
    let target = game.hazards[1].object;
    // Native registerObject only marks cells dirty. The isolated cleanup
    // callback follows PartitionManager::update, as it does in GameLogic.
    assert_eq!(
        game.closest_cleanup_hazard(cleaner, [300.0, 300.0], 101.0),
        None
    );
    game.update_ground_contacts();
    game.update_cleanup_module(cleaner, 0);
    assert!(matches!(game.entities[cleaner].ai.order, Order::Attack(id) if id == target));
    assert!(matches!(
        game.entities[cleaner].weapon_set.lock,
        Lock::Temporary
    ));
    assert_eq!(game.entities[cleaner].behaviors.cleanup[0].scan_left, 30);
    game.update_cleanup_module(cleaner, 0);
    assert_eq!(game.entities[cleaner].behaviors.cleanup[0].scan_left, 29);
    game.set_hacked_until(cleaner, crate::skirmish::ai_dispatch::native_frame(100));
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[cleaner].behaviors.cleanup[0].scan_left, 29);
}

#[test]
fn area_cleanup_preserves_internal_return_and_native_player_idle_order() {
    let (mut game, cleaner, source) = fixture();
    let point = [500.0, 300.0];
    game.clean_area(cleaner, point).unwrap();
    assert_eq!(game.entities[cleaner].cleaning_area(), Some(point));
    game.cleanup_order(cleaner, Order::Idle);
    game.create_weapon_hazard(source, 0, 1, [550.0, 300.0, 0.0], 1);
    game.update_ground_contacts();
    game.update_cleanup_module(cleaner, 0);
    // Human AI acquisition does not chase beyond attack range (AIStates.cpp2613).
    // Area selection still retains this field, which is outside passive ScanRange.
    assert_eq!(
        game.entities[cleaner].behaviors.cleanup[0].target,
        Some(game.hazards[0].object)
    );
    assert!(matches!(
        game.entities[cleaner].weapon_set.lock,
        Lock::Temporary
    ));
    assert!(game.entities[cleaner].ai_idle());
    assert!(!game.entities[cleaner].ai.command.allowed_to_chase);
    assert!(game.entities[cleaner].last_command_from_ai());
    game.stop(cleaner).unwrap();
    game.update_cleanup_module(cleaner, 0);
    // Source isIdle branch precedes last-command-source rejection: native Stop
    // returns to idle and resumes cleanup; a nonidle player command cancels.
    assert_eq!(game.entities[cleaner].cleaning_area(), Some(point));
    assert!(game.clean_area(cleaner, [f32::NAN, 0.0]).is_err());
}

#[test]
fn failed_internal_path_keeps_area_targeting_but_player_move_cancels() {
    let (mut game, cleaner, source) = fixture();
    let center = [500.0, 300.0];
    game.clean_area(cleaner, center).unwrap();
    // A queued path callback reported failure to the internal Move state.
    game.entities[cleaner].ai.path_request.failed = true;
    assert!(!game.travel_ground(cleaner, center, 0.1, None));
    assert!(matches!(game.entities[cleaner].ai.order, Order::Idle));
    assert_eq!(game.entities[cleaner].cleaning_area(), Some(center));
    assert!(
        game.entities[cleaner].ai.attack_context.source
            == crate::skirmish::builder_disarm::Source::Ai
    );

    // This field is outside passive ScanRange, but inside the retained area.
    game.create_weapon_hazard(source, 0, 1, [550.0, 300.0, 0.0], 1);
    let target = game.hazards[0].object;
    game.update_ground_contacts();
    game.update_cleanup_module(cleaner, 0);
    assert_eq!(
        game.entities[cleaner].behaviors.cleanup[0].target,
        Some(target)
    );
    assert!(matches!(
        game.entities[cleaner].weapon_set.lock,
        Lock::Temporary
    ));
    assert!(game.entities[cleaner].ai_idle());
    assert!(!game.entities[cleaner].ai.command.allowed_to_chase);
    assert!(game.entities[cleaner].last_command_from_ai());

    game.move_to(cleaner, [350.0, 300.0]).unwrap();
    game.update_cleanup_module(cleaner, 0);
    assert!(game.entities[cleaner].cleaning_area().is_none());
    game.entities[cleaner].ai.path_request.failed = true;
    assert!(!game.travel_ground(cleaner, [350.0, 300.0], 0.1, None));
    assert!(game.entities[cleaner].cleaning_area().is_none());
    assert!(
        game.entities[cleaner].ai.attack_context.source
            == crate::skirmish::builder_disarm::Source::Player
    );
}

#[test]
fn cleanup_missile_records_geometry_center_but_launches_toward_field_origin() {
    use crate::skirmish::{WeaponMotion, point_defense};

    let (mut game, cleaner, source) = fixture();
    let mut rules = point_defense::tests::missile_rules();
    rules.follow_target = false;
    let projectile_type = game.catalog.len();
    game.catalog
        .push(point_defense::tests::missile_template(rules, 1.0));
    let cleaner_type = game.entities[cleaner].type_index;
    game.catalog[cleaner_type].weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: [Vec::new(), Vec::new(), Vec::new()],
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: Some(projectile_type),
        collide_mask: 4,
        shell_speed_scale: None,
    });
    game.create_weapon_hazard(source, 0, 1, [340.0, 300.0, 0.0], 1);
    let target = game.hazards[0].object;
    let position = game.hazards[0].position;
    assert_eq!(
        game.body_geometry_bounds(target).center_height.to_bits(),
        0.5_f32.to_bits()
    );
    assert_eq!(
        game.body_geometry_bounds(target).max_height.to_bits(),
        1.0_f32.to_bits()
    );
    game.fire_weapon(cleaner, target);

    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(
        game.presentation.shots[0].target_position.map(f32::to_bits),
        [position[0], position[1], position[2] + 0.5].map(f32::to_bits)
    );
    assert_eq!(game.projectiles.len(), 1);
    let projectile = &game.projectiles[0];
    assert_eq!(projectile.target, Some(target));
    assert_eq!(projectile.hazard_target, None);
    let flight = game.entities[projectile.object.unwrap()]
        .projectile_flight
        .as_ref()
        .unwrap();
    assert!(matches!(
        flight.controls[0],
        crate::skirmish::projectiles::Control::Missile(_)
    ));
    // WeaponTemplate's FX uses GeometryInfo center; the actual projectile
    // destination is the object origin. MissileAIUpdate sees no elevated target
    // and therefore adds no launch lift (SH Weapon.cpp977,1159 / MissileAIUpdate232).
    assert_eq!(flight.motion.axes[0][2].to_bits(), 0.0_f32.to_bits());
}

#[test]
fn cleanup_area_damage_uses_real_field_health_and_immediate_native_death_callback() {
    let (mut game, cleaner, source) = fixture();
    let field_type = game
        .catalog
        .iter()
        .position(|unit| unit.name == "PoisonFieldMedium")
        .unwrap();
    game.catalog[field_type].behavior.death_fx[0].effect = "FieldDeath".into();
    game.create_weapon_hazard(source, 0, 1, [340.0, 300.0, 0.0], 1);
    game.create_weapon_hazard(source, 0, 1, [600.0, 300.0, 0.0], 1);
    let object = game.hazards[0].object;
    game.update_ground_contacts();
    game.deal_bound_weapon_damage(
        cleaner,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[cleaner].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[cleaner].weapon_slot(),
        },
        None,
        [340.0, 300.0, 0.0],
        1.0,
    );
    assert_eq!(game.hazards.len(), 2);
    assert_eq!(
        game.hazards[0].health(&game.entities).to_bits(),
        20.0_f32.to_bits()
    );
    assert!(game.presentation.object_fx_events.is_empty());
    game.deal_bound_weapon_damage(
        cleaner,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[cleaner].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[cleaner].weapon_slot(),
        },
        None,
        [340.0, 300.0, 0.0],
        1.0,
    );
    // Actual DestroyDie queues destruction after immediate health/DIE effects;
    // the native Object and its index remain until that destruction boundary.
    assert!(game.entities[object].is_effectively_dead());
    assert!(game.update_objects[game.entities[object].update_owner].destroy_requested);
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    game.process_update_destruction();
    assert_eq!(game.hazards.len(), 1);
    assert_eq!(
        game.hazards[0].health(&game.entities).to_bits(),
        120.0_f32.to_bits()
    );
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    let event = &game.presentation.object_fx_events[0];
    assert_eq!(event.object, object);
    assert_eq!(event.effect, "FieldDeath");
    assert_eq!(
        event
            .secondary_position
            .map(|position| position.map(f32::to_bits)),
        Some(game.object_position(cleaner).map(f32::to_bits))
    );
    assert_eq!(
        game.presentation
            .weapon_fx
            .iter()
            .filter(|event| matches!(event, crate::skirmish::WeaponFxEvent::ObjectFx(0)))
            .count(),
        1
    );
}
