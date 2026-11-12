//! `HelixContain` and the Helix's persistent napalm ability, SH 2bfaea60.
use super::{Entity, Skirmish};
mod bomb;

pub(super) struct State {
    pub(super) passenger_bonus: bool,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            passenger_bonus: false,
        }
    }
}

impl Entity {
    pub(super) const fn garrison_weapon_bonus(&self) -> bool {
        self.garrisoned() || self.helix.passenger_bonus
    }
}

impl Skirmish {
    pub(super) fn deploy_helix_passengers(&mut self, carrier: usize) -> bool {
        if !self.catalog[self.entities[carrier].type_index].is_helix_container() {
            return false;
        }
        // HelixContain::redeployOccupants places every real passenger eight
        // units above the host. Its portable object is outside this list.
        let position = self.object_position(carrier);
        for index in 0..self.entities[carrier].containment.passengers.len() {
            let id = self.entities[carrier].containment.passengers[index];
            self.entities[id].position = [position[0], position[1]];
            self.entities[id].containment.height = position[2] + 8.0;
        }
        true
    }
}

#[cfg(test)]
pub(in crate::skirmish) mod tests;
