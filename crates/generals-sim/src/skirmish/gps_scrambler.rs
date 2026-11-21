//! SH 2bfaea60 `GrantStealthBehavior`.
use super::{Skirmish, stealth::Event};

pub struct Rules {
    pub start_radius: f32,
    pub final_radius: f32,
    pub radius_per_frame: f32,
    pub kind_of: Option<Vec<String>>,
    pub radius_particle: Option<String>,
}

pub(super) struct State {
    radius: f32,
    emitter_started: bool,
}

impl Skirmish {
    pub(super) fn construct_gps_scrambler(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        let ordinal = actor.behaviors.gps_scramblers.len();
        let rules = &self.catalog[actor.type_index].behavior.gps_scramblers[ordinal];
        actor.behaviors.gps_scramblers.push(State {
            radius: rules.start_radius,
            emitter_started: false,
        });
    }

    pub(super) fn update_gps_scrambler(&mut self, id: usize, ordinal: usize) -> i32 {
        if self.entities[id].is_effectively_dead() {
            return i32::from_ne_bytes(super::module_updates::FOREVER.to_ne_bytes());
        }
        let position = self.entities[id].position;
        let world_position = self.object_position(id);
        let actor = &mut self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.gps_scramblers[ordinal];
        let state = &mut actor.behaviors.gps_scramblers[ordinal];
        if rules.radius_particle.is_some() && !state.emitter_started {
            self.presentation.stealth_events.push(Event::GrantEmitter {
                object: id,
                ordinal,
                position: world_position,
            });
            state.emitter_started = true;
        }
        state.radius += rules.radius_per_frame;
        let final_scan = state.radius >= rules.final_radius;
        if final_scan {
            state.radius = rules.final_radius;
        }
        let radius = state.radius;
        let squared_radius = radius * radius;
        let off_map = actor.is_off_map();
        let targets: Vec<_> = self
            .partition
            .fastest_range_objects(position, radius)
            .into_iter()
            .filter(|&target| {
                let actor = &self.entities[target];
                let dx = actor.position[0] - position[0];
                let dy = actor.position[1] - position[1];
                let x_squared = dx * dx;
                let y_squared = dy * dy;
                target != id
                    && x_squared + y_squared < squared_radius
                    && !actor.is_effectively_dead()
                    && actor.is_off_map() == off_map
                    && self.object_relationship(id, target) == 2
                    && actor.stealth.is_some()
                    && rules.kind_of.as_ref().map_or_else(
                        || !self.catalog[actor.type_index].kind_of.is_empty(),
                        |kinds| kinds.iter().any(|kind| self.attack_kind(target, kind)),
                    )
            })
            .collect();
        for target in targets {
            // receiveGrant rejects disguisers, but the original still flashes them.
            self.receive_stealth_grant(target, true, 0);
            if self.entities[target].has_drawable() {
                self.presentation
                    .stealth_events
                    .push(Event::GrantFlash { object: target });
            }
        }
        if final_scan {
            self.destroy_maintenance_object(id);
            return i32::from_ne_bytes(super::module_updates::FOREVER.to_ne_bytes());
        }
        1
    }

    pub(super) fn delete_gps_scrambler(&mut self, id: usize) {
        for (ordinal, state) in self.entities[id]
            .behaviors
            .gps_scramblers
            .iter_mut()
            .enumerate()
        {
            if std::mem::take(&mut state.emitter_started) {
                self.presentation
                    .stealth_events
                    .push(Event::DestroyGrantEmitter {
                        object: id,
                        ordinal,
                    });
            }
        }
    }
}

#[cfg(test)]
mod tests;
