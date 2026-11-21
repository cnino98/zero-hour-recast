//! INI data for `WeaponBonusUpdateModuleData`.
use super::{blocks, field, scalar, source_numbers, source_real};
use generals_assets::content::Declaration;
use generals_sim::skirmish::{containment::bonuses::NAMES, weapon_bonus_update::Rules};
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "WeaponBonusUpdate")
        })
        .map(|module| {
            let flags =
                |key| field(module, 1, key).map_or_else(Vec::new, |line| line.tokens[1..].to_vec());
            let duration = |key| {
                field(module, 1, key)
                    .map_or(Ok(0), |line| source_numbers::duration_frames(scalar(line)?))
            };
            let value =
                scalar(field(module, 1, "BonusConditionType").ok_or("missing weapon bonus type")?)?;
            let bonus = NAMES
                .iter()
                .position(|name| name.eq_ignore_ascii_case(value))
                .ok_or("invalid temporary weapon bonus type")?;
            Ok(Rules {
                required: flags("RequiredAffectKindOf"),
                forbidden: flags("ForbiddenAffectKindOf"),
                duration: duration("BonusDuration")?,
                repeat: duration("BonusDelay")?,
                range: field(module, 1, "BonusRange")
                    .map_or(Ok(0.0), |line| source_real(scalar(line)?))?,
                bonus: bonus.try_into()?,
            })
        })
        .collect()
}
