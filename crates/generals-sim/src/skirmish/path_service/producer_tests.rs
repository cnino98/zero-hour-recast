//! Actual producer callbacks; native heap/FIFO comparisons remain separate evidence.
use crate::skirmish::{
    GroundAppearance, GroundRules, Kind, Order, Skirmish, UnitAi,
    module_updates::{Helper, ModuleRegistration},
    physical_ocl::{CollisionRules, LiveRules, MotionRules},
    startup,
};

fn producers() -> (Skirmish, usize, usize, usize) {
    let mut game = crate::skirmish::tests::game();
    game.clear_match_state();
    game.initialize_map_side_players().unwrap();
    let unit = game.type_for(0, Kind::Tank).unwrap();
    let tank = &mut game.catalog[unit];
    tank.ai.acquisition &= !UnitAi::IDLE;
    tank.build_seconds = 1.0;
    tank.locomotor.ground = Some(GroundRules {
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
    tank.live_physics = Some(LiveRules {
        motion: MotionRules {
            center_of_mass_offset: 0.0,
            mass: 50.0,
            gravity: -0.1,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.25,
        kill_when_resting: false,
        collision: CollisionRules {
            stiffness: 0.5,
            rubble_height: 0.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.0; 3],
        min_fall_speed: 99.0,
        fall_damage_factor: 1.0,
    });
    crate::skirmish::tests::register_update(tank, "PhysicsBehavior");
    let building = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[building].energy = 1;
    let factory = game.spawn(building, 0, [305.0, 405.0], true);
    let mover = game.spawn(unit, 0, [105.0, 105.0], true);
    game.update_power();
    (game, factory, mover, unit)
}

fn registration(game: &Skirmish, id: usize, family: &str) -> usize {
    game.catalog[game.entities[id].type_index]
        .update_modules
        .iter()
        .position(|entry| matches!(entry, ModuleRegistration::Behavior { family: found, .. } if found == family))
        .unwrap()
}

#[test]
fn real_ai_requeue_precedes_production_exit_in_late_service() {
    let (mut game, factory, mover, unit) = producers();
    for _ in 0..3 {
        game.step();
    }
    game.move_to(mover, [605.0, 105.0]).unwrap();
    game.step();
    assert_eq!(game.entities[mover].ai.blocking.path_frame, 4);
    assert!(game.entities[mover].ai.ground_path.is_some());
    game.move_to(mover, [605.0, 205.0]).unwrap();
    assert_eq!(game.entities[mover].ai.path_request.queue_at, Some(34));
    assert_eq!(game.path_service.head, game.path_service.tail);
    game.do_painted_status_damage(mover, 30.0);
    let before_payment = game.players[0].cash;
    let price = game.build_cost(0, unit);
    game.train(factory, unit).unwrap();
    assert_eq!(game.players[0].cash, before_payment - price);
    let child = game.entities.len();
    while game.tick < 33 {
        game.step();
    }
    assert_eq!(game.entities.len(), child);
    assert!(game.entities[mover].painted());
    assert_eq!(game.entities[mover].ai.path_request.queue_at, Some(34));
    assert_eq!(game.path_service.head, game.path_service.tail);
    let mut expected_random = game.random;
    // Locomotor.cpp673–675 initializes these even for TREADS.
    let _ = startup::random_real(
        &mut expected_random,
        -std::f32::consts::PI / 6.0,
        std::f32::consts::PI / 6.0,
    );
    let _ = startup::random_real(&mut expected_random, 0.8, 1.2);
    let _ = startup::random_between(&mut expected_random, 1);
    let mut child_idle_offset = 0;
    // AIStates.cpp691–693 constructs three nested Idle states, then
    // AIUpdate.cpp621–630 initializes primary Idle, before the exit command.
    for _ in 0..4 {
        child_idle_offset = startup::random_between(&mut expected_random, 60);
    }
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.entities.len(), child + 1);
    assert_eq!(game.entities[child].producer, Some(factory));
    assert!(game.entities[factory].queue.is_empty());
    assert_eq!(game.random, expected_random);
    assert_eq!(game.entities[child].ai.idle.sleep_offset, child_idle_offset);
    assert!(game.entities[child].ai.primary.initialized);
    assert!(!game.entities[mover].painted());
    assert!(game.entities[mover].ai.path_request.queue_at.is_none());
    let head = game.path_service.head;
    assert_eq!(game.path_service.tail, (head + 2) % 512);
    assert_eq!(game.path_service.entries[head], Some(mover));
    assert_eq!(game.path_service.entries[(head + 1) % 512], Some(child));
    assert!(matches!(game.entities[child].ai.order, Order::Exit(_)));
    assert!(game.entities[child].ai.path.raw.is_empty());
    for family in ["AIUpdateInterface", "PhysicsBehavior"] {
        let ordinal = registration(&game, child, family);
        assert_eq!(game.update_remaining_sleep(child, ordinal), 1);
    }
    let weapon = game.catalog[unit]
        .update_modules
        .iter()
        .position(|entry| matches!(entry, ModuleRegistration::Helper(Helper::WeaponStatus)))
        .unwrap();
    assert_eq!(game.update_remaining_sleep(mover, weapon), 1);
    assert_eq!(game.update_remaining_sleep(child, weapon), 1);
    let birth = game.object_position(child);
    game.update_ground_contacts();
    game.process_path_service();
    assert_eq!(game.path_service.head, game.path_service.tail);
    for id in [mover, child] {
        assert!(game.entities[id].ai.path_request.pending.is_none());
        assert!(game.entities[id].ai.ground_path.is_some());
        assert_eq!(game.entities[id].ai.blocking.path_frame, 34);
    }
    game.step();
    assert!(game.object_position(child)[0] > birth[0]);
    assert!(game.entities[child].physics_velocity()[0] > 0.0);
}
