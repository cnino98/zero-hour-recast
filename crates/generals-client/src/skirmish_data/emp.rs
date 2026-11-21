//! INI data for `EMPUpdateModuleData`.
use super::{blocks, definition, duration_frames, field, scalar, source_real};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::emp::Rules;
use std::error::Error;

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
                .is_some_and(|name| name == "EMPUpdate")
        })
    else {
        return Ok(None);
    };
    let frames = |key, default| field(module, 1, key).map_or(Ok(default), duration_frames);
    let real =
        |key, default| field(module, 1, key).map_or(Ok(default), |line| source_real(scalar(line)?));
    let sparks = field(module, 1, "DisableFXParticleSystem")
        .map(scalar)
        .transpose()?
        .filter(|name| !name.eq_ignore_ascii_case("None"))
        .unwrap_or("");
    if !sparks.is_empty() {
        definition(files, "ParticleSystem", sparks)?;
    }
    let mut reject_allies = false;
    if let Some(line) = field(module, 1, "DoesNotAffect") {
        for name in &line.tokens[1..] {
            match name.to_ascii_uppercase().as_str() {
                "ALL" | "ALLIES" | "+ALLIES" => reject_allies = true,
                "NONE" | "-ALLIES" => reject_allies = false,
                _ => {}
            }
        }
    }
    let ignore_own_buildings = match field(module, 1, "DoesNotAffectMyOwnBuildings")
        .map(scalar)
        .transpose()?
    {
        Some(value) if value.eq_ignore_ascii_case("yes") => true,
        Some(value) if !value.eq_ignore_ascii_case("no") => {
            return Err("invalid EMP own-building flag".into());
        }
        _ => false,
    };
    let lifetime = frames("Lifetime", 1)?;
    let fade_frame = frames("StartFadeTime", 0)?;
    if fade_frame >= lifetime {
        return Err("EMPUpdate fade must begin before death".into());
    }
    Ok(Some(Rules {
        lifetime,
        fade_frame,
        disabled_frames: frames("DisabledDuration", 0)?,
        start_scale: real("StartScale", 1.0)?,
        target_scale: [real("TargetScaleMin", 1.0)?, real("TargetScaleMax", 1.0)?],
        start_color: field(module, 1, "StartColor").map_or(Ok([1.0; 3]), color)?,
        end_color: field(module, 1, "EndColor").map_or(Ok([0.0; 3]), color)?,
        sparks: sparks.into(),
        sparks_per_cubic_foot: real("SparksPerCubicFoot", 0.001)?,
        radius: real("EffectRadius", 200.0)?,
        reject_allies,
        ignore_own_buildings,
    }))
}

fn color(line: &ContentLine) -> Result<[f32; 3], Box<dyn Error>> {
    let mut tokens = line.tokens[1..]
        .iter()
        .flat_map(|token| token.split(':').filter(|part| !part.is_empty()));
    let mut rgb = [0.0; 3];
    for (channel, key) in rgb.iter_mut().zip(["R", "G", "B"]) {
        if tokens.next() != Some(key) {
            return Err("invalid EMP RGB color".into());
        }
        *channel = f32::from(
            tokens
                .next()
                .ok_or("missing EMP RGB component")?
                .parse::<u8>()?,
        ) / 255.0;
    }
    Ok(rgb)
}
