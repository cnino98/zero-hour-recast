use super::*;
use crate::skirmish::{ClipReload, DeathType, Kind, Order, flight, tests::game};

fn aircraft() -> (crate::skirmish::Skirmish, usize) {
    let mut game = game();
    let index = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[index];
    let mut normal =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Tank).locomotor;
    normal.ground = None;
    normal.flight = Some(flight::tests::rules(flight::Appearance::Wings));
    normal.legal_surfaces = 8;
    normal.airborne_targeting_height = 10.0;
    let mut taxi = std::mem::replace(&mut unit.locomotor, normal);
    taxi.ground = Some(crate::skirmish::locomotor::GroundRules {
        wander: None,
        is_dozer: false,
        downhill_only: false,
        appearance: crate::skirmish::locomotor::GroundAppearance::Treads,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 999_999.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.1,
        lateral_friction: 0.2,
        extra_friction: 0.0,
    });
    unit.aircraft = Some(Rules {
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
        takeoff_pause: 15,
        idle_return: 300,
        out_of_ammo_damage: 0.1,
        min_height: 5.0,
        parking_offset: 3.0,
        attack_locomotor_frames: 0,
        attackers_miss_frames: 0,
        sneaky_offset: 0.0,
        lockon: super::lockon::Rules::native(),
        sounds: super::Sounds {
            afterburner: String::new(),
            low_fuel: String::new(),
            wheel_screech: String::new(),
        },
    });
    for entry in &mut unit.update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } = entry
            && family == "AIUpdateInterface"
        {
            *family = "JetAIUpdate".into();
        }
    }
    for entry in &mut unit.behavior_modules {
        if entry.family == "AIUpdateInterface" {
            entry.family = "JetAIUpdate".into();
        }
    }
    unit.kind_of.extend(["AIRCRAFT".into(), "VEHICLE".into()]);
    crate::skirmish::aircraft_death::tests::install_physics(unit);
    unit.weapon.clip_size = 4;
    unit.weapon.reload_seconds = 4.0;
    unit.weapon.reload = ClipReload::ReturnToBase;
    let id = game.spawn(index, 0, [200.0; 2], true);
    game.initialize_aircraft_producer(id);
    (game, id)
}

#[test]
fn native_taxi_locomotor_rejects_rubble_before_and_after_an_airborne_transition() {
    let (mut game, aircraft) = aircraft();
    game.choose_aircraft_locomotor(aircraft, false);
    let mut structure = crate::skirmish::tests::fixture_type(
        crate::skirmish::Faction::Civilian,
        Kind::TechBuilding,
    );
    structure.maintenance.rubble_height = Some(5.0);
    let index = game.catalog.len();
    game.catalog.push(structure);
    let building = game.spawn(index, game.neutral_owner(), [355.0, 205.0], true);
    game.rebuild_navigation();
    game.apply_hit(building, aircraft, 2000.0);
    assert!(game.retained_rubble(building));
    let center = game.cell_at([355.0, 205.0]).unwrap();
    // JetAIUpdate chooses TAXIING at creation and on runway touchdown;
    // Pathfinder uses the current locomotor's surfaces for the query.
    assert!(game.navigation_for(aircraft).is_blocked(center));
    let path_clear = |game: &crate::skirmish::Skirmish| {
        let goal = [405.0, 205.0];
        game.ground_line_passable_on(
            aircraft,
            [305.0, 205.0],
            goal,
            game.movement_goal_layer(aircraft, goal),
        )
    };
    assert!(!path_clear(&game));
    game.choose_aircraft_locomotor(aircraft, true);
    assert!(!game.navigation_for(aircraft).is_blocked(center));
    assert!(path_clear(&game));
    game.choose_aircraft_locomotor(aircraft, false);
    assert!(game.navigation_for(aircraft).is_blocked(center));
    assert!(!path_clear(&game));
}

#[test]
fn partial_rearm_uses_needed_ammunition_time_and_never_removes_existing_rounds() {
    let (mut game, id) = aircraft();
    let index = game.entities[id].type_index;
    game.entities[id].weapon_set.weapons[0].clip_remaining = 2;
    let reload = rearm::Reload::begin(&game.entities[id], &game.catalog[index], 100, 0);
    game.entities[id].aircraft.as_mut().unwrap().rearm = reload;
    game.tick = 130;
    assert!(!game.reload_aircraft(id));
    assert_eq!(game.entities[id].weapon_set.weapons[0].clip_remaining, 2);
    game.tick = 145;
    assert!(!game.reload_aircraft(id));
    assert_eq!(game.entities[id].weapon_set.weapons[0].clip_remaining, 3);
    game.tick = 160;
    assert!(game.reload_aircraft(id));
    assert_eq!(game.entities[id].weapon_set.weapons[0].clip_remaining, 4);
}

#[test]
fn takeoff_defers_replacement_commands_and_reload_defers_stop() {
    let (mut game, id) = aircraft();
    game.initialize_aircraft_producer(id);
    game.entities[id]
        .aircraft
        .as_mut()
        .unwrap()
        .set(TAKEOFF, true);
    game.native_move_from(
        id,
        [350.0, 220.0, 40.0],
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
    );
    assert!(matches!(
        game.entities[id].aircraft.as_ref().unwrap().pending,
        Some(crate::skirmish::ai_dispatch::stored_command::Command {
            kind: crate::skirmish::ai_dispatch::stored_command::Kind::Move([350.0, 220.0, 40.0]),
            ..
        })
    ));
    game.native_guard_from(
        id,
        [300.0, 300.0, 25.0],
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
    );
    assert!(matches!(
        game.entities[id].aircraft.as_ref().unwrap().pending,
        Some(crate::skirmish::ai_dispatch::stored_command::Command {
            kind: crate::skirmish::ai_dispatch::stored_command::Kind::Guard([300.0, 300.0, 25.0]),
            ..
        })
    ));
    let state = game.entities[id].aircraft.as_mut().unwrap();
    state.set(TAKEOFF, false);
    state.phase = Phase::Reload;
    game.native_idle_from(id, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
    assert!(matches!(
        game.entities[id].aircraft.as_ref().unwrap().pending,
        Some(crate::skirmish::ai_dispatch::stored_command::Command {
            kind: crate::skirmish::ai_dispatch::stored_command::Kind::Idle,
            ..
        })
    ));
}

#[test]
fn rocket_pod_order_survives_takeoff_deferral_with_its_native_temporary_slot() {
    let (mut game, id) = aircraft();
    let index = game.entities[id].type_index;
    game.catalog[index].location_weapons = 4;
    let mut weapon = crate::skirmish::WeaponRules::unarmed();
    weapon.damage = 1.0;
    weapon.range = 500.0;
    game.catalog[index].tertiary_weapon = Some(weapon);
    let id = game.spawn(index, 0, [200.0; 2], true);
    game.initialize_aircraft_producer(id);
    game.entities[id]
        .aircraft
        .as_mut()
        .unwrap()
        .set(TAKEOFF, true);
    game.fire_weapon_at(id, 2, [350.0, 300.0]).unwrap();
    let state = game.entities[id].aircraft.as_mut().unwrap();
    let order = state.pending.take().unwrap();
    assert!(matches!(
        order.kind,
        crate::skirmish::ai_dispatch::stored_command::Kind::Attack {
            goal: crate::skirmish::combat::attack::Goal::Position([350.0, 300.0, 0.0]),
            ..
        }
    ));
    state.set(TAKEOFF, false);
    game.replay_stored_command(id, order);
    assert!(matches!(
        game.entities[id].ai.order,
        Order::FireAt { weapon: 2, .. }
    ));
    assert_eq!(game.entities[id].weapon_slot(), 2);
    assert!(matches!(
        game.entities[id].weapon_set.lock,
        crate::skirmish::weapons::Lock::Temporary
    ));
}

#[test]
fn unproduced_aircraft_use_normal_flight_and_ground_draw_offset_is_visual_only() {
    let (mut game, id) = aircraft();
    game.initialize_aircraft_producer(id);
    assert!(game.entities[id].aircraft.as_ref().unwrap().flag(ALLOW_AIR));
    game.choose_aircraft_locomotor(id, false);
    game.set_flight_position(id, [200.0, 200.0, 0.0]);
    game.update_aircraft_presentation(id);
    assert_eq!(
        game.entities[id].aircraft_draw_height().to_bits(),
        5.0_f32.to_bits()
    );
    assert_eq!(game.object_position(id)[2].to_bits(), 0.0_f32.to_bits());
}

#[test]
fn no_airfield_attrition_has_no_attacker_or_enemy_kill_credit() {
    let (mut game, id) = aircraft();
    game.apply_native_damage(id, None, 200.0, DeathType::Normal);
    assert!(!game.entities[id].alive());
    assert!(game.entities[id].damage_state.last_attacker.is_none());
    assert_eq!(game.players[0].statistics.units_lost, 1);
    assert_eq!(game.players[1].statistics.units_destroyed, 0);
}

fn airfield(game: &mut crate::skirmish::Skirmish) -> usize {
    let index = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[index].kind_of.push("FS_AIRFIELD".into());
    game.catalog[index].behavior.production_exit = None;
    game.catalog[index].parking = Some(crate::skirmish::parking::tests::rules());
    crate::skirmish::parking::tests::bind_bones(&mut game.catalog[index]);
    crate::skirmish::tests::register_update(&mut game.catalog[index], "ParkingPlaceBehavior");
    game.catalog[index]
        .behavior_modules
        .last_mut()
        .unwrap()
        .interfaces |= 2;
    game.catalog[index].production_doors = crate::skirmish::parking::DoorRules {
        count: 4,
        opening: 60,
        waiting: 90,
        closing: 60,
        complete: 30,
    };
    let id = game.spawn(index, 0, [300.0, 200.0], true);
    game.entities[id].facing = 0.0;
    game.build_parking_info(id);
    id
}

#[test]
fn paid_capacity_cancellation_and_strict_door_edge_precede_bone_spawn() {
    let (mut game, prototype) = aircraft();
    // Energy::getEnergySupplyRatio returns raw production at zero consumption.
    // This door-boundary fixture requires full-speed, one-frame production.
    let center = game.type_for(0, Kind::CommandCenter).unwrap();
    game.catalog[center].energy = 1;
    game.update_power();
    assert_eq!(
        game.players[0].energy.production.to_bits(),
        1.0_f32.to_bits()
    );
    let index = game.entities[prototype].type_index;
    game.catalog[index].build_seconds = 1.0 / 30.0;
    let producer = airfield(&mut game);
    let before = game.players[0].cash;
    for _ in 0..4 {
        game.queue_unit(producer, index).unwrap();
    }
    assert!(game.queue_unit(producer, index).is_err());
    assert_eq!(game.players[0].cash, before - 800);
    let cancelled = game.entities[producer].queue[1].id;
    game.cancel_production(0, producer, cancelled).unwrap();
    assert_eq!(game.players[0].cash, before - 600);
    assert!(game.parking_available(producer, index));
    game.queue_unit(producer, index).unwrap();
    let count = game.entities.len();
    for tick in 1..=61 {
        game.tick = tick;
        game.run_object_updates();
    }
    assert_eq!(game.entities.len(), count);
    game.tick = 62;
    game.run_object_updates();
    assert_eq!(game.entities.len(), count + 1);
    let spawned = &game.entities[count];
    assert_eq!(spawned.producer, Some(producer));
    assert_eq!(
        spawned.position.map(f32::to_bits),
        [300.0_f32.to_bits(), 200.0_f32.to_bits()]
    );
    assert!(spawned.aircraft.as_ref().unwrap().phase == Phase::TaxiFromHangar);
    assert!(
        crate::skirmish::parking::doors::condition_names(
            game.entities[producer].production_door_flags
        )
        .contains(&"DOOR_1_WAITING_OPEN")
    );
}

#[test]
fn captured_airfield_transfers_parked_jets_but_releases_foreign_airborne_ones() {
    let (mut game, parked) = aircraft();
    let index = game.entities[parked].type_index;
    let airborne = game.spawn(index, 0, [450.0, 300.0], true);
    let producer = airfield(&mut game);
    for id in [parked, airborne] {
        game.entities[id].producer = Some(producer);
        game.reserve_parking_space(producer, id).unwrap();
    }
    game.set_flight_position(parked, [320.0, 200.0, 0.0]);
    game.set_flight_position(airborne, [450.0, 300.0, 100.0]);
    let team = game.team_membership.default_team(1);
    game.defect_to_team(producer, team, 0);
    assert_eq!(game.entities[parked].owner, 1);
    assert_eq!(game.entities[parked].producer, Some(producer));
    assert_eq!(game.entities[airborne].owner, 0);
    assert_eq!(game.entities[airborne].producer, None);
    assert!(
        game.entities[producer]
            .parking
            .as_ref()
            .unwrap()
            .space_index(airborne)
            .is_none()
    );
}

#[test]
fn last_special_round_ends_ordinary_attack_instead_of_waiting_forever() {
    let (mut game, id) = aircraft();
    let enemy = game.type_for(1, Kind::Tank).unwrap();
    let target = game.spawn(enemy, 1, [230.0, 200.0], true);
    game.entities[id].weapon_set.weapons[0].clip_remaining = 0;
    assert!(!game.update_attack(id, target));
}

#[test]
fn engine_sound_follows_native_transition_motion_and_airborne_rules() {
    let (mut game, id) = aircraft();
    game.entities[id].aircraft.as_mut().unwrap().flags = 0;
    game.update_aircraft_engine(id);
    assert!(!game.entities[id].aircraft_engine_on());
    game.entities[id]
        .aircraft
        .as_mut()
        .unwrap()
        .set(TAKEOFF, true);
    game.update_aircraft_engine(id);
    assert!(game.entities[id].aircraft_engine_on());
    game.entities[id]
        .aircraft
        .as_mut()
        .unwrap()
        .set(TAKEOFF, false);
    game.set_object_position(id, [200.0, 200.0, 30.0]);
    game.update_aircraft_engine(id);
    assert!(game.entities[id].aircraft_engine_on());
    game.set_object_position(id, [200.0, 200.0, 0.0]);
    game.update_aircraft_engine(id);
    assert!(!game.entities[id].aircraft_engine_on());
    let index = game.entities[id].type_index;
    game.catalog[index].aircraft.as_mut().unwrap().helipad = true;
    let helicopter = game.spawn(index, 0, [250.0, 200.0], true);
    game.update_aircraft_engine(helicopter);
    assert!(game.entities[helicopter].aircraft_engine_on());
    game.entities[helicopter].health = 0.0;
    assert!(!game.entities[helicopter].aircraft_engine_on());
}

#[test]
fn aurora_attack_speed_expires_before_its_targeting_offset_and_empty_return() {
    let (mut game, id) = aircraft();
    let index = game.entities[id].type_index;
    let mut supersonic =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Tank).locomotor;
    supersonic.flight = Some(flight::tests::rules(flight::Appearance::Wings));
    let mut sluggish =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Tank).locomotor;
    sluggish.flight = Some(flight::tests::rules(flight::Appearance::Wings));
    let rules = game.catalog[index].aircraft.as_mut().unwrap();
    rules.attacking = Some(LocomotorSet {
        role: crate::skirmish::locomotor::set::SUPERSONIC,
        locomotor: supersonic,
        speed: 480.0,
    });
    rules.returning = Some(LocomotorSet {
        role: crate::skirmish::locomotor::set::SLUGGISH,
        locomotor: sluggish,
        speed: 150.0,
    });
    rules.attack_locomotor_frames = 3;
    rules.attackers_miss_frames = 60;
    rules.sneaky_offset = -20.0;
    game.choose_aircraft_locomotor(id, true);
    game.entities[id].facing = std::f32::consts::FRAC_PI_2;
    game.entities[id].ai.order = Order::Attack(0);
    game.entities[id].weapon_set.status.attacking = true;
    game.tick = 100;
    game.update_jet_ai_wrapper(id);
    assert_eq!(
        game.entities[id].ai.locomotor_set,
        crate::skirmish::locomotor::set::SUPERSONIC
    );
    assert_eq!(
        game.entities[id].aircraft_speed(&game.catalog[index]),
        Some(480.0)
    );
    game.entities[id].ai.order = Order::Idle;
    game.entities[id].weapon_set.status.attacking = false;
    game.tick = 102;
    game.update_jet_ai_wrapper(id);
    assert_eq!(
        game.entities[id].ai.locomotor_set,
        crate::skirmish::locomotor::set::SUPERSONIC
    );
    game.tick = 103;
    game.update_jet_ai_wrapper(id);
    // CRC=0 resets the attack locomotor at the exact expiry frame even when
    // the aircraft retains ammunition and has no special return flag.
    assert_eq!(
        game.entities[id].ai.locomotor_set,
        crate::skirmish::locomotor::set::NORMAL
    );
    let offset = game.entities[id]
        .aircraft_targeting_offset(&game.catalog[index], 159)
        .unwrap();
    assert!(offset[0].abs() < 0.00001);
    assert_eq!(offset[1].to_bits(), (-20.0_f32).to_bits());
    assert_eq!(offset[2].to_bits(), 0.0_f32.to_bits());
    game.entities[id]
        .aircraft
        .as_mut()
        .unwrap()
        .set(SPECIAL_RETURN, true);
    game.update_jet_ai_wrapper(id);
    assert_eq!(
        game.entities[id].ai.locomotor_set,
        crate::skirmish::locomotor::set::SLUGGISH
    );
    assert_eq!(
        game.entities[id].aircraft_speed(&game.catalog[index]),
        Some(150.0)
    );
    assert!(
        game.entities[id]
            .aircraft_targeting_offset(&game.catalog[index], 160)
            .is_none()
    );
    game.choose_aircraft_locomotor(id, false);
    game.tick = 160;
    game.update_jet_ai_wrapper(id);
    assert_eq!(
        game.entities[id].ai.locomotor_set,
        crate::skirmish::locomotor::set::TAXI
    );
}

#[test]
fn aurora_landing_entry_preserves_sluggish_return_until_touchdown_taxi() {
    let (mut game, id) = aircraft();
    let index = game.entities[id].type_index;
    let mut sluggish =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Tank).locomotor;
    sluggish.flight = Some(flight::tests::rules(flight::Appearance::Wings));
    game.catalog[index].aircraft.as_mut().unwrap().returning = Some(LocomotorSet {
        role: crate::skirmish::locomotor::set::SLUGGISH,
        locomotor: sluggish,
        speed: 150.0,
    });
    let producer = airfield(&mut game);
    game.reserve_parking_space(producer, id).unwrap();
    game.entities[id].producer = Some(producer);
    game.initialize_aircraft_producer(id);
    game.choose_aircraft_locomotor(id, true);
    game.entities[id]
        .aircraft
        .as_mut()
        .unwrap()
        .set(SPECIAL_RETURN, true);
    game.set_flight_position(id, [500.0, 200.0, 100.0]);
    // Native JetTakeoffOrLandingState::onEnter asks for NORMAL, but the
    // JetAI override must retain the out-of-ammo sluggish locomotor.
    game.enter_aircraft_phase(id, Phase::Landing);
    let state = game.entities[id].aircraft.as_ref().unwrap();
    assert!(state.phase == Phase::Landing);
    assert_eq!(
        game.entities[id].ai.locomotor_set,
        crate::skirmish::locomotor::set::SLUGGISH
    );
    assert!(state.flag(LANDING));
    assert!(state.flag(SPECIAL_RETURN));
    assert_eq!(
        game.entities[id].aircraft_speed(&game.catalog[index]),
        Some(150.0)
    );
    game.enter_aircraft_phase(id, Phase::TaxiFromLanding);
    let state = game.entities[id].aircraft.as_ref().unwrap();
    assert_eq!(
        game.entities[id].ai.locomotor_set,
        crate::skirmish::locomotor::set::TAXI
    );
    assert!(!state.flag(ALLOW_AIR));
}

#[test]
fn aircraft_named_role_aliases_do_not_reconstruct_and_idle_update_keeps_script_role() {
    use crate::skirmish::{Faction, LocomotorVariant, locomotor::set, tests::fixture_type};
    let (mut game, id) = aircraft();
    let index = game.entities[id].type_index;
    let mut sets = (0..2).map(|_| LocomotorSet {
        role: set::SLUGGISH,
        locomotor: fixture_type(Faction::Usa, Kind::Tank).locomotor,
        speed: 150.0,
    });
    let rules = game.catalog[index].aircraft.as_mut().unwrap();
    rules.attacking = sets.next();
    rules.returning = sets.next();
    game.catalog[index]
        .locomotor_sets
        .wander
        .push(LocomotorVariant {
            locomotor: fixture_type(Faction::Usa, Kind::Tank).locomotor,
            speed: 40.0,
        });
    game.choose_aircraft_locomotor(id, true);
    game.entities[id].aircraft.as_mut().unwrap().attack_until = 10;
    game.choose_locomotor_set(id, set::WANDER);
    assert_eq!(game.entities[id].ai.locomotor_set, set::SLUGGISH);
    let random = game.random;
    let state = game.entities[id].aircraft.as_mut().unwrap();
    state.attack_until = 0;
    state.set(SPECIAL_RETURN, true);
    game.choose_locomotor_set(id, set::NORMAL);
    assert_eq!(game.entities[id].ai.locomotor_set, set::SLUGGISH);
    assert_eq!(game.random, random);
    game.entities[id]
        .aircraft
        .as_mut()
        .unwrap()
        .set(SPECIAL_RETURN, false);
    game.choose_locomotor_set(id, set::WANDER);
    let random = game.random;
    game.update_jet_ai_wrapper(id);
    assert_eq!(game.entities[id].ai.locomotor_set, set::WANDER);
    assert_eq!(game.random, random);
}

#[test]
fn lockon_targeters_share_one_deadline_and_last_removal_retires_drawable_on_update() {
    let (mut game, jet) = aircraft();
    let cursor = game.catalog[0].name.clone();
    let kind = game.entities[jet].type_index;
    let rules = &mut game.catalog[kind].aircraft.as_mut().unwrap().lockon;
    rules.frames = 6;
    rules.cursor = cursor;
    rules.angle_spin = 0.0;
    game.tick = 10;
    game.add_attack_targeter(jet, 2);
    let position = game.object_position(jet);
    let drawable = game.entities[jet].lockon_drawable().unwrap();
    assert_eq!(
        drawable.position.map(f32::to_bits),
        [position[0] + 100.0, position[1], position[2]].map(f32::to_bits)
    );
    game.tick = 12;
    game.add_attack_targeter(jet, 2);
    game.add_attack_targeter(jet, 3);
    assert_eq!(
        game.entities[jet].aircraft.as_ref().unwrap().targeters,
        [2, 3]
    );
    assert_eq!(
        game.entities[jet]
            .aircraft
            .as_ref()
            .unwrap()
            .untargetable_expire,
        16
    );
    game.remove_attack_targeter(jet, 2);
    assert!(game.prevents_attack_aim(jet, 3));
    game.remove_attack_targeter(jet, 3);
    assert!(!game.prevents_attack_aim(jet, 3));
    assert!(game.entities[jet].lockon_drawable().is_some());
    game.update_aircraft_lockon(jet);
    assert!(game.entities[jet].lockon_drawable().is_none());
}

#[test]
fn lockon_blink_emits_ordered_tick_only_on_falling_phase_and_expires_at_deadline() {
    let (mut game, jet) = aircraft();
    let cursor = game.catalog[0].name.clone();
    let kind = game.entities[jet].type_index;
    let rules = &mut game.catalog[kind].aircraft.as_mut().unwrap().lockon;
    rules.frames = 6;
    rules.cursor = cursor;
    rules.frequency = 1.0;
    rules.blinky = true;
    rules.tick_sound = "LockonTick".into();
    game.add_attack_targeter(jet, 2);
    assert!(game.entities[jet].lockon_drawable().unwrap().hidden);
    game.tick = 4;
    game.update_aircraft_lockon(jet);
    assert!(game.entities[jet].lockon_drawable().unwrap().hidden);
    assert!(game.presentation.object_sounds.is_empty());
    game.tick = 5;
    game.update_aircraft_lockon(jet);
    assert!(!game.entities[jet].lockon_drawable().unwrap().hidden);
    assert_eq!(game.presentation.object_sounds.len(), 1);
    assert_eq!(game.presentation.object_sounds[0].name, "LockonTick");
    assert!(matches!(
        game.presentation.weapon_fx.last(),
        Some(crate::skirmish::WeaponFxEvent::ObjectSound(0))
    ));
    game.tick = 6;
    assert!(!game.prevents_attack_aim(jet, 2));
    game.update_aircraft_lockon(jet);
    assert!(game.entities[jet].lockon_drawable().is_none());
    game.add_attack_targeter(jet, 3);
    assert!(!game.prevents_attack_aim(jet, 3));
    assert!(game.entities[jet].lockon_drawable().is_none());
}

#[test]
fn late_dead_transition_releases_jet_runway_and_keeps_derived_callback_running() {
    let (mut game, jet) = aircraft();
    let producer = airfield(&mut game);
    game.reserve_parking_space(producer, jet).unwrap();
    game.entities[jet].producer = Some(producer);
    game.initialize_aircraft_producer(jet);
    game.set_object_position(jet, [500.0, 200.0, 100.0]);
    game.enter_aircraft_phase(jet, Phase::Landing);
    assert!(game.entities[jet].aircraft.as_ref().unwrap().flag(LANDING));
    let runway = game.entities[producer]
        .parking
        .as_ref()
        .unwrap()
        .runway_index(jet)
        .unwrap();
    assert_eq!(
        game.entities[producer]
            .parking
            .as_ref()
            .unwrap()
            .runway_owner(runway),
        Some(jet)
    );
    let kind = game.entities[jet].type_index;
    let cursor = game.catalog[0].name.clone();
    let lockon = &mut game.catalog[kind].aircraft.as_mut().unwrap().lockon;
    lockon.frames = 2;
    lockon.cursor = cursor;
    game.add_attack_targeter(jet, producer);
    game.entities[jet].ai.primary.sleep_until = game.update_frame() + 100;
    game.mark_ai_dead(jet);
    assert!(game.entities[jet].aircraft.as_ref().unwrap().flag(LANDING));
    assert_eq!(game.update_ai_module(jet), 1);
    assert_eq!(game.entities[jet].aircraft_phase(), Some("dead"));
    assert!(!game.entities[jet].aircraft.as_ref().unwrap().flag(LANDING));
    assert_eq!(
        game.entities[producer]
            .parking
            .as_ref()
            .unwrap()
            .runway_owner(runway),
        None
    );
    assert!(game.entities[jet].lockon_drawable().is_some());
    game.tick += 2;
    game.set_object_position(jet, [500.0, 200.0, 2.0]);
    assert_eq!(game.update_ai_module(jet), 1);
    assert!(game.entities[jet].lockon_drawable().is_none());
    // AI_DEAD is outside the Jet-specific state range: an airborne corpse no
    // longer inherits LANDING's minimum-height drawable correction.
    assert_eq!(
        game.entities[jet].aircraft_draw_height().to_bits(),
        0.0_f32.to_bits()
    );
}

fn parked_aircraft() -> (crate::skirmish::Skirmish, usize, usize) {
    let (mut game, prototype) = aircraft();
    let producer = airfield(&mut game);
    let kind = game.entities[prototype].type_index;
    let id = game.new_object(kind, 0, true);
    game.entities[id].producer = Some(producer);
    game.reserve_parking_space(producer, id).unwrap();
    let info = game.aircraft_parking(id).unwrap();
    game.set_object_position(id, info.parking.position);
    game.set_object_orientation(id, info.parking.orientation());
    (game, id, producer)
}

#[test]
fn parked_move_runs_taxi_pause_takeoff_and_replays_full_script_goal() {
    let (mut game, id, producer) = parked_aircraft();
    let goal = [950.0, 550.0, 87.0];
    game.native_move_from(id, goal, 1);
    assert!(matches!(
        game.entities[id].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::Jet
    ));
    assert!(game.entities[id].aircraft.as_ref().unwrap().phase == Phase::AwaitTakeoff);
    let mut saw_taxi = false;
    let mut saw_pause = false;
    let mut saw_takeoff = false;
    for _ in 0..3600 {
        game.step();
        let state = game.entities[id].aircraft.as_ref().unwrap();
        saw_taxi |= state.phase == Phase::TaxiTakeoff;
        saw_pause |= state.phase == Phase::Pause;
        saw_takeoff |= state.phase == Phase::Takeoff;
        if matches!(game.entities[id].ai.order, Order::Move(_)) && state.pending.is_none() {
            break;
        }
    }
    assert!(
        saw_taxi && saw_pause && saw_takeoff,
        "source taxi/pause/takeoff chain must execute"
    );
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
    assert_eq!(
        game.entities[id].ai_goal_position().map(f32::to_bits),
        goal.map(f32::to_bits)
    );
    assert_eq!(game.entities[id].ai.command.last_command_source, 1);
    assert!(game.entities[id].aircraft.as_ref().unwrap().flag(ALLOW_AIR));
    assert!(
        game.entities[producer]
            .parking
            .as_ref()
            .unwrap()
            .space_index(id)
            .is_some()
    );
}

#[test]
fn empty_parked_guard_retains_order_without_consuming_runway_or_starting_takeoff() {
    let (mut game, id, _) = parked_aircraft();
    game.entities[id].weapon_set.weapons[0].clip_remaining = 0;
    let goal = [720.0, 340.0, 91.0];
    game.native_guard_from(id, goal, 1);
    let state = game.entities[id].aircraft.as_ref().unwrap();
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(!state.transitioning() && !state.flag(ALLOW_AIR));
    assert!(
        matches!(state.pending.as_ref().map(|command| &command.kind), Some(crate::skirmish::ai_dispatch::stored_command::Kind::Guard(point)) if point.map(f32::to_bits) == goal.map(f32::to_bits))
    );
    assert!(!game.entities[id].ai_idle());
    game.step();
    assert!(!game.entities[id].aircraft.as_ref().unwrap().transitioning());
    assert!(
        game.entities[id]
            .aircraft
            .as_ref()
            .unwrap()
            .pending
            .is_some()
    );
}

#[test]
fn actual_attack_target_loss_schedules_runway_jet_return_at_the_current_frame() {
    let (mut game, id) = aircraft();
    let enemy = game.type_for(1, Kind::Tank).unwrap();
    let target = game.spawn(enemy, 1, [220.0, 200.0], true);
    game.native_attack_goal_from(
        id,
        crate::skirmish::combat::attack::Goal::Object(target),
        0x7fff_ffff,
        0,
        false,
    );
    game.entities[target].set_effectively_dead(true);
    game.tick = 73;
    assert!(!game.update_native_attack(id, crate::skirmish::combat::attack::Goal::Object(target)));
    assert_eq!(game.entities[id].aircraft.as_ref().unwrap().return_at, 73);
}

#[test]
fn queued_jet_deletion_releases_parking_before_destroying_lockon_drawable() {
    let (mut game, jet, producer) = parked_aircraft();
    let kind = game.entities[jet].type_index;
    let cursor = game.catalog[0].name.clone();
    let lockon = &mut game.catalog[kind].aircraft.as_mut().unwrap().lockon;
    lockon.frames = 100;
    lockon.cursor = cursor;
    game.add_attack_targeter(jet, producer);
    assert!(game.entities[jet].lockon_drawable().is_some());
    assert!(
        game.entities[producer]
            .parking
            .as_ref()
            .unwrap()
            .space_index(jet)
            .is_some()
    );
    game.destroy_maintenance_object(jet);
    assert!(game.entities[jet].exists());
    assert!(
        game.entities[producer]
            .parking
            .as_ref()
            .unwrap()
            .space_index(jet)
            .is_none()
    );
    assert!(game.entities[jet].lockon_drawable().is_some());
    game.process_update_destruction();
    assert!(!game.entities[jet].exists());
    assert!(game.entities[jet].lockon_drawable().is_none());
}
