use crate::skirmish::{DamageType, DeathType, Kind, Order, tests::game};

fn fixture() -> (crate::skirmish::Skirmish, usize, usize) {
    let mut game = game();
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    game.catalog[5].secondary_weapon = Some(crate::skirmish::WeaponRules {
        name: "PilotSniper".into(),
        damage: 1.0,
        range: 225.0,
        speed: 999_999.0,
        anti_mask: 2,
        damage_type: DamageType::KillPilot,
        auto_choose: 0,
        clip_size: 1,
        reload_seconds: 30.0,
        ..crate::skirmish::WeaponRules::unarmed()
    });
    let vehicle = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[vehicle].veterancy.trainable = true;
    game.catalog[vehicle].veterancy.required = [0, 100, 200, 400];
    game.catalog[vehicle].veterancy.health_bonus = [1.0, 1.2, 1.3, 1.5];
    game.catalog[vehicle]
        .maintenance
        .auto_heal
        .push(crate::skirmish::tests::self_heal(
            10.0,
            2,
            2,
            &["Upgrade_Veterancy_ELITE", "Upgrade_Veterancy_HEROIC"],
        ));
    crate::skirmish::tests::register_update(&mut game.catalog[vehicle], "AutoHealBehavior");
    game.catalog[5].maintenance.weapon_upgrade = Some(crate::skirmish::WeaponUpgrade {
        trigger: "AP".into(),
    });
    crate::skirmish::tests::register_behavior(&mut game.catalog[5], "WeaponBonusUpgrade", 0x80);
    game.catalog[5]
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers
        .push("AP".into());
    let sniper = game.spawn(5, 0, [300.0, 300.0], true);
    let victim = game.spawn(vehicle, 1, [350.0, 300.0], true);
    game.update_visibility();
    (game, sniper, victim)
}

#[test]
fn ordinary_hero_cursor_excludes_pilot_weapon_until_explicit_secondary_order() {
    let (mut game, sniper, victim) = fixture();
    game.catalog[5].kind_of.push("HERO".into());
    game.catalog[5].weapon.damage_type = DamageType::Sniper;
    let vehicle = game.entities[victim].type_index;
    game.catalog[vehicle].armor[DamageType::Sniper as usize] = 0.0;
    assert!(!game.weapon_target_reachable(sniper, victim));
    assert!(game.can_fire_pilot_weapon(sniper, victim, 1));
    game.snipe_vehicle(sniper, victim, 1).unwrap();
    assert!(matches!(game.entities[sniper].ai.order, Order::Attack(id) if id == victim));
    assert!(game.entities[sniper].ai.attack.is_some());
    assert!(game.weapon_target_reachable(sniper, victim));
    game.catalog[vehicle].armor[DamageType::KillPilot as usize] = 0.0;
    assert!(!game.can_fire_pilot_weapon(sniper, victim, 1));
}

#[test]
fn pilot_clip_clock_uses_original_reload_start_and_readiness_flicker_frames() {
    let (mut game, sniper, victim) = fixture();
    game.tick = 10;
    assert!(game.pilot_weapon_ready(sniper, 1));
    game.snipe_vehicle(sniper, victim, 1).unwrap();
    game.fire_weapon(sniper, victim);
    assert!(!game.pilot_weapon_ready(sniper, 1));
    assert_eq!(
        game.pilot_reload_progress(sniper, 1).unwrap().to_bits(),
        0.0_f32.to_bits()
    );
    for _ in 0..450 {
        game.step();
    }
    assert_eq!(
        game.pilot_reload_progress(sniper, 1).unwrap().to_bits(),
        0.5_f32.to_bits()
    );
    for _ in 0..450 {
        game.step();
    }
    assert!(game.pilot_reload_progress(sniper, 1).is_none());
    assert!(!game.pilot_weapon_ready(sniper, 1));
    game.step();
    assert!(!game.pilot_weapon_ready(sniper, 1));
    game.step();
    assert!(game.pilot_weapon_ready(sniper, 1));
}

#[test]
fn first_ap_bonus_transition_restarts_pilot_reload_once_even_without_a_pilot_bonus() {
    let (mut game, sniper, victim) = fixture();
    game.catalog[5].weapon.bonuses.template[5][0] = 1.25;
    game.tick = 10;
    game.snipe_vehicle(sniper, victim, 1).unwrap();
    game.fire_weapon(sniper, victim);
    for _ in 0..100 {
        game.step();
    }
    let old_deadline = game.entities[sniper].weapon_set.weapons[1].possible_next_shot;
    game.players[0].upgrades.push("AP".into());
    game.activate_upgrade_modules(sniper);
    let deadline = game.entities[sniper].weapon_set.weapons[1].possible_next_shot;
    assert_eq!(deadline, game.tick + 900);
    assert!(deadline > old_deadline);
    assert_eq!(
        game.pilot_reload_progress(sniper, 1).unwrap().to_bits(),
        0.0_f32.to_bits()
    );
    for _ in 0..30 {
        game.step();
    }
    game.activate_upgrade_modules(sniper);
    assert_eq!(
        game.entities[sniper].weapon_set.weapons[1].possible_next_shot,
        deadline
    );
}

#[test]
fn original_pilot_damage_neutralizes_without_health_damage_kill_credit_or_healing() {
    let (mut game, sniper, victim) = fixture();
    game.set_veterancy(victim, 3);
    game.entities[victim].health = game.entities[victim].max_health * 0.5;
    game.entities[victim].ai.order = Order::Attack(sniper);
    let lost = game.players[1].statistics.units_lost;
    let kills = game.players[0].statistics.units_destroyed;
    game.snipe_vehicle(sniper, victim, 1).unwrap();
    let position = game.object_position(victim);
    game.deal_bound_weapon_damage(
        sniper,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[sniper].type_index,
            weapon_index: 1,
            weapon_slot: game.entities[sniper].weapon_slot(),
        },
        Some(victim),
        position,
        1.0,
    );
    assert!(game.entities[victim].alive());
    assert!(game.entities[victim].unmanned());
    assert_eq!(game.entities[victim].owner, game.neutral_owner());
    assert_eq!(game.entities[victim].veterancy, 0);
    assert_eq!(game.entities[victim].experience, 0);
    assert_eq!(game.entities[victim].health.to_bits(), 50.0_f32.to_bits());
    assert!(matches!(game.entities[victim].ai.order, Order::Idle));
    assert!(game.move_to(victim, [450.0, 350.0]).is_err());
    assert_eq!(game.players[1].statistics.units_lost, lost);
    assert_eq!(game.players[0].statistics.units_destroyed, kills);
    assert_eq!(game.presentation.pilot_events.len(), 1);
    game.apply_typed_hit(victim, sniper, 1.0, DeathType::Normal);
    game.tick += 30;
    game.run_object_updates();
    assert_eq!(game.entities[victim].health.to_bits(), 49.0_f32.to_bits());
}

#[test]
fn ordinary_infantry_enter_recrews_only_at_its_ignored_host_without_driver_death_credit() {
    let (mut game, sniper, victim) = fixture();
    game.kill_vehicle_pilot(victim, sniper, 1.0);
    game.catalog[5].live_physics =
        crate::skirmish::projectiles::fixtures::shell_template(100, false).live_physics;
    crate::skirmish::tests::register_update(&mut game.catalog[5], "PhysicsBehavior");
    let driver = game.spawn(5, 0, [325.0, 300.0], true);
    game.map_objects.insert("Driver".into(), driver);
    let losses = game.players[0].statistics.units_lost;
    game.object_contact(driver, victim, true);
    assert!(game.entities[victim].unmanned());
    assert!(game.can_enter(driver, victim));
    game.enter(driver, victim).unwrap();
    game.object_contact(driver, victim, true);
    assert!(!game.entities[victim].unmanned());
    assert_eq!(game.entities[victim].owner, 0);
    assert!(game.entities[driver].exists());
    assert!(game.update_objects[game.entities[driver].update_owner].destroy_requested);
    assert_eq!(game.players[0].statistics.units_lost, losses);
    assert_eq!(game.map_objects["Driver"], victim);
    game.process_update_destruction();
    assert!(!game.entities[driver].alive());
    assert!(game.entities[driver].masked());
    assert_eq!(game.players[0].statistics.units_lost, losses);
    assert_eq!(game.map_objects["Driver"], victim);
    assert!(game.move_to(victim, [450.0, 350.0]).is_ok());
}

#[test]
fn snipe_and_recrew_admission_preserve_drone_airborne_and_reject_unmanned_rules() {
    let (mut game, sniper, victim) = fixture();
    assert!(game.can_snipe_vehicle(sniper, victim));
    let kind = game.entities[victim].type_index;
    game.catalog[kind].kind_of.push("DRONE".into());
    assert!(!game.can_snipe_vehicle(sniper, victim));
    game.catalog[kind].kind_of.pop();
    game.catalog[kind].locomotor.airborne_targeting_height = 10.0;
    let position = game.object_position(victim);
    game.set_object_position(victim, [position[0], position[1], position[2] + 100.0]);
    game.update_ground_locomotor(victim);
    assert!(game.entities[victim].is_airborne_target());
    assert!(!game.can_snipe_vehicle(sniper, victim));
    game.set_object_position(victim, position);
    game.update_ground_locomotor(victim);
    assert!(!game.entities[victim].is_airborne_target());
    game.kill_vehicle_pilot(victim, sniper, 1.0);
    assert!(!game.can_snipe_vehicle(sniper, victim));
    assert!(game.can_enter(sniper, victim));
    game.catalog[5].kind_of.push("REJECT_UNMANNED".into());
    assert!(!game.can_enter(sniper, victim));
    let health = game.entities[victim].health.to_bits();
    game.catalog[kind].armor[DamageType::KillPilot as usize] = 0.0;
    game.deal_bound_weapon_damage(
        sniper,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[sniper].type_index,
            weapon_index: 1,
            weapon_slot: game.entities[sniper].weapon_slot(),
        },
        Some(victim),
        game.object_position(victim),
        1.0,
    );
    assert_eq!(game.entities[victim].health.to_bits(), health);
}

#[test]
fn shared_primary_reload_drives_pilot_clock_and_ready_query_refreshes_retained_status() {
    use crate::skirmish::ai_dispatch::WeaponStatus;
    let (mut game, _, victim) = fixture();
    game.catalog[5].weapon.clip_size = 1;
    game.catalog[5].weapon.reload_seconds = 4.0;
    game.catalog[5].weapon_set_rules.share_reload = true;
    let sniper = game.spawn(5, 0, [300.0, 300.0], true);
    game.tick = 10;
    game.fire_weapon(sniper, victim);
    assert!(!game.pilot_weapon_ready(sniper, 1));
    let secondary = &game.entities[sniper].weapon_set.weapons[1];
    assert!(matches!(secondary.status, WeaponStatus::Reloading));
    assert_eq!(secondary.possible_next_shot, 130);
    // Native shared reload copies status/deadline, not another slot's start.
    assert_eq!(secondary.last_reload_started, 0);
    assert_eq!(secondary.clip_remaining, 1);
    assert_eq!(
        game.pilot_reload_progress(sniper, 1).unwrap().to_bits(),
        (10.0_f32 / 130.0).to_bits()
    );
    game.tick = 130;
    assert!(!game.pilot_weapon_ready(sniper, 1));
    assert!(game.pilot_reload_progress(sniper, 1).is_none());
    assert!(matches!(
        game.entities[sniper].weapon_set.weapons[1].status,
        WeaponStatus::Ready
    ));
    game.tick = 131;
    assert!(!game.pilot_weapon_ready(sniper, 1));
    game.tick = 132;
    assert!(game.pilot_weapon_ready(sniper, 1));
}
