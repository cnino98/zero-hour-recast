//! SH `Drawable::getPristineBonePositions` and `SparseMatchFinder`, 2bfaea60.
use super::Skirmish;
use std::collections::{BTreeMap, BTreeSet};

pub struct State {
    pub conditions: Vec<BTreeSet<String>>,
    pub transforms: BTreeMap<String, [[f32; 4]; 3]>,
}

pub struct Draw {
    pub states: Vec<State>,
    pub ignored: BTreeSet<String>,
}

impl Draw {
    pub(in crate::skirmish) fn select(&self, conditions: &BTreeSet<String>) -> Option<&State> {
        let mut result = None;
        let mut best_yes = 0;
        let mut best_extra = 999;
        for state in &self.states {
            for flags in state.conditions.iter().rev() {
                let yes = flags
                    .iter()
                    .filter(|flag| conditions.contains(*flag) && !self.ignored.contains(*flag))
                    .count();
                let extra = flags.len() - yes;
                if yes > best_yes || (yes >= best_yes && extra < best_extra) {
                    result = Some(state);
                    best_yes = yes;
                    best_extra = extra;
                }
            }
        }
        result
    }
}

impl Skirmish {
    /// Pristine model-local position for the state logic currently selects.
    #[must_use]
    pub fn logical_bone_position(&self, id: usize, bone: &str) -> Option<[f32; 3]> {
        self.logical_bone_matrix(id, bone)
            .map(|matrix| matrix.map(|row| row[3]))
    }

    /// Complete pristine row-major matrix, including the authored drawable scale.
    #[must_use]
    pub fn logical_bone_matrix(&self, id: usize, bone: &str) -> Option<[[f32; 4]; 3]> {
        let actor = &self.entities[id];
        if !actor.drawable.present {
            return None;
        }
        let unit = &self.catalog[actor.type_index];
        let conditions = self.model_flags(actor, unit.body_damage_thresholds);
        let name = bone.to_ascii_lowercase();
        unit.pristine_draws
            .iter()
            .find_map(|draw| draw.select(&conditions)?.transforms.get(&name).copied())
    }
}
