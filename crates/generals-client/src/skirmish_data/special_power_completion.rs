//! INI data for `SpecialPowerCompletionDieModuleData`.
use super::{blocks, definition, text};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::special_power_completion::Rules;
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
                .is_some_and(|name| name == "SpecialPowerCompletionDie")
        })
        .map(|module| {
            let power = text(module, 1, "SpecialPowerTemplate")?;
            definition(files, "SpecialPower", power)?;
            Ok(Rules {
                power: power.into(),
                conditions: super::death_callbacks::conditions(module)?,
            })
        })
        .collect()
}
