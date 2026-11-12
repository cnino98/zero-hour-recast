//! `StealthDetectorUpdate`'s visible scan callbacks and pristine IR bone attachment.
use super::{Particles, Pose, Template};
use crate::model_animation::Animations;
use generals_sim::skirmish::{Skirmish, UnitType, stealth::Event};
use std::{collections::BTreeMap, error::Error};

pub(super) type Bindings = BTreeMap<(usize, usize), [Option<usize>; 3]>;

pub(super) fn names(types: &[UnitType]) -> impl Iterator<Item = &str> {
    types
        .iter()
        .flat_map(|unit| &unit.behavior.detectors)
        .filter_map(|rules| rules.feedback.as_ref())
        .flat_map(|feedback| feedback.particles.iter().map(String::as_str))
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
}

pub(super) fn load(types: &[UnitType], templates: &[Template]) -> Result<Bindings, Box<dyn Error>> {
    let mut result = BTreeMap::new();
    for (unit, rules) in types.iter().enumerate() {
        for (module, detector) in rules.behavior.detectors.iter().enumerate() {
            let Some(feedback) = &detector.feedback else {
                continue;
            };
            let mut bindings = [None; 3];
            for (slot, name) in bindings.iter_mut().zip(&feedback.particles) {
                if !name.is_empty() && !name.eq_ignore_ascii_case("NONE") {
                    *slot = Some(
                        templates
                            .iter()
                            .position(|template| template.name.eq_ignore_ascii_case(name))
                            .ok_or_else(|| {
                                format!("{}: missing detector ParticleSystem {name}", rules.name)
                            })?,
                    );
                }
            }
            result.insert((unit, module), bindings);
        }
    }
    Ok(result)
}

pub(super) struct Scan {
    object: usize,
    pose: Pose,
    offset: [f32; 3],
    wind_origin: [f32; 3],
    templates: [Option<usize>; 2],
}

impl Particles {
    pub(super) fn detector_scans(&self, game: &Skirmish, animations: &Animations) -> Vec<Scan> {
        game.presentation
            .stealth_events
            .iter()
            .filter_map(|event| {
                let Event::Scan {
                    detector,
                    module,
                    found,
                    feedback: Some(event),
                } = event
                else {
                    return None;
                };
                if event.visible_players & 1 == 0 {
                    return None;
                }
                let bindings = self.detectors.get(&(event.type_index, *module))?;
                let feedback = game.catalog[event.type_index].behavior.detectors[*module]
                    .feedback
                    .as_ref()?;
                let offset = animations.detector_bone_position(
                    event.type_index,
                    event.health_ratio,
                    &feedback.bone,
                );
                let (sine, cosine) = event.facing.sin_cos();
                let mut pose = Pose {
                    position: event.position,
                    axes: [[cosine, sine, 0.0], [-sine, cosine, 0.0], [0.0, 0.0, 1.0]],
                };
                let rotated = pose.rotate(offset);
                for (value, delta) in pose.position.iter_mut().zip(rotated) {
                    *value += delta;
                }
                Some(Scan {
                    object: *detector,
                    pose,
                    offset,
                    wind_origin: std::array::from_fn(|axis| event.position[axis] + offset[axis]),
                    templates: [bindings[usize::from(*found)], bindings[2]],
                })
            })
            .collect()
    }

    pub(super) fn emit_detector_scans(
        &mut self,
        scans: Vec<Scan>,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        for scan in scans {
            for template in scan.templates.into_iter().flatten() {
                let id = self.create(template, scan.pose.clone(), None, random);
                let emitter = self
                    .emitters
                    .iter_mut()
                    .find(|emitter| emitter.id == id)
                    .expect("created scan system");
                emitter.object_attachment = Some((scan.object, Pose::at(scan.offset)));
                emitter.wind_origin = Some(scan.wind_origin);
            }
        }
    }
}

#[cfg(test)]
mod tests;
