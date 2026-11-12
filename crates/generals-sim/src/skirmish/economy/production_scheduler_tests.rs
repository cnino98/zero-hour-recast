use crate::skirmish::{
    Kind, ProductionItem, Skirmish, maintenance::Research, module_updates::ModuleRegistration,
    parking::DoorRules, tests::game,
};

fn factory(game: &mut Skirmish, doors: DoorRules, disabled: u32) -> (usize, usize) {
    let index = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[index].production_doors = doors;
    game.catalog[index].production_disabled_types = disabled;
    game.catalog[index].energy = 1;
    let producer = game.spawn(index, 0, [350.0, 250.0], true);
    game.update_power();
    let registration = game.catalog[index].update_modules.iter().position(|entry| {
        matches!(entry, ModuleRegistration::Behavior { family, .. } if family == "ProductionUpdate")
    }).unwrap();
    (producer, registration)
}

#[test]
fn ordinary_step_produces_once_and_idle_callback_delivers_cancelled_flags() {
    let mut game = game();
    let (producer, registration) = factory(&mut game, DoorRules::NONE, 1 << 3);
    game.catalog[1].build_seconds = 4.0 / 30.0;
    game.train(producer, 1).unwrap();
    game.step();
    assert_eq!(
        game.entities[producer].queue[0].elapsed_frames.to_bits(),
        1.0_f32.to_bits()
    );
    assert!(game.entities[producer].actively_constructing());
    assert_eq!(game.update_remaining_sleep(producer, registration), 1);
    let pending = game.entities[producer].queue[0].id;
    game.cancel_production(0, producer, pending).unwrap();
    assert!(game.entities[producer].actively_constructing());
    game.step();
    assert!(!game.entities[producer].actively_constructing());
    // Idle ProductionUpdate never sleeps: a later paid item runs next frame.
    game.train(producer, 1).unwrap();
    game.step();
    assert_eq!(
        game.entities[producer].queue[0].elapsed_frames.to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn ground_door_uses_strict_open_wait_close_edges_and_dirty_flags() {
    let mut game = game();
    game.catalog[1].build_seconds = 1.0 / 30.0;
    let (producer, _) = factory(
        &mut game,
        DoorRules {
            count: 1,
            opening: 2,
            waiting: 1,
            closing: 1,
            complete: 2,
        },
        1 << 3,
    );
    let cash = game.players[0].cash;
    let cost = game.build_cost(0, 1);
    let objects = game.entities.len();
    game.train(producer, 1).unwrap();
    for tick in 1..=3 {
        game.tick = tick;
        game.run_object_updates();
        assert_eq!(game.entities.len(), objects);
        assert_eq!(game.entities[producer].queue.len(), 1);
    }
    assert!(
        crate::skirmish::parking::doors::condition_names(
            game.entities[producer].production_door_flags
        )
        .contains(&"DOOR_1_OPENING")
    );
    game.tick = 4;
    game.run_object_updates();
    assert_eq!(game.players[0].cash, cash - cost);
    assert!(game.entities[producer].queue.is_empty());
    assert_eq!(game.entities.len(), objects + 1);
    assert_eq!(game.entities[objects].producer, Some(producer));
    game.tick = 5;
    game.run_object_updates();
    assert!(
        crate::skirmish::parking::doors::condition_names(
            game.entities[producer].production_door_flags
        )
        .contains(&"DOOR_1_WAITING_OPEN")
    );
    for tick in [6, 7] {
        game.tick = tick;
        game.run_object_updates();
        assert!(
            crate::skirmish::parking::doors::condition_names(
                game.entities[producer].production_door_flags
            )
            .contains(&"DOOR_1_CLOSING")
        );
    }
    game.tick = 8;
    game.run_object_updates();
    assert!(
        crate::skirmish::parking::doors::condition_names(
            game.entities[producer].production_door_flags
        )
        .is_empty()
    );
}

#[test]
fn disabled_queue_keeps_polling_and_held_uses_the_authored_accepted_mask() {
    let mut game = game();
    let (producer, registration) = factory(&mut game, DoorRules::NONE, 1 << 3);
    game.catalog[1].build_seconds = 4.0 / 30.0;
    game.train(producer, 1).unwrap();
    game.set_hacked_until(producer, crate::skirmish::ai_dispatch::native_frame(100));
    for tick in 1..=3 {
        game.tick = tick;
        game.run_object_updates();
        assert_eq!(
            game.entities[producer].queue[0].elapsed_frames.to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(game.update_remaining_sleep(producer, registration), 1);
    }
    game.clear_hacked_disable(producer);
    game.set_disabled_held(producer);
    game.tick = 4;
    game.run_object_updates();
    assert_eq!(
        game.entities[producer].queue[0].elapsed_frames.to_bits(),
        1.0_f32.to_bits()
    );
    let (blocked, _) = factory(&mut game, DoorRules::NONE, 0);
    game.train(blocked, 1).unwrap();
    game.set_disabled_held(blocked);
    game.tick = 5;
    game.run_object_updates();
    assert_eq!(
        game.entities[blocked].queue[0].elapsed_frames.to_bits(),
        0.0_f32.to_bits()
    );
    game.clear_disabled_held(blocked);
    game.tick = 6;
    game.run_object_updates();
    assert_eq!(
        game.entities[blocked].queue[0].elapsed_frames.to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn research_shares_dispatch_and_producer_death_refunds_only_remaining_work() {
    let mut game = game();
    let (producer, _) = factory(&mut game, DoorRules::NONE, 1 << 3);
    let type_index = game.entities[producer].type_index;
    game.catalog[type_index]
        .maintenance
        .research
        .push(Research {
            name: "ScheduledResearch".into(),
            cost: 100,
            frames: 1,
            player: true,
        });
    let cash = game.players[0].cash;
    game.queue_upgrade(producer, 0).unwrap();
    game.train(producer, 1).unwrap();
    game.tick = 1;
    game.run_object_updates();
    assert!(game.has_upgrade(0, "ScheduledResearch"));
    assert!(matches!(
        game.entities[producer].queue[0].item,
        ProductionItem::Unit(1)
    ));
    assert_eq!(
        game.entities[producer].queue[0].elapsed_frames.to_bits(),
        0.0_f32.to_bits()
    );
    game.apply_native_damage(producer, None, 10000.0, crate::skirmish::DeathType::Normal);
    assert!(game.entities[producer].queue.is_empty());
    assert_eq!(game.players[0].cash, cash - 100);
    let objects = game.entities.len();
    game.tick = 2;
    game.run_object_updates();
    assert_eq!(game.entities.len(), objects);
}
