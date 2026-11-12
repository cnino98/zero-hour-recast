//! Native `EjectPilotDieModuleData`; OCL pointers are bound to the ordinary runtime.
use super::{blocks, death_callbacks, definition, duration_frames, field, object_creation, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{eject_pilot::Rules, object_creation::List};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    let sound_blocks = blocks(&object.lines, 0, "UnitSpecificSounds")?;
    let sound = |key| -> Result<String, Box<dyn Error>> {
        sound_blocks.first().map_or(Ok(String::new()), |lines| {
            Ok(field(lines, 1, key)
                .map(scalar)
                .transpose()?
                .unwrap_or("")
                .into())
        })
    };
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "EjectPilotDie")
        {
            continue;
        }
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            if !matches!(
                line.tokens[0].as_str(),
                "DeathTypes"
                    | "VeterancyLevels"
                    | "ExemptStatus"
                    | "RequiredStatus"
                    | "AirCreationList"
                    | "GroundCreationList"
                    | "InvulnerableTime"
            ) {
                return Err(format!("unsupported EjectPilotDie field {}", line.tokens[0]).into());
            }
        }
        // Native stores this module field but never reads it. Actual pilot
        // invulnerability comes from the selected GenericObjectCreationNugget.
        if let Some(line) = field(module, 1, "InvulnerableTime") {
            duration_frames(line)?;
        }
        result.push(Rules {
            conditions: death_callbacks::conditions(module)?,
            ground: list(files, module, "GroundCreationList")?,
            airborne: list(files, module, "AirCreationList")?,
            sounds: [sound("VoiceEject")?, sound("SoundEject")?],
        });
    }
    Ok(result)
}

fn list(
    files: &[ContentFile],
    module: &[ContentLine],
    key: &str,
) -> Result<Option<List>, Box<dyn Error>> {
    let name = field(module, 1, key).map(scalar).transpose()?.unwrap_or("");
    if name.is_empty() || name.eq_ignore_ascii_case("NONE") {
        return Ok(None);
    }
    object_creation::parse_list(definition(files, "ObjectCreationList", name)?).map(Some)
}

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut result = Vec::new();
    for name in selected {
        let object = definition(files, "Object", name)?;
        for rules in load(files, object)? {
            for list in [rules.ground, rules.airborne].into_iter().flatten() {
                for nugget in list.objects {
                    for name in nugget
                        .names
                        .iter()
                        .chain(std::iter::once(&nugget.container))
                        .filter(|name| !name.is_empty())
                    {
                        let name = definition(files, "Object", name)?.name.as_str();
                        if !selected.contains(&name) && !result.contains(&name) {
                            result.push(name);
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
