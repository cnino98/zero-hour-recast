use super::HealRules;
use crate::skirmish::{Kind, Order, containment::tests::setup};

fn hospital() -> (crate::skirmish::Skirmish, usize) {
    let (mut game, carrier) = setup(0.0, 0.0);
    let index = game.entities[carrier].type_index;
    game.destroy_maintenance_object(carrier);
    game.process_update_destruction();
    game.catalog[index].kind = Kind::Barracks;
    game.catalog[index].kind_of = vec!["STRUCTURE".into(), "FS_BARRACKS".into()];
    let rules = game.catalog[index].contain.as_mut().unwrap();
    rules.heal = Some(HealRules {
        full_heal_frames: 60,
        allow_enemies: false,
        allow_neutral: false,
    });
    rules.capacity = 0;
    rules.exit_frames = 0;
    rules.exit_flags = 0;
    rules.weapons.fire = false;
    for module in &mut game.catalog[index].behavior_modules {
        if module.family == "TransportContain" {
            module.family = "HealContain".into();
        }
    }
    for module in &mut game.catalog[index].update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } = module
            && family == "TransportContain"
        {
            *family = "HealContain".into();
        }
    }
    let carrier = game.spawn(index, 0, [355.0, 255.0], true);
    (game, carrier)
}

#[test]
fn heal_containment_rejects_full_health_manual_entry_but_not_open_contain_admission() {
    let (mut game, carrier) = hospital();
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    assert!(!game.can_enter(rider, carrier));
    assert!(game.enter(rider, carrier).is_err());
    game.entities[rider].health *= 0.5;
    assert!(game.can_enter(rider, carrier));
    game.board(rider, carrier);
    assert!(game.entities[rider].masked());
    assert!(!game.entities[rider].selectable(&game.catalog[game.entities[rider].type_index]));
    assert!(!game.passenger_can_fire(rider));
    let second = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(second, carrier);
    assert_eq!(game.passengers(carrier), &[rider, second]);
}

#[test]
fn heal_containment_uses_maximum_health_slivers_and_elapsed_time_exit() {
    let (mut game, carrier) = hospital();
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.entities[rider].max_health = 120.0;
    game.entities[rider].health = 100.0;
    game.tick = 10;
    game.board(rider, carrier);
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), 102.0_f32.to_bits());
    for frame in 11..70 {
        game.tick = frame;
        game.run_object_updates();
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    }
    assert_eq!(game.entities[rider].health.to_bits(), 120.0_f32.to_bits());
    game.entities[rider].health = 1.0;
    game.tick = 70;
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), 120.0_f32.to_bits());
    assert_eq!(game.entities[rider].contained_by(), None);
    assert!(matches!(game.entities[rider].ai.order, Order::FollowPath));
    assert!(!game.entities[rider].masked());
    assert!(game.entities[carrier].container_door_open(game.tick));
}

#[test]
fn heal_containment_preserves_allied_module_permission_and_manual_faction_restriction() {
    let (mut game, carrier) = hospital();
    game.slots[0].team = Some(1);
    game.slots[1].team = Some(1);
    let ally = game.spawn(5, 1, [355.0, 255.0], true);
    game.entities[ally].health *= 0.5;
    assert!(!game.can_enter(ally, carrier));
    game.board(ally, carrier);
    assert_eq!(game.entities[ally].contained_by(), Some(carrier));
    let enemy = game.spawn(5, 2, [355.0, 255.0], true);
    game.board(enemy, carrier);
    assert!(game.entities[enemy].contained_by().is_none());
    assert_eq!(game.entities[carrier].owner, 0);
}

#[test]
fn zero_duration_heal_and_destroyed_host_do_not_leave_held_riders() {
    let (mut game, carrier) = hospital();
    let index = game.entities[carrier].type_index;
    game.catalog[index]
        .contain
        .as_mut()
        .unwrap()
        .heal
        .as_mut()
        .unwrap()
        .full_heal_frames = 0;
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.entities[rider].health = 1.0;
    game.board(rider, carrier);
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(
        game.entities[rider].health.to_bits(),
        game.entities[rider].max_health.to_bits()
    );
    assert!(game.entities[rider].contained_by().is_none());
    game.entities[rider].health = 1.0;
    game.board(rider, carrier);
    game.apply_hit(carrier, carrier, game.entities[carrier].max_health);
    assert!(game.entities[rider].alive());
    assert!(game.entities[rider].contained_by().is_none());
    game.board(rider, carrier);
    assert!(game.entities[rider].contained_by().is_none());
}

#[test]
fn unfinished_or_dead_entrants_and_disabled_host_follow_container_guards() {
    let (mut game, carrier) = hospital();
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.entities[rider].health *= 0.5;
    game.entities[carrier].build_progress = 0.5;
    assert!(!game.can_enter(rider, carrier));
    game.entities[carrier].build_progress = 1.0;
    game.entities[rider].build_progress = 0.5;
    assert!(!game.can_enter(rider, carrier));
    game.entities[rider].build_progress = 1.0;
    game.board(rider, carrier);
    let health = game.entities[rider].health;
    game.set_hacked_until(carrier, crate::skirmish::ai_dispatch::native_frame(100));
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), health.to_bits());
    game.clear_hacked_disable(carrier);
    game.kill_object(rider, crate::skirmish::DeathType::Normal);
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[rider].alive());
}

#[test]
fn selling_healing_barracks_requests_ordinary_authored_exit() {
    let (mut game, carrier) = hospital();
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.entities[rider].health *= 0.5;
    game.board(rider, carrier);
    game.sell(0, carrier).unwrap();
    assert!(game.entities[carrier].selling());
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    assert!(matches!(
        game.entities[rider].ai.order,
        Order::ExitContainer
    ));
    game.update_container_exit(rider);
    assert!(game.entities[rider].alive());
    assert!(game.entities[rider].contained_by().is_none());
    assert!(matches!(game.entities[rider].ai.order, Order::FollowPath));
}

#[test]
fn healing_callbacks_clear_poison_only_after_health_increases() {
    let (mut game, carrier) = hospital();
    game.catalog[5].behavior.poisoned = vec![crate::skirmish::hazards::PoisonRules {
        interval: 30,
        duration: 90,
    }];
    game.catalog[5].update_modules.push(
        crate::skirmish::module_updates::ModuleRegistration::Behavior {
            family: "PoisonedBehavior".into(),
            tag: "PoisonRegression".into(),
            source: crate::skirmish::module_updates::ModuleSource {
                path: "PoisonedBehavior.cpp".into(),
                origin: "source regression".into(),
                line: 1,
            },
        },
    );

    crate::skirmish::tests::poison_damage_fixture(&mut game, 5);
    let mut field =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Civilian, Kind::Effect);
    field.name = "HospitalPoisonSourceField".into();
    field.kind_of = ["IMMOBILE", "CLEANUP_HAZARD", "INERT", "NO_COLLIDE"]
        .map(str::to_owned)
        .to_vec();
    field.kind_of.push("NO_COLLIDE".into());
    let field_type = game.catalog.len();
    game.catalog.push(field);
    let source = game.spawn(field_type, 0, [355.0, 255.0], true);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(rider, carrier);
    game.poison_damage_received(
        rider,
        0,
        crate::skirmish::hazards::PoisonSource::Object(source),
        1.0,
        crate::skirmish::DeathType::Poisoned,
    );
    game.tick = 1;
    game.run_object_updates();
    assert!(game.entities[rider].poisoned());
    game.entities[rider].health -= 1.0;
    game.tick = 2;
    game.run_object_updates();
    assert!(!game.entities[rider].poisoned());
}

#[test]
fn healing_container_uses_actual_body_healing_coefficient() {
    let (mut game, carrier) = hospital();
    game.catalog[5].armor[crate::skirmish::DamageType::Healing as usize] = 0.5;
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let max = game.entities[rider].max_health;
    game.entities[rider].health = max * 0.5;
    game.board(rider, carrier);
    let before = game.entities[rider].health;
    game.tick = 1;
    game.run_object_updates();
    let sliver = (max / 60.0) * 0.5;
    assert_eq!(
        game.entities[rider].health.to_bits(),
        (before + sliver).to_bits()
    );
    assert_eq!(game.object_disabled_mask(rider) & (1 << 3), 0);
}
