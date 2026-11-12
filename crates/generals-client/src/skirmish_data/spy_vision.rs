//! Original Internet Center self-powered `SpyVisionUpdate` modules.
use super::{blocks, definition, duration_frames, field, scalar};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::spy_vision::Rules;
use std::error::Error;

#[cfg(test)]
mod tests;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "SpyVisionUpdate")
        {
            continue;
        }
        let needs_upgrade = bool_field(module, "NeedsUpgrade")?;
        let self_powered = bool_field(module, "SelfPowered")?;
        for key in [
            "ConflictsWith",
            "RequiresAllTriggers",
            "RequiresAllConflictingTriggers",
            "RemoveUpgrade",
        ] {
            if field(module, 1, key).is_some() {
                return Err(format!("untranslated spy-vision mask {key}").into());
            }
        }
        let trigger = field(module, 1, "TriggeredBy")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .to_owned();
        if needs_upgrade && trigger.is_empty() {
            return Err("missing spy-vision trigger".into());
        }
        if !trigger.is_empty() {
            definition(files, "Upgrade", &trigger)?;
        }
        let command_centers_only = match field(module, 1, "SpyOnKindof").map(scalar).transpose()? {
            None | Some("ALL") => false,
            Some("COMMANDCENTER") => true,
            _ => return Err("untranslated spy-vision kind mask".into()),
        };
        result.push(Rules {
            needs_upgrade,
            self_powered,
            trigger,
            duration: field(module, 1, "SelfPoweredDuration").map_or(Ok(0), duration_frames)?,
            interval: field(module, 1, "SelfPoweredInterval").map_or(Ok(0), duration_frames)?,
            command_centers_only,
        });
    }
    Ok(result)
}

fn bool_field(
    lines: &[generals_assets::content::ContentLine],
    key: &str,
) -> Result<bool, Box<dyn Error>> {
    match field(lines, 1, key).map(scalar).transpose()? {
        None => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
        Some(_) => Err(format!("invalid SpyVisionUpdate {key}").into()),
    }
}
