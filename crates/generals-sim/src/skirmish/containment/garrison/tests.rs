use super::GarrisonRules;
use crate::skirmish::{Kind, Order, Skirmish};

pub(super) fn setup(immune: bool) -> (Skirmish, usize) {
    let (mut game, carrier) = super::super::tests::setup(0.0, 0.5);
    let unit = &mut game.catalog[game.entities[carrier].type_index];
    unit.kind = Kind::Barracks;
    unit.kind_of = vec!["STRUCTURE".into()];
    let rules = unit.contain.as_mut().unwrap();
    rules.garrison = Some(GarrisonRules {
        allow_enemies: false,
        allow_neutral: true,
        immune_to_clear: immune,
        heal_frames: None,
        stations: None,
    });
    rules.weapons.fire = false;
    rules.art = std::array::from_fn(|stage| {
        let height = [30.0, 20.0, 10.0][stage];
        super::super::Art {
            exits: Vec::new(),
            fire_points: crate::skirmish::containment::tests::fire_points(&[
                [10.0, 0.0, height],
                [-10.0, 0.0, height],
            ]),
        }
    });
    game.entities[carrier].owner = game.neutral_owner();
    game.rebuild_navigation();
    let carrier = crate::skirmish::containment::tests::respawn_contain_family(
        &mut game,
        carrier,
        "GarrisonContain",
    );
    (game, carrier)
}

#[test]
fn admission_counts_occupants_and_last_removal_restores_original_owner() {
    let (mut game, carrier) = setup(false);
    game.catalog[5].transport_slots = 5;
    let first = game.spawn(5, 0, [355.0, 255.0], true);
    let second = game.spawn(5, 0, [355.0, 255.0], true);
    assert!(game.can_enter(first, carrier));
    game.board(first, carrier);
    assert_eq!(game.entities[carrier].owner, 0);
    assert!(game.can_enter(second, carrier));
    game.board(second, carrier);
    assert!(game.passenger_can_fire(first));
    let third = game.spawn(5, 0, [355.0, 255.0], true);
    assert!(!game.can_enter(third, carrier));
    game.remove_passenger(second, carrier);
    assert_eq!(game.entities[carrier].owner, 0);
    game.remove_passenger(first, carrier);
    assert_eq!(game.entities[carrier].owner, game.neutral_owner());
    assert!(!game.entities[first].garrisoned());
}

#[test]
fn garrison_rejects_enemies_no_garrison_and_badly_damaged_buildings() {
    let (mut game, carrier) = setup(false);
    let first = game.spawn(5, 0, [355.0, 255.0], true);
    let enemy = game.spawn(5, 1, [355.0, 255.0], true);
    game.board(first, carrier);
    assert!(!game.can_enter(enemy, carrier));
    game.entities[carrier].health = game.entities[carrier].max_health * 0.1;
    assert!(!game.can_enter(enemy, carrier));
    let unit = &mut game.catalog[game.entities[carrier].type_index];
    unit.kind_of.push("GARRISONABLE_UNTIL_DESTROYED".into());
    let ally = game.spawn(5, 0, [355.0, 255.0], true);
    assert!(game.can_enter(ally, carrier));
    game.catalog[5].kind_of.push("NO_GARRISON".into());
    assert!(!game.can_enter(ally, carrier));
}

#[test]
fn firepoint_probe_does_not_release_occupied_point_and_track_uses_free_closest() {
    let (mut game, carrier) = setup(false);
    let first = game.spawn(5, 0, [355.0, 255.0], true);
    let second = game.spawn(5, 0, [355.0, 255.0], true);
    let target = game.spawn(6, 1, [415.0, 255.0], true);
    game.board(first, carrier);
    game.board(second, carrier);
    game.set_order(first, Order::Attack(target));
    game.set_order(second, Order::Attack(target));
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_points(carrier).unwrap()[0], Some(first));
    assert_eq!(game.garrison_points(carrier).unwrap()[2], Some(second));
    let point = game.garrison_attack_position(first, target).unwrap();
    assert_eq!(
        point.map(f32::to_bits),
        [355.0_f32, 255.0, 0.0].map(f32::to_bits)
    );
    game.set_order(first, Order::Idle);
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_points(carrier).unwrap()[0], Some(second));
    assert_eq!(
        game.entities[first].position.map(f32::to_bits),
        game.entities[carrier].position.map(f32::to_bits)
    );
}

#[test]
fn garrison_bonus_extends_firepoint_range_and_is_removed_on_exit() {
    let (mut game, carrier) = setup(false);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let target = game.spawn(6, 1, [610.0, 255.0], true);
    game.catalog[5].weapon.bonuses.global[0] = [1.25, 1.0, 2.0, 1.5, 1.0];
    assert!(!game.within_attack_range(rider, target));
    game.board(rider, carrier);
    // Place the victim just past the center's inclusive range but within
    // range of the east window at +10. Command admission must use the window.
    game.entities[carrier].facing = 0.0;
    let reach =
        game.effective_weapon_range(rider) + game.object_radius(rider) + game.object_radius(target);
    game.entities[target].position = [game.entities[carrier].position[0] + reach + 5.0, 255.0];
    assert!(!game.within_attack_range(rider, target));
    assert!(game.player_attack_target_reachable(carrier, target));
    assert!(game.prepare_garrison_attack(rider, target));
    assert!(game.within_attack_range(rider, target));
    let actor = &game.entities[rider];
    assert_eq!(
        actor
            .weapon_damage_bonus(actor.active_weapon(&game.catalog[5]), 0)
            .to_bits(),
        1.25_f32.to_bits()
    );
    assert_eq!(
        actor
            .weapon_rate_for(actor.active_weapon(&game.catalog[5]), 0)
            .to_bits(),
        1.5_f32.to_bits()
    );
    game.exit_container(rider).unwrap();
    game.update_container_exit(rider);
    assert_eq!(game.entities[rider].contained_by(), None);
    assert!(!game.within_attack_range(rider, target));
}

#[test]
fn damage_threshold_orders_exit_and_regular_evacuation_has_no_transport_delay() {
    let (mut game, carrier) = setup(false);
    let first = game.spawn(5, 0, [355.0, 255.0], true);
    let second = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(first, carrier);
    game.board(second, carrier);
    let damage = game.entities[carrier].max_health * 0.91;
    game.apply_hit(carrier, 2, damage);
    assert!(matches!(
        game.entities[first].ai.order,
        Order::ExitContainer
    ));
    assert!(matches!(
        game.entities[second].ai.order,
        Order::ExitContainer
    ));
    game.update_container_exit(second);
    game.update_container_exit(first);
    assert!(game.passengers(carrier).is_empty());
    assert_eq!(game.entities[carrier].owner, game.neutral_owner());
    assert!(!game.entities[carrier].container_door_open(game.tick));
}

#[test]
fn garrison_healing_uses_containment_frame_and_exact_per_frame_fraction() {
    let (mut game, carrier) = setup(false);
    game.catalog[game.entities[carrier].type_index]
        .contain
        .as_mut()
        .unwrap()
        .garrison
        .as_mut()
        .unwrap()
        .heal_frames = Some(4.0);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.entities[rider].health = 1.0;
    game.board(rider, carrier);
    game.update_garrison(carrier);
    assert_eq!(
        game.entities[rider].health.to_bits(),
        (1.0 + game.entities[rider].max_health / 4.0).to_bits()
    );
    game.tick += 4;
    game.update_garrison(carrier);
    assert_eq!(
        game.entities[rider].health.to_bits(),
        game.entities[rider].max_health.to_bits()
    );
}

#[test]
fn clearing_kills_front_first_with_count_kind_filters_and_immunity() {
    let (mut game, carrier) = setup(false);
    let first = game.spawn(5, 0, [355.0, 255.0], true);
    let second = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(first, carrier);
    game.board(second, carrier);
    assert_eq!(
        game.clear_garrison(2, carrier, 1, &["INFANTRY".into()], &[]),
        1
    );
    assert!(!game.entities[first].alive());
    assert!(game.entities[second].alive());
    assert_eq!(
        game.clear_garrison(2, carrier, 1, &[], &["INFANTRY".into()]),
        0
    );
    game.catalog[game.entities[carrier].type_index]
        .contain
        .as_mut()
        .unwrap()
        .garrison
        .as_mut()
        .unwrap()
        .immune_to_clear = true;
    assert_eq!(game.clear_garrison(2, carrier, 1, &[], &[]), 0);
    assert!(game.entities[second].alive());
}

#[test]
fn forced_removal_after_death_preserves_survivors_without_transport_scatter() {
    let (mut game, carrier) = setup(false);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(rider, carrier);
    let health = game.entities[carrier].health;
    game.apply_hit(carrier, 2, health);
    assert!(game.entities[rider].alive());
    assert_eq!(
        game.entities[rider].health.to_bits(),
        (game.entities[rider].max_health * 0.5).to_bits()
    );
    assert_eq!(game.entities[rider].contained_by(), None);
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert_eq!(
        game.entities[rider].position.map(f32::to_bits),
        game.entities[carrier].position.map(f32::to_bits)
    );
}

#[test]
fn flashbang_collision_clears_one_rider_without_normal_detonation_or_structure_damage() {
    use crate::{
        collision_geometry::{Geometry, Shape},
        skirmish::{
            ClearBuilding, FlightRules, LaunchTransform, ProjectileFxStage, ShellContact,
            WeaponBinding, WeaponFxEvent, WeaponMotion, damage::WeaponTarget,
            projectiles::fixtures::shell_template,
        },
    };
    let (mut game, carrier) = setup(false);
    game.set_projectile_terrain(81, 61, [0.0; 2], vec![0.0; 81 * 61], 40.0)
        .unwrap();
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let source = game.spawn(5, 1, [415.0, 255.0], true);
    game.board(rider, carrier);
    game.update_ground_contacts();
    let mut shell = shell_template(30, false);
    shell.geometry = Geometry {
        is_small: true,
        shape: Shape::Sphere,
        major: 1.0,
        minor: 1.0,
        height: 1.0,
    };
    shell.geometry_bounds = crate::skirmish::GeometryBounds {
        center_height: 0.0,
        sphere_radius: 1.0,
        max_height: 1.0,
    };
    let FlightRules::Shell(rules) = &mut shell.projectile_rules[0] else {
        unreachable!()
    };
    rules.heights = [0.0; 2];
    rules.indents = [0.0, 1.0];
    rules.adjust_per_frame = 0.0;
    rules.contact = Some(ShellContact {
        geometry: Geometry {
            is_small: true,
            shape: Shape::Sphere,
            major: 1.0,
            minor: 1.0,
            height: 1.0,
        },
        clear: Some(ClearBuilding {
            count: 1,
            allowed: vec!["INFANTRY".into()],
            forbidden: Vec::new(),
        }),
    });
    let projectile_type = game.catalog.len();
    game.catalog.push(shell);
    game.catalog[5].weapon.damage = 999.0;
    game.catalog[5].weapon.speed = 30.0;
    game.catalog[5].weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| {
            vec![LaunchTransform {
                position: [-60.0, 0.0, 0.0],
                axes: LaunchTransform::IDENTITY.axes,
            }]
        }),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: Some(projectile_type),
        collide_mask: 0,
        shell_speed_scale: None,
    });
    game.create_weapon_projectile(
        source,
        &WeaponTarget {
            object: Some(carrier),
            position: [365.0, 255.0, 0.0],
            projectile_position: [365.0, 255.0, 0.0],
            scatter_radius: 0.0,
        },
        WeaponBinding {
            type_index: 5,
            weapon_index: 0,
            weapon_slot: 0,
        },
        1.0,
        0,
    );
    let projectile_object = game.projectiles.last().unwrap().object.unwrap();
    let health = game.entities[carrier].health;
    game.update_ground_contacts();
    game.process_update_destruction();
    assert!(!game.entities[rider].alive());
    assert!(game.projectiles.is_empty());
    assert_eq!(game.entities[carrier].health.to_bits(), health.to_bits());
    assert_eq!(game.entities[carrier].owner, game.neutral_owner());
    let kill_event = game.presentation.weapon_fx.iter().position(|event|
        matches!(event, WeaponFxEvent::Projectile(event) if matches!(event.stage, ProjectileFxStage::GarrisonKill) && event.object == Some(carrier))).unwrap();
    let destroyed = game.presentation.weapon_fx.iter().position(|event|
        matches!(event, WeaponFxEvent::DrawableDestroyed(id) if *id == projectile_object)).unwrap();
    assert!(
        kill_event < destroyed,
        "native clearing FX precedes projectile destruction"
    );
    assert!(!game.presentation.weapon_fx.iter().any(|event| matches!(event, WeaponFxEvent::Projectile(event) if matches!(event.stage, ProjectileFxStage::Detonation))));
}

#[test]
fn garrison_muzzle_uses_previous_shot_and_original_four_frame_lifetime() {
    let (mut game, carrier) = setup(false);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let target = game.spawn(6, 1, [415.0, 255.0], true);
    game.board(rider, carrier);
    game.tick = 10;
    game.set_order(rider, Order::Attack(target));
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_effect(carrier, 0), Some((10, false)));
    let destination = game.weapon_target(target, None);
    assert!(
        game.fire_current_weapon_payload(rider, destination)
            .is_some()
    );
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_effect(carrier, 0), Some((10, false)));
    game.tick = 11;
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_effect(carrier, 0), Some((10, true)));
    game.tick = 15;
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_effect(carrier, 0), Some((10, true)));
    game.tick = 16;
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_effect(carrier, 0), Some((10, false)));
}

#[test]
fn ordinary_garrison_attack_orders_riders_and_stop_releases_windows() {
    let (mut game, carrier) = setup(false);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let target = game.spawn(6, 1, [415.0, 255.0], true);
    game.board(rider, carrier);
    assert!(game.container_can_attack(carrier, target));
    game.attack(carrier, target).unwrap();
    assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
    game.match_garrison_points(carrier);
    assert!(
        game.garrison_points(carrier)
            .unwrap()
            .contains(&Some(rider))
    );
    game.stop(carrier).unwrap();
    game.match_garrison_points(carrier);
    assert!(
        !game
            .garrison_points(carrier)
            .unwrap()
            .contains(&Some(rider))
    );
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
}

#[test]
fn allied_new_occupant_orders_foreign_riders_out_and_takes_control() {
    let (mut game, carrier) = setup(false);
    game.slots[0].team = Some(1);
    game.slots[1].team = Some(1);
    let first = game.spawn(5, 0, [355.0, 255.0], true);
    let second = game.spawn(5, 1, [355.0, 255.0], true);
    game.board(first, carrier);
    game.tick = 1;
    game.run_object_updates();
    game.enter(second, carrier).unwrap();
    assert!(game.container_contact(second, carrier));
    // Native front() remains the original occupant until its exit callback.
    assert_eq!(game.entities[carrier].owner, 0);
    assert!(matches!(
        game.entities[first].ai.order,
        Order::ExitContainer
    ));
    game.update_container_exit(first);
    assert_eq!(game.passengers(carrier), &[second]);
    assert_eq!(game.entities[carrier].owner, 1);
    game.remove_passenger(second, carrier);
    assert_eq!(game.entities[carrier].owner, game.neutral_owner());
}

#[test]
fn capture_rejects_occupants_but_ejects_late_entries_without_claiming_building() {
    let (mut game, carrier) = setup(false);
    let carrier_type = game.entities[carrier].type_index;
    game.catalog[carrier_type].maintenance.kind_flags |=
        crate::skirmish::MaintenanceRules::CAPTURABLE;
    game.catalog[carrier_type].kind_of.push("CAPTURABLE".into());
    game.catalog[5].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    crate::skirmish::special_ability::capture_fixture::bind(
        &mut game.catalog[5],
        crate::skirmish::special_ability::PowerType::SpecialInfantryCaptureBuilding,
        [1, 2, 1, 1],
        true,
    );
    game.catalog[5].behavior.special_abilities[0]
        .approach
        .start_range = 10.0;
    game.players[1].upgrades.push("Capture".into());
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let capturer = game.spawn(5, 1, [365.0, 255.0], true);
    game.tick += 1;
    game.update_visibility();
    game.board(rider, carrier);
    assert!(!game.can_capture(capturer, carrier));
    game.remove_garrison_contents(carrier);
    game.update_visibility();
    // GarrisonContain returns its neutral apparent player even while empty.
    // ActionManager therefore rejects it until it is actually enemy-controlled.
    assert!(!game.can_capture(capturer, carrier));
    game.transfer_maintenance_owner(carrier, 0);
    assert!(game.can_capture(capturer, carrier));
    // A late stealth-garrison entrant must be exposed by removeAllContained(TRUE).
    let mut late =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Infantry);
    late.kind_of.push("STEALTH_GARRISON".into());
    late.behavior.stealth = Some(crate::skirmish::stealth::Rules {
        module_order: late.behavior_modules.len(),
        delay: 60,
        forbidden: 0,
        threshold_speed: 0.1,
        required_status: Vec::new(),
        forbidden_status: Vec::new(),
        hint_status: Vec::new(),
        innate: true,
        use_rider: false,
        idle_enemies_on_reveal: false,
        wake: crate::skirmish::stealth::Wake::Automatic,
        upgrades: Vec::new(),
        reveal_distance: 0.0,
        black_market_frames: 0,
        friendly_opacity: [0.5, 1.0],
        pulse_frames: 30,
    });
    crate::skirmish::tests::register_update(&mut late, "StealthUpdate");
    // This isolated actor must finish its authored stealth delay before exposure.
    late.ai.acquisition = 0;
    game.catalog.push(late);
    let late = game.spawn(game.catalog.len() - 1, 0, [355.0, 255.0], true);
    for _ in 0..60 {
        game.step();
    }
    assert!(game.entities[late].stealthed());
    game.capture(capturer, carrier).unwrap();
    game.board(late, carrier);
    assert!(game.entities[late].garrisoned());
    for _ in 0..300 {
        game.step();
        if !game.entities[late].garrisoned() {
            break;
        }
    }
    assert!(!game.entities[late].garrisoned());
    assert!(game.entities[late].alive());
    game.update_stealth_module(late);
    assert!(game.entities[late].detected());
    assert_eq!(game.entities[carrier].owner, game.neutral_owner());
}

#[test]
fn interrupted_attack_reenters_aim_even_when_reissued_against_same_object() {
    let (mut game, carrier) = setup(false);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let target = game.spawn(6, 1, [415.0, 255.0], true);
    game.board(rider, carrier);
    game.set_order(rider, Order::Attack(target));
    assert!(game.prepare_garrison_attack(rider, target));
    assert_eq!(game.garrison_points(carrier).unwrap()[0], Some(rider));
    game.entities[target].position = [295.0, 255.0];
    game.stop(carrier).unwrap();
    game.attack(carrier, target).unwrap();
    assert!(game.prepare_garrison_attack(rider, target));
    assert_eq!(game.garrison_points(carrier).unwrap()[1], Some(rider));
}

#[test]
fn relocated_gun_keeps_native_initial_facing_until_next_tracking_update() {
    let (mut game, carrier) = setup(false);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let target = game.spawn(6, 1, [415.0, 255.0], true);
    game.board(rider, carrier);
    game.set_order(rider, Order::Attack(target));
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_points(carrier).unwrap()[0], Some(rider));
    game.entities[target].position = [295.0, 270.0];
    game.match_garrison_points(carrier);
    assert_eq!(game.garrison_points(carrier).unwrap()[1], Some(rider));
    assert_eq!(
        game.garrison_effect_facing(carrier, 1).unwrap().to_bits(),
        0.0_f32.to_bits()
    );
    game.match_garrison_points(carrier);
    assert!(game.garrison_effect_facing(carrier, 1).unwrap() > 2.0);
}

#[test]
fn toxin_missile_clears_two_original_occupants_and_skips_normal_explosion() {
    use crate::skirmish::{
        ClearBuilding, ProjectileFxStage, WeaponFxEvent,
        point_defense::tests::{missile_from_template, missile_rules, missile_template},
    };
    let (mut game, carrier) = setup(false);
    let riders: Vec<_> = (0..3)
        .map(|_| game.spawn(5, 0, [355.0, 255.0], true))
        .collect();
    for &rider in &riders {
        game.board(rider, carrier);
    }
    game.update_ground_contacts();
    let position = game.object_position(carrier);
    let mut rules = missile_rules();
    rules.follow_target = false;
    rules.clear = Some(ClearBuilding {
        count: 2,
        allowed: vec!["INFANTRY".into()],
        forbidden: Vec::new(),
    });
    let before = game.entities[carrier].health;
    missile_from_template(
        &mut game,
        position,
        1,
        Some(carrier),
        position,
        missile_template(rules, 1.0),
    );
    game.tick = 1;
    game.run_object_updates();
    game.update_ground_contacts();
    game.process_update_destruction();
    assert!(game.projectiles.is_empty());
    assert!(!game.entities[riders[0]].alive() && !game.entities[riders[1]].alive());
    assert!(game.entities[riders[2]].alive());
    assert_eq!(game.entities[carrier].health.to_bits(), before.to_bits());
    assert!(game.presentation.weapon_fx.iter().any(|event| matches!(event, WeaponFxEvent::Projectile(event) if matches!(event.stage, ProjectileFxStage::GarrisonKill) && event.object == Some(carrier))));
    assert!(!game.presentation.weapon_fx.iter().any(|event| matches!(event, WeaponFxEvent::Projectile(event) if matches!(event.stage, ProjectileFxStage::Detonation))));
}

#[test]
fn garrison_player_attack_fanout_retains_neutral_targetability_and_force_provenance() {
    let (mut game, carrier) = setup(false);
    game.entities[carrier].owner = 0;
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let target = game.spawn(6, game.neutral_owner(), [415.0, 255.0], true);
    game.board(rider, carrier);
    assert!(game.container_can_attack(carrier, target));
    assert!(game.attack(carrier, target).is_err());
    game.set_map_body_controls(target, true, false);
    game.attack(carrier, target).unwrap();
    assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
    assert!(!game.entities[rider].ai.command.from_ai);
    assert!(!game.entities[rider].ai.attack_context.forced);
    game.set_map_body_controls(target, false, false);
    assert!(!game.update_attack(rider, target));
    assert!(game.force_attack_target_reachable(carrier, target));
    game.force_attack(carrier, target).unwrap();
    assert!(game.entities[rider].ai.attack_context.forced);
    assert!(game.update_attack(rider, target));
    game.entities[target].owner = 0;
    assert!(game.force_attack_target_reachable(carrier, target));
    game.force_attack(carrier, target).unwrap();
    assert!(game.update_attack(rider, target));
}

#[test]
fn fresh_forced_garrison_attack_validates_incoming_mode_before_rider_command_copy() {
    let (mut game, carrier) = setup(false);
    game.entities[carrier].owner = 0;
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(rider, carrier);
    game.catalog[6].behavior.stealth = Some(crate::skirmish::stealth::Rules {
        module_order: 0,
        delay: 2,
        forbidden: 0,
        threshold_speed: 0.1,
        required_status: vec![],
        forbidden_status: vec![],
        hint_status: vec![],
        innate: true,
        use_rider: false,
        idle_enemies_on_reveal: false,
        wake: crate::skirmish::stealth::Wake::Automatic,
        upgrades: vec![],
        reveal_distance: 0.0,
        black_market_frames: 0,
        friendly_opacity: [0.5, 1.0],
        pulse_frames: 30,
    });
    let target = game.spawn(6, 0, [415.0, 255.0], true);
    game.grant_stealth(target);
    assert!(game.entities[target].stealthed());
    assert!(!game.entities[target].detected());
    assert!(!game.entities[rider].ai.attack_context.forced);
    assert!(!game.container_can_attack(carrier, target));
    assert!(!game.weapon_can_target(rider, target));
    assert!(game.force_attack_target_reachable(carrier, target));
    game.force_attack(carrier, target).unwrap();
    assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
    assert!(game.entities[rider].ai.attack_context.forced);
    assert!(game.update_attack(rider, target));
    assert!(!game.player_attack_target_reachable(carrier, target));
    game.stop(carrier).unwrap();
    assert!(!game.entities[carrier].ai.attack_context.forced);
    assert!(!game.entities[rider].ai.attack_context.forced);
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert!(!game.weapon_can_target(rider, target));
}

#[test]
fn clearing_scores_before_immortal_body_rejects_the_kill() {
    let (mut game, carrier) = setup(false);
    game.catalog[5].minimum_health = 1.0;
    let first = game.spawn(5, 0, [355.0, 255.0], true);
    let second = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(first, carrier);
    game.board(second, carrier);
    let source = 2;
    let owner = game.entities[source].owner;
    let before = game.players[owner].statistics.units_destroyed;
    assert_eq!(game.clear_garrison(source, carrier, 1, &[], &[]), 1);
    assert_eq!(game.entities[first].health.to_bits(), 1.0_f32.to_bits());
    assert!(game.entities[first].alive());
    assert_eq!(
        game.entities[second].health.to_bits(),
        game.entities[second].max_health.to_bits()
    );
    assert_eq!(game.players[owner].statistics.units_destroyed, before + 1);
    assert_eq!(game.passengers(carrier), &[first, second]);
}
