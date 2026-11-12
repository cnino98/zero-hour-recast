use super::State;

impl State {
    pub(in crate::skirmish) fn runway_index(&self, id: usize) -> Option<usize> {
        self.space_index(id)
            .map(|index| self.spaces[index].geometry.runway)
    }

    pub(in crate::skirmish) fn runway_owner(&self, index: usize) -> Option<usize> {
        self.runways[index].object
    }

    pub(in crate::skirmish) fn reserve_runway(&mut self, id: usize, landing: bool) -> bool {
        let Some(index) = self.space_index(id) else {
            return false;
        };
        if self.spaces.len() > self.runways.len() && index >= self.runways.len() {
            let postponed = &mut self.spaces[index].postponed;
            if landing {
                *postponed = false;
            } else if !*postponed {
                *postponed = true;
                return false;
            }
        }
        let runway = &mut self.runways[self.spaces[index].geometry.runway];
        if runway.object == Some(id) {
            return true;
        }
        if runway.object.is_none() {
            runway.object = Some(id);
            if runway.next == Some(id) {
                runway.next = None;
                runway.was_in_line = true;
            } else {
                runway.was_in_line = false;
            }
            return true;
        }
        if !landing && runway.next.is_none() {
            runway.next = Some(id);
        }
        false
    }

    pub(in crate::skirmish) fn transfer_runway(&mut self, id: usize) {
        for runway in &mut self.runways {
            if runway.object == Some(id) {
                if runway.next.is_some() {
                    runway.object = runway.next.take();
                    runway.was_in_line = true;
                }
                break;
            }
        }
    }

    pub(in crate::skirmish) fn release_runway(&mut self, id: usize) {
        for runway in &mut self.runways {
            if runway.object == Some(id) {
                runway.object = None;
                runway.was_in_line = false;
            }
            if runway.next == Some(id) {
                runway.next = None;
            }
        }
    }
}
