use super::*;
use crate::skirmish::ai_goals::COMMAND_FROM_SCRIPT;

fn active_hacker() -> (Skirmish, usize) {
    let (mut game, id) = hacker();
    game.hack_internet(id).unwrap();
    for _ in 0..3 {
        game.step();
    }
    assert_eq!(game.entities[id].hacking_condition(), Some("FIRING_A"));
    (game, id)
}

#[test]
fn allied_capture_preserves_income_counter_and_pays_the_new_owner() {
    let (mut game, id) = active_hacker();
    game.slots[0].team = Some(1);
    game.slots[1].team = Some(1);
    let remaining = game.entities[id].hacking.remaining;
    let cash = game.players[1].cash;
    game.transfer_maintenance_owner(id, 1);
    assert_eq!(game.entities[id].owner, 1);
    assert_eq!(game.entities[id].hacking_condition(), Some("FIRING_A"));
    assert_eq!(game.entities[id].hacking.remaining, remaining);
    assert!(game.entities[id].hacking.pending.is_none());
    for _ in 0..=remaining {
        game.step();
    }
    assert_eq!(game.players[0].statistics.money_earned, 0);
    assert_eq!(game.players[1].cash, cash + 5);
    assert_eq!(game.players[1].statistics.money_earned, 5);
}

#[test]
fn hostile_capture_packs_before_replaying_native_idle_without_extra_income() {
    let (mut game, id) = active_hacker();
    game.transfer_maintenance_owner(id, 1);
    assert_eq!(game.entities[id].hacking_condition(), Some("PACKING"));
    assert_eq!(game.entities[id].hacking.remaining, 2);
    assert!(matches!(
        game.entities[id].hacking.pending,
        Some(Command {
            source: COMMAND_FROM_AI,
            kind: CommandKind::Idle
        })
    ));
    for _ in 0..3 {
        game.step();
    }
    assert_eq!(game.entities[id].hacking_condition(), None);
    assert!(!game.entities[id].ai_idle());
    game.step();
    assert!(game.entities[id].ai_idle());
    assert!(game.entities[id].hacking.pending.is_none());
    assert_eq!(
        game.entities[id].ai.command.last_command_source,
        COMMAND_FROM_AI
    );
    assert_eq!(game.players[0].statistics.money_earned, 0);
    assert_eq!(game.players[1].statistics.money_earned, 0);
}

#[test]
fn hostile_capture_replaces_pending_script_move_without_restarting_pack() {
    let (mut game, id) = active_hacker();
    let position = game.entities[id].position;
    game.native_move_from(id, [450.0, 200.0, 0.0], COMMAND_FROM_SCRIPT);
    game.step();
    let remaining = game.entities[id].hacking.remaining;
    let animation = game.entities[id].hacking.animation;
    let random = game.random;
    game.transfer_maintenance_owner(id, 1);
    assert_eq!(game.random, random);
    assert_eq!(game.entities[id].hacking.remaining, remaining);
    assert_eq!(game.entities[id].hacking.animation, animation);
    assert!(matches!(
        game.entities[id].hacking.pending,
        Some(Command {
            source: COMMAND_FROM_AI,
            kind: CommandKind::Idle
        })
    ));
    for _ in 0..remaining + 2 {
        game.step();
    }
    assert!(game.entities[id].ai_idle());
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert_eq!(game.players[0].statistics.money_earned, 0);
    assert_eq!(game.players[1].statistics.money_earned, 0);
}
