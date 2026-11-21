//! Authored `LeafletDropBehavior` and `SmartBombTargetHomingUpdate`.
use super::{blocks, field, finite_number, scalar, source_numbers};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::{leaflet_drop, smart_bomb};
use std::error::Error;

pub(super) fn leaflets(object: &Declaration) -> Result<Vec<leaflet_drop::Rules>, Box<dyn Error>> {
    modules(object, "LeafletDropBehavior")?
        .map(|module| {
            let duration = |key, default| {
                field(module, 1, key).map_or(Ok(default), |line| {
                    source_numbers::duration_frames(scalar(line)?)
                })
            };
            Ok(leaflet_drop::Rules {
                delay: duration("Delay", 1)?,
                disabled: duration("DisabledDuration", 0)?,
                radius: field(module, 1, "AffectRadius")
                    .map_or(Ok(60.0), |line| finite_number(scalar(line)?))?,
                particle: field(module, 1, "LeafletFXParticleSystem")
                    .map(scalar)
                    .transpose()?
                    .unwrap_or("")
                    .into(),
            })
        })
        .collect()
}

pub(super) fn smart_bombs(object: &Declaration) -> Result<Vec<smart_bomb::Rules>, Box<dyn Error>> {
    modules(object, "SmartBombTargetHomingUpdate")?
        .map(|module| {
            Ok(smart_bomb::Rules {
                course_scalar: field(module, 1, "CourseCorrectionScalar")
                    .map_or(Ok(0.99), |line| finite_number(scalar(line)?))?,
            })
        })
        .collect()
}

fn modules<'a>(
    object: &'a Declaration,
    family: &'a str,
) -> Result<impl Iterator<Item = &'a [ContentLine]> + 'a, Box<dyn Error>> {
    Ok(blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(move |module| module[0].tokens.get(1).is_some_and(|name| name == family)))
}
