//! Native object-bound audio callbacks preserve ordering with weapon and object FX.
use super::{Skirmish, WeaponFxEvent, visibility::Status};

pub enum Action {
    Play,
    PlayAtPosition,
    PlayAtPositionUnowned,
    PlayLoop(usize),
    StopLoop(usize),
    Stop,
    StopAmbient,
}

pub struct Event {
    pub object: usize,
    pub position: [f32; 3],
    pub owner: usize,
    pub clear_cells: u16,
    pub name: String,
    pub action: Action,
}

impl Skirmish {
    pub(super) fn object_sound(&mut self, object: usize, name: String, action: Action) {
        let position = self.object_position(object);
        self.sound_at(object, name, position, action);
    }

    pub(super) fn position_sound(&mut self, object: usize, name: String, position: [f32; 3]) {
        self.sound_at(object, name, position, Action::PlayAtPosition);
    }

    pub(super) fn sound_at(
        &mut self,
        object: usize,
        name: String,
        position: [f32; 3],
        action: Action,
    ) {
        if !matches!(action, Action::StopAmbient)
            && (name.is_empty() || name.eq_ignore_ascii_case("NoSound"))
        {
            return;
        }
        let clear_cells = (0..self.players.len()).fold(0, |mask, owner| {
            if matches!(
                self.visibility_cell(owner, [position[0], position[1]]),
                Status::Clear
            ) {
                mask | (1 << owner)
            } else {
                mask
            }
        });
        self.presentation.weapon_fx.push(WeaponFxEvent::ObjectSound(
            self.presentation.object_sounds.len(),
        ));
        self.presentation.object_sounds.push(Event {
            object,
            position,
            owner: self.entities[object].owner,
            clear_cells,
            name,
            action,
        });
    }
}
