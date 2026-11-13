use super::*;
use crate::skirmish::{
    Faction, Kind, RepairDock,
    tests::{fixture_type, game},
};

fn pad(game: &mut Skirmish, owner: usize) -> usize {
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.kind_of.push("REPAIR_PAD".into());
    unit.neutral_tech.repair_dock = Some(RepairDock {
        full_heal_frames: 150.0,
        approaches: 5,
        approach_bones: 0,
    });
    unit.supply.dock = Some(crate::skirmish::SupplyDock {
        number_approaches: 5,
        approach_bones: 0,
        temporary_stealth_frames: 0,
        approaches: Vec::new(),
        enter: None,
        action: None,
        exit: None,
        allows_passthrough: true,
    });
    crate::skirmish::tests::register_update(&mut unit, "RepairDockUpdate");
    let index = game.catalog.len();
    game.catalog.push(unit);
    game.spawn(index, owner, [350.0, 100.0], true)
}

#[test]
fn repair_pad_uses_fixed_missing_health_rate_and_resets_only_after_full_health_action() {
    let mut game = game();
    let pad = pad(&mut game, 0);
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let first = game.spawn(tank, 0, [300.0, 100.0], true);
    game.entities[first].max_health = 500.0;
    game.entities[first].health = 200.0;
    assert!(game.repair_dock_action(first, pad));
    assert_eq!(game.entities[first].health.to_bits(), 202.0_f32.to_bits());
    // Source cancelDock does not reset RepairDockUpdate's last-repair/rate.
    game.set_order(first, Order::Idle);
    let second = game.spawn(tank, 0, [310.0, 100.0], true);
    game.entities[second].max_health = 500.0;
    game.entities[second].health = 350.0;
    assert!(game.repair_dock_action(second, pad));
    assert_eq!(game.entities[second].health.to_bits(), 352.0_f32.to_bits());
    game.entities[second].health = 500.0;
    assert!(!game.repair_dock_action(second, pad));
    game.entities[first].health = 350.0;
    assert!(game.repair_dock_action(first, pad));
    assert_eq!(game.entities[first].health.to_bits(), 351.0_f32.to_bits());
}

#[test]
fn repair_pad_admission_rejects_aircraft_full_health_and_hostile_owner() {
    let mut game = game();
    let pad = pad(&mut game, 0);
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [300.0, 100.0], true);
    game.entities[id].health *= 0.5;
    game.catalog[tank].maintenance.healing_kind_flags = 2;
    game.update_visibility();
    assert!(game.repair_pad_admits(id, pad));
    game.catalog[tank].maintenance.healing_kind_flags |= 4;
    assert!(!game.repair_pad_admits(id, pad));
    game.catalog[tank].maintenance.healing_kind_flags = 2;
    game.entities[id].health = game.entities[id].max_health;
    assert!(!game.repair_pad_admits(id, pad));
    game.entities[id].health *= 0.5;
    game.transfer_maintenance_owner(pad, 1);
    assert!(!game.repair_pad_admits(id, pad));
}

#[test]
fn ordinary_repair_order_reaches_pad_heals_and_returns_idle() {
    let (mut game, id) = crate::skirmish::ground_movement::tests::moving_tank();
    let pad = pad(&mut game, 0);
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].maintenance.healing_kind_flags = 2;
    game.set_object_position(id, [250.0, 100.0, 0.0]);
    game.entities[id].health = game.entities[id].max_health * 0.5;
    let injured = game.entities[id].health;
    game.update_visibility();
    game.repair(id, pad).unwrap();
    assert!(matches!(game.entities[id].ai.order, Order::RepairAt(target) if target == pad));
    for _ in 0..1800 {
        game.step();
        if matches!(game.entities[id].ai.order, Order::Idle) {
            break;
        }
    }
    assert!(game.entities[id].health > injured);
    assert_eq!(
        game.entities[id].health.to_bits(),
        game.entities[id].max_health.to_bits()
    );
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(game.entities[id].ai.docking.is_none());
}

#[test]
fn repair_pad_geometry_is_not_a_supply_delivery_provider() {
    let mut game = game();
    let pad = pad(&mut game, 0);
    let index = game.entities[pad].type_index;
    game.catalog[index].supply.dock = Some(crate::skirmish::SupplyDock {
        number_approaches: -1,
        approach_bones: 0,
        temporary_stealth_frames: 0,
        approaches: Vec::new(),
        enter: None,
        action: None,
        exit: None,
        allows_passthrough: true,
    });
    let collector = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].supply.capacity > 0)
        .unwrap();
    game.entities[collector].cargo = 1;
    let cash = game.players[0].cash;
    assert!(game.deliver_supplies(collector, pad).is_err());
    assert_eq!(game.entities[collector].cargo, 1);
    assert_eq!(game.players[0].cash, cash);
}

#[test]
fn repair_pad_keeps_cached_dead_drone_without_resurrecting_it() {
    let mut game = game();
    let pad = pad(&mut game, 0);
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].maintenance.healing_kind_flags = 2;
    let tank = game.spawn(kind, 0, [250.0, 100.0], true);
    game.entities[tank].health *= 0.5;
    let drone = game.spawn(kind, 0, [250.0, 125.0], true);
    game.kill_object(drone, crate::skirmish::DeathType::Normal);
    game.update_visibility();
    game.repair(tank, pad).unwrap();
    game.update_repair_pad(tank, pad);
    game.entities[tank]
        .ai
        .docking
        .as_mut()
        .unwrap()
        .repair_drone = Some(drone);
    assert!(game.repair_dock_action(tank, pad));
    assert_eq!(game.entities[drone].health.to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        game.entities[tank]
            .ai
            .docking
            .as_ref()
            .unwrap()
            .repair_drone,
        Some(drone)
    );
}

#[test]
fn repair_pad_submits_drone_maximum_health_through_armor_with_pad_as_source() {
    let mut game = game();
    let pad = pad(&mut game, 0);
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].maintenance.healing_kind_flags = 2;
    game.catalog[kind].armor[crate::skirmish::DamageType::Healing as usize] = 0.5;
    let tank = game.spawn(kind, 0, [250.0, 100.0], true);
    game.entities[tank].health *= 0.5;
    let drone = game.spawn(kind, 0, [250.0, 125.0], true);
    game.entities[drone].max_health = 100.0;
    game.entities[drone].health = 20.0;
    game.update_visibility();
    game.repair(tank, pad).unwrap();
    game.update_repair_pad(tank, pad);
    game.entities[tank]
        .ai
        .docking
        .as_mut()
        .unwrap()
        .repair_drone = Some(drone);
    assert!(game.repair_dock_action(tank, pad));
    // Native amount=100, armor=0.5: adds50, not the deficit80 times0.5.
    assert_eq!(game.entities[drone].health.to_bits(), 70.0_f32.to_bits());
    for id in [tank, drone] {
        assert_eq!(game.entities[id].damage_state.retained_source, Some(pad));
        assert!(matches!(
            game.entities[id].damage_state.damage_type,
            crate::skirmish::DamageType::Healing
        ));
    }
}

#[test]
fn repair_pad_heals_the_cached_drone_while_it_remains_masked_in_world() {
    let mut game = game();
    let pad = pad(&mut game, 0);
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].maintenance.healing_kind_flags = 2;
    let tank = game.spawn(kind, 0, [250.0, 100.0], true);
    game.entities[tank].health *= 0.5;
    let drone = game.spawn(kind, 0, [250.0, 125.0], true);
    game.entities[drone].health *= 0.5;
    game.mask_object(drone, true);
    game.update_visibility();
    game.repair(tank, pad).unwrap();
    game.update_repair_pad(tank, pad);
    game.entities[tank]
        .ai
        .docking
        .as_mut()
        .unwrap()
        .repair_drone = Some(drone);
    assert!(game.repair_dock_action(tank, pad));
    assert_eq!(
        game.entities[drone].health.to_bits(),
        game.entities[drone].max_health.to_bits()
    );
    assert!(game.entities[drone].masked());
}
