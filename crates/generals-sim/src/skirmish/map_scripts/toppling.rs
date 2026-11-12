//! `ScriptEngine`'s retained named-object topple-direction overrides.
use super::State;
use crate::skirmish::{Skirmish, toppling::normalize};

impl State {
    pub(super) fn set_topple_direction(&mut self, name: &str, direction: Option<[f32; 3]>) {
        if name.is_empty() {
            return;
        }
        if let Some(index) = self
            .topple_directions
            .iter()
            .position(|(existing, _)| existing == name)
        {
            if let Some(direction) = direction {
                self.topple_directions[index].1 = direction;
            } else {
                self.topple_directions.remove(index);
            }
        } else if let Some(direction) = direction {
            self.topple_directions
                .insert(0, (name.to_owned(), direction));
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn adjust_topple_direction(
        &self,
        id: usize,
        direction: [f32; 3],
    ) -> [f32; 3] {
        self.map_scripts
            .topple_directions
            .iter()
            .find_map(|(name, value)| (self.named_map_object(name) == Some(id)).then_some(*value))
            .map_or(direction, normalize)
    }
}
