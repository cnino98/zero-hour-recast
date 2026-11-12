use super::*;
use crate::skirmish::tests::game;

fn fixture() -> Skirmish {
    let mut game = game();
    let builder = game.entities[1].type_index;
    game.catalog[builder].maintenance.repair_per_second = 0.02;
    game.catalog[builder].maintenance.idle_repair = Some(IdleRepair {
        frames: 150.0,
        range: 150.0,
    });
    game.entities[1].position = [200.0, 200.0];
    game.observe_ground_partition(1);
    game.update_ground_contacts();
    game
}

fn structure(game: &mut Skirmish, owner: usize, position: [f32; 2]) -> usize {
    let id = game.spawn(0, owner, position, true);
    game.entities[id].health = 500.0;
    game.update_ground_contacts();
    id
}

#[test]
fn idle_repair_waits_strictly_past_bored_time_and_restarts_after_moving() {
    let mut game = fixture();
    let target = structure(&mut game, 0, [300.0, 200.0]);
    game.tick = 150;
    game.update_idle_repair(1);
    assert!(matches!(game.entities[1].ai.order, Order::Idle));
    game.tick += 1;
    game.update_idle_repair(1);
    assert_eq!(game.entities[1].repair_task_target(), Some(target));
    // Player movement cancels the current task, not an unselected pending slot.
    game.tick += 1;
    game.run_object_updates();
    game.move_to(1, [200.0, 400.0]).unwrap();
    assert_eq!(game.entities[1].repair_task_target(), None);
    game.tick = 200;
    game.update_idle_repair(1);
    game.stop(1).unwrap();
    game.tick = 350;
    game.update_idle_repair(1);
    assert!(matches!(game.entities[1].ai.order, Order::Idle));
    game.tick += 1;
    game.update_idle_repair(1);
    assert_eq!(game.entities[1].repair_task_target(), Some(target));
}

#[test]
fn idle_repair_runs_in_normal_frames_and_heals_through_the_original_repair_order() {
    let mut game = fixture();
    let target = structure(&mut game, 0, [300.0, 200.0]);
    let cash = game.players[0].cash;
    for _ in 0..150 {
        game.step();
    }
    assert_eq!(game.entities[target].health.to_bits(), 500.0_f32.to_bits());
    for _ in 0..300 {
        game.step();
        if game.entities[target].health > 500.0 {
            break;
        }
    }
    assert!(game.entities[target].health > 500.0);
    assert_eq!(
        game.entities[target].maintenance.healing_benefactor,
        Some(1)
    );
    assert_eq!(game.players[0].cash, cash);
}

#[test]
fn idle_repair_chooses_same_owner_nearest_and_native_iterator_ties() {
    let mut game = fixture();
    let first = structure(&mut game, 0, [280.0, 200.0]);
    let second = structure(&mut game, 0, [280.0, 200.0]);
    let other = structure(&mut game, 1, [240.0, 200.0]);
    game.slots[0].team = Some(1);
    game.slots[1].team = Some(1);
    let native_first = game
        .partition
        .outward_range([200.0; 2], 150.0)
        .into_iter()
        .rev()
        .find(|id| *id == first || *id == second)
        .unwrap();
    assert_eq!(game.idle_repair_target(1), Some(native_first));
    game.entities[native_first].health = game.entities[native_first].max_health;
    assert_eq!(
        game.idle_repair_target(1),
        Some(if native_first == first { second } else { first })
    );
    assert_ne!(game.idle_repair_target(1), Some(other));
}

#[test]
fn idle_repair_does_not_fall_back_when_nearest_has_an_active_benefactor() {
    let mut game = fixture();
    let nearest = structure(&mut game, 0, [250.0, 200.0]);
    let _farther = structure(&mut game, 0, [320.0, 200.0]);
    game.entities[nearest].maintenance.healing_benefactor = Some(0);
    game.entities[nearest].maintenance.healing_benefactor_until = 1000;
    game.tick = 151;
    game.update_idle_repair(1);
    assert!(matches!(game.entities[1].ai.order, Order::Idle));
    game.entities[nearest].maintenance.healing_benefactor = None;
    game.tick = 301;
    game.update_idle_repair(1);
    assert!(matches!(game.entities[1].ai.order, Order::Idle));
    game.tick += 1;
    game.update_idle_repair(1);
    assert_eq!(game.entities[1].repair_task_target(), Some(nearest));
}

#[test]
fn idle_repair_ai_radius_and_fractional_frame_time_use_source_values() {
    let mut game = fixture();
    let target = structure(&mut game, 0, [420.0, 200.0]);
    assert_eq!(game.idle_repair_target(1), None);
    game.slots[0].difficulty = Some(crate::skirmish::OpponentDifficulty::Hard);
    assert_eq!(game.idle_repair_target(1), Some(target));
    let builder = game.entities[1].type_index;
    game.catalog[builder]
        .maintenance
        .idle_repair
        .as_mut()
        .unwrap()
        .frames = 1.5;
    game.tick = 1;
    game.update_idle_repair(1);
    assert!(matches!(game.entities[1].ai.order, Order::Idle));
    game.tick += 1;
    game.update_idle_repair(1);
    assert_eq!(game.entities[1].repair_task_target(), Some(target));
}
