//! Native body-state callbacks: old systems retire even on healing or Water.
use super::{Skirmish, VisualAction, physical_ocl};
use crate::skirmish::DamageType;

impl Skirmish {
    pub(in crate::skirmish) fn structure_damage_transition(&mut self, id: usize, old_health: f32) {
        let actor = &self.entities[id];
        let Some(rules) = &self.catalog[actor.type_index].structure_death else {
            return;
        };
        let old = body_state(old_health / actor.max_health, rules.damage_thresholds);
        let new = body_state(actor.health / actor.max_health, rules.damage_thresholds);
        if old == new {
            return;
        }
        let water = matches!(actor.damage_state.damage_type, DamageType::Water);
        let transitions: Vec<_> = rules
            .transitions
            .iter()
            .enumerate()
            .map(|(module, rules)| {
                let particles: Vec<_> = if new > old && (!water || !rules.suppress_water) {
                    rules.particles[new]
                        .iter()
                        .map(|particle| (particle.template.clone(), particle.position))
                        .collect()
                } else {
                    Vec::new()
                };
                (module, particles)
            })
            .collect();
        let position = self.object_position(id);
        let basis = self.entities[id]
            .physical_basis()
            .unwrap_or_else(|| physical_ocl::transform::yaw(self.entities[id].facing));
        for (module, particles) in transitions {
            self.structure_visual(id, VisualAction::TransitionClear { module }, None);
            for (template, local) in particles {
                let snapshot = self.snapshot_object_fx(id, String::new(), position, basis);
                self.structure_visual(
                    id,
                    VisualAction::TransitionParticle {
                        module,
                        template,
                        local,
                    },
                    Some(snapshot),
                );
            }
        }
    }
}

fn body_state(ratio: f32, thresholds: [f32; 2]) -> usize {
    if ratio > thresholds[0] {
        0
    } else if ratio > thresholds[1] {
        1
    } else if ratio > 0.0 {
        2
    } else {
        3
    }
}
