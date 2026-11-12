//! Snapshot at `FXList::doFXObj`, before later ownership or pose mutations.
use super::{Skirmish, visibility::Status};

pub struct Event {
    pub frame: u64,
    pub object: usize,
    pub type_index: usize,
    pub owner: usize,
    pub position: [f32; 3],
    pub basis: [[f32; 3]; 3],
    pub oriented_object: bool,
    pub secondary_position: Option<[f32; 3]>,
    pub primary_geometry_radius: f32,
    pub override_radius: f32,
    pub visible_players: u16,
    pub clear_cells: u16,
    pub effect: String,
}

impl Skirmish {
    pub(super) fn object_fx(&mut self, id: usize, effect: String) {
        let (position, basis) = self.object_transform(id);
        self.object_fx_at(id, effect, position, basis);
    }

    pub(super) fn object_fx_at(
        &mut self,
        id: usize,
        effect: String,
        position: [f32; 3],
        basis: [[f32; 3]; 3],
    ) {
        if effect.is_empty() || effect.eq_ignore_ascii_case("NONE") {
            return;
        }
        let event = self.snapshot_object_fx(id, effect, position, basis);
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::ObjectFx(
                self.presentation.object_fx_events.len(),
            ));
        self.presentation.object_fx_events.push(event);
    }

    pub(in crate::skirmish) fn positional_fx_with_radius(
        &mut self,
        id: usize,
        effect: String,
        position: [f32; 3],
        radius: f32,
    ) {
        let mut event = self.snapshot_object_fx(
            id,
            effect,
            position,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        );
        event.oriented_object = false;
        event.override_radius = radius;
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::ObjectFx(
                self.presentation.object_fx_events.len(),
            ));
        self.presentation.object_fx_events.push(event);
    }

    pub(super) fn snapshot_object_fx(
        &self,
        id: usize,
        effect: String,
        position: [f32; 3],
        basis: [[f32; 3]; 3],
    ) -> Event {
        let actor = &self.entities[id];
        let visible_players = (0..self.players.len()).fold(0, |mask, owner| {
            if matches!(
                self.visibility_status(owner, id),
                Status::Clear | Status::Partial
            ) {
                mask | (1 << owner)
            } else {
                mask
            }
        });
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
        Event {
            frame: self.tick,
            object: id,
            type_index: actor.type_index,
            owner: actor.owner,
            position,
            basis,
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: self.object_radius(id),
            override_radius: 0.0,
            visible_players,
            clear_cells,
            effect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::visibility::VisibilityRules;

    #[test]
    fn callback_keeps_position_cell_visibility_before_later_sight_changes() {
        let mut game = crate::skirmish::tests::game();
        game.entities.clear();
        game.partition.clear();
        for unit in &mut game.catalog {
            unit.sight.clearing_range = 0.0;
        }
        game.configure_visibility(VisibilityRules {
            cell_size: 40.0,
            unlook_persist_frames: 150,
            use_shroud: true,
        })
        .unwrap();
        let target = game.spawn(0, 1, [300.0, 300.0], true);
        game.update_visibility();
        game.object_fx(target, "Callback".into());
        assert_eq!(game.presentation.object_fx_events[0].clear_cells & 1, 0);
        game.catalog[5].sight.clearing_range = 80.0;
        game.spawn(5, 0, [300.0, 300.0], true);
        game.update_visibility();
        assert!(matches!(
            game.visibility_cell(0, [300.0, 300.0]),
            Status::Clear
        ));
        game.object_fx(target, "Callback".into());
        assert_eq!(game.presentation.object_fx_events[0].clear_cells & 1, 0);
        assert_eq!(game.presentation.object_fx_events[1].clear_cells & 1, 1);
    }
}
