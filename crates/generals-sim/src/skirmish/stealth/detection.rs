use super::{Event, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn update_stealth_detector(&mut self, id: usize, index: usize) -> i32 {
        if self.entities[id].is_effectively_dead() {
            return i32::from_ne_bytes(super::super::module_updates::FOREVER.to_ne_bytes());
        }
        if self.entities[id].under_construction() {
            return 1;
        }
        if self.entities[id].selling() {
            return i32::from_ne_bytes(super::super::module_updates::FOREVER.to_ne_bytes());
        }
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.detectors[index];
        let rate = rules.rate;
        if actor.contained_by().is_some_and(|carrier| {
            let garrisoned = self.catalog[self.entities[carrier].type_index]
                .contain
                .as_ref()
                .is_some_and(|contain| contain.garrison.is_some());
            if garrisoned {
                !rules.while_garrisoned
            } else {
                !rules.while_contained
            }
        }) {
            return i32::from_ne_bytes(rate.to_ne_bytes());
        }
        let radius = if rules.range > 0.0 {
            rules.range
        } else {
            actor.vision_range(&self.catalog[actor.type_index])
        };
        let position = actor.position;
        let owner = actor.owner;
        let targets: Vec<_> = self
            .partition
            .outward_range(position, radius)
            .into_iter()
            .filter(|&other| {
                let victim = &self.entities[other];
                let unit = &self.catalog[victim.type_index];
                victim.alive()
                    && !self.allied(owner, victim.owner)
                    && victim.contained_by().is_none()
                    && (rules.required_kind.is_empty()
                        || unit
                            .kind_of
                            .iter()
                            .any(|kind| rules.required_kind.contains(kind)))
                    && !unit
                        .kind_of
                        .iter()
                        .any(|kind| rules.forbidden_kind.contains(kind))
                    && super::super::movement::distance_squared(position, victim.position)
                        <= radius * radius
                    && (victim.stealthed()
                        || unit
                            .contain
                            .as_ref()
                            .is_some_and(|contain| contain.garrison.is_some())
                            && self
                                .passengers(other)
                                .iter()
                                .any(|rider| self.entities[*rider].stealthed()))
            })
            .collect();
        let mut found = false;
        for other in targets {
            found |= self.reveal_stealth_target(id, other, rate);
        }
        self.presentation.stealth_events.push(Event::Scan {
            detector: id,
            module: index,
            found,
            feedback: self.detector_feedback(id, index),
        });
        i32::from_ne_bytes(rate.to_ne_bytes())
    }

    fn detector_feedback(&self, id: usize, index: usize) -> Option<super::ScanFeedback> {
        use crate::skirmish::visibility::Status;
        let actor = &self.entities[id];
        self.catalog[actor.type_index].behavior.detectors[index]
            .feedback
            .as_ref()?;
        let visible_players = (0..self.players.len()).fold(0, |mask, player| {
            if (!actor.stealthed() || actor.owner == player)
                && matches!(
                    self.visibility_status(player, id),
                    Status::Clear | Status::Partial
                )
            {
                mask | (1 << player)
            } else {
                mask
            }
        });
        Some(super::ScanFeedback {
            type_index: actor.type_index,
            position: self.object_position(id),
            facing: actor.facing,
            health_ratio: actor.health / actor.max_health,
            visible_players,
        })
    }
    fn reveal_stealth_target(&mut self, detector: usize, other: usize, rate: u32) -> bool {
        let mut found = false;
        if self.entities[other].stealth.is_some() {
            found = true;
            if !self.entities[other].detected() {
                self.presentation.stealth_events.push(Event::Detection {
                    detector,
                    target: other,
                });
            }
            self.mark_detected(other, rate + 1);
            if !self.catalog[self.entities[other].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "MINE")
            {
                self.presentation
                    .stealth_events
                    .push(Event::Heat { object: other });
            }
        } else {
            for rider in self.passengers(other).to_vec() {
                if self.entities[rider].stealth.is_some() {
                    found = true;
                    if !self.allied(self.entities[detector].owner, self.entities[rider].owner) {
                        self.mark_detected(rider, rate + 2);
                    }
                }
            }
        }
        found
    }
}
