//! Original typed weapon deaths and Terrorist `FireWeaponWhenDeadBehavior` modules.
use super::{blocks, field, scalar, weapons};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{DeathType, WeaponRules, death_weapons::Rules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "FireWeaponWhenDeadBehavior")
        {
            continue;
        }
        let mut rules = Rules {
            weapon: None,
            death_mask: u32::MAX,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
            starts_active: false,
            triggers: Vec::new(),
            conflicts: Vec::new(),
            requires_all: false,
            removals: Vec::new(),
            upgrade_fx: String::new(),
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens[0].as_str() {
                "DeathWeapon" => {
                    let name = scalar(line)?;
                    rules.weapon = if let Some(declaration) =
                        super::instant_death::nullable(files, "Weapon", name)?
                    {
                        let mut weapon = WeaponRules::unarmed();
                        name.clone_into(&mut weapon.name);
                        weapons::parse(&mut weapon, &declaration.lines)?;
                        Some(weapon)
                    } else {
                        None
                    };
                }
                "DeathTypes" => rules.death_mask = mask(Some(line))?,
                "VeterancyLevels" => rules.veterancy_mask = super::instant_death::veterancy(line)?,
                "ExemptStatus" => {
                    rules.exempt_status = super::instant_death::status(line, rules.exempt_status)?;
                }
                "RequiredStatus" => {
                    rules.required_status =
                        super::instant_death::status(line, rules.required_status)?;
                }
                "StartsActive" => {
                    rules.starts_active = boolean(std::slice::from_ref(line), "StartsActive")?;
                }
                "RequiresAllTriggers" => {
                    rules.requires_all =
                        boolean(std::slice::from_ref(line), "RequiresAllTriggers")?;
                }
                "TriggeredBy" => rules.triggers = line.tokens[1..].to_vec(),
                "ConflictsWith" => rules.conflicts = line.tokens[1..].to_vec(),
                "Removes" => rules.removals = line.tokens[1..].to_vec(),
                "FXListUpgrade" => {
                    let name = scalar(line)?;
                    rules.upgrade_fx =
                        if super::instant_death::nullable(files, "FXList", name)?.is_some() {
                            name.to_owned()
                        } else {
                            String::new()
                        };
                }
                _ => {}
            }
        }
        result.push(rules);
    }
    Ok(result)
}

fn boolean(lines: &[ContentLine], key: &str) -> Result<bool, Box<dyn Error>> {
    match field(lines, 1, key).map(scalar).transpose()? {
        None => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("Yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("No") => Ok(false),
        Some(_) => Err(format!("invalid death weapon {key}").into()),
    }
}

pub(super) fn mask(line: Option<&ContentLine>) -> Result<u32, Box<dyn Error>> {
    let mut result = u32::MAX;
    if let Some(line) = line {
        for token in &line.tokens[1..] {
            match token.as_str() {
                "ALL" => result = u32::MAX,
                "NONE" => result = 0,
                value if value.starts_with(['+', '-']) => {
                    let bit = 1 << death_type(value.trim_start_matches(['+', '-']))? as u32;
                    if value.starts_with('-') {
                        result &= !bit;
                    } else {
                        result |= bit;
                    }
                }
                _ => return Err("invalid DeathTypes flag".into()),
            }
        }
    }
    Ok(result)
}

pub(super) fn death_type(name: &str) -> Result<DeathType, Box<dyn Error>> {
    Ok(match name {
        "NORMAL" => DeathType::Normal,
        "NONE" => DeathType::None,
        "CRUSHED" => DeathType::Crushed,
        "BURNED" => DeathType::Burned,
        "EXPLODED" => DeathType::Exploded,
        "POISONED" => DeathType::Poisoned,
        "TOPPLED" => DeathType::Toppled,
        "FLOODED" => DeathType::Flooded,
        "SUICIDED" => DeathType::Suicided,
        "LASERED" => DeathType::Lasered,
        "DETONATED" => DeathType::Detonated,
        "SPLATTED" => DeathType::Splatted,
        "POISONED_BETA" => DeathType::PoisonedBeta,
        "EXTRA_2" => DeathType::Extra2,
        "EXTRA_3" => DeathType::Extra3,
        "EXTRA_4" => DeathType::Extra4,
        "EXTRA_5" => DeathType::Extra5,
        "EXTRA_6" => DeathType::Extra6,
        "EXTRA_7" => DeathType::Extra7,
        "EXTRA_8" => DeathType::Extra8,
        "POISONED_GAMMA" => DeathType::PoisonedGamma,
        _ => return Err(format!("invalid weapon DeathType {name}").into()),
    })
}

pub(super) fn weapon_fields(
    stats: &mut WeaponRules,
    lines: &[ContentLine],
) -> Result<(), Box<dyn Error>> {
    stats.death_type = death_type(
        field(lines, 0, "DeathType")
            .map(scalar)
            .transpose()?
            .unwrap_or("NORMAL"),
    )?;
    stats.reload = match field(lines, 0, "AutoReloadsClip")
        .map(scalar)
        .transpose()?
        .unwrap_or("YES")
        .to_ascii_uppercase()
        .as_str()
    {
        "YES" => generals_sim::skirmish::ClipReload::Automatic,
        "NO" => generals_sim::skirmish::ClipReload::None,
        "RETURN_TO_BASE" => generals_sim::skirmish::ClipReload::ReturnToBase,
        _ => return Err("invalid AutoReloadsClip".into()),
    };
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod toxin_shell_tests;
