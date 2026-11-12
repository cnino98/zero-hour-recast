use crate::skirmish::{DamageType, Faction, Kind, Skirmish, tests};

fn fixture() -> (Skirmish, usize) {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::Gla, Kind::Factory);
    unit.behavior.production_exit = None;
    unit.behavior_modules
        .retain(|entry| entry.family != "DefaultProductionExitUpdate");
    unit.update_modules.retain(|entry| {
        !matches!(entry,
        crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. }
        if family == "DefaultProductionExitUpdate")
    });
    unit.parking = Some(super::tests::rules());
    super::tests::bind_bones(&mut unit);
    tests::register_update(&mut unit, "ParkingPlaceBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces |= 2;
    let kind = game.catalog.len();
    game.catalog.push(unit);
    (game, kind)
}

#[test]
fn constructor_defers_bones_until_complete_but_heals_through_the_actual_callback() {
    let (mut game, kind) = fixture();
    let producer = game.spawn(kind, 0, [350.0, 250.0], false);
    let target_kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[target_kind].armor[DamageType::Healing as usize] = 0.5;
    let target = game.spawn(target_kind, 0, [400.0, 250.0], true);
    game.change_body_health(target, -10.0);
    let health = game.entities[target].health;
    assert!(!game.entities[producer].parking.as_ref().unwrap().got_info);
    game.parking_set_healee(producer, target, true);
    for frame in 1..=6 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert!(!game.entities[producer].parking.as_ref().unwrap().got_info);
    assert_eq!(
        game.entities[target].health.to_bits(),
        (health + 1.0).to_bits()
    );
    assert_eq!(
        game.entities[target].damage_state.previous_health.to_bits(),
        health.to_bits()
    );
    let basis = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
    game.set_object_transform(producer, [350.0, 250.0, 40.0], basis);
    game.entities[producer].build_progress = 1.0;
    game.tick = 7;
    game.run_object_updates();
    let parking = game.entities[producer].parking.as_ref().unwrap();
    assert!(parking.got_info);
    assert_eq!(
        parking.spaces[0]
            .geometry
            .parking
            .position
            .map(f32::to_bits),
        [350.0_f32, 250.0, 20.0].map(f32::to_bits)
    );
}

#[test]
fn healee_membership_and_purge_reset_deadline_without_a_second_healing_loop() {
    let (mut game, kind) = fixture();
    let producer = game.spawn(kind, 0, [350.0, 250.0], true);
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let first = game.spawn(tank, 0, [400.0, 250.0], true);
    let second = game.spawn(tank, 0, [450.0, 250.0], true);
    game.change_body_health(first, -20.0);
    let before = game.entities[first].health;
    game.parking_set_healee(producer, first, true);
    game.tick = 4;
    game.parking_set_healee(producer, first, true);
    assert_eq!(
        game.entities[producer].parking.as_ref().unwrap().next_heal,
        6
    );
    game.parking_set_healee(producer, second, true);
    assert_eq!(
        game.entities[producer].parking.as_ref().unwrap().next_heal,
        10
    );
    game.tick = 6;
    game.run_object_updates();
    assert_eq!(game.entities[first].health.to_bits(), before.to_bits());
    game.entities[second].set_effectively_dead(true);
    game.tick = 7;
    game.run_object_updates();
    assert_eq!(
        game.entities[producer].parking.as_ref().unwrap().next_heal,
        13
    );
    for frame in 8..=13 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[first].health.to_bits(),
        (before + 2.0).to_bits()
    );
}

#[test]
fn unsigned_healing_deadline_wrap_and_empty_forever_match_native_comparisons() {
    let mut state = super::State::new();
    state.heal(9, true, u64::from(u32::MAX) - 2);
    assert_eq!(state.next_heal, 3);
    // Native >= is deliberately not a wrapping deadline comparison.
    assert_eq!(state.healing(u64::from(u32::MAX) - 1), [9]);
    state.heal(9, false, u64::from(u32::MAX));
    assert_eq!(state.next_heal, 0x3fff_ffff);
    assert!(state.healing(0x3fff_ffff).is_empty());
    assert_eq!(state.next_heal, 0x4000_0005);
}

#[test]
fn authored_parking_on_die_kills_parked_units_and_purges_the_claim() {
    let (mut game, kind) = fixture();
    let producer = game.spawn(kind, 0, [350.0, 250.0], true);
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let target = game.spawn(tank, 0, [400.0, 250.0], true);
    game.reserve_parking_space(producer, target).unwrap();
    game.kill_object(producer, crate::skirmish::DeathType::Normal);
    assert!(game.entities[target].is_effectively_dead());
    assert!(
        game.entities[producer]
            .parking
            .as_ref()
            .unwrap()
            .space_index(target)
            .is_none()
    );
}

#[test]
fn parking_caches_current_condition_once_but_helipad_exit_queries_current_model() {
    let (mut game, kind) = fixture();
    let mut damaged = game.catalog[kind].pristine_draws[0].states[0]
        .transforms
        .clone();
    damaged.get_mut("runway1parking1").unwrap()[0][3] = 80.0;
    damaged.get_mut("helipark01").unwrap()[0][3] = -90.0;
    game.catalog[kind].pristine_draws[0]
        .states
        .push(crate::skirmish::model_bones::State {
            conditions: vec![std::iter::once("DAMAGED".into()).collect()],
            transforms: damaged,
        });
    let producer = game.spawn(kind, 0, [350.0, 250.0], true);
    let maximum = game.entities[producer].max_health;
    game.change_body_health(producer, -maximum * 0.6);
    game.build_parking_info(producer);
    assert_eq!(
        game.entities[producer].parking.as_ref().unwrap().spaces[0]
            .geometry
            .parking
            .position[0]
            .to_bits(),
        430.0_f32.to_bits()
    );
    game.change_body_health(producer, maximum);
    game.build_parking_info(producer);
    assert_eq!(
        game.entities[producer].parking.as_ref().unwrap().spaces[0]
            .geometry
            .parking
            .position[0]
            .to_bits(),
        430.0_f32.to_bits()
    );
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank]
        .kind_of
        .push("PRODUCED_AT_HELIPAD".into());
    game.queue_unit(producer, tank).unwrap();
    let child = game.spawn_parking_unit(producer, tank, false, 0).unwrap();
    assert_eq!(
        game.object_position(child).map(f32::to_bits),
        [310.0_f32, 250.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn absent_current_bone_uses_object_cached_position_and_raw_matrix_orientation() {
    let (mut game, kind) = fixture();
    game.catalog[kind].pristine_draws.clear();
    let producer = game.spawn(kind, 0, [350.0, 250.0], true);
    let basis = crate::skirmish::physical_ocl::transform::yaw(0.75);
    game.set_object_transform(producer, [350.0, 250.0, 25.0], basis);
    game.build_parking_info(producer);
    let state = game.entities[producer].parking.as_ref().unwrap();
    assert_eq!(
        state.spaces[0].geometry.hangar.position.map(f32::to_bits),
        game.object_position(producer).map(f32::to_bits)
    );
    assert_eq!(
        state.spaces[0].geometry.hangar.direction.map(f32::to_bits),
        basis.map(|row| row[0].to_bits())
    );
}
