//! Original Ambulance positional special power and selected-object voice response.
use super::{CommandMode, Mouse, locally_controlled};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};

#[cfg(test)]
mod tests;

pub fn issue(battle: &mut Match, mouse: &mut Mouse, point: [f32; 2]) -> bool {
    let response = VoiceRequest::new(
        &battle.simulation,
        &battle.selected,
        Voice::CleanupArea,
        false,
    );
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) && battle.simulation.cleanup_ready(id) {
            issued |= battle.simulation.clean_area(id, point).is_ok();
        }
    }
    if issued {
        mouse.voices.push(response);
        mouse.pending_command = CommandMode::Normal;
    }
    issued
}
