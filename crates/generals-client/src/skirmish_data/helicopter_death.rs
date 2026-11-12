//! SH 2bfaea60 `HelicopterSlowDeathBehaviorModuleData`, native logic-frame units.
use super::{blocks, field, locomotor, physical_ocl, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{aircraft_death::Phase, helicopter_death::Rules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
    module: &[ContentLine],
) -> Result<Rules, Box<dyn Error>> {
    let phase = |fx, ocl| -> Result<Phase, Box<dyn Error>> {
        let objects = name(module, ocl)?;
        Ok(Phase {
            effect: name(module, fx)?,
            objects: (!objects.is_empty())
                .then(|| physical_ocl::load(files, &objects))
                .transpose()?,
        })
    };
    let angular = |key| -> Result<f32, Box<dyn Error>> {
        Ok(locomotor::signed_real(module, 1, key, 0.0)?
            * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0)))
    };
    let real_frames = |key| -> Result<f32, Box<dyn Error>> {
        Ok(locomotor::signed_real(module, 1, key, 0.0)? * (30.0 / 1000.0))
    };
    let fall = field(module, 1, "FallHowFast")
        .map(scalar)
        .transpose()?
        .unwrap_or("0%");
    let fall: f32 = fall.trim_end_matches('%').parse::<f32>()? * 0.01;
    if !fall.is_finite() {
        return Err("invalid helicopter fall percentage".into());
    }
    let pilot = name(module, "OCLEjectPilot")?;
    let sounds = blocks(&object.lines, 0, "UnitSpecificSounds")?;
    let sound = |key| -> Result<String, Box<dyn Error>> {
        sounds
            .first()
            .map_or(Ok(String::new()), |lines| name(lines, key))
    };
    Ok(Rules {
        orbit_turn: angular("SpiralOrbitTurnRate")?,
        orbit_speed: locomotor::per_frame(module, 1, "SpiralOrbitForwardSpeed", 0.0, 1.0 / 30.0)?,
        damping: locomotor::signed_real(module, 1, "SpiralOrbitForwardSpeedDamping", 1.0)?,
        spin: [angular("MinSelfSpin")?, angular("MaxSelfSpin")?],
        spin_delay: real_frames("SelfSpinUpdateDelay")?,
        spin_step: locomotor::signed_real(module, 1, "SelfSpinUpdateAmount", 0.0)?.to_radians(),
        fall,
        blade_delay: [
            real_frames("MinBladeFlyOffDelay")?,
            real_frames("MaxBladeFlyOffDelay")?,
        ],
        phases: [
            phase("FXBlade", "OCLBlade")?,
            phase("FXHitGround", "OCLHitGround")?,
            phase("FXFinalBlowUp", "OCLFinalBlowUp")?,
        ],
        final_delay: real_frames("DelayFromGroundToFinalDeath")?,
        rubble: name(module, "FinalRubbleObject")?,
        loop_sound: name(module, "SoundDeathLoop")?,
        braking: field(module, 1, "MaxBraking").map_or(Ok(99_999.0), |_| {
            locomotor::per_frame(module, 1, "MaxBraking", 0.0, (1.0 / 30.0) * (1.0 / 30.0))
        })?,
        particle: name(module, "AttachParticle")?,
        particle_bone: name(module, "AttachParticleBone")?,
        particle_point: field(module, 1, "AttachParticleLoc")
            .map(physical_ocl::coordinate)
            .transpose()?
            .unwrap_or([0.0; 3]),
        blade_bone: name(module, "BladeBoneName")?,
        blade_point: None,
        eject_pilot: (!pilot.is_empty())
            .then(|| {
                super::object_creation::parse_list(super::definition(
                    files,
                    "ObjectCreationList",
                    &pilot,
                )?)
            })
            .transpose()?,
        eject_sounds: [sound("VoiceEject")?, sound("SoundEject")?],
    })
}

fn name(module: &[ContentLine], key: &str) -> Result<String, Box<dyn Error>> {
    let value = field(module, 1, key).map(scalar).transpose()?.unwrap_or("");
    Ok(if value.eq_ignore_ascii_case("NONE") {
        String::new()
    } else {
        value.into()
    })
}
pub(super) fn validate(catalog: &[super::UnitType]) -> Result<(), Box<dyn Error>> {
    for unit in catalog {
        for rule in unit.helicopter_death_rules() {
            if rule
                .blade_delay
                .iter()
                .any(|&delay| !(0.0..4_294_967_296.0).contains(&delay))
            {
                return Err(format!(
                    "{}: helicopter blade delay outside native unsigned conversion domain",
                    unit.name
                )
                .into());
            }
            if !rule.rubble.is_empty() && !catalog.iter().any(|unit| unit.name == rule.rubble) {
                return Err(
                    format!("{}: missing helicopter rubble {}", unit.name, rule.rubble).into(),
                );
            }
        }
    }
    Ok(())
}
