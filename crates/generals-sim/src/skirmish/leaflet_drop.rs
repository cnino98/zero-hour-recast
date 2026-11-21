//! `LeafletDropBehavior` in the original EMPUpdate.cpp.
use super::{
    Skirmish, WeaponFxEvent, helicopter_death::ParticleAttachment, module_updates::FOREVER,
};

pub struct Rules {
    pub delay: u32,
    pub disabled: u32,
    pub radius: f32,
    pub particle: String,
}

pub(in crate::skirmish) struct State {
    start: u32,
    fx_fired: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_leaflet_drop(&mut self, id: usize, ordinal: usize) {
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .leaflet_drops[ordinal];
        let start = self.update_frame().wrapping_add(rules.delay);
        self.entities[id].behaviors.leaflet_drops.push(State {
            start,
            fx_fired: false,
        });
    }

    pub(in crate::skirmish) fn update_leaflet_drop(&mut self, id: usize, ordinal: usize) -> i32 {
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        let state = &mut actor.behaviors.leaflet_drops[ordinal];
        if !state.fx_fired {
            let name = self.catalog[actor.type_index].behavior.leaflet_drops[ordinal]
                .particle
                .clone();
            if !name.is_empty() {
                self.presentation
                    .weapon_fx
                    .push(WeaponFxEvent::AttachedParticle(
                        self.presentation.attached_particles.len(),
                    ));
                self.presentation
                    .attached_particles
                    .push(ParticleAttachment {
                        object: id,
                        name,
                        point: [0.0; 3],
                    });
            }
            state.fx_fired = true;
        }
        if now < state.start {
            return i32::from_ne_bytes(FOREVER.to_ne_bytes());
        }
        self.leaflet_disable(id, ordinal);
        1
    }

    /// The container also performs the pulse on physical death.
    pub(in crate::skirmish) fn leaflet_disable(&mut self, id: usize, ordinal: usize) {
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .leaflet_drops[ordinal];
        let deadline = self.update_frame().wrapping_add(rules.disabled);
        for victim in self.emp_range_targets(id, rules.radius) {
            let actor = &self.entities[victim];
            if self.object_relationship(victim, id) == 0
                && self.catalog[actor.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| matches!(kind.as_str(), "INFANTRY" | "VEHICLE"))
            {
                self.set_emp_disabled_until(victim, deadline);
            }
        }
    }
}

#[cfg(test)]
mod tests;
