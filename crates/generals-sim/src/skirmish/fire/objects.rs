//! Fire presentation pose initialization; actual authored modules own gameplay timers.
use super::State;
use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn initialize_fire(&mut self, id: usize) {
        let unit = &self.catalog[self.entities[id].type_index];
        if unit.fire.is_none() {
            return;
        }
        self.entities[id].fire = Some(State {
            created: self.tick,
            height: self.object_position(id)[2],
            dimensions: [
                unit.geometry.height,
                unit.geometry.major,
                unit.geometry.minor,
            ],
            started: None,
            reversed: None,
        });
    }
}
