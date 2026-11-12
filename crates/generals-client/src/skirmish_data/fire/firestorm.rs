//! Native sixteen particle template slots and independent firestorm declarations.
use crate::skirmish_data::{blocks, duration_frames, field, number, scalar};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::fire::{firestorm::Rules, geometry};
use std::error::Error;

pub(in crate::skirmish_data) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "FirestormDynamicGeometryInfoUpdate")
        {
            continue;
        }
        let dimensions = |prefix: &str| -> Result<_, Box<dyn Error>> {
            Ok([
                real(module, &format!("{prefix}Height"), 0.0)?,
                real(module, &format!("{prefix}MajorRadius"), 0.0)?,
                real(module, &format!("{prefix}MinorRadius"), 0.0)?,
            ])
        };
        let mut particles = std::array::from_fn(|_| None);
        for (index, entry) in particles.iter_mut().enumerate() {
            if let Some(name) = field(module, 1, &format!("ParticleSystem{}", index + 1))
                .map(scalar)
                .transpose()?
                && !name.eq_ignore_ascii_case("NONE")
            {
                *entry = Some(name.into());
            }
        }
        let reverse = field(module, 1, "ReverseAtTransitionTime")
            .map(scalar)
            .transpose()?
            .map_or(Ok(false), |value| {
                if value.eq_ignore_ascii_case("Yes") {
                    Ok(true)
                } else if value.eq_ignore_ascii_case("No") {
                    Ok(false)
                } else {
                    Err("invalid native reverse geometry boolean")
                }
            })?;
        result.push(Rules {
            geometry: geometry::Rules {
                delay: field(module, 1, "InitialDelay").map_or(Ok(0), duration_frames)?,
                initial: dimensions("Initial")?,
                final_size: dimensions("Final")?,
                transition: field(module, 1, "TransitionTime").map_or(Ok(1), duration_frames)?,
                reverse,
            },
            interval: real(module, "DelayBetweenDamageFrames", 0.0)? * (30.0 / 1000.0),
            damage: real(module, "DamageAmount", 0.0)?,
            height: real(module, "MaxHeightForDamage", 20.0)?,
            particles,
            particle_height: real(module, "ParticleOffsetZ", 0.0)?,
            effect: field(module, 1, "FXList")
                .map(scalar)
                .transpose()?
                .unwrap_or("")
                .into(),
            scorch: real(module, "ScorchSize", 0.0)?,
        });
    }
    Ok(result)
}

fn real(module: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(default), |_| number(module, 1, key))
}
