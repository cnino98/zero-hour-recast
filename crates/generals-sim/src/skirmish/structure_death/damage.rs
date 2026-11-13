//! Native body-state callbacks: old systems retire even on healing or Water.
use super::{Skirmish, TRANSITION_SLOTS, VisualAction, physical_ocl};
use crate::skirmish::DamageType;

impl Skirmish {
    pub(in crate::skirmish) fn structure_damage_transition(&mut self, id: usize, old_health: f32) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if unit.behavior.damage_transitions.is_empty() {
            return;
        }
        let old = body_state(old_health / actor.max_health, unit.body_damage_thresholds);
        let new = body_state(actor.health / actor.max_health, unit.body_damage_thresholds);
        if old == new {
            return;
        }
        let water = matches!(actor.damage_state.damage_type, DamageType::Water);
        let (position, basis) = self.object_transform(id);
        let appearance = actor.disguised_appearance();
        let draw_type = appearance.map_or(actor.type_index, |appearance| appearance.type_index);
        let conditions = actor.has_drawable().then(|| {
            let mut conditions = self.model_flags(actor, unit.body_damage_thresholds);
            if appearance.is_some() {
                conditions.insert("DISGUISED".into());
            }
            conditions
        });
        let modules = unit.behavior.damage_transitions.len();
        for module in 0..modules {
            self.structure_visual(id, VisualAction::TransitionClear { module }, None);
            if new < old {
                continue;
            }
            for slot in 1..=TRANSITION_SLOTS {
                let unit = &self.catalog[self.entities[id].type_index];
                let rules = &unit.behavior.damage_transitions[module];
                let effect = (!water || !rules.suppress_water_fx)
                    .then(|| rules.effects[new].iter().find(|effect| effect.slot == slot))
                    .flatten()
                    .map(|effect| (effect.template.clone(), effect.local));
                let particle = (!water || !rules.suppress_water)
                    .then(|| {
                        rules.particles[new]
                            .iter()
                            .find(|particle| particle.slot == slot)
                    })
                    .flatten()
                    .map(|particle| {
                        let positions = if particle.random_bone {
                            conditions.as_ref().map_or_else(Vec::new, |conditions| {
                                transition_bones(
                                    &self.catalog[draw_type],
                                    conditions,
                                    &particle.bone,
                                )
                            })
                        } else {
                            vec![particle.position]
                        };
                        (particle.template.clone(), positions)
                    });
                if let Some((template, local)) = effect {
                    let offset = physical_ocl::transform::vector(basis, local);
                    self.structure_position_fx(
                        id,
                        template,
                        std::array::from_fn(|axis| position[axis] + offset[axis]),
                    );
                }
                if let Some((template, positions)) = particle {
                    let snapshot = self.snapshot_object_fx(id, String::new(), position, basis);
                    self.structure_visual(
                        id,
                        VisualAction::TransitionParticle {
                            module,
                            template,
                            positions,
                        },
                        Some(snapshot),
                    );
                }
            }
        }
    }
}

fn transition_bones(
    unit: &crate::skirmish::UnitType,
    conditions: &std::collections::BTreeSet<String>,
    prefix: &str,
) -> Vec<[f32; 3]> {
    let prefix = prefix.to_ascii_lowercase();
    let mut result = Vec::new();
    for draw in &unit.pristine_draws {
        let Some(state) = draw.select(conditions) else {
            continue;
        };
        for index in 1..=32 {
            let Some(matrix) = state.transforms.get(&format!("{prefix}{index:02}")) else {
                break;
            };
            result.push(matrix.map(|row| row[3]));
            if result.len() == 32 {
                return result;
            }
        }
    }
    result
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
