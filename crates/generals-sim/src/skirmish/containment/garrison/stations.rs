//! Non-enclosing station assignment in SH `GarrisonContain.cpp`, 2bfaea60.
use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish::containment) fn station_garrison(&self, carrier: usize) -> bool {
        self.catalog[self.entities[carrier].type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.garrison.as_ref())
            .is_some_and(|rules| rules.stations.is_some())
    }

    pub(in crate::skirmish::containment) fn position_station_passengers(&mut self, carrier: usize) {
        let Some(state) = &self.entities[carrier].containment.garrison else {
            return;
        };
        if state.stations.is_empty() {
            let points = self.catalog[self.entities[carrier].type_index]
                .contain
                .as_ref()
                .and_then(|rules| rules.garrison.as_ref())
                .and_then(|rules| rules.stations.as_ref())
                .expect("non-enclosing garrison stations")
                .iter()
                .map(|point| self.container_point(carrier, *point))
                .collect();
            self.entities[carrier]
                .containment
                .garrison
                .as_mut()
                .expect("garrison state")
                .stations = points;
        }
        for index in 0..self.passengers(carrier).len() {
            let id = self.passengers(carrier)[index];
            let state = self.entities[carrier]
                .containment
                .garrison
                .as_mut()
                .expect("garrison state");
            let slots = &mut state.points[..state.stations.len()];
            if let Some(station) = slots.iter().position(|other| *other == Some(id)) {
                let position = state.stations[station];
                self.entities[id].position = [position[0], position[1]];
                self.entities[id].containment.height = position[2];
                self.observe_ground_partition(id);
                self.update_position_claim(id);
            } else if let Some(station) = slots.iter_mut().find(|other| other.is_none()) {
                // Native pickAStationForMe assigns only; the next positioning
                // pass moves the newly assigned occupant to this world point.
                *station = Some(id);
            }
        }
    }
}

#[cfg(test)]
mod tests;
