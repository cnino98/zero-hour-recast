use super::Notification;
use crate::skirmish::{
    DeathType, Faction, Kind, Skirmish, SupplyDock, supply::WarehouseRules,
    supply_crippling::Rules, tests,
};

fn warehouse(game: &mut Skirmish) -> usize {
    let mut unit = tests::fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.initial_health = 100.0;
    unit.max_health = 100.0;
    unit.kind_of = vec!["SUPPLY_SOURCE".into(), "IMMOBILE".into()];
    unit.supply.warehouse = Some(WarehouseRules {
        starting_boxes: 10,
        delete_when_empty: false,
    });
    unit.supply.dock = Some(SupplyDock {
        number_approaches: -1,
        approach_bones: 0,
        temporary_stealth_frames: 0,
        approaches: Vec::new(),
        enter: None,
        action: None,
        exit: None,
        allows_passthrough: true,
    });
    unit.behavior.supply_crippling.push(Rules {
        suppression: 1,
        delay: 1,
        amount: 10.0,
    });
    tests::register_behavior(&mut unit, "SupplyWarehouseCreate", 8);
    tests::register_update(&mut unit, "SupplyWarehouseDockUpdate");
    tests::register_update(&mut unit, "SupplyWarehouseCripplingBehavior");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    game.spawn(kind, 0, [300.0, 300.0], true)
}

#[test]
fn really_damaged_blocks_dock_clearance_until_healing_and_rubble_clears_crippling() {
    let mut game = tests::game();
    let id = warehouse(&mut game);
    let worker = 1;
    game.apply_typed_hit(id, worker, 95.0, DeathType::Normal);
    assert_eq!(game.body_damage_state(id), 2);
    assert!(game.entities[id].dock.as_ref().unwrap().crippled);
    assert!(game.native_dock_reserve(id, worker).is_some());
    game.native_dock_notify(id, worker, &Notification::Approach);
    game.update_dock_module(id);
    assert!(!game.native_dock_clear_to_enter(id, worker));

    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 15.0_f32.to_bits());
    assert_eq!(game.body_damage_state(id), 1);
    game.update_dock_module(id);
    assert!(game.native_dock_clear_to_enter(id, worker));
    game.native_dock_cancel(id, worker);

    game.apply_typed_hit(id, worker, 14.0, DeathType::Normal);
    assert!(game.entities[id].dock.as_ref().unwrap().crippled);
    game.apply_typed_hit(id, worker, 2.0, DeathType::Normal);
    assert_eq!(game.body_damage_state(id), 3);
    assert!(!game.entities[id].dock.as_ref().unwrap().crippled);
}
