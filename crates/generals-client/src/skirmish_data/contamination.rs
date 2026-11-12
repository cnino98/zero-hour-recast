//! Original cooldown OCL modules retain their own field and upgrade conditions.
use super::{blocks, duration_frames, field, hazards, scalar, text, unsigned};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::contamination::Rules;
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
                .is_some_and(|name| name == "FireOCLAfterWeaponCooldownUpdate")
        })
        .map(|module| {
            let slot = match field(module, 1, "WeaponSlot")
                .map(scalar)
                .transpose()?
                .unwrap_or("PRIMARY")
            {
                "PRIMARY" => 0,
                "SECONDARY" => 1,
                "TERTIARY" => 2,
                _ => return Err("invalid cooldown OCL weapon slot".into()),
            };
            Ok(Rules {
                slot,
                minimum_shots: field(module, 1, "MinShotsToCreateOCL")
                    .map_or(Ok(1), |_| unsigned(module, 1, "MinShotsToCreateOCL"))?,
                lifetime_per_second: field(module, 1, "OCLLifetimePerSecond")
                    .map_or(Ok(1000), |_| unsigned(module, 1, "OCLLifetimePerSecond"))?,
                maximum_lifetime: field(module, 1, "OCLLifetimeMaxCap")
                    .map_or(Ok(1000), duration_frames)?,
                triggers: super::salvage::words(module, 1, "TriggeredBy"),
                conflicts: super::salvage::words(module, 1, "ConflictsWith"),
                requires_all: requires_all(module)?,
                field: hazards::hazard(files, text(module, 1, "OCL")?)?,
            })
        })
        .collect()
}

fn requires_all(module: &[ContentLine]) -> Result<bool, Box<dyn Error>> {
    match field(module, 1, "RequiresAllTriggers")
        .map(scalar)
        .transpose()?
    {
        None => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("No") => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("Yes") => Ok(true),
        Some(_) => Err("invalid cooldown OCL RequiresAllTriggers".into()),
    }
}

#[cfg(test)]
mod tests;
