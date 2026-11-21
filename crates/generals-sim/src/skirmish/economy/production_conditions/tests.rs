use crate::skirmish::{Kind, QueueExit, maintenance::Research, tests::game};

#[test]
fn unit_and_research_share_the_flag_until_the_last_removal_is_delivered() {
    let mut game = game();
    // Energy::getEnergySupplyRatio needs positive supply for normal queue timing.
    game.catalog[0].energy = 1;
    game.update_power();
    game.catalog[1].build_seconds = 2.0 / 30.0;
    game.catalog[0].maintenance.research.push(Research {
        name: "QueuePresentation".into(),
        cost: 100,
        frames: 2,
        player: true,
        sciences: Vec::new(),
        ignores_underpowered: false,
    });
    game.train(0, 1).unwrap();
    game.queue_upgrade(0, 0).unwrap();
    assert!(!game.entities[0].actively_constructing());
    for remaining in [2, 1, 1, 0] {
        game.update_production(0);
        assert_eq!(game.entities[0].queue.len(), remaining);
        assert!(game.entities[0].actively_constructing());
        assert!(
            game.model_flags(&game.entities[0], [0.5, 0.1])
                .contains("ACTIVELY_CONSTRUCTING")
        );
    }
    assert!(game.has_upgrade(0, "QueuePresentation"));
    game.update_production(0);
    assert!(!game.entities[0].actively_constructing());
}

#[test]
fn cancellation_changes_only_the_final_queue_flag_on_the_next_update() {
    let mut game = game();
    game.catalog[0].maintenance.research.push(Research {
        name: "QueuePresentation".into(),
        cost: 100,
        frames: 20,
        player: true,
        sciences: Vec::new(),
        ignores_underpowered: false,
    });
    game.train(0, 1).unwrap();
    game.queue_upgrade(0, 0).unwrap();
    game.update_production(0);
    let production = game.entities[0].queue[0].id;
    game.cancel_production(0, 0, production).unwrap();
    game.update_production(0);
    assert!(game.entities[0].actively_constructing());
    game.cancel_upgrade(0, 0, 0).unwrap();
    assert!(game.entities[0].queue.is_empty());
    assert!(game.entities[0].actively_constructing());
    game.update_production(0);
    assert!(!game.entities[0].actively_constructing());
}

#[test]
fn blocked_completed_production_keeps_its_original_animation_until_it_exits() {
    let mut game = game();
    game.catalog[0].energy = 1;
    game.update_power();
    game.catalog[1].build_seconds = 1.0 / 30.0;
    game.catalog[0]
        .behavior
        .production_exit
        .as_mut()
        .unwrap()
        .queue = Some(QueueExit {
        allow_airborne_creation: false,
        delay_frames: 9,
        initial_burst: 0,
    });
    game.spawn_produced_unit(0, 1, false).unwrap();
    game.train(0, 1).unwrap();
    let objects = game.entities.len();
    for _ in 0..9 {
        game.update_production(0);
        assert_eq!(game.entities.len(), objects);
        assert_eq!(game.entities[0].queue.len(), 1);
        assert!(game.entities[0].actively_constructing());
        game.update_queue_exit(0);
    }
    game.update_production(0);
    assert_eq!(game.entities.len(), objects + 1);
    assert!(game.entities[0].queue.is_empty());
    assert!(game.entities[0].actively_constructing());
    game.update_production(0);
    assert!(!game.entities[0].actively_constructing());
}

#[test]
fn same_callback_enqueue_cancel_preserves_native_dirty_flag_order() {
    let mut game = game();
    game.train(0, 1).unwrap();
    let production = game.entities[0].queue[0].id;
    game.cancel_production(0, 0, production).unwrap();
    // Native removeFromProductionQueue sees the still-clear drawable bit and
    // does not erase the set bit awaiting the first ProductionUpdate callback.
    assert!(!game.entities[0].actively_constructing());
    game.update_production(0);
    assert!(game.entities[0].actively_constructing());
    game.train(0, 1).unwrap();
    let production = game.entities[0].queue[0].id;
    game.cancel_production(0, 0, production).unwrap();
    game.update_production(0);
    assert!(!game.entities[0].actively_constructing());
}

#[test]
fn removal_never_sets_a_flag_cleared_by_an_earlier_cancel_and_requeue() {
    let mut game = game();
    game.train(0, 1).unwrap();
    game.update_production(0);
    let production = game.entities[0].queue[0].id;
    game.cancel_production(0, 0, production).unwrap();
    for _ in 0..2 {
        game.train(0, 1).unwrap();
    }
    // The new entries saw the delivered set bit and did not replace the pending
    // clear. Removing a nonfinal entry must not invent a new set callback.
    game.update_production(0);
    assert!(!game.entities[0].actively_constructing());
    let production = game.entities[0].queue[0].id;
    game.cancel_production(0, 0, production).unwrap();
    game.update_production(0);
    assert_eq!(game.entities[0].queue.len(), 1);
    assert!(!game.entities[0].actively_constructing());
}

#[test]
fn builder_work_survives_empty_production_updates_and_stops_with_its_order() {
    let mut game = game();
    let worker = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    let building = game.type_for(0, Kind::Barracks).unwrap();
    let point = game.placement_near(building, [350.0, 250.0]).unwrap();
    // Native Dozer task timestamps use zero as the absent-command sentinel.
    game.step();
    game.build(worker, building, point).unwrap();
    for _ in 0..600 {
        game.step();
        if game.entities[worker].actively_constructing() {
            break;
        }
    }
    assert!(game.entities[worker].actively_constructing());
    game.update_production(worker);
    assert!(game.entities[worker].actively_constructing());
    game.stop(worker).unwrap();
    assert!(!game.entities[worker].actively_constructing());
}
