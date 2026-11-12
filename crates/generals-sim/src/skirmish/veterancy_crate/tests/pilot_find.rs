use super::*;
use crate::skirmish::{
    module_updates::{FOREVER, ModuleRegistration},
    pilot_find_vehicle,
};

fn scanner(game: &Skirmish, id: usize) -> usize {
    game.catalog[game.entities[id].type_index].update_modules.iter().position(|entry| matches!(entry, ModuleRegistration::Behavior { family, .. } if family == "PilotFindVehicleUpdate")).unwrap()
}

fn rules() -> pilot_find_vehicle::Rules {
    pilot_find_vehicle::Rules {
        scan_frames: 30,
        scan_range: 300.0,
        min_health: 0.5,
    }
}

#[test]
fn scheduled_ai_scan_skips_unhealthy_nearer_vehicle_then_real_enter_collects() {
    let (mut game, pilot, vehicle) = fixture_with_scan(1, Some(rules()));
    let unhealthy = game.spawn(6, 1, [330.0, 200.0], true);
    let damage = game.entities[unhealthy].max_health * 0.51;
    game.apply_native_damage(unhealthy, None, damage, crate::skirmish::DeathType::Normal);
    let registration = scanner(&game, pilot);
    game.step();
    assert_eq!(game.entities[pilot].ai.command.goal_object, Some(vehicle));
    assert_eq!(game.update_remaining_sleep(pilot, registration), 30);
    for _ in 0..180 {
        game.step();
        if !game.entities[pilot].exists() {
            break;
        }
    }
    assert!(!game.entities[pilot].exists());
    assert_eq!(game.entities[vehicle].experience, 100);
    assert_eq!(game.entities[unhealthy].veterancy, 0);
}

#[test]
fn human_callback_sleeps_forever_and_owner_change_does_not_invent_a_wake() {
    let (mut game, pilot, _) = fixture_with_scan(0, Some(rules()));
    let registration = scanner(&game, pilot);
    game.step();
    assert_eq!(
        game.update_frame() + game.update_remaining_sleep(pilot, registration),
        FOREVER
    );
    assert!(game.entities[pilot].ai.command.goal_object.is_none());
    game.slots[1].difficulty = Some(crate::skirmish::OpponentDifficulty::Hard);
    game.transfer_maintenance_owner(pilot, 1);
    game.step();
    assert_eq!(
        game.update_frame() + game.update_remaining_sleep(pilot, registration),
        FOREVER
    );
    assert!(game.entities[pilot].ai.command.goal_object.is_none());
}

#[test]
fn equal_distance_scan_retains_partition_prepend_order_and_min_health_equality() {
    let (mut game, pilot, vehicle) = fixture_with_scan(1, Some(rules()));
    let tied = game.spawn(6, 1, [300.0, 260.0], true);
    let damage = game.entities[tied].max_health * 0.5;
    game.apply_native_damage(tied, None, damage, crate::skirmish::DeathType::Normal);
    game.update_ground_contacts();
    let order = game
        .partition
        .fastest_range_objects(game.entities[pilot].position, 300.0);
    let expected = order
        .into_iter()
        .find(|&id| id == tied || id == vehicle)
        .unwrap();
    let random = game.random;
    assert_eq!(game.update_pilot_find_vehicle(pilot, 0), 30);
    assert_eq!(game.random, random);
    assert_eq!(game.entities[pilot].ai.command.goal_object, Some(expected));
}
