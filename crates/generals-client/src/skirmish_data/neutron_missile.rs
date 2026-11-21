//! Original neutron missile flight and derived slow-death fields.
use super::{blocks, definition, field, fx_definition, scalar, source_numbers, source_real};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::neutron_missile::{Blast, DeathRules, Rules};
use std::{error::Error, f32::consts::PI};

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let Some(module) = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "NeutronMissileUpdate")
        })
    else {
        return Ok(None);
    };
    let turn_rate = field(module, 1, "MaxTurnRate").map_or(Ok(999.0), |line| {
        Ok::<_, Box<dyn Error>>(source_real(scalar(line)?)? * ((1.0 / 30.0) * (PI / 180.0)))
    })?;
    let weapon = definition(files, "Weapon", "NeutronMissileWeapon")?;
    let exhaust = field(&weapon.lines, 0, "ProjectileExhaust")
        .map(scalar)
        .transpose()?
        .filter(|name| !name.eq_ignore_ascii_case("None"))
        .unwrap_or("");
    if !exhaust.is_empty() {
        definition(files, "ParticleSystem", exhaust)?;
    }
    Ok(Some(Rules {
        initial_distance: real(module, "DistanceToTravelBeforeTurning", 0.0)?,
        turn_rate,
        damping: real(module, "ForwardDamping", 0.0)?,
        speed: real(module, "RelativeSpeed", 1.0)?,
        approach_height: real(module, "TargetFromDirectlyAbove", 0.0)?,
        rise_frames: field(module, 1, "SpecialSpeedTime")
            .map_or(Ok(0), |line| source_numbers::duration_frames(scalar(line)?))?,
        rise_height: real(module, "SpecialSpeedHeight", 0.0)?,
        rise_acceleration: real(module, "SpecialAccelFactor", 1.0)?,
        jitter: real(module, "SpecialJitterDistance", 0.0)?,
        launch_fx: effect(files, module, "LaunchFX")?,
        ignition_fx: effect(files, module, "IgnitionFX")?,
        decal: super::spectre::decal(module, "DeliveryDecal")?,
        decal_radius: real(module, "DeliveryDecalRadius", 0.0)?,
        exhaust: exhaust.into(),
    }))
}

pub(super) fn load_death(
    files: &[ContentFile],
    module: &[ContentLine],
) -> Result<DeathRules, Box<dyn Error>> {
    let mut blasts = Vec::new();
    for index in 1..=9 {
        let read = |suffix| real(module, &format!("Blast{index}{suffix}"), 0.0);
        let Some(enabled) = field(module, 1, &format!("Blast{index}Enabled"))
            .map(scalar)
            .transpose()?
        else {
            continue;
        };
        if enabled.eq_ignore_ascii_case("no") {
            continue;
        }
        if !enabled.eq_ignore_ascii_case("yes") {
            return Err("invalid neutron wave Enabled flag".into());
        }
        // Native BlastInfo delays are Real frames, unlike integer SlowDeath delays.
        blasts.push(Blast {
            delay: read("Delay")? * (30.0 / 1000.0),
            scorch_delay: read("ScorchDelay")? * (30.0 / 1000.0),
            inner_radius: read("InnerRadius")?,
            outer_radius: read("OuterRadius")?,
            max_damage: read("MaxDamage")?,
            min_damage: read("MinDamage")?,
            topple_speed: read("ToppleSpeed")?,
        });
    }
    Ok(DeathRules {
        scorch_size: real(module, "ScorchMarkSize", 0.0)?,
        effect: effect(files, module, "FXList")?,
        blasts,
    })
}

fn real(lines: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(lines, 1, key).map_or(Ok(default), |line| source_real(scalar(line)?))
}

fn effect(
    files: &[ContentFile],
    lines: &[ContentLine],
    key: &str,
) -> Result<String, Box<dyn Error>> {
    let Some(name) = field(lines, 1, key).map(scalar).transpose()? else {
        return Ok(String::new());
    };
    Ok(fx_definition(files, name)?.map_or(String::new(), |_| name.into()))
}
