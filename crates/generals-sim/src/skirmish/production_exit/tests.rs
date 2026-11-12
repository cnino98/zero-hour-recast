use crate::skirmish::locomotor::{GroundAppearance, GroundRules};
use crate::skirmish::movement::distance_squared;
use crate::skirmish::{Kind, Order, Skirmish};

#[test]
fn production_birth_waits_until_all_initial_ai_and_physics_updates_finish() {
    let (mut game, producer, unit) = factory(false);
    game.catalog[unit].build_seconds = 1.0 / 30.0;
    game.train(producer, unit).unwrap();
    let count = game.entities.len();
    for id in 0..count {
        if game.entities[id].ai.registration.is_some()
            && game.entities[id].projectile_flight.is_none()
        {
            game.update_ai_module(id);
        }
    }
    assert_eq!(game.entities.len(), count);
    assert_eq!(game.entities[producer].queue.len(), 1);
    game.update_physics(count);
    assert_eq!(game.entities.len(), count);
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities.len(), count + 1);
    assert!(game.entities[producer].queue.is_empty());
    assert_eq!(game.entities[count].producer, Some(producer));
    assert!(matches!(game.entities[count].ai.order, Order::Exit(_)));
}

#[test]
fn queue_exit_burst_overrides_delay_then_waits_exact_native_updates() {
    let mut state = super::QueueState { delay: 0, burst: 2 };
    assert!(state.free());
    state.exited(9);
    assert!(state.free());
    state.update();
    assert_eq!(state.delay, 0);
    state.exited(9);
    for remaining in (1..=9).rev() {
        assert!(!state.free());
        assert_eq!(state.delay, remaining);
        state.update();
    }
    assert!(state.free());
}

#[test]
fn blocked_queue_exit_does_not_resolve_variations_until_it_can_create_an_object() {
    let (mut game, producer, unit) = factory(false);
    let building = game.entities[producer].type_index;
    game.catalog[building]
        .behavior
        .production_exit
        .as_mut()
        .unwrap()
        .queue = Some(crate::skirmish::QueueExit {
        allow_airborne_creation: false,
        delay_frames: 9,
        initial_burst: 0,
    });
    for name in ["FirstVariation", "SecondVariation"] {
        let mut variant =
            crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Tank);
        variant.name = name.into();
        game.catalog.push(variant);
        game.catalog[unit].variations.push(name.into());
    }
    game.entities[producer].queue_exit.delay = 1;
    let random = game.random;
    let count = game.entities.len();
    for _ in 0..4 {
        assert!(game.spawn_produced_unit(producer, unit, false).is_none());
        assert_eq!(game.random, random);
        assert_eq!(game.entities.len(), count);
    }
    game.update_queue_exit(producer);
    let mut expected = random;
    let variation = crate::skirmish::startup::random_value(&mut expected) % 2;
    let _ = crate::skirmish::startup::random_real(
        &mut expected,
        -std::f32::consts::PI / 6.0,
        std::f32::consts::PI / 6.0,
    );
    let _ = crate::skirmish::startup::random_real(&mut expected, 0.8, 1.2);
    let _ = crate::skirmish::startup::random_between(&mut expected, 1);
    // AIStates.cpp691–693 constructs attack-move and both attack-follow child
    // Idle machines; AIUpdate.cpp630 then enters primary Idle.
    for _ in 0..4 {
        crate::skirmish::startup::random_between(&mut expected, 60);
    }
    let child = game.spawn_produced_unit(producer, unit, false).unwrap();
    assert_eq!(
        game.catalog[game.entities[child].type_index].name,
        if variation == 0 {
            "FirstVariation"
        } else {
            "SecondVariation"
        }
    );
    assert!(
        game.catalog[game.entities[child].type_index]
            .locomotor
            .wander_length
            .is_some()
    );
    assert_eq!(game.random, expected);
}

#[test]
fn queue_default_duplicates_raw_natural_rally_but_explicit_rally_is_adjusted() {
    for explicit in [false, true] {
        let (mut game, producer, unit) = factory(true);
        let building = game.entities[producer].type_index;
        let exit = game.catalog[building]
            .behavior
            .production_exit
            .as_mut()
            .unwrap();
        exit.snap_rally = true;
        exit.queue = Some(crate::skirmish::QueueExit {
            allow_airborne_creation: false,
            delay_frames: 9,
            initial_burst: 0,
        });
        let origin = game.entities[producer].position;
        let natural = game
            .snap_unit_position(unit, [origin[0] + 55.0, origin[1]])
            .unwrap();
        let blocker = game.spawn(unit, 0, natural, true);
        game.reserve_goal(blocker, natural);
        if explicit {
            game.set_rally_point(producer, [natural[0], natural[1], 0.0])
                .unwrap();
        }
        let child = game.spawn_produced_unit(producer, unit, false).unwrap();
        let Order::Exit(first) = game.entities[child].ai.order else {
            panic!("Production must retain its original quick exit segment");
        };
        assert_eq!(
            first.map(f32::to_bits),
            [natural[0], natural[1], 0.0].map(f32::to_bits)
        );
        let final_point = game.entities[child]
            .ai
            .waypoints
            .points
            .get(1)
            .copied()
            .unwrap();
        if explicit {
            assert_ne!(
                final_point.map(f32::to_bits),
                [natural[0], natural[1], 0.0].map(f32::to_bits)
            );
        } else {
            assert_eq!(
                final_point.map(f32::to_bits),
                [natural[0], natural[1], 0.0].map(f32::to_bits)
            );
        }
    }
}

#[test]
fn quantity_batch_charges_once_and_keeps_the_entry_until_both_units_exit() {
    let (mut game, producer, unit) = factory(false);
    let building = game.entities[producer].type_index;
    game.catalog[building].production = Some(vec![(game.catalog[unit].name.clone(), 2)]);
    game.catalog[building]
        .behavior
        .production_exit
        .as_mut()
        .unwrap()
        .queue = Some(crate::skirmish::QueueExit {
        allow_airborne_creation: false,
        delay_frames: 9,
        initial_burst: 0,
    });
    game.catalog[unit].build_seconds = 1.0 / 30.0;
    let cash = game.players[0].cash;
    game.train(producer, unit).unwrap();
    assert_eq!(game.players[0].cash, cash - game.catalog[unit].cost);
    let first = game.entities.len();
    game.update_production(producer);
    assert_eq!(game.entities.len(), first + 1);
    assert_eq!(game.entities[producer].queue[0].produced, 1);
    for _ in 0..8 {
        game.update_queue_exit(producer);
        game.update_production(producer);
        assert_eq!(game.entities.len(), first + 1);
    }
    game.update_queue_exit(producer);
    game.update_production(producer);
    assert_eq!(game.entities.len(), first + 2);
    assert!(game.entities[producer].queue.is_empty());
    assert_eq!(game.players[0].cash, cash - game.catalog[unit].cost);
    assert!(
        game.entities[first]
            .ai
            .waypoints
            .points
            .get(1)
            .copied()
            .is_some()
    );
    assert!(
        game.entities[first + 1]
            .ai
            .waypoints
            .points
            .get(1)
            .copied()
            .is_some()
    );
}

#[test]
fn superhackers_partial_batch_cannot_be_refunded_even_on_producer_death() {
    let (mut game, producer, unit) = factory(false);
    let building = game.entities[producer].type_index;
    game.catalog[building].production = Some(vec![(game.catalog[unit].name.clone(), 2)]);
    game.catalog[building]
        .behavior
        .production_exit
        .as_mut()
        .unwrap()
        .queue = Some(crate::skirmish::QueueExit {
        allow_airborne_creation: false,
        delay_frames: 9,
        initial_burst: 0,
    });
    game.catalog[unit].build_seconds = 1.0 / 30.0;
    game.train(producer, unit).unwrap();
    game.train(producer, unit).unwrap();
    game.update_production(producer);
    let cash = game.players[0].cash;
    let entry = game.entities[producer].queue[0].id;
    assert_eq!(
        game.cancel_production(0, producer, entry),
        Err("A batch that has started exiting cannot be cancelled")
    );
    assert_eq!(game.players[0].cash, cash);
    game.refund_production_on_death(producer);
    assert!(game.entities[producer].queue.is_empty());
    assert_eq!(game.players[0].cash, cash + game.catalog[unit].cost);
}

fn factory(native_ground: bool) -> (Skirmish, usize, usize) {
    let mut game = crate::skirmish::tests::game();
    // These exit/rally fixtures require normal production, supplied by their
    // existing completed center rather than the zero-energy base fixture.
    game.catalog[0].energy = 1;
    game.update_power();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    if native_ground {
        game.catalog[tank].locomotor.ground = Some(GroundRules {
            wander: None,
            appearance: GroundAppearance::Treads,
            is_dozer: false,
            downhill_only: false,
            acceleration: 100.0 / 900.0,
            acceleration_damaged: 100.0 / 900.0,
            braking: 100.0 / 900.0,
            min_speed: 0.0,
            close_enough: 1.0,
            mass: 50.0,
            forward_friction: 0.1,
            lateral_friction: 0.2,
            extra_friction: 0.0,
        });
    }
    let factory_type = game.type_for(0, Kind::Factory).unwrap();
    let factory = game.spawn(factory_type, 0, [305.0, 405.0], true);
    (game, factory, tank)
}

fn arrive(game: &mut Skirmish, ids: &[usize]) {
    for _ in 0..1800 {
        game.step();
        if ids
            .iter()
            .all(|&id| matches!(game.entities[id].ai.order, Order::Idle))
        {
            return;
        }
    }
    panic!("Units did not complete their source movement orders");
}

#[test]
fn consecutive_production_keeps_factory_rally_and_finds_unclaimed_destinations() {
    for appearance in [GroundAppearance::Treads, GroundAppearance::Legs] {
        let (mut game, factory, tank) = factory(true);
        game.catalog[tank]
            .locomotor
            .ground
            .as_mut()
            .unwrap()
            .appearance = appearance;
        let rally = [505.0, 405.0];
        game.set_rally_point(factory, [rally[0], rally[1], 0.0])
            .unwrap();
        let first = game.entities.len();
        for _ in 0..3 {
            game.train(factory, tank).unwrap();
        }
        for _ in 0..1800 {
            game.step();
            if game.entities.len() >= first + 3 && game.entities[factory].queue.is_empty() {
                break;
            }
        }
        assert_eq!(game.entities.len(), first + 3);
        let ids = [first, first + 1, first + 2];
        arrive(&mut game, &ids);
        let cells: Vec<_> = ids
            .iter()
            .map(|&id| {
                let actor = &game.entities[id];
                assert!(actor.ai.ignored_producer.is_none());
                assert!(distance_squared(actor.position, rally) < 40.0 * 40.0);
                game.movement_cell(id, actor.position).unwrap()
            })
            .collect();
        assert!(cells[0] != cells[1] && cells[0] != cells[2] && cells[1] != cells[2]);
        assert_eq!(
            game.entities[factory].rally_point,
            Some([rally[0], rally[1], 0.0])
        );

        // A later manual order must not clear the producer's retained rally.
        game.move_to(first, [405.0, 505.0]).unwrap();
        arrive(&mut game, &[first]);
        let fourth = game.entities.len();
        game.train(factory, tank).unwrap();
        for _ in 0..600 {
            game.step();
            if game.entities.len() > fourth {
                break;
            }
        }
        assert!(game.entities.len() > fourth);
        arrive(&mut game, &[fourth]);
        assert!(distance_squared(game.entities[fourth].position, rally) < 1.0);
        assert_eq!(
            game.entities[factory].rally_point,
            Some([rally[0], rally[1], 0.0])
        );
    }
}

#[test]
fn rally_is_readjusted_when_claimed_after_creation_and_before_exit_completion() {
    let (mut game, factory, tank) = factory(true);
    let rally = [505.0, 405.0];
    game.set_rally_point(factory, [rally[0], rally[1], 0.0])
        .unwrap();
    let id = game.spawn_produced_unit(factory, tank, false).unwrap();
    let original_goal = game.entities[id]
        .ai
        .waypoints
        .points
        .get(1)
        .copied()
        .unwrap();
    let blocker = game.spawn(tank, 0, [original_goal[0], original_goal[1]], true);
    game.refresh_occupancy();
    arrive(&mut game, &[id]);
    assert!(distance_squared(game.entities[id].position, rally) < 40.0 * 40.0);
    assert!(
        game.movement_cell(id, game.entities[id].position)
            != game.movement_cell(blocker, game.entities[blocker].position)
    );

    // A final Alt-waypoint can likewise become occupied during its first leg.
    let next = [505.0, 505.0];
    game.queue_move_to(id, [405.0, 505.0]).unwrap();
    game.queue_move_to(id, next).unwrap();
    let blocker = game.spawn(tank, 0, next, true);
    game.refresh_occupancy();
    arrive(&mut game, &[id]);
    assert!(distance_squared(game.entities[id].position, next) < 40.0 * 40.0);
    assert!(
        game.movement_cell(id, game.entities[id].position)
            != game.movement_cell(blocker, game.entities[blocker].position)
    );
}

#[test]
fn aircraft_default_exit_omits_user_rally_but_supply_exit_retains_it() {
    for kind in [Kind::Factory, Kind::SupplyCenter] {
        let (mut game, factory, tank) = factory(false);
        game.catalog[tank].movement_surface = crate::skirmish::MovementSurface::Air;
        game.catalog[tank].locomotor.legal_surfaces = 8;
        game.catalog[tank].locomotor.ground = None;
        game.catalog[tank].locomotor.flight = Some(crate::skirmish::flight::tests::rules(
            crate::skirmish::flight::Appearance::Hover,
        ));
        let producer = if kind == Kind::SupplyCenter {
            let source = game.type_for(0, kind).unwrap();
            game.catalog[source]
                .behavior
                .production_exit
                .as_mut()
                .unwrap()
                .extend_rally = false;
            game.spawn(source, 0, [405.0, 405.0], true)
        } else {
            factory
        };
        let rally = [505.0, 405.0];
        game.set_rally_point(producer, [rally[0], rally[1], 0.0])
            .unwrap();
        let id = game.spawn_produced_unit(producer, tank, false).unwrap();
        assert_eq!(
            game.entities[id].ai.waypoints.points.get(1).copied(),
            (kind == Kind::SupplyCenter).then_some([rally[0], rally[1], 0.0])
        );
    }
}

#[test]
fn wrapper_production_snaps_the_actual_chassis_geometry_before_exit() {
    let (mut game, producer, chassis) = factory(true);
    game.catalog[chassis].name = "ResolvedChassis".into();
    game.catalog[chassis].geometry.major = 10.0;
    game.catalog[chassis].geometry.minor = 10.0;
    let mut wrapper =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Vehicle);
    wrapper.name = "NativeVariationWrapper".into();
    wrapper.initial_health = 0.0;
    wrapper.geometry.major = 1.0;
    wrapper.geometry.minor = 1.0;
    wrapper.variations.push(game.catalog[chassis].name.clone());
    let wrapper_index = game.catalog.len();
    game.catalog.push(wrapper);
    let production = game.entities[producer].type_index;
    let exit = game.catalog[production]
        .behavior
        .production_exit
        .as_mut()
        .unwrap();
    exit.extend_rally = false;
    exit.snap_rally = true;
    exit.rally = [50.0, 0.0, 0.0];
    let point = [355.0, 405.0];
    let actual = game.snap_unit_position(chassis, point).unwrap();
    let placeholder = game.snap_unit_position(wrapper_index, point).unwrap();
    assert_ne!(actual.map(f32::to_bits), placeholder.map(f32::to_bits));
    let id = game
        .spawn_produced_unit(producer, wrapper_index, false)
        .unwrap();
    assert_eq!(game.entities[id].type_index, chassis);
    assert!(game.entities[id].ai.path_request.pending.is_some());
    game.process_path_service();
    assert_eq!(
        game.entities[id]
            .ai
            .path_endpoint
            .unwrap()
            .map(f32::to_bits),
        actual.map(f32::to_bits)
    );
}

#[test]
fn final_wheeled_rally_uses_native_easy_arrival_only_above_extra_distance_threshold() {
    for extra in [1.0, 167.858_06] {
        let (mut game, producer, tank) = factory(true);
        game.catalog[tank]
            .locomotor
            .ground
            .as_mut()
            .unwrap()
            .appearance = GroundAppearance::Wheels {
            min_turn_speed: 20.0 / 30.0,
            can_move_backwards: false,
        };
        let id = game.spawn_produced_unit(producer, tank, false).unwrap();
        game.process_path_service();
        let goal = [405.0, 405.0];
        game.entities[id].position = [406.0, 418.0];
        game.entities[id].ai.ignored_producer = None;
        game.native_follow_path_from(
            id,
            vec![[goal[0], goal[1], 0.0]],
            None,
            crate::skirmish::ai_goals::COMMAND_FROM_AI,
            true,
        );
        game.process_path_service();
        game.entities[id].ai.command.move_to.waiting = false;
        game.entities[id].ai.command.move_to.path_goal = [goal[0], goal[1], 0.0];
        game.entities[id].ai.path_endpoint = Some(goal);
        game.entities[id].ai.command.path_extra_distance = extra;
        game.entities[id].ai.ground_path = Some(crate::skirmish::path_following::Path::new(
            [[300.0, 405.0, 0.0], [405.0, 405.0, 0.0]]
                .into_iter()
                .map(|position| crate::skirmish::path_following::Node {
                    position,
                    layer: 1,
                    previous_layer: 1,
                    can_optimize: true,
                })
                .collect(),
        ));
        game.update_native_follow_path(id);
        assert_eq!(
            matches!(game.entities[id].ai.order, Order::Idle),
            extra > 1.0
        );
    }
}

#[test]
fn original_producer_command_rejects_internal_products_without_spending_or_queuing() {
    let (mut game, factory, tank) = factory(false);
    let factory_type = game.entities[factory].type_index;
    game.catalog[tank].name = "Tank".into();
    game.catalog[factory_type].buildable_objects = vec!["Tank".into()];
    let mut drone =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Vehicle);
    drone.name = "InternalDrone".into();
    let drone_index = game.catalog.len();
    game.catalog.push(drone);
    let cash = game.players[0].cash;
    assert!(game.train(factory, drone_index).is_err());
    assert!(game.entities[factory].queue.is_empty());
    assert_eq!(game.players[0].cash, cash);
    game.train(factory, tank).unwrap();
    assert_eq!(game.entities[factory].queue.len(), 1);
    assert_eq!(game.players[0].cash, cash - game.catalog[tank].cost);
}

#[test]
fn reskin_production_uses_last_equivalent_command_and_quantity_modifier() {
    let (mut game, producer, tank) = factory(false);
    let facility = game.entities[producer].type_index;
    game.catalog[tank].name = "OriginalTank".into();
    game.catalog[tank].build_rules.sciences = vec!["UnavailableScience".into()];
    let mut variant =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Usa, Kind::Tank);
    variant.name = "ReskinnedTank".into();
    variant.reskin = Some("OriginalTank".into());
    let reskin = game.catalog.len();
    game.catalog.push(variant);
    game.catalog[facility].buildable_objects = vec!["OriginalTank".into(), "ReskinnedTank".into()];
    game.catalog[facility].production = Some(vec![("ReskinnedTank".into(), 2)]);
    game.catalog[facility].production_queue_limit = 1;
    let cash = game.players[0].cash;
    game.train(producer, tank).unwrap();
    assert_eq!(game.entities[producer].queue[0].quantity, 2);
    assert!(game.train(producer, reskin).is_err());
    assert_eq!(game.players[0].cash, cash - game.catalog[tank].cost);
    game.entities[producer].queue.clear();
    game.catalog[facility].buildable_objects.reverse();
    assert!(game.train(producer, reskin).is_err());
    assert!(game.entities[producer].queue.is_empty());
}

#[test]
fn nonpositive_authored_queue_limits_reject_without_spending() {
    let (mut game, producer, unit) = factory(false);
    let facility = game.entities[producer].type_index;
    let cash = game.players[0].cash;
    for limit in [-1, 0] {
        game.catalog[facility].production_queue_limit = limit;
        assert!(game.train(producer, unit).is_err());
        assert_eq!(game.players[0].cash, cash);
        assert!(game.entities[producer].queue.is_empty());
    }
}

#[test]
fn disabling_unit_building_refunds_pending_units_but_preserves_dozers_and_partial_batches() {
    for (dozer, partial) in [(false, false), (true, false), (false, true)] {
        let (mut game, producer, unit) = factory(false);
        if dozer {
            game.catalog[unit].kind_of.push("DOZER".into());
        }
        let cash = game.players[0].cash;
        let cost = game.catalog[unit].cost;
        game.train(producer, unit).unwrap();
        if partial {
            game.entities[producer].queue[0].quantity = 2;
            game.entities[producer].queue[0].produced = 1;
        }
        game.set_map_build_allowed(0, true, false);
        game.update_production(producer);
        if dozer || partial {
            assert_eq!(game.entities[producer].queue.len(), 1);
            assert_eq!(
                game.entities[producer].queue[0].elapsed_frames.to_bits(),
                1.0_f32.to_bits()
            );
            assert_eq!(game.players[0].cash, cash - cost);
        } else {
            assert!(game.entities[producer].queue.is_empty());
            assert_eq!(game.players[0].cash, cash);
            assert_eq!(game.players[0].statistics.money_earned, 0);
        }
    }
}

#[test]
fn natural_rally_extension_uses_native_reciprocal_rounding_for_admitted_zero_z() {
    let (mut game, producer, unit) = factory(false);
    let building = game.entities[producer].type_index;
    game.set_object_position(producer, [0.0; 3]);
    game.set_object_orientation(producer, 0.0);
    let exit = game.catalog[building]
        .behavior
        .production_exit
        .as_mut()
        .unwrap();
    exit.rally = [1.0, 6.0, 0.0];
    // Native Vector3 reciprocal multiplication differs from component division
    // by one bit in the Y exit coordinate for this admitted zero-Z rally.
    let expected = [0x4089_3721, 0x41cd_d2b2, 0];
    assert_eq!(super::natural_rally(exit).map(f32::to_bits), expected);
    let id = game.spawn_produced_unit(producer, unit, false).unwrap();
    let Order::Exit(rally) = game.entities[id].ai.order else {
        panic!("Produced unit must receive its natural exit");
    };
    assert_eq!(rally.map(f32::to_bits), expected);
    let rider = game.spawn(unit, 0, [100.0, 100.0], true);
    game.exit_delivered_unit(producer, rider);
    let Order::Exit(rally) = game.entities[rider].ai.order else {
        panic!("Delivered unit must receive its natural exit");
    };
    assert_eq!(rally.map(f32::to_bits), expected);
}

#[test]
fn zero_rally_does_not_acquire_nan_from_normalization() {
    let (mut game, producer, _) = factory(false);
    let building = game.entities[producer].type_index;
    let exit = game.catalog[building]
        .behavior
        .production_exit
        .as_mut()
        .unwrap();
    exit.rally = [0.0; 3];
    assert_eq!(super::natural_rally(exit).map(f32::to_bits), [0; 3]);
}

#[test]
fn model_exit_uses_full_matrix_and_default_route_retains_authored_height() {
    let (mut game, producer, unit) = factory(true);
    let kind = game.entities[producer].type_index;
    let exit = game.catalog[kind]
        .behavior
        .production_exit
        .as_mut()
        .unwrap();
    exit.create = [10.0, 20.0, 30.0];
    exit.rally = [3.0, 4.0, 12.0];
    game.set_object_transform(
        producer,
        [300.0, 300.0, 40.0],
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    );
    let rally = [500.0, 400.0, 75.0];
    game.set_rally_point(producer, rally).unwrap();
    let displayed = game.producer_rally_path(producer).unwrap();
    assert_eq!(
        displayed[0].map(f32::to_bits),
        [330.0_f32, 310.0, 60.0].map(f32::to_bits)
    );
    assert_eq!(
        displayed[1].map(f32::to_bits),
        [312.0_f32, 303.0, 44.0].map(f32::to_bits)
    );
    assert_eq!(displayed[2].map(f32::to_bits), rally.map(f32::to_bits));
    let child = game.spawn_produced_unit(producer, unit, false).unwrap();
    assert_eq!(
        game.object_position(child).map(f32::to_bits),
        [330.0_f32, 310.0, 0.0].map(f32::to_bits)
    );
    let Order::Exit(goal) = game.entities[child].ai.order else {
        panic!("Native production exit");
    };
    let reciprocal = 1.0_f32 / 13.0;
    let extension = (4.0 * reciprocal) * 20.0;
    assert_eq!(goal[2].to_bits(), (40.0 + (4.0 + extension)).to_bits());
    // privateFollowPath retains the final goal; AIFollowPath::update publishes
    // its current internal segment only when the primary callback runs.
    assert_eq!(
        game.entities[child].ai_goal_position().map(f32::to_bits),
        [505.0_f32, 405.0, 0.0].map(f32::to_bits)
    );
    game.update_ai_module(child);
    assert_eq!(
        game.entities[child].ai_goal_position().map(f32::to_bits),
        goal.map(f32::to_bits)
    );
    assert_eq!(
        game.entities[child]
            .ai
            .path_request
            .pending
            .as_ref()
            .unwrap()
            .goal
            .map(f32::to_bits),
        goal.map(f32::to_bits)
    );
}

#[test]
fn queue_airborne_creation_keeps_transformed_z_only_when_authored_and_snaps_rally_to_ground() {
    for allow_airborne_creation in [false, true] {
        let (mut game, producer, unit) = factory(false);
        let kind = game.entities[producer].type_index;
        let exit = game.catalog[kind]
            .behavior
            .production_exit
            .as_mut()
            .unwrap();
        exit.create = [10.0, 20.0, 30.0];
        exit.rally = [3.0, 4.0, 12.0];
        exit.snap_rally = true;
        exit.queue = Some(crate::skirmish::QueueExit {
            allow_airborne_creation,
            delay_frames: 0,
            initial_burst: 0,
        });
        game.set_object_transform(
            producer,
            [300.0, 300.0, 40.0],
            [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        );
        let child = game.spawn_produced_unit(producer, unit, false).unwrap();
        let expected_height: f32 = if allow_airborne_creation { 60.0 } else { 0.0 };
        assert_eq!(
            game.object_position(child)[2].to_bits(),
            expected_height.to_bits()
        );
        let Order::Exit(goal) = game.entities[child].ai.order else {
            panic!("Native queue exit");
        };
        assert_eq!(goal[2].to_bits(), 0.0_f32.to_bits());
        assert_eq!(
            game.entities[child]
                .ai
                .waypoints
                .points
                .get(1)
                .copied()
                .unwrap()
                .map(f32::to_bits),
            goal.map(f32::to_bits)
        );
    }
}
