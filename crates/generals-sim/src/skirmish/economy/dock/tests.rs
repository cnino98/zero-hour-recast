use super::*;
use crate::skirmish::{Kind, SupplyBoost, tests};

fn warehouse(count: i32) -> (Skirmish, usize) {
    let mut game = tests::game();
    let template = game.entities[game.supplies[0].object].type_index;
    game.catalog[template]
        .supply
        .dock
        .as_mut()
        .unwrap()
        .number_approaches = count;
    let dock = game.spawn(template, game.neutral_owner(), [350.0, 100.0], true);
    (game, dock)
}

#[test]
fn fixed_five_boneless_approaches_do_not_become_dynamic() {
    let (mut game, dock) = warehouse(5);
    let tank = game.type_for(0, Kind::Tank).unwrap();
    for slot in 0..5 {
        let docker = game.spawn(tank, 0, [300.0, 100.0], true);
        assert_eq!(game.native_dock_reserve(dock, docker).unwrap().0, slot);
    }
    let sixth = game.spawn(tank, 0, [300.0, 100.0], true);
    assert!(!game.native_dock_clear_to_approach(dock, sixth));
    assert!(game.native_dock_reserve(dock, sixth).is_none());
}

#[test]
fn fixed_zero_refuses_reservations_but_dynamic_slots_grow_and_reuse() {
    let (mut game, dock) = warehouse(0);
    assert!(game.native_dock_reserve(dock, 1).is_none());
    let (mut game, dock) = warehouse(-1);
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let mut dockers = Vec::new();
    for slot in 0..11 {
        let docker = game.spawn(tank, 0, [300.0, 100.0], true);
        assert_eq!(game.native_dock_reserve(dock, docker).unwrap().0, slot);
        dockers.push(docker);
    }
    game.native_dock_cancel(dock, dockers[3]);
    assert_eq!(game.native_dock_reserve(dock, 1).unwrap().0, 3);
}

#[test]
fn clearance_waits_for_normal_update_and_cancel_releases_active_drawable_flags() {
    let (mut game, dock) = warehouse(1);
    game.native_dock_reserve(dock, 1).unwrap();
    game.native_dock_notify(dock, 1, &Notification::Approach);
    assert!(!game.native_dock_clear_to_enter(dock, 1));
    game.update_dock_module(dock);
    assert!(game.native_dock_clear_to_enter(dock, 1));
    game.native_dock_notify(dock, 1, &Notification::Enter);
    game.native_dock_notify(dock, 1, &Notification::Action);
    assert_ne!(game.entities[1].docking_flags & docking_flags::ACTIVE, 0);
    game.native_dock_cancel(dock, 1);
    assert!(!game.native_dock_clear_to_enter(dock, 1));
    assert_eq!(game.entities[1].docking_flags, 0);
    assert_eq!(game.entities[dock].docking_flags, 0);
    assert!(game.native_dock_clear_to_approach(dock, 1));
}

#[test]
fn docking_bones_use_row_major_rotated_pitched_matrix_translation() {
    let (mut game, dock) = warehouse(1);
    let template = game.entities[dock].type_index;
    game.catalog[template].supply.dock.as_mut().unwrap().enter = Some([2.0, 3.0, 5.0]);
    game.set_object_transform(
        dock,
        [350.0, 100.0, 20.0],
        [[0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]],
    );
    let point = game.native_dock_position(dock, 1, &Position::Enter);
    assert_eq!(
        point.map(f32::to_bits),
        [347.0_f32, 95.0, 22.0].map(f32::to_bits)
    );
    game.catalog[template].supply.dock.as_mut().unwrap().enter = Some([0.0; 3]);
    let dock = game.spawn(template, game.neutral_owner(), [350.0, 100.0], true);
    assert_eq!(
        game.native_dock_position(dock, 1, &Position::Enter)
            .map(f32::to_bits),
        game.object_position(1).map(f32::to_bits)
    );
}

#[test]
fn supply_lines_prorates_partial_cargo_and_tracks_collector_owner() {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(game.players[0].faction, Kind::Harvester);
    unit.supply.capacity = 8;
    unit.maintenance.collector_boost = Some(SupplyBoost {
        trigger: "Upgrade_AmericaSupplyLines".into(),
        amount: 60,
    });
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let center_type = game.type_for(0, Kind::SupplyCenter).unwrap();
    let center = game.spawn(center_type, 0, [450.0, 100.0], true);
    let collector = game.spawn(kind, 0, [450.0, 100.0], true);
    game.players[0]
        .upgrades
        .push("Upgrade_AmericaSupplyLines".into());
    for (capacity, cargo, expected) in [(8, 8, 660), (8, 3, 247), (8, 1, 82), (0, 1, 75)] {
        game.catalog[kind].supply.capacity = capacity;
        let cash = game.players[0].cash;
        let earned = game.players[0].statistics.money_earned;
        game.entities[collector].cargo = cargo;
        assert!(!game.native_supply_center_action(center, collector));
        assert_eq!(game.players[0].cash, cash + expected);
        assert_eq!(game.players[0].statistics.money_earned, earned + expected);
        assert_eq!(game.entities[collector].cargo, 0);
        let event = game.presentation.deposit_events.last().unwrap();
        assert_eq!(event.amount, i32::try_from(expected).unwrap());
        assert_eq!(
            event.position.map(f32::to_bits),
            [450.0_f32, 100.0, 0.0].map(f32::to_bits)
        );
    }
    game.transfer_maintenance_owner(collector, 1);
    assert_eq!(game.upgraded_collector_boost(collector), 0);
    game.players[1]
        .upgrades
        .push("Upgrade_AmericaSupplyLines".into());
    assert_eq!(game.upgraded_collector_boost(collector), 60);
}

#[test]
fn absent_drawable_keeps_unloaded_approach_count_and_does_not_bias_the_origin() {
    let (mut game, dock) = warehouse(1);
    game.set_object_position(dock, [900.0, 100.0, 0.0]);
    game.entities[dock].drawable.present = false;
    let point = game.native_dock_reserve(dock, 1).unwrap().1;
    assert_eq!(
        point.map(f32::to_bits),
        [900.0_f32, 100.0, 0.0].map(f32::to_bits)
    );
    assert!(
        game.entities[dock]
            .dock
            .as_ref()
            .unwrap()
            .positions
            .is_none()
    );
}
