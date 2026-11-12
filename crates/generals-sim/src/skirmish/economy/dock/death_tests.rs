use crate::skirmish::{
    DeathType, Faction, Kind, Order, Skirmish,
    ai_goals::MainState,
    death_callbacks::Conditions,
    death_lifecycle::{Derived, Rules, SlowDeath},
    tests,
};

fn collector() -> (Skirmish, usize) {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::Gla, Kind::Harvester);
    unit.supply.capacity = 4;
    unit.kind_of.push("HARVESTER".into());
    tests::register_update(&mut unit, "SlowDeathBehavior");
    let module = unit.behavior_modules.last_mut().unwrap();
    module.interfaces |= 2;
    unit.death_lifecycle = Some(Rules {
        on_die: Vec::new(),
        slow: vec![SlowDeath {
            derived: Derived::None,
            tag: module.tag.clone(),
            conditions: Conditions {
                death_mask: u32::MAX,
                veterancy_mask: 15,
                exempt_status: 0,
                required_status: 0,
            },
            probability: 100,
            overkill_bonus: 0.0,
            sink_delay: [90, 0],
            destruction_delay: [90, 0],
            sink_rate: 0.0,
            fling: None,
            fx: std::array::from_fn(|_| Vec::new()),
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: std::array::from_fn(|_| Vec::new()),
        }],
        physics: None,
        weapons: Vec::new(),
    });
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, 0, [185.0, 255.0], true);
    game.update_visibility();
    (game, id)
}

#[test]
fn killed_collector_keeps_nested_dock_until_late_dead_then_releases_it() {
    for returning in [false, true] {
        let (mut game, id) = collector();
        let target = if returning {
            let kind = game.type_for(0, Kind::SupplyCenter).unwrap();
            let center = game.spawn(kind, 0, [300.0, 200.0], true);
            game.entities[id].cargo = 2;
            game.update_visibility();
            game.deliver_supplies(id, center).unwrap();
            center
        } else {
            game.harvest(id, 0).unwrap();
            game.supplies[0].object
        };
        assert!(game.entities[id].ai.docking.is_some());
        assert!(
            game.entities[target]
                .dock
                .as_ref()
                .unwrap()
                .owners
                .contains(&Some(id))
        );
        game.update_ai_module(id);
        assert!(game.entities[id].ai.blocking.path_through_units);
        game.kill_object(id, DeathType::Normal);
        assert!(game.entities[id].is_ai_dead());
        assert!(!matches!(
            game.entities[id].ai.command.main_state,
            MainState::Dead
        ));
        let retained = game.entities[id].ai.docking.is_some();
        let stock = game.supplies[0].boxes;
        let cash = game.players[0].cash;
        // This used to dispatch Harvest/ReturnSupply into an already-cleared
        // nested machine before the source's late AI_DEAD transition.
        game.step();
        assert!(retained);
        assert!(matches!(
            game.entities[id].ai.command.main_state,
            MainState::Dead
        ));
        assert!(matches!(game.entities[id].ai.order, Order::Idle));
        assert!(game.entities[id].ai.docking.is_none());
        assert!(!game.entities[id].ai.blocking.path_through_units);
        assert!(game.entities[id].ai.ignored_producer.is_none());
        assert!(
            !game.entities[target]
                .dock
                .as_ref()
                .unwrap()
                .owners
                .contains(&Some(id))
        );
        for _ in 0..3 {
            game.step();
        }
        assert_eq!(game.supplies[0].boxes, stock);
        assert_eq!(game.players[0].cash, cash);
        assert!(game.presentation.deposit_events.is_empty());
    }
}

#[test]
fn actual_collector_destruction_releases_approach_without_waiting_for_ai_dead() {
    let (mut game, id) = collector();
    game.harvest(id, 0).unwrap();
    let target = game.supplies[0].object;
    assert!(
        game.entities[target]
            .dock
            .as_ref()
            .unwrap()
            .owners
            .contains(&Some(id))
    );
    game.update_ai_module(id);
    assert!(game.entities[id].ai.blocking.path_through_units);
    game.destroy_maintenance_object(id);
    assert!(game.entities[id].ai.docking.is_some());
    game.process_update_destruction();
    assert!(!game.entities[id].exists());
    assert!(game.entities[id].ai.docking.is_none());
    assert!(!game.entities[id].ai.blocking.path_through_units);
    assert!(game.entities[id].ai.ignored_producer.is_none());
    assert!(
        !game.entities[target]
            .dock
            .as_ref()
            .unwrap()
            .owners
            .contains(&Some(id))
    );
    game.destroy_maintenance_object(id);
    game.process_update_destruction();
    assert!(game.entities[id].ai.docking.is_none());
    assert!(game.native_dock_reserve(target, 1).is_some());
}

#[test]
fn lost_dock_target_exits_collector_state_and_releases_approach() {
    let (mut game, id) = collector();
    game.harvest(id, 0).unwrap();
    let target = game.supplies[0].object;
    let stock = game.supplies[0].boxes;
    game.kill_object(target, DeathType::Normal);
    game.destroy_maintenance_object(target);
    game.process_update_destruction();
    game.step();
    assert!(game.entities[id].alive());
    assert!(game.entities[id].ai.docking.is_none());
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(!game.entities[id].ai.blocking.path_through_units);
    assert!(game.entities[id].ai.ignored_producer.is_none());
    assert!(
        !game.entities[target]
            .dock
            .as_ref()
            .unwrap()
            .owners
            .contains(&Some(id))
    );
    assert_eq!(game.supplies[0].boxes, stock);
    assert_eq!(game.entities[id].cargo, 0);
}

#[test]
fn killed_vehicle_repair_dock_survives_until_dead_exit_without_healing() {
    use crate::skirmish::{RepairDock, SupplyDock};
    let (mut game, id) = collector();
    let mut unit = tests::fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.kind_of.push("REPAIR_PAD".into());
    unit.neutral_tech.repair_dock = Some(RepairDock {
        full_heal_frames: 150.0,
        approaches: 5,
        approach_bones: 0,
    });
    unit.supply.dock = Some(SupplyDock {
        number_approaches: 5,
        approach_bones: 0,
        temporary_stealth_frames: 0,
        approaches: Vec::new(),
        enter: None,
        action: None,
        exit: None,
        allows_passthrough: true,
    });
    tests::register_update(&mut unit, "RepairDockUpdate");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let target = game.spawn(kind, 0, [230.0, 255.0], true);
    game.entities[id].health *= 0.5;
    game.update_visibility();
    game.repair(id, target).unwrap();
    assert!(matches!(game.entities[id].ai.order, Order::RepairAt(pad) if pad == target));
    game.update_ai_module(id);
    assert!(game.entities[id].ai.blocking.path_through_units);
    game.kill_object(id, DeathType::Normal);
    game.step();
    assert!(matches!(
        game.entities[id].ai.command.main_state,
        MainState::Dead
    ));
    assert!(game.entities[id].ai.docking.is_none());
    assert!(!game.entities[id].ai.blocking.path_through_units);
    assert!(game.entities[id].ai.ignored_producer.is_none());
    assert!(
        !game.entities[target]
            .dock
            .as_ref()
            .unwrap()
            .owners
            .contains(&Some(id))
    );
    assert_eq!(game.entities[id].health.to_bits(), 0.0_f32.to_bits());
    game.step();
    assert_eq!(game.entities[id].health.to_bits(), 0.0_f32.to_bits());
}
