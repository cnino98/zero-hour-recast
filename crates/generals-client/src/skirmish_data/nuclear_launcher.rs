//! INI data for `MissileLauncherBuildingUpdateModuleData`.
use super::{blocks, definition, duration_frames, field, fx_definition, scalar, text};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::nuclear_launcher::Rules;
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
                .is_some_and(|name| name == "MissileLauncherBuildingUpdate")
        })
    else {
        return Ok(None);
    };
    let power = text(module, 1, "SpecialPowerTemplate")?;
    definition(files, "SpecialPower", power)?;
    let mut effects = std::array::from_fn(|_| String::new());
    for (effect, key) in effects.iter_mut().zip([
        "DoorClosedFX",
        "DoorOpeningFX",
        "DoorOpenFX",
        "DoorWaitingToCloseFX",
        "DoorClosingFX",
    ]) {
        if let Some(name) = field(module, 1, key).map(scalar).transpose()?
            && fx_definition(files, name)?.is_some()
        {
            *effect = name.into();
        }
    }
    Ok(Some(Rules {
        power: power.into(),
        open_frames: field(module, 1, "DoorOpenTime").map_or(Ok(0), duration_frames)?,
        wait_open_frames: field(module, 1, "DoorWaitOpenTime").map_or(Ok(0), duration_frames)?,
        close_frames: field(module, 1, "DoorCloseTime").map_or(Ok(0), duration_frames)?,
        effects,
        open_sound: field(module, 1, "DoorOpenIdleAudio")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
    }))
}
