//! Actual zero-radius `VeterancyCrateCollide`, including original Pilot fields.
use super::{blocks, field, finite_number, fx_definition, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::veterancy_crate::{Restrictions, Rules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "VeterancyCrateCollide")
        })
        .map(|module| parse(files, module))
        .collect()
}

fn parse(files: &[ContentFile], module: &[ContentLine]) -> Result<Rules, Box<dyn Error>> {
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        if !matches!(
            line.tokens.first().map(String::as_str),
            Some(
                "RequiredKindOf"
                    | "ForbiddenKindOf"
                    | "ForbidOwnerPlayer"
                    | "BuildingPickup"
                    | "HumanOnly"
                    | "AllowMultiPickup"
                    | "PickupScience"
                    | "ExecuteFX"
                    | "ExecuteAnimation"
                    | "ExecuteAnimationTime"
                    | "ExecuteAnimationZRise"
                    | "ExecuteAnimationFades"
                    | "EffectRange"
                    | "AddsOwnerVeterancy"
                    | "IsPilot"
                    | "End"
            )
        ) {
            return Err("untranslated VeterancyCrateCollide field".into());
        }
    }
    let text = |key| field(module, 1, key).map(scalar).transpose();
    if finite_number(text("EffectRange")?.unwrap_or("0"))? != 0.0 {
        return Err(
            "VeterancyCrateCollide radius dispatch requires source partition iteration".into(),
        );
    }
    if text("ExecuteAnimation")?
        .is_some_and(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
    {
        return Err("VeterancyCrateCollide world animation is not translated".into());
    }
    // The native base never reads ExecuteAnimationFades; it always supplies
    // WORLD_ANIM_FADE_ON_EXPIRE. Validate the authored boolean without storing it.
    boolean(module, "ExecuteAnimationFades", true)?;
    for key in ["ExecuteAnimationTime", "ExecuteAnimationZRise"] {
        if let Some(value) = text(key)? {
            finite_number(value)?;
        }
    }
    let science = text("PickupScience")?
        .filter(|name| !name.eq_ignore_ascii_case("NONE"))
        .map(str::to_owned);
    Ok(Rules {
        restrictions: Restrictions {
            required: kinds(module, "RequiredKindOf")?,
            forbidden: kinds(module, "ForbiddenKindOf")?,
            forbid_owner: boolean(module, "ForbidOwnerPlayer", false)?,
            building_pickup: boolean(module, "BuildingPickup", false)?,
            human_only: boolean(module, "HumanOnly", false)?,
            science,
        },
        adds_owner_veterancy: boolean(module, "AddsOwnerVeterancy", false)?,
        is_pilot: boolean(module, "IsPilot", false)?,
        allow_multi_pickup: boolean(module, "AllowMultiPickup", false)?,
        execute_fx: match text("ExecuteFX")? {
            Some(name) if fx_definition(files, name)?.is_some() => name.into(),
            _ => String::new(),
        },
    })
}

fn kinds(module: &[ContentLine], key: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let mut result = Vec::new();
    if let Some(line) = field(module, 1, key) {
        for word in &line.tokens[1..] {
            if word == "NONE" {
                result.clear();
            } else if word == "ALL" {
                return Err(
                    "VeterancyCrateCollide ALL kind mask requires complete kind registry".into(),
                );
            } else if let Some(name) = word.strip_prefix('-') {
                result.retain(|kind| kind != name);
            } else {
                let name = word.strip_prefix('+').unwrap_or(word);
                if !result.iter().any(|kind| kind == name) {
                    result.push(name.to_owned());
                }
            }
        }
    }
    Ok(result)
}

fn boolean(module: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(default), |line| {
        let value = scalar(line)?;
        if value.eq_ignore_ascii_case("Yes") {
            Ok(true)
        } else if value.eq_ignore_ascii_case("No") {
            Ok(false)
        } else {
            Err(format!("invalid VeterancyCrateCollide {key}").into())
        }
    })
}

#[cfg(test)]
mod tests;
