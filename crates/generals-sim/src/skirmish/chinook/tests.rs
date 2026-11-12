mod commands;
mod rappel;

use super::{Flight, Phase, Rules};
use crate::skirmish::{
    Faction, Kind, MovementSurface, Order, Skirmish, containment::tests::setup, tests::fixture_type,
};

pub(in crate::skirmish) fn aircraft() -> (Skirmish, usize) {
    let (mut game, id) = setup(0.0, 1.0);
    let index = game.entities[id].type_index;
    let taxi = fixture_type(Faction::Usa, Kind::Vehicle).locomotor;
    let unit = &mut game.catalog[index];
    unit.chinook = Some(Rules {
        lift: 120.0 / 900.0,
        lift_damaged: 80.0 / 900.0,
        gravity: -64.0 / 900.0,
        damping: 1.0,
        speed_limit_z: 999_999.0,
        taxi,
        taxi_speed: 30.0,
        rotor_wash: String::new(),
        upgraded_supply_boost: 0,
        drop: super::DropRules {
            rope_name: "GenericRope".into(),
            ropes: 4,
            width: 0.5,
            color: [0.9, 0.8, 0.7],
            wobble_length: 10.0,
            wobble_amplitude: 1.0,
            wobble_rate: 0.1,
            final_height: 0.0,
            drop_speed: 1e10,
            rappel_speed: (64.0_f32 / 900.0).abs() * 30.0 * 0.5,
            delay: [0x7fff_ffff; 2],
            min_height: 30.0,
            wait_for_ropes: true,
        },
    });
    unit.preferred_height = 100.0;
    unit.movement_surface = MovementSurface::Air;
    unit.locomotor.ground = None;
    unit.locomotor.legal_surfaces = 8;
    unit.locomotor.flight = Some(crate::skirmish::flight::tests::rules(
        crate::skirmish::flight::Appearance::Hover,
    ));
    unit.locomotor.airborne_targeting_height = 30.0;
    unit.contain.as_mut().unwrap().exit_frames = 3;
    unit.live_physics = Some(crate::skirmish::physical_ocl::LiveRules {
        motion: crate::skirmish::physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 50.0,
            gravity: -64.0 / 900.0,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
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
        shock_rotation: [0.0; 3],
        min_fall_speed: 99.0,
        fall_damage_factor: 1.0,
    });
    unit.ai.update = crate::skirmish::UnitAiUpdate::SupplyTruck;
    for entry in &mut unit.update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } = entry
            && family == "AIUpdateInterface"
        {
            *family = "ChinookAIUpdate".into();
        }
    }
    for entry in &mut unit.behavior_modules {
        if entry.family == "AIUpdateInterface" {
            entry.family = "ChinookAIUpdate".into();
        }
    }
    crate::skirmish::tests::register_update(unit, "PhysicsBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces = 0x11;
    let host = crate::skirmish::containment::tests::respawn_contain_family(
        &mut game,
        id,
        "TransportContain",
    );
    game.set_object_position(host, [350.0, 335.0, 100.0]);
    (game, host)
}

fn run_until(game: &mut Skirmish, predicate: impl Fn(&Skirmish) -> bool) {
    for _ in 0..1800 {
        if predicate(game) {
            return;
        }
        game.step();
    }
    panic!(
        "source Chinook transition did not finish at frame {}",
        game.tick
    );
}

#[test]
fn boarding_waits_for_vertical_contact_loses_boxes_and_takes_off_after_last_rider() {
    let (mut game, host) = aircraft();
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.entities[host].cargo = 8;
    game.enter(rider, host).unwrap();
    game.step();
    assert!(game.passengers(host).is_empty());
    assert_eq!(game.entities[host].cargo, 0);
    assert!(matches!(
        game.entities[host].chinook.as_ref().unwrap().flight,
        Flight::Landing(_)
    ));
    run_until(&mut game, |game| {
        game.entities[rider].contained_by() == Some(host)
    });
    assert!(
        game.entities[host].flight_height().unwrap() <= game.catalog[5].geometry_bounds.max_height
    );
    assert!(game.entities[rider].masked());
    run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            Flight::Flying
        )
    });
    assert!(game.entities[host].flight_height().unwrap() >= 97.0);
}

#[test]
fn evacuation_lands_then_uses_separate_source_exit_delays() {
    let (mut game, host) = aircraft();
    let first = game.spawn(5, 0, [355.0, 325.0], true);
    let second = game.spawn(5, 0, [380.0, 325.0], true);
    for id in [first, second] {
        game.enter(id, host).unwrap();
    }
    run_until(&mut game, |game| {
        game.passengers(host).len() == 2
            && matches!(
                game.entities[host].chinook.as_ref().unwrap().flight,
                Flight::Flying
            )
    });
    game.evacuate(host).unwrap();
    assert_eq!(game.passengers(host).len(), 2);
    run_until(&mut game, |game| game.passengers(host).len() == 1);
    let first_exit = game.tick;
    assert!(game.entities[host].landed());
    run_until(&mut game, |game| game.passengers(host).is_empty());
    assert!(game.tick >= first_exit + 3);
    assert!(
        game.entities[first].contained_by().is_none()
            && game.entities[second].contained_by().is_none()
    );
    run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            Flight::Flying
        )
    });
}

#[test]
fn latest_move_during_landing_waits_for_landing_then_takeoff() {
    let (mut game, host) = aircraft();
    let rider = game.spawn(5, 0, [355.0, 450.0], true);
    game.enter(rider, host).unwrap();
    game.step();
    game.move_to(host, [600.0, 200.0]).unwrap();
    game.move_to(host, [500.0, 400.0]).unwrap();
    run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            Flight::TakingOff(_)
        )
    });
    assert!(game.entities[host].flight_height().unwrap() <= 3.0);
    run_until(&mut game, |game| {
        matches!(game.entities[host].ai.order, Order::Idle)
            && game.entities[host].position[0] > 490.0
    });
    // This fixture's navigation cells are one world unit wide. The native
    // destination is its adjusted center, not the ten-unit installed-map cell.
    let expected = game.path_position(host, game.cell_at([500.0, 400.0]).unwrap());
    assert!((game.entities[host].position[1] - expected[1]).abs() < 1.0);
    assert_eq!(
        game.entities[host].ai_goal_position().map(f32::to_bits),
        [500.0_f32, 400.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn pending_entry_cancellation_does_not_interrupt_landing_and_air_death_kills_contents() {
    let (mut game, host) = aircraft();
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.enter(rider, host).unwrap();
    game.step();
    game.stop(rider).unwrap();
    run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            Flight::TakingOff(_)
        )
    });
    run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            Flight::Flying
        )
    });
    game.enter(rider, host).unwrap();
    run_until(&mut game, |game| {
        game.entities[rider].contained_by().is_some()
    });
    run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            Flight::Flying
        )
    });
    game.apply_hit(host, rider, 999_999.0);
    assert!(!game.entities[rider].alive());
}

#[test]
fn group_evacuate_during_landing_defers_until_the_current_transition_finishes() {
    let (mut game, host) = aircraft();
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.enter(rider, host).unwrap();
    game.step();
    let Flight::Landing(original) = game.entities[host].chinook.as_ref().unwrap().flight else {
        panic!("landing must begin");
    };
    game.group_evacuate(host).unwrap();
    assert!(!game.entities[host].ai_idle());
    game.step();
    let Flight::Landing(current) = game.entities[host].chinook.as_ref().unwrap().flight else {
        panic!("pending group command must not replace landing");
    };
    assert_eq!(current.map(f32::to_bits), original.map(f32::to_bits));
    assert!(
        game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .is_some_and(|command| matches!(
                command.kind,
                crate::skirmish::ai_dispatch::stored_command::Kind::MoveEvacuate { .. }
            ))
    );
    run_until(&mut game, |game| {
        game.entities[rider].contained_by().is_none() && game.entities[host].ai_idle()
    });
}

#[test]
fn single_passenger_exit_waits_for_moving_chinook_to_be_idle_and_land() {
    let (mut game, host) = aircraft();
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.enter(rider, host).unwrap();
    run_until(&mut game, |game| {
        game.entities[rider].contained_by().is_some()
            && matches!(
                game.entities[host].chinook.as_ref().unwrap().flight,
                Flight::Flying
            )
    });
    game.move_to(host, [650.0, 400.0]).unwrap();
    game.exit_container(rider).unwrap();
    game.step();
    assert_eq!(game.entities[rider].contained_by(), Some(host));
    assert!(matches!(
        game.entities[host].chinook.as_ref().unwrap().flight,
        Flight::Flying
    ));
    run_until(&mut game, |game| {
        game.entities[rider].contained_by().is_none()
    });
    assert!(game.entities[host].landed());
    assert!((game.entities[host].position[0] - 650.0).abs() < 50.0);
}

#[test]
fn group_enter_ignores_chinook_self_or_incompatible_target_without_cancelling_move() {
    let (mut game, host) = aircraft();
    let requested = [550.0, 300.0];
    game.move_to(host, requested).unwrap();
    // The public player command stores the native adjusted cell center.
    let goal = game.path_position(host, game.cell_at(requested).unwrap());
    game.begin_container_enter(host, host);
    assert!(
        matches!(game.entities[host].ai.order, Order::Move(point) if point.map(f32::to_bits) == goal.map(f32::to_bits)),
        "move goal {:?}, machine goal {:?}, pending {}",
        match game.entities[host].ai.order {
            Order::Move(point) => Some(point),
            _ => None,
        },
        game.entities[host].ai_goal_position(),
        game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .pending
            .is_some()
    );
    let ranger = game.spawn(5, 0, [355.0, 335.0], true);
    game.begin_container_enter(host, ranger);
    assert!(
        matches!(game.entities[host].ai.order, Order::Move(point) if point.map(f32::to_bits) == goal.map(f32::to_bits)),
        "move goal {:?}, machine goal {:?}, pending {}",
        match game.entities[host].ai.order {
            Order::Move(point) => Some(point),
            _ => None,
        },
        game.entities[host].ai_goal_position(),
        game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .pending
            .is_some()
    );
    assert!(game.entities[host].containment.waiting.is_empty());
}

#[test]
fn group_evacuation_move_is_interruptible_and_landing_retains_a_replacement_command() {
    let (mut game, host) = aircraft();
    game.group_evacuate(host).unwrap();
    assert!(matches!(
        game.entities[host].chinook.as_ref().unwrap().phase,
        Some(Phase::MoveEvac)
    ));
    // Selected PRESERVE_CHINOOK_PASSENGER_DUMPING uses ordinary unlocked MoveTo.
    game.stop(host).unwrap();
    assert!(
        game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .phase
            .is_none()
    );
    game.group_evacuate(host).unwrap();
    run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            Flight::Landing(_)
        )
    });
    assert!(matches!(
        game.entities[host].chinook.as_ref().unwrap().flight,
        Flight::Landing(_)
    ));
    game.move_to(host, [550.0, 400.0]).unwrap();
    assert!(
        game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .pending
            .is_some()
    );
}

#[test]
fn taxiing_claims_ground_cells_and_takeoff_releases_them() {
    let (mut game, host) = aircraft();
    // Keep the waiting passenger outside boarding contact until landing finishes.
    let rider = game.spawn(5, 0, [700.0, 700.0], true);
    game.enter(rider, host).unwrap();
    run_until(&mut game, |game| game.entities[host].landed());
    let cell = game
        .movement_cell(host, game.entities[host].position)
        .unwrap();
    assert_eq!(
        game.occupancy
            .position_owner_on(crate::grid::LayerCell { cell, layer: 1 }),
        Some(host)
    );
    assert!(game.occupancy.goal_cell(host) == Some(cell));
    assert!(
        game.entities[rider].alive(),
        "rider died at frame {} health {} last attacker {:?} XYZ {:?}, host XYZ {:?}, rider goal {:?}",
        game.tick,
        game.entities[rider].health,
        game.entities[rider].damage_state.last_attacker,
        game.object_position(rider),
        game.object_position(host),
        game.entities[rider].ai.command.goal_object
    );
    assert!(game.entities[rider].contained_by().is_none());
    game.stop(rider).unwrap();
    game.step();
    assert!(matches!(
        game.entities[host].chinook.as_ref().unwrap().flight,
        Flight::TakingOff(_)
    ));
    assert_ne!(
        game.occupancy
            .position_owner_on(crate::grid::LayerCell { cell, layer: 1 }),
        Some(host)
    );
    // Takeoff releases UNIT_GOAL on the ground, but its explicit locomotor
    // goal is retained separately by Pathfinder::updateAircraftGoal.
    assert!(!game.occupancy.landing_claimed(cell));
    assert!(game.occupancy.goal_cell(host) == Some(cell));
}

#[test]
fn invalid_enter_replaces_pending_move_before_rejection_after_flight_transition() {
    for landing in [true, false] {
        for self_entry in [true, false] {
            let (mut game, host) = aircraft();
            let ranger = game.spawn(5, 0, [450.0, 450.0], true);
            if !landing {
                game.set_object_position(host, [350.0, 335.0, 0.0]);
            }
            game.set_chinook_state(
                host,
                if landing { Phase::Land } else { Phase::Takeoff },
                None,
                None,
                crate::skirmish::ai_goals::COMMAND_FROM_AI,
            );
            game.move_to(host, [650.0, 300.0]).unwrap();
            let target = if self_entry { host } else { ranger };
            game.begin_container_enter(host, target);
            assert!(
                matches!(game.entities[host].chinook.as_ref().unwrap().pending.as_ref().map(|command| &command.kind), Some(crate::skirmish::ai_dispatch::stored_command::Kind::Enter(id)) if *id == target)
            );
            assert!(
                game.entities[host]
                    .chinook
                    .as_ref()
                    .unwrap()
                    .pending
                    .is_some()
            );
            assert!(game.entities[host].containment.waiting.is_empty());
            run_until(&mut game, |game| {
                let state = game.entities[host].chinook.as_ref().unwrap();
                !state.transitioning() && state.pending.is_none()
            });
            assert!(matches!(game.entities[host].ai.order, Order::Idle));
            assert!(
                game.entities[host]
                    .chinook
                    .as_ref()
                    .unwrap()
                    .pending
                    .is_none()
            );
            assert!(game.entities[host].ai.ignored_producer.is_none());
            assert!(game.entities[host].position[0] < 450.0);
        }
    }
}

#[test]
fn rising_terrain_clips_chinook_after_horizontal_physics_and_updates_flight_height() {
    let (mut game, id) = aircraft();
    game.set_projectile_terrain(
        5,
        5,
        [90.0, 90.0],
        [0.0, 0.0, 50.0, 50.0, 50.0].repeat(5),
        10.0,
    )
    .unwrap();
    game.set_object_position(id, [100.0, 100.0, 1.0]);
    game.set_object_orientation(id, 0.0);
    game.entities[id].set_aircraft_physics([100.0, 100.0, 1.0], [2.0, 0.0, 0.0]);
    game.update_flight(id);
    game.update_object_physics(id);
    let actor = &game.entities[id];
    let surface = game.ground_height(actor.position);
    assert!(surface > 1.0);
    assert_eq!(
        actor.chinook.as_ref().unwrap().height.to_bits(),
        surface.to_bits()
    );
    assert_eq!(actor.chinook.as_ref().unwrap().velocity.to_bits(), 0);
    assert_eq!(
        actor.flight.as_ref().unwrap().height.to_bits(),
        surface.to_bits()
    );
}

#[test]
fn falling_terrain_does_not_clip_chinook_to_the_old_surface_or_discard_descent() {
    let (mut game, id) = aircraft();
    game.set_projectile_terrain(
        5,
        5,
        [90.0, 90.0],
        [50.0, 50.0, 0.0, 0.0, 0.0].repeat(5),
        10.0,
    )
    .unwrap();
    game.set_object_position(id, [100.0, 100.0, 50.1]);
    game.set_object_orientation(id, 0.0);
    game.entities[id].set_aircraft_physics([100.0, 100.0, 50.1], [2.0, 0.0, -0.2]);
    let actor = &game.entities[id];
    let state = actor.chinook.as_ref().unwrap();
    let velocity = state.velocity
        + game.catalog[actor.type_index]
            .chinook
            .as_ref()
            .unwrap()
            .gravity;
    let height = state.height + velocity;
    game.update_flight(id);
    game.update_object_physics(id);
    let actor = &game.entities[id];
    assert!(game.ground_height(actor.position) < height && height < 50.0);
    assert_eq!(
        actor.chinook.as_ref().unwrap().height.to_bits(),
        height.to_bits()
    );
    assert_eq!(
        actor.chinook.as_ref().unwrap().velocity.to_bits(),
        velocity.to_bits()
    );
    assert_eq!(
        actor.flight.as_ref().unwrap().height.to_bits(),
        height.to_bits()
    );
}

#[test]
fn landed_taxi_escapes_water_using_ground_surfaces_instead_of_default_air_surfaces() {
    use crate::skirmish::locomotor::{GroundAppearance, GroundGoal, GroundRules};

    let (mut game, id) = aircraft();
    game.movement_terrain = crate::terrain::TerrainMap::classify(
        80,
        80,
        [],
        (9..=11).flat_map(|y| (9..=10).map(move |x| crate::Cell { x, y })),
    )
    .unwrap();
    game.set_object_position(id, [105.0, 105.0, 0.0]);
    game.set_object_orientation(id, 0.0);
    let actor = &mut game.entities[id];
    let state = actor.chinook.as_mut().unwrap();
    state.flight = Flight::Landed;
    state.height = 0.0;
    let taxi = &mut game.catalog[actor.type_index]
        .chinook
        .as_mut()
        .unwrap()
        .taxi;
    taxi.legal_surfaces = 1;
    taxi.ground = Some(GroundRules {
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
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    game.choose_locomotor_set(id, crate::skirmish::locomotor::set::TAXI);
    game.entities[id].ai.motion.request_path(GroundGoal {
        height: 0.0,
        position: [55.0, 105.0],
        distance: 50.0,
        desired_speed: 1.0,
        no_slow_down: false,
        ultra_accurate: false,
    });
    game.update_ground_locomotor(id);
    game.update_object_physics(id);
    let actor = &game.entities[id];
    // Native fixInvalidPosition pushes away from the three water cells at x-10.
    assert!((actor.ai.motion.velocity[0] - 0.6).abs() < 0.000_001);
    assert_eq!(actor.ai.motion.velocity[1].to_bits(), 0);
    assert_eq!(actor.facing.to_bits(), 0);
}

#[test]
fn grounded_chinook_shock_uses_one_vertical_integrator_and_preserves_idle_lift() {
    let (mut game, id) = aircraft();
    let kind = game.entities[id].type_index;
    let gravity = game.catalog[kind].chinook.as_ref().unwrap().gravity;
    game.set_object_position(id, [355.0, 255.0, 0.0]);
    game.entities[id].set_aircraft_physics([355.0, 255.0, 0.0], [0.0, 0.0, 0.25]);
    let mass = game.object_physics_mass(id).unwrap();
    game.apply_damage_shock(id, [5.0, 0.0, 0.0], mass, 50.0, 1.0);
    game.update_chinook_lift(id);
    let lift = game.entities[id].chinook.as_ref().unwrap().acceleration;
    game.integrate_flight(id);
    game.integrate_chinook(id);
    game.clip_chinook(id);
    assert_eq!(
        game.entities[id].chinook.as_ref().unwrap().height.to_bits(),
        0.0_f32.to_bits()
    );
    game.integrate_live_physics(id);
    let expected = 0.25_f32 + (1.0 + lift + gravity);
    assert!((game.entities[id].physics_velocity()[2] - expected).abs() < 0.000_001);
    assert_eq!(
        game.entities[id]
            .chinook
            .as_ref()
            .unwrap()
            .velocity
            .to_bits(),
        game.entities[id].physics_velocity()[2].to_bits()
    );
    let first = game.object_position(id);
    game.integrate_live_physics(id);
    assert_eq!(
        game.object_position(id).map(f32::to_bits),
        first.map(f32::to_bits)
    );
    game.tick += 1;
    game.update_chinook_lift(id);
    assert!(game.entities[id].chinook.as_ref().unwrap().acceleration > 0.0);
    game.integrate_chinook(id);
    game.clip_chinook(id);
    game.integrate_live_physics(id);
    assert!(game.object_position(id)[2] > first[2]);
}

#[test]
fn landing_retains_full_script_path_and_factory_exit_does_not_force_takeoff() {
    use crate::skirmish::ai_dispatch::stored_command::Kind;
    let (mut game, host) = aircraft();
    let points = vec![[450.0, 335.0, 64.0], [600.0, 355.0, 83.0]];
    game.set_chinook_state(host, Phase::Land, None, None, 2);
    game.native_follow_path_from(host, points.clone(), Some(0), 1, false);
    let command = game.entities[host]
        .chinook
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap();
    assert_eq!(command.source, 1);
    let Kind::FollowPath {
        points: stored,
        ignore,
        exit_production,
    } = &command.kind
    else {
        panic!("retained native path");
    };
    assert_eq!(
        stored
            .iter()
            .map(|p| p.map(f32::to_bits))
            .collect::<Vec<_>>(),
        points
            .iter()
            .map(|p| p.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert_eq!(*ignore, Some(0));
    assert!(!exit_production);
    assert_eq!(game.entities[host].ai.command.last_command_source, 2);
    run_until(&mut game, |game| {
        matches!(game.entities[host].ai.order, Order::FollowPath)
    });
    assert_eq!(game.entities[host].ai.command.last_command_source, 1);
    assert_eq!(
        game.entities[host].ai.waypoints.points[1].map(f32::to_bits),
        points[1].map(f32::to_bits)
    );
    assert!(matches!(
        game.entities[host].chinook.as_ref().unwrap().flight,
        Flight::Flying
    ));

    game.set_chinook_state(host, Phase::Land, None, None, 2);
    run_until(&mut game, |game| game.entities[host].landed());
    game.native_follow_path_from(host, vec![[400.0, 335.0, 0.0]], Some(0), 2, true);
    assert!(matches!(game.entities[host].ai.order, Order::Exit(_)));
    assert!(
        game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .pending
            .is_none()
    );
    assert!(game.entities[host].landed());
}

#[test]
fn combat_drop_uses_model_bones_and_defers_stop_until_ropes_release() {
    use crate::skirmish::{ai_goals::COMMAND_FROM_AI, model_bones};
    use std::collections::{BTreeMap, BTreeSet};
    let (mut game, host) = aircraft();
    let kind = game.entities[host].type_index;
    let rules = &mut game.catalog[kind].chinook.as_mut().unwrap().drop;
    rules.delay = [1, 1];
    rules.ropes = 1;
    let mut transforms = BTreeMap::new();
    transforms.insert(
        "ropestart01".into(),
        [
            [1.0, 0.0, 0.0, 4.0],
            [0.0, 1.0, 0.0, 6.0],
            [0.0, 0.0, 1.0, 8.0],
        ],
    );
    transforms.insert(
        "ropeend01".into(),
        [
            [1.0, 0.0, 0.0, 4.0],
            [0.0, 1.0, 0.0, 6.0],
            [0.0, 0.0, 1.0, 2.0],
        ],
    );
    game.catalog[kind].pristine_draws.push(model_bones::Draw {
        states: vec![model_bones::State {
            conditions: vec![BTreeSet::new()],
            transforms,
        }],
        ignored: BTreeSet::new(),
    });
    let position = [350.0, 335.0, 100.0];
    game.set_object_transform(
        host,
        position,
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
    );
    game.set_chinook_state(
        host,
        Phase::CombatDrop,
        None,
        Some([350.0, 335.0, 0.0]),
        COMMAND_FROM_AI,
    );
    assert!(game.entities[host].disabled_held());
    assert_eq!(game.rope_drawables.drawables.len(), 1);
    let rope = &game.rope_drawables.drawables[0];
    assert_eq!(
        rope.position.map(f32::to_bits),
        [344.0_f32, 339.0, 108.0].map(f32::to_bits)
    );
    assert_eq!(rope.length.to_bits(), 0);
    assert!(matches!(
        game.presentation.weapon_fx.last(),
        Some(crate::skirmish::WeaponFxEvent::Chinook(
            super::VisualEvent::RopeCreated(0)
        ))
    ));
    game.native_idle_from(host, COMMAND_FROM_AI);
    // aiDoCommand retains STOP during DOING_COMBAT_DROP, just like landing.
    assert!(game.entities[host].disabled_held());
    assert!(matches!(
        game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .map(|command| &command.kind),
        Some(crate::skirmish::ai_dispatch::stored_command::Kind::Idle)
    ));
    run_until(&mut game, |game| !game.entities[host].disabled_held());
    assert!(!game.entities[host].disabled_held());
    assert!(matches!(
        game.entities[host].chinook.as_ref().unwrap().flight,
        Flight::Flying
    ));
    let release = game.rope_drawables.drawables[0].released.as_ref().unwrap();
    assert_eq!(release.expires, game.update_frame() + 150);
    assert_eq!(release.speed.to_bits(), (game.gravity * 30.0).to_bits());
}

#[test]
fn missing_rope_bone_fails_drop_and_cleans_held_state() {
    let (mut game, host) = aircraft();
    game.set_chinook_state(host, Phase::CombatDrop, None, Some([350.0, 335.0, 0.0]), 2);
    assert!(!game.entities[host].disabled_held());
    assert!(
        game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .phase
            .is_none()
    );
    assert!(game.rope_drawables.drawables.is_empty());
}
