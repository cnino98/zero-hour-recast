//! `CommandXlat` uses the original enter-object command for car conversion.
use super::{CommandMode, Mouse, locally_controlled};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};

pub fn can_target(battle: &Match, target: usize, context: bool) -> bool {
    let Some(car) = battle.simulation.entities.get(target) else {
        return false;
    };
    // The explicit button admits allied/neutral targets. Context evaluation
    // delegates directly to ActionManager and the collision module instead.
    (context || !battle.simulation.enemies(0, car.owner))
        && battle.selected.iter().any(|&id| {
            locally_controlled(battle, id) && battle.simulation.can_make_car_bomb(id, target)
        })
}

pub fn context_target(battle: &Match, mouse: &Mouse, target: Option<usize>) -> Option<usize> {
    if mouse.waypoint_mode || !matches!(mouse.pending_command, CommandMode::Normal) {
        return None;
    }
    let target = target?;
    // Native repair/get-repaired and hijack choices precede conversion; the
    // latter two are absent from the currently represented selection commands.
    if battle
        .selected
        .iter()
        .any(|&id| locally_controlled(battle, id) && battle.simulation.can_repair(id, target))
    {
        return None;
    }
    can_target(battle, target, true).then_some(target)
}

pub fn issue(battle: &mut Match, mouse: &mut Mouse, target: usize, context: bool) -> bool {
    if !can_target(battle, target, context) {
        return false;
    }
    let car = &battle.simulation.entities[target];
    let voice = if battle.simulation.allied(0, car.owner) {
        Voice::Enter
    } else {
        Voice::EnterHostile
    };
    let response = VoiceRequest::new(&battle.simulation, battle.command_drawables(), voice, false);
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) && battle.simulation.can_make_car_bomb(id, target) {
            issued |= battle.simulation.make_car_bomb(id, target).is_ok();
        }
    }
    if issued {
        mouse.voices.push(response);
        mouse.pending_command = CommandMode::Normal;
    }
    issued
}
