//! Original jet death timings and physical OCL dependencies.
use super::{field, locomotor, physical_ocl, scalar};
use generals_assets::content::{ContentFile, ContentLine};
use generals_sim::skirmish::aircraft_death::{Jet, Phase};
use std::error::Error;

#[cfg(test)]
mod tests;

pub(super) fn validate_targets(catalog: &[super::UnitType]) -> Result<(), Box<dyn Error>> {
    for unit in catalog {
        for list in generals_sim::skirmish::physical_ocl::lists(unit) {
            for nugget in &list.nuggets {
                for name in &nugget.names {
                    let target = if nugget.debris_mass.is_some() {
                        "GenericDebris"
                    } else {
                        name
                    };
                    if !catalog
                        .iter()
                        .any(|unit| unit.name == target && unit.physical.is_some())
                    {
                        return Err(format!(
                            "{}: physical death OCL {} requires admitted physical object {target}",
                            unit.name, list.name
                        )
                        .into());
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn jet(files: &[ContentFile], module: &[ContentLine]) -> Result<Jet, Box<dyn Error>> {
    let phase = |fx, ocl| -> Result<Phase, Box<dyn Error>> {
        let effect = name(module, fx)?;
        let ocl = name(module, ocl)?;
        Ok(Phase {
            effect,
            objects: (!ocl.is_empty())
                .then(|| physical_ocl::load(files, &ocl))
                .transpose()?,
        })
    };
    Ok(Jet {
        phases: [
            phase("FXOnGroundDeath", "OCLOnGroundDeath")?,
            phase("FXInitialDeath", "OCLInitialDeath")?,
            phase("FXSecondary", "OCLSecondary")?,
            phase("FXHitGround", "OCLHitGround")?,
            phase("FXFinalBlowUp", "OCLFinalBlowUp")?,
        ],
        secondary_delay: frames(module, "DelaySecondaryFromInitialDeath")?,
        final_delay: frames(module, "DelayFinalBlowUpFromHitGround")?,
        roll_rate: locomotor::signed_real(module, 1, "RollRate", 0.0)?,
        roll_delta: percent(module, "RollRateDelta", 1.0)?,
        pitch_rate: locomotor::signed_real(module, 1, "PitchRate", 0.0)?,
        fall_speed: percent(module, "FallHowFast", 0.0)?,
        loop_sound: name(module, "DeathLoopSound")?,
    })
}

fn name(module: &[ContentLine], key: &str) -> Result<String, Box<dyn Error>> {
    let name = field(module, 1, key).map(scalar).transpose()?.unwrap_or("");
    Ok(if name.eq_ignore_ascii_case("NONE") {
        String::new()
    } else {
        name.into()
    })
}
fn frames(module: &[ContentLine], key: &str) -> Result<u32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(0), |line| {
        super::source_numbers::duration_frames(scalar(line)?)
    })
}
fn percent(module: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(default), |line| {
        let value: f32 = scalar(line)?.trim_end_matches('%').parse()?;
        if !value.is_finite() {
            return Err("invalid aircraft death percentage".into());
        }
        Ok(value * 0.01)
    })
}
