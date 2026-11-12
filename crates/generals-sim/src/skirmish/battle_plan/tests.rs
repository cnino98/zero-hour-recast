use super::super::{Faction, Kind, Order, Skirmish, tests, turret};
use super::{Plan, Rules, Status};

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    let mut center = tests::fixture_type(Faction::Gla, Kind::TechBuilding);
    center.name = "StrategyCenter".into();
    center.kind_of.push("POWERED".into());
    center.battle_plan = Some(Rules {
        animation_frames: [3, 4, 5],
        transition_idle_frames: 2,
        paralyze_frames: 7,
        valid_kind_of: vec!["VEHICLE".into(), "INFANTRY".into()],
        invalid_kind_of: vec!["DOZER".into(), "HARVESTER".into()],
        armor_scalar: 0.8,
        sight_scalar: 1.2,
        center_sight_scalar: 2.0,
        center_health_scalar: 2.0,
        center_detects_stealth: true,
    });
    center.turret = Some(turret::Rules {
        controlled_slots: 1,
        turn_rate: 0.2,
        ..turret::Rules::source_defaults()
    });
    let kind = game.catalog.len();
    tests::register_update(&mut center, "BattlePlanUpdate");
    game.catalog.push(center);
    game.catalog[6].weapon.bonuses.global[12][0] = 1.2;
    game.catalog[6].weapon.bonuses.global[14][2] = 1.2;
    let center = game.spawn(kind, 0, [300.0, 200.0], true);
    let tank = game.spawn(6, 0, [300.0, 300.0], true);
    (game, center, tank)
}

fn update(game: &mut Skirmish, frame: u64) {
    game.tick = frame;
    game.run_object_updates();
}

fn activate(game: &mut Skirmish, center: usize, plan: Plan) {
    assert!(game.choose_battle_plan(center, plan));
    // Run the next native update before reading the transition it starts.
    update(game, game.tick + 1);
    let deadline = game.entities[center]
        .battle_plan
        .as_ref()
        .unwrap()
        .next_ready;
    update(game, deadline);
    assert!(game.entities[center].battle_plan() == Some(plan));
}

#[test]
fn plan_flag_edges_restart_active_reload_without_restarting_ready_or_unchanged_weapons() {
    let (mut game, center, tank) = fixture();
    let index = game.entities[tank].type_index;
    game.catalog[index].weapon.delay = 2.0;
    game.catalog[index].weapon.reload_seconds = 3.0;
    // Author the templates before constructing the native weapon instances.
    let tank = game.spawn(index, 0, [300.0, 350.0], true);
    let loading = game.spawn(index, 0, [350.0, 300.0], true);
    let ready = game.spawn(index, 0, [400.0, 300.0], true);
    // getStatus resolves the absolute deadline; cooldown alone is a view cache.
    game.entities[tank].weapon_set.weapons[0].status =
        crate::skirmish::ai_dispatch::WeaponStatus::BetweenShots;
    game.entities[tank].weapon_set.weapons[0].possible_next_shot = 5;
    game.entities[loading].weapon_set.weapons[0].status =
        crate::skirmish::ai_dispatch::WeaponStatus::Reloading;
    game.entities[loading].weapon_set.weapons[0].possible_next_shot = 7;
    activate(&mut game, center, Plan::Bombardment);
    assert_eq!(
        game.entities[tank].weapon_set.weapons[0].cooldown.to_bits(),
        60.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[loading].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        90.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[ready].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        0.0_f32.to_bits()
    );
    game.entities[tank].weapon_set.weapons[0].cooldown = 4.0;
    game.entities[loading].weapon_set.weapons[0].cooldown = 6.0;
    game.initialize_battle_bonuses(tank);
    game.run_object_updates();
    assert_eq!(
        game.entities[tank].weapon_set.weapons[0].cooldown.to_bits(),
        4.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[loading].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        6.0_f32.to_bits()
    );
    assert!(game.choose_battle_plan(center, Plan::SearchAndDestroy));
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(
        game.entities[tank].weapon_set.weapons[0].cooldown.to_bits(),
        60.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[loading].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        90.0_f32.to_bits()
    );
}

#[test]
fn changes_finish_unpack_pack_and_idle_before_applying_the_next_plan() {
    let (mut game, center, tank) = fixture();
    assert!(game.choose_battle_plan(center, Plan::HoldTheLine));
    update(&mut game, 10);
    assert!(game.entities[center].battle_plan().is_none());
    assert!(game.choose_battle_plan(center, Plan::SearchAndDestroy));
    update(&mut game, 13);
    assert_eq!(
        (game.entities[center].max_health).to_bits(),
        1000.0_f32.to_bits()
    );
    update(&mut game, 14);
    assert!(game.entities[center].battle_plan() == Some(Plan::HoldTheLine));
    assert_eq!(
        (game.entities[tank].battle_bonuses.armor).to_bits(),
        0.8_f32.to_bits()
    );
    update(&mut game, 15);
    assert_eq!(
        (game.entities[tank].battle_bonuses.armor).to_bits(),
        1.0_f32.to_bits()
    );
    assert!(game.entities[tank].paralyzed());
    assert_eq!(game.entities[tank].battle_bonuses.paralyzed_until, 22);
    assert_eq!(
        (game.entities[center].max_health).to_bits(),
        1000.0_f32.to_bits()
    );
    update(&mut game, 19);
    assert!(game.entities[center].battle_plan.as_ref().unwrap().status == Status::Idle);
    update(&mut game, 20);
    assert!(game.entities[center].battle_plan_condition().is_none());
    update(&mut game, 21);
    assert_eq!(
        game.entities[center].battle_plan_condition(),
        Some("DOOR_3_OPENING")
    );
    update(&mut game, 26);
    assert!(game.entities[center].battle_plan() == Some(Plan::SearchAndDestroy));
    assert_eq!(
        (game.entities[tank].weapon_range(&game.catalog[6], 0)).to_bits(),
        180.0_f32.to_bits()
    );
    assert_eq!(
        (game.entities[tank].vision_range(&game.catalog[6])).to_bits(),
        180.0_f32.to_bits()
    );
    assert_eq!(
        (game.entities[center].shroud_range(&game.catalog[game.entities[center].type_index]))
            .to_bits(),
        300.0_f32.to_bits()
    );
}

#[test]
fn simultaneous_centers_stack_distinct_plans_and_remove_only_last_matching_provider() {
    let (mut game, first, tank) = fixture();
    let kind = game.entities[first].type_index;
    let second = game.spawn(kind, 0, [350.0, 200.0], true);
    let third = game.spawn(kind, 0, [400.0, 200.0], true);
    activate(&mut game, first, Plan::Bombardment);
    activate(&mut game, second, Plan::HoldTheLine);
    activate(&mut game, third, Plan::HoldTheLine);
    assert_eq!(
        (game.entities[tank]
            .weapon_damage_bonus(game.entities[tank].active_weapon(&game.catalog[6]), 0))
        .to_bits(),
        1.2_f32.to_bits()
    );
    assert_eq!(
        (game.entities[tank].adjust_damage(
            &game.catalog[6],
            crate::skirmish::DamageType::SmallArms,
            1.0
        ))
        .to_bits(),
        0.8_f32.to_bits()
    );
    game.delete_battle_plan(second);
    assert_eq!(
        (game.entities[tank].adjust_damage(
            &game.catalog[6],
            crate::skirmish::DamageType::SmallArms,
            1.0
        ))
        .to_bits(),
        0.8_f32.to_bits()
    );
    game.delete_battle_plan(third);
    assert_eq!(
        (game.entities[tank].adjust_damage(
            &game.catalog[6],
            crate::skirmish::DamageType::SmallArms,
            1.0
        ))
        .to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        (game.entities[tank]
            .weapon_damage_bonus(game.entities[tank].active_weapon(&game.catalog[6]), 0))
        .to_bits(),
        1.2_f32.to_bits()
    );
    let future = game.spawn(6, 0, [450.0, 400.0], true);
    assert_eq!(
        (game.entities[future]
            .weapon_damage_bonus(game.entities[future].active_weapon(&game.catalog[6]), 0))
        .to_bits(),
        1.2_f32.to_bits()
    );
    game.delete_battle_plan(first);
    assert_eq!(
        (game.entities[future]
            .weapon_damage_bonus(game.entities[future].active_weapon(&game.catalog[6]), 0))
        .to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn hold_the_line_preserves_damage_ratio_and_capture_transfers_army_bonuses() {
    let (mut game, center, tank) = fixture();
    let enemy = game.spawn(6, 1, [450.0, 400.0], true);
    game.entities[center].health = 400.0;
    activate(&mut game, center, Plan::HoldTheLine);
    assert_eq!(
        (game.entities[center].max_health).to_bits(),
        2000.0_f32.to_bits()
    );
    assert_eq!(
        (game.entities[center].health).to_bits(),
        800.0_f32.to_bits()
    );
    game.transfer_maintenance_owner(center, 1);
    assert_eq!(
        (game.entities[center].health).to_bits(),
        800.0_f32.to_bits()
    );
    assert_eq!(
        (game.entities[tank].adjust_damage(
            &game.catalog[6],
            crate::skirmish::DamageType::SmallArms,
            1.0
        ))
        .to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        (game.entities[enemy].adjust_damage(
            &game.catalog[6],
            crate::skirmish::DamageType::SmallArms,
            1.0
        ))
        .to_bits(),
        0.8_f32.to_bits()
    );
    game.apply_hit(center, tank, 900.0);
    assert_eq!(
        (game.entities[enemy].adjust_damage(
            &game.catalog[6],
            crate::skirmish::DamageType::SmallArms,
            1.0
        ))
        .to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn disabled_updates_suspend_transitions_without_removing_an_active_bonus() {
    let (mut game, center, tank) = fixture();
    activate(&mut game, center, Plan::HoldTheLine);
    game.players[0].energy.sabotaged_until = 99;
    game.sabotage_power(0);
    assert!(!game.choose_battle_plan(center, Plan::SearchAndDestroy));
    update(&mut game, 100);
    assert_eq!(
        (game.entities[tank].adjust_damage(
            &game.catalog[6],
            crate::skirmish::DamageType::SmallArms,
            1.0
        ))
        .to_bits(),
        0.8_f32.to_bits()
    );
    game.update_power();
    assert!(game.choose_battle_plan(center, Plan::SearchAndDestroy));
    game.set_hacked_until(center, crate::skirmish::ai_dispatch::native_frame(200));
    update(&mut game, 101);
    assert!(game.entities[center].battle_plan() == Some(Plan::HoldTheLine));
    game.clear_hacked_disable(center);
    update(&mut game, 102);
    assert!(game.entities[center].battle_plan().is_none());
}

#[test]
fn bombardment_recenters_while_disabled_and_cannot_be_retargeted_during_packing() {
    let (mut game, center, _) = fixture();
    activate(&mut game, center, Plan::Bombardment);
    game.entities[center].ai.turret.angle = 0.3;
    let enemy = game.spawn(6, 1, [350.0, 250.0], true);
    game.entities[center].ai.order = Order::Attack(enemy);
    assert!(game.choose_battle_plan(center, Plan::HoldTheLine));
    update(&mut game, 10);
    assert!(!game.entities[center].ai.turret.enabled);
    assert!(game.entities[center].battle_plan() == Some(Plan::Bombardment));
    assert!(game.presentation.shots.is_empty());
    for frame in 11..16 {
        game.tick = frame;
        game.update_idle_turret(center);
        game.run_object_updates();
    }
    assert_eq!(
        (game.entities[center].ai.turret.angle).to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[center].battle_plan_condition(),
        Some("DOOR_1_CLOSING")
    );
}

#[test]
fn paralysis_excludes_authored_invalid_kinds_and_does_not_affect_newly_created_units() {
    let (mut game, center, tank) = fixture();
    game.catalog[1].kind_of.push("DOZER".into());
    let dozer = game.spawn(1, 0, [500.0, 200.0], true);
    activate(&mut game, center, Plan::SearchAndDestroy);
    assert!(game.choose_battle_plan(center, Plan::HoldTheLine));
    update(&mut game, 20);
    assert!(game.entities[tank].paralyzed());
    assert!(!game.entities[dozer].paralyzed());
    let future = game.spawn(6, 0, [500.0, 300.0], true);
    assert!(!game.entities[future].paralyzed());
    update(&mut game, 27);
    assert!(game.entities[tank].paralyzed());
    game.expire_paralysis();
    assert!(!game.entities[tank].paralyzed());
}

#[test]
fn projectile_detonation_uses_its_own_membership_and_retains_launch_flags() {
    use crate::skirmish::projectiles::fixtures::{launch_shell, shell_template};
    let (mut game, center, tank) = fixture();
    game.set_projectile_terrain(32, 32, [0.0; 2], vec![0.0; 1024], 40.0)
        .unwrap();
    let target = game.spawn(5, 1, [600.0, 300.0], true);
    let kind = game.catalog.len();
    game.catalog.push(shell_template(100, false));
    let (before, _) = launch_shell(&mut game, tank, kind, target);
    game.set_army_battle_plan(center, Some(Plan::Bombardment));
    let index = game.projectile_index(before).unwrap();
    // PROJECTILE is absent from this plan's actual valid KindOf list.
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[index])
            .to_bits(),
        1.0_f32.to_bits()
    );
    let (during, _) = launch_shell(&mut game, tank, kind, target);
    game.delete_battle_plan(center);
    let index = game.projectile_index(during).unwrap();
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[index])
            .to_bits(),
        1.2_f32.to_bits()
    );
}

#[test]
fn projectile_current_conditions_join_launch_conditions_once() {
    use crate::skirmish::projectiles::fixtures::{launch_shell, shell_template};
    let (mut game, center, tank) = fixture();
    let center_kind = game.entities[center].type_index;
    game.catalog[center_kind]
        .battle_plan
        .as_mut()
        .unwrap()
        .valid_kind_of
        .push("PROJECTILE".into());
    game.set_projectile_terrain(32, 32, [0.0; 2], vec![0.0; 1024], 40.0)
        .unwrap();
    let target = game.spawn(5, 1, [600.0, 300.0], true);
    let kind = game.catalog.len();
    game.catalog.push(shell_template(100, false));
    let (before, _) = launch_shell(&mut game, tank, kind, target);
    game.set_army_battle_plan(center, Some(Plan::Bombardment));
    let index = game.projectile_index(before).unwrap();
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[index])
            .to_bits(),
        1.2_f32.to_bits()
    );
    let (during, _) = launch_shell(&mut game, tank, kind, target);
    let index = game.projectile_index(during).unwrap();
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[index])
            .to_bits(),
        1.2_f32.to_bits()
    );
    game.delete_battle_plan(center);
    let index = game.projectile_index(before).unwrap();
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[index])
            .to_bits(),
        1.0_f32.to_bits()
    );
    let index = game.projectile_index(during).unwrap();
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[index])
            .to_bits(),
        1.2_f32.to_bits()
    );
}

#[test]
fn direct_delayed_damage_keeps_the_bonus_from_firing() {
    use crate::skirmish::{WeaponBinding, damage::WeaponTarget};
    let (mut game, center, tank) = fixture();
    let target = game.spawn(5, 1, [600.0, 300.0], true);
    game.catalog[6].weapon.motion = None;
    game.catalog[6].weapon.speed = 300.0;
    game.set_army_battle_plan(center, Some(Plan::Bombardment));
    let position = game.target_center(target);
    let target = WeaponTarget {
        object: Some(target),
        position,
        projectile_position: position,
        scatter_radius: 0.0,
    };
    game.fire_owned_weapon_payload(
        tank,
        WeaponBinding {
            type_index: 6,
            weapon_index: 0,
            weapon_slot: 0,
        },
        0,
        target,
        true,
    );
    let serial = game.projectile_serial;
    game.delete_battle_plan(center);
    let projectile = &game.projectiles[game.projectile_index(serial).unwrap()];
    assert!(projectile.object.is_none());
    assert!(projectile.direct.is_some());
    assert_eq!(projectile.damage_bonus.to_bits(), 1.2_f32.to_bits());
}

#[test]
fn paralysis_suspends_ai_but_does_not_stop_the_absolute_weapon_reload_clock() {
    let (mut game, center, tank) = fixture();
    activate(&mut game, center, Plan::SearchAndDestroy);
    assert!(game.choose_battle_plan(center, Plan::HoldTheLine));
    update(&mut game, 20);
    game.entities[tank].weapon_set.weapons[0].status =
        crate::skirmish::ai_dispatch::WeaponStatus::BetweenShots;
    game.entities[tank].weapon_set.weapons[0].possible_next_shot = game.tick + 4;
    game.entities[tank].ai.order = Order::Move([400.0, 300.0]);
    let start = game.entities[tank].position;
    for _ in 0..4 {
        game.step();
    }
    assert!(game.entities[tank].paralyzed());
    assert_eq!(
        game.entities[tank].position.map(f32::to_bits),
        start.map(f32::to_bits)
    );
    // AI paralysis does not query the weapon, so its optional presentation
    // cache need not refresh. Native getStatus still observes the deadline.
    assert!(matches!(
        game.entities[tank].weapon_status(
            0,
            &game.catalog[6],
            crate::skirmish::ai_dispatch::native_frame(game.tick)
        ),
        Some(crate::skirmish::ai_dispatch::WeaponStatus::Ready)
    ));
}

fn next_bombardment_target(game: &mut Skirmish, center: usize) -> Option<usize> {
    for _ in 0..120 {
        game.step();
        if let Some(fired) = game
            .presentation
            .shots
            .iter()
            .find(|shot| shot.source == center)
        {
            return fired.target;
        }
    }
    None
}

#[test]
fn bombardment_independent_idle_turret_fires_none_gun_while_body_attack_rejects_it() {
    let (mut game, center, _) = fixture();
    let kind = game.entities[center].type_index;
    // Use three actual configured hostile player slots so a target capture
    // changes its Team while preserving hostility toward the firing player.
    game.layout.starts.push(crate::skirmish::PlayerStart {
        position: [685.0, 505.0],
        rally: None,
    });
    let mut slots = game.slots.clone();
    slots.push(crate::skirmish::setup::PlayerSlot {
        faction: Some(Faction::Gla),
        occupied: true,
        difficulty: None,
        team: None,
        start: Some(2),
        color: Some(2),
    });
    game.reset_slots(slots).unwrap();
    game.catalog[kind].weapon.damage = 60.0;
    game.catalog[kind].weapon.range = 150.0;
    game.catalog[kind].weapon.speed = 600.0;
    game.catalog[kind].weapon.delay = 2.0;
    game.catalog[kind].weapon.auto_choose = 0;
    let center = game.spawn(kind, 0, [300.0, 200.0], true);
    let enemy = game.spawn(6, 1, [400.0, 200.0], true);
    game.step();
    assert!(
        game.presentation
            .shots
            .iter()
            .all(|shot| shot.source != center)
    );
    activate(&mut game, center, Plan::Bombardment);
    // Keep the actual CREATE PRIMARY lock while exercising automatic firing.
    assert!(matches!(
        game.entities[center].weapon_set.lock,
        crate::skirmish::weapons::Lock::Temporary
    ));
    assert_eq!(next_bombardment_target(&mut game, center), Some(enemy));
    assert!(game.enemies(0, 2));
    game.tick += 61;
    game.transfer_maintenance_owner(enemy, 2);
    // Body Aim reassigns its turret target every callback. A changed Team
    // remains a valid attack victim while the relationship is still ENEMIES.
    assert_eq!(next_bombardment_target(&mut game, center), Some(enemy));
    // A friendly transfer instead invalidates both the body attack and turret.
    game.transfer_maintenance_owner(enemy, 0);
    for _ in 0..3 {
        game.step();
        assert!(
            game.presentation
                .shots
                .iter()
                .all(|shot| shot.source != center)
        );
    }
    assert!(game.choose_battle_plan(center, Plan::HoldTheLine));
    for _ in 0..8 {
        game.step();
        assert!(
            game.presentation
                .shots
                .iter()
                .all(|shot| shot.source != center)
        );
    }
    assert!(!game.entities[center].ai.turret.enabled);
    // Ordinary Stop releases the creation lock. NONE is then unselectable by
    // generic body Attack; this is separate from the locked firing case above.
    game.set_order(center, Order::Idle);
    assert!(!game.choose_weapon(center, enemy));
    assert!(
        !game.enter_native_attack(center, crate::skirmish::combat::attack::Goal::Object(enemy))
    );
}

#[test]
fn battle_plan_create_sets_actual_weapon_condition_lock_and_initial_turret_disable() {
    let (mut game, center, _) = fixture();
    let actor = &game.entities[center];
    assert!(actor.weapon_set.forced_veteran);
    assert!(matches!(
        actor.weapon_set.lock,
        crate::skirmish::weapons::Lock::Temporary
    ));
    assert_eq!(actor.weapon_slot(), 0);
    assert!(!actor.ai.turret.enabled);
    game.set_order(center, Order::Idle);
    assert!(matches!(
        game.entities[center].weapon_set.lock,
        crate::skirmish::weapons::Lock::None
    ));
}
