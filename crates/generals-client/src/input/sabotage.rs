//! `CommandXlat` sabotage mode and context action use the ordinary `MSG_ENTER`.
use super::{CommandMode, Mouse, PickTarget, context_order, locally_controlled};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};
use bevy::prelude::Vec2;

#[cfg(test)]
mod tests;

pub fn targeted(battle: &mut Match, mouse: &mut Mouse, point: Vec2, nearest: Option<usize>) {
    if mouse.waypoint_mode {
        context_order(battle, Some(point), nearest.map(PickTarget::Unit), mouse);
    } else if let Some(target) = nearest {
        issue(battle, mouse, target);
    }
}

pub fn can_target(battle: &Match, target: usize) -> bool {
    battle
        .selected
        .iter()
        .any(|&id| locally_controlled(battle, id) && battle.simulation.can_sabotage(id, target))
}

pub fn context_target(battle: &Match, mouse: &Mouse, target: Option<usize>) -> Option<usize> {
    if mouse.waypoint_mode || !matches!(mouse.pending_command, CommandMode::Normal) {
        return None;
    }
    let target = target?;
    // Repair/get-healed precede sabotage in evaluateContextCommand.
    if battle
        .selected
        .iter()
        .any(|&id| locally_controlled(battle, id) && battle.simulation.can_repair(id, target))
    {
        return None;
    }
    can_target(battle, target).then_some(target)
}

pub fn issue(battle: &mut Match, mouse: &mut Mouse, target: usize) -> bool {
    if !can_target(battle, target) {
        return false;
    }
    let response = VoiceRequest::new(
        &battle.simulation,
        battle.command_drawables(),
        Voice::EnterHostile,
        false,
    );
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) && battle.simulation.can_sabotage(id, target) {
            issued |= battle.simulation.enter(id, target).is_ok();
        }
    }
    if issued {
        mouse.voices.push(response);
        mouse.pending_command = CommandMode::Normal;
    }
    issued
}
