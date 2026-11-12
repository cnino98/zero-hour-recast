use super::{OBJECTS, duration, optional};
use crate::skirmish_data::{blocks, field, scalar};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::fire::{Body, Rules, Storm, geometry};
use std::error::Error;

pub(in crate::skirmish_data) fn rules(
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    if !OBJECTS.contains(&object.name.as_str()) {
        return Ok(None);
    }
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let find = |name| {
        modules
            .iter()
            .copied()
            .find(|module| module[0].tokens.get(1).is_some_and(|value| value == name))
    };
    let lifetime = find("DeletionUpdate")
        .or_else(|| find("LifetimeUpdate"))
        .ok_or("fire object lacks lifetime")?;
    let lifetime = [
        duration(lifetime, "MinLifetime", 0)?,
        duration(lifetime, "MaxLifetime", 0)?,
    ];
    if lifetime[1] < lifetime[0] {
        return Err("inverted fire object lifetime".into());
    }
    Ok(Some(Rules {
        ambient: field(&object.lines, 0, "SoundAmbient")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
        body: match blocks(&object.lines, 0, "Body")?
            .first()
            .and_then(|module| module[0].tokens.get(1))
            .map(String::as_str)
        {
            Some("InactiveBody") => Body::Inactive,
            Some("ImmortalBody") => Body::Immortal,
            _ => return Err("untranslated fire object body".into()),
        },
        lifetime,
        initial_delay: find("FireWeaponUpdate")
            .map_or(Ok(0), |module| duration(module, "InitialDelay", 0))?,
        storm: find("FirestormDynamicGeometryInfoUpdate")
            .map(storm)
            .transpose()?,
    }))
}

fn storm(module: &[ContentLine]) -> Result<Storm, Box<dyn Error>> {
    let mut particles = Vec::new();
    for index in 1..=16 {
        if let Some(line) = field(module, 1, &format!("ParticleSystem{index}")) {
            let name = scalar(line)?;
            if !name.eq_ignore_ascii_case("NONE") {
                particles.push(name.into());
            }
        }
    }
    let dimensions = |prefix: &str| -> Result<_, Box<dyn Error>> {
        Ok([
            optional(module, &format!("{prefix}Height"), 0.0)?,
            optional(module, &format!("{prefix}MajorRadius"), 0.0)?,
            optional(module, &format!("{prefix}MinorRadius"), 0.0)?,
        ])
    };
    let geometry = geometry::Rules {
        delay: duration(module, "InitialDelay", 0)?,
        initial: dimensions("Initial")?,
        final_size: dimensions("Final")?,
        transition: duration(module, "TransitionTime", 1)?,
        reverse: match field(module, 1, "ReverseAtTransitionTime")
            .map(scalar)
            .transpose()?
        {
            None | Some("No") => false,
            Some("Yes") => true,
            _ => return Err("invalid reverse-geometry boolean".into()),
        },
    };
    if geometry.transition == 0 {
        return Err("zero fire geometry transition".into());
    }
    Ok(Storm {
        geometry,
        interval: optional(module, "DelayBetweenDamageFrames", 0.0)? * (30.0 / 1000.0),
        damage: optional(module, "DamageAmount", 0.0)?,
        height: optional(module, "MaxHeightForDamage", 20.0)?,
        particles,
        particle_height: optional(module, "ParticleOffsetZ", 0.0)?,
        effect: field(module, 1, "FXList")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
        scorch: optional(module, "ScorchSize", 0.0)?,
    })
}
