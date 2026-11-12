//! Native `FireWeaponUpdate` parser preserves optional Weapon and ordered instances.
use crate::skirmish_data::{blocks, definition, duration_frames, field, scalar, weapons};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{WeaponRules, fire::weapon_update::Rules};
use std::error::Error;

pub(in crate::skirmish_data) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "FireWeaponUpdate")
        {
            continue;
        }
        let weapon = field(module, 1, "Weapon")
            .map(scalar)
            .transpose()?
            .filter(|name| !name.eq_ignore_ascii_case("NONE"));
        let weapon = weapon
            .map(|name| -> Result<_, Box<dyn Error>> {
                let mut rules = WeaponRules::unarmed();
                name.clone_into(&mut rules.name);
                weapons::parse(&mut rules, &definition(files, "Weapon", name)?.lines)?;
                Ok(rules)
            })
            .transpose()?;
        result.push(Rules {
            weapon,
            initial_delay: field(module, 1, "InitialDelay").map_or(Ok(0), duration_frames)?,
            exclusive_delay: field(module, 1, "ExclusiveWeaponDelay")
                .map_or(Ok(0), duration_frames)?,
        });
    }
    Ok(result)
}
