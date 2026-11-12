//! `ScriptEngine::transferObjectName` retains the displaced name's null cache entry.
use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn transfer_crew_name(&mut self, actor: usize, target: usize) {
        let Some(name) = self
            .map_objects
            .iter()
            .find_map(|(name, &id)| (id == actor).then(|| name.clone()))
        else {
            // An unnamed entering unit leaves the vehicle's own name unchanged.
            return;
        };
        let displaced: Vec<_> = self
            .map_objects
            .iter()
            .filter(|(_, id)| **id == target)
            .map(|(name, _)| name.clone())
            .collect();
        for previous in displaced {
            self.map_objects.remove(&previous);
            self.removed_map_names.insert(previous);
        }
        self.removed_map_names.remove(&name);
        self.map_objects.insert(name, target);
    }

    pub(in crate::skirmish) fn named_map_object_destroyed(&self, name: &str) -> bool {
        self.named_map_object(name).map_or_else(
            || self.removed_map_names.contains(name),
            |id| !self.entities[id].alive(),
        )
    }
}
