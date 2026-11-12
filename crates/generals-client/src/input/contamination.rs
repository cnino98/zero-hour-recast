//! Original multi-selection `FIRE_WEAPON` position dispatch and poison-location voice.
use super::{CommandMode, Mouse, locally_controlled};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};

#[cfg(test)]
mod tests;

pub fn issue(battle: &mut Match, mouse: &mut Mouse, slot: usize, point: [f32; 2]) {
    let response = VoiceRequest::new(
        &battle.simulation,
        &battle.selected,
        if slot == 2 {
            Voice::Attack
        } else if battle.selected.iter().any(|&id| {
            battle.simulation.catalog[battle.simulation.entities[id].type_index].name
                == "ChinaTankDragon"
        }) {
            Voice::FlameLocation
        } else {
            Voice::PoisonLocation
        },
        false,
    );
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) && battle.simulation.location_weapon_ready(id, slot) {
            issued |= battle.simulation.fire_weapon_at(id, slot, point).is_ok();
        }
    }
    if issued {
        mouse.voices.push(response);
        mouse.pending_command = CommandMode::Normal;
    }
}

pub fn mine_mode(battle: &mut Match, mouse: &mut Mouse, slot: usize) {
    let mut accepted = false;
    for &id in &battle.selected {
        if locally_controlled(battle, id) {
            accepted |= battle.simulation.select_mine_clearing(id, slot).is_ok();
        }
    }
    if accepted {
        battle.placing = None;
        mouse.pending_command = CommandMode::ClearMines(slot);
        // The original button has no UnitSpecificSound. VoiceDisarm belongs
        // to the later MSG_DO_WEAPON_AT_LOCATION dispatch.
    }
}

pub fn clear_mines(battle: &mut Match, mouse: &mut Mouse, slot: usize, point: [f32; 2]) {
    let response = VoiceRequest::new(&battle.simulation, &battle.selected, Voice::Disarm, false);
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) {
            issued |= battle.simulation.clear_mines_at(id, slot, point).is_ok();
        }
    }
    if issued {
        mouse.voices.push(response);
        mouse.pending_command = CommandMode::Normal;
    }
}
