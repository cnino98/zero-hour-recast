//! Original `MSG_DO_WEAPON_AT_OBJECT` pilot weapon and specialty response.
use super::{CommandMode, Mouse, locally_controlled};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};

pub fn can_target(battle: &mut Match, target: usize, slot: usize) -> bool {
    for index in 0..battle.selected.len() {
        let id = battle.selected[index];
        if locally_controlled(battle, id)
            && battle.simulation.pilot_weapon_ready(id, slot)
            && battle.simulation.can_fire_pilot_weapon(id, target, slot)
        {
            return true;
        }
    }
    false
}

pub fn issue(battle: &mut Match, mouse: &mut Mouse, target: usize, slot: usize) -> bool {
    if !can_target(battle, target, slot) {
        return false;
    }
    let response = VoiceRequest::new(
        &battle.simulation,
        &battle.selected,
        Voice::SnipePilot,
        false,
    );
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) {
            issued |= battle.simulation.snipe_vehicle(id, target, slot).is_ok();
        }
    }
    if issued {
        mouse.voices.push(response);
        mouse.pending_command = CommandMode::Normal;
    }
    issued
}
