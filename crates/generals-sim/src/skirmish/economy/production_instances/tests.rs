use crate::skirmish::{
    Kind, Production, ProductionItem, ProductionRules, Skirmish, maintenance::Research,
    parking::DoorRules, tests,
};

fn factory() -> (Skirmish, usize) {
    let mut game = tests::game();
    let kind = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[kind].energy = 1;
    game.catalog[kind].maintenance.research.push(Research {
        name: "IndependentProduction".into(),
        cost: 100,
        frames: 6,
        player: false,
        sciences: Vec::new(),
        ignores_underpowered: false,
    });
    game.catalog[kind]
        .additional_production
        .push(ProductionRules {
            quantities: Vec::new(),
            queue_limit: 9,
            disabled_types: 1 << 3,
            doors: DoorRules {
                count: 1,
                opening: 2,
                waiting: 3,
                closing: 4,
                complete: 2,
            },
        });
    tests::register_update(&mut game.catalog[kind], "ProductionUpdate");
    let id = game.spawn(kind, 0, [350.0, 250.0], true);
    game.update_power();
    (game, id)
}

#[test]
fn paid_first_interface_research_advances_once_with_independent_empty_second_instance() {
    let (mut game, id) = factory();
    let cash = game.players[0].cash;
    game.queue_upgrade(id, 0).unwrap();
    assert_eq!(game.players[0].cash, cash - 100);
    assert_eq!(game.entities[id].production_next_id(1), 1);
    for elapsed in 1_u16..6 {
        game.step();
        assert_eq!(
            game.entities[id].queue[0].elapsed_frames.to_bits(),
            f32::from(elapsed).to_bits()
        );
        assert!(game.entities[id].production_queue_at(1).is_empty());
        assert!(game.entities[id].actively_constructing());
    }
    game.step();
    assert!(game.entities[id].queue.is_empty());
    assert!(game.object_has_upgrade(id, "IndependentProduction"));
    game.step();
    assert!(!game.entities[id].actively_constructing());
}

#[test]
fn empty_second_instance_advances_its_own_timers_and_only_delivers_its_dirty_flags() {
    let (mut game, id) = factory();
    game.tick = 1;
    game.entities[id]
        .production_doors_at_mut(1)
        .set_hold(0, true, 1);
    game.entities[id].production_added_at(0);
    game.update_production_instance(id, 0);
    game.update_production_instance(id, 1);
    assert!(game.entities[id].actively_constructing());
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("DOOR_1_OPENING")
    );
    assert_eq!(
        game.entities[id]
            .production_doors
            .update_doors(&DoorRules::NONE, 1),
        (0, 0)
    );
    game.tick = 3;
    game.update_production_instance(id, 1);
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("DOOR_1_OPENING")
    );
    game.tick = 4;
    game.update_production_instance(id, 1);
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("DOOR_1_WAITING_OPEN")
    );
    game.entities[id].production_removed_at(1);
    game.update_production_instance(id, 0);
    assert!(game.entities[id].actively_constructing());
    game.update_production_instance(id, 1);
    assert!(!game.entities[id].actively_constructing());
}

#[test]
fn module_death_refunds_its_own_queue_while_destructor_only_removes_entries() {
    let (mut game, id) = factory();
    let cash = game.players[0].cash;
    game.queue_upgrade(id, 0).unwrap();
    // Exercise the second concrete native module's queue callbacks directly;
    // ordinary public commands above deliberately address only the first.
    game.entities[id]
        .production_queue_at_mut(1)
        .push_back(Production {
            quantity: 1,
            produced: 0,
            parking_door: None,
            id: 1,
            item: ProductionItem::Upgrade(0),
            remaining_seconds: 0.2,
            total_seconds: 0.2,
            elapsed_frames: 0.0,
        });
    game.players[0].cash -= 100;
    game.refund_production_instance(id, 1);
    assert_eq!(game.players[0].cash, cash - 100);
    assert_eq!(game.entities[id].queue.len(), 1);
    assert!(game.entities[id].production_queue_at(1).is_empty());
    game.destroy_maintenance_object(id);
    game.process_update_destruction();
    assert_eq!(game.players[0].cash, cash - 100);
    assert!(game.entities[id].queue.is_empty());
}

#[test]
fn actual_death_visits_each_production_module_once_instead_of_refunding_first_twice() {
    let (mut game, id) = factory();
    let cash = game.players[0].cash;
    game.queue_upgrade(id, 0).unwrap();
    game.entities[id]
        .production_queue_at_mut(1)
        .push_back(Production {
            quantity: 1,
            produced: 0,
            parking_door: None,
            id: 1,
            item: ProductionItem::Upgrade(0),
            remaining_seconds: 0.2,
            total_seconds: 0.2,
            elapsed_frames: 0.0,
        });
    game.players[0].cash -= 100;
    game.kill_object(id, crate::skirmish::DeathType::Normal);
    assert_eq!(game.players[0].cash, cash);
    assert!(game.entities[id].queue.is_empty());
    assert!(game.entities[id].production_queue_at(1).is_empty());
}
