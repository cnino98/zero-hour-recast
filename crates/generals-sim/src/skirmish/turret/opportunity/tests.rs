use crate::skirmish::{
    DamageType, Kind, Order, Skirmish,
    tests::{fixture_type, game},
    turret::{Phase, Rules},
};

fn setup() -> (Skirmish, usize, usize) {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut rules = Rules::source_defaults();
    rules.controlled_slots = 1;
    rules.turn_rate = std::f32::consts::PI;
    game.catalog[kind].turret = Some(rules);
    game.catalog[kind].weapon.pre_attack_frames = 0;
    game.catalog[kind].ai.mood_check_frames = 10;
    let source = game.spawn(kind, 0, [300.0, 200.0], true);
    let victim = game.spawn(kind, 1, [350.0, 200.0], true);
    game.entities[source].facing = 0.0;
    game.entities[source].ai.turret.phase = Phase::Idle(Some(1000));
    game.entities[source].ai.mood.next_check = 0;
    game.entities[source].ai.mood.offset_check = false;
    game.set_order(source, Order::Move([450.0, 200.0]));
    game.update_visibility();
    (game, source, victim)
}

fn update(game: &mut Skirmish, source: usize) {
    game.tick += 1;
    game.update_weapon_clocks(source);
    game.update_idle_turret(source);
}

#[test]
fn moving_turret_acquires_aims_then_fires_without_replacing_its_order_or_path() {
    let (mut game, source, target) = setup();
    let goal = [450.0, 200.0];
    for _ in 0..2 {
        update(&mut game, source);
        assert!(game.presentation.shots.is_empty());
    }
    assert!(matches!(game.entities[source].ai.turret.phase, Phase::Fire));
    update(&mut game, source);
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(game.presentation.shots[0].target, Some(target));
    assert!(
        matches!(game.entities[source].ai.order, Order::Move(point) if point.map(f32::to_bits) == goal.map(f32::to_bits))
    );
    assert!(game.entities[source].ai.ai_target.is_none());
    assert_eq!(
        game.entities[source].position.map(f32::to_bits),
        [300.0_f32, 200.0].map(f32::to_bits)
    );
    // The source post-shot current-victim copy clears an opportunity on Move.
    assert!(game.entities[source].ai.turret.opportunity.is_none());
    update(&mut game, source);
    assert!(matches!(
        game.entities[source].ai.turret.phase,
        Phase::Hold(_)
    ));
    while game.tick < 10 {
        update(&mut game, source);
        assert!(game.entities[source].ai.turret.opportunity.is_none());
    }
    update(&mut game, source);
    assert_eq!(
        game.entities[source].ai.turret.opportunity,
        Some((target, 1))
    );
}

#[test]
fn slaved_move_refresh_does_not_cancel_turret_pre_attack() {
    let (mut game, source, target) = setup();
    let kind = game.entities[source].type_index;
    game.catalog[kind].weapon.pre_attack_frames = 9;
    update(&mut game, source);
    update(&mut game, source);
    let start = game.tick;
    game.update_weapon_status(source);
    assert!(!game.entities[source].attacking());
    assert!(game.entities[source].weapon_condition().is_none());
    assert_eq!(
        game.entities[source].weapon_set.weapons[0]
            .pre_attack
            .as_ref()
            .unwrap()
            .frames,
        9
    );
    for _ in 0..8 {
        game.set_order(source, Order::Move([460.0, 210.0]));
        update(&mut game, source);
        assert!(game.presentation.shots.is_empty());
        assert_eq!(
            game.entities[source].weapon_set.weapons[0]
                .pre_attack
                .as_ref()
                .unwrap()
                .start,
            start
        );
    }
    update(&mut game, source);
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(game.presentation.shots[0].target, Some(target));
    assert!(matches!(game.entities[source].ai.order, Order::Move(_)));
}

#[test]
fn manual_attack_replaces_opportunity_and_only_fires_once_per_frame() {
    let (mut game, source, _) = setup();
    update(&mut game, source);
    let kind = game.entities[source].type_index;
    let target = game.spawn(kind, 1, [360.0, 200.0], true);
    game.attack(source, target).unwrap();
    game.tick += 1;
    game.update_weapon_clocks(source);
    game.update_ai_module(source);
    assert!(game.presentation.shots.is_empty());
    game.tick += 1;
    game.update_ai_module(source);
    let shots = game.presentation.shots.len();
    assert_eq!(shots, 1);
    game.update_idle_turret(source);
    assert_eq!(game.presentation.shots.len(), shots);
    assert!(game.entities[source].ai.turret.opportunity.is_none());
    assert_eq!(game.presentation.shots[0].target, Some(target));
}

#[test]
fn opportunity_aim_rejects_changed_owner_dead_or_out_of_controlled_range_targets() {
    for change in 0..3 {
        let (mut game, source, target) = setup();
        update(&mut game, source);
        match change {
            0 => game.transfer_maintenance_owner(target, 0),
            1 => game.kill_object(target, crate::skirmish::DeathType::Normal),
            _ => game.entities[target].position = [650.0, 200.0],
        }
        update(&mut game, source);
        assert!(game.presentation.shots.is_empty());
        assert!(game.entities[source].ai.turret.opportunity.is_none());
        assert!(matches!(
            game.entities[source].ai.turret.phase,
            Phase::Hold(_)
        ));
        assert!(matches!(game.entities[source].ai.order, Order::Move(_)));
    }
}

#[test]
fn disabled_turret_and_unarmed_buildings_do_not_acquire_opportunities() {
    let (mut game, source, target) = setup();
    game.entities[source].ai.turret.enabled = false;
    update(&mut game, source);
    assert!(game.entities[source].ai.turret.opportunity.is_none());
    game.entities[source].ai.turret.enabled = true;
    game.entities[target].type_index = game.type_for(1, Kind::CommandCenter).unwrap();
    update(&mut game, source);
    assert!(game.entities[source].ai.turret.opportunity.is_none());
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn controlled_slot_admission_precedes_selection_and_fire_rechecks_slot() {
    let (mut game, source, _) = setup();
    let kind = game.entities[source].type_index;
    game.catalog[kind].secondary_weapon =
        Some(fixture_type(game.catalog[kind].faction, Kind::Tank).weapon);
    game.entities[source].weapon_set.current = 1;
    update(&mut game, source);
    assert!(game.entities[source].ai.turret.opportunity.is_none());
    game.entities[source].weapon_set.current = 0;
    while game.tick < 11 {
        update(&mut game, source);
        assert!(game.presentation.shots.is_empty());
    }
    update(&mut game, source);
    assert!(matches!(game.entities[source].ai.turret.phase, Phase::Fire));
    game.entities[source].weapon_set.current = 1;
    update(&mut game, source);
    assert!(game.presentation.shots.is_empty());
    assert!(matches!(game.entities[source].ai.turret.phase, Phase::Aim));
}

#[test]
fn status_weapon_opportunity_paints_without_damaging_target_health() {
    let (mut game, source, target) = setup();
    let kind = game.entities[source].type_index;
    game.catalog[kind].weapon.damage_type = DamageType::Status;
    game.catalog[kind].weapon.damage = 200.0;
    game.catalog[kind].weapon.laser = true;
    let before = game.entities[target].health;
    for _ in 0..3 {
        update(&mut game, source);
    }
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(game.presentation.shots[0].target, Some(target));
    assert_eq!(game.entities[target].health.to_bits(), before.to_bits());
    assert!(game.entities[target].painted());
}

#[test]
fn opportunity_requires_both_vision_and_weapon_range_without_chasing() {
    let (mut game, source, target) = setup();
    let kind = game.entities[source].type_index;
    game.catalog[kind].ai.vision_range = 39.0;
    game.catalog[kind].weapon.range = 200.0;
    update(&mut game, source);
    assert!(
        game.entities[source].ai.turret.opportunity.is_none(),
        "target={:?}, separation={}, vision={}, factor={}",
        game.entities[source].ai.turret.opportunity,
        game.range_distance(source, target),
        game.entities[source].vision_range(&game.catalog[kind]),
        game.ai_rules.guard_outer_human
    );
    game.catalog[kind].ai.vision_range = 200.0;
    game.catalog[kind].weapon.range = 10.0;
    game.entities[source].ai.mood.next_check = 0;
    update(&mut game, source);
    assert!(game.entities[source].ai.turret.opportunity.is_none());
    assert!(game.presentation.shots.is_empty());
    assert!(matches!(game.entities[source].ai.order, Order::Move(_)));
    assert_eq!(
        game.entities[target].position.map(f32::to_bits),
        [350.0_f32, 200.0].map(f32::to_bits)
    );
}

#[test]
fn leaving_range_during_pre_attack_cancels_fire_before_next_aim_releases_target() {
    let (mut game, source, target) = setup();
    let kind = game.entities[source].type_index;
    game.catalog[kind].weapon.pre_attack_frames = 8;
    update(&mut game, source);
    update(&mut game, source);
    game.entities[target].position = [650.0, 200.0];
    update(&mut game, source);
    assert!(matches!(game.entities[source].ai.turret.phase, Phase::Aim));
    assert!(
        game.entities[source].weapon_set.weapons[0]
            .pre_attack
            .is_none()
    );
    update(&mut game, source);
    assert!(game.entities[source].ai.turret.opportunity.is_none());
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn fire_poll_cannot_start_pre_attack_that_on_enter_rejected_during_cooldown() {
    let (mut game, source, _) = setup();
    let kind = game.entities[source].type_index;
    game.catalog[kind].weapon.pre_attack_frames = 8;
    update(&mut game, source);
    game.entities[source].weapon_set.weapons[0].possible_next_shot = game.tick + 3;
    game.entities[source].weapon_set.weapons[0].status =
        crate::skirmish::ai_dispatch::WeaponStatus::BetweenShots;
    update(&mut game, source);
    assert!(matches!(game.entities[source].ai.turret.phase, Phase::Fire));
    assert!(
        game.entities[source].weapon_set.weapons[0]
            .pre_attack
            .is_none()
    );
    update(&mut game, source);
    assert!(matches!(game.entities[source].ai.turret.phase, Phase::Aim));
    assert!(
        game.entities[source].weapon_set.weapons[0]
            .pre_attack
            .is_none()
    );
    update(&mut game, source);
    assert!(matches!(game.entities[source].ai.turret.phase, Phase::Fire));
    assert_eq!(
        game.entities[source].weapon_set.weapons[0]
            .pre_attack
            .as_ref()
            .unwrap()
            .start,
        game.tick
    );
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn independent_shot_snapshots_pose_before_same_frame_flight_turn() {
    use crate::skirmish::flight::{Appearance, Motion, tests::rules};
    let (mut game, source, target) = setup();
    update(&mut game, source);
    update(&mut game, source);
    let kind = game.entities[source].type_index;
    game.catalog[kind].locomotor.flight = Some(rules(Appearance::Hover));
    game.catalog[kind].locomotor.legal_surfaces = 8;
    game.entities[source].flight = Some(Box::new(Motion::new(100.0)));
    game.move_to(source, [300.0, 400.0]).unwrap();
    game.step();
    let shot = game
        .presentation
        .shots
        .iter()
        .find(|shot| shot.source == source)
        .unwrap();
    assert_eq!(shot.target, Some(target));
    assert_eq!(shot.source_facing.to_bits(), 0.0_f32.to_bits());
    assert!(game.entities[source].facing > 0.0);
    assert!(matches!(game.entities[source].ai.order, Order::Move(_)));
}
