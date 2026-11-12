//! `InstantDeathBehavior` keeps every nullable authored choice and its RNG slot.
use super::{blocks, definition, weapons};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{WeaponRules, instant_death::Rules};
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
            .is_none_or(|family| family != "InstantDeathBehavior")
        {
            continue;
        }
        let mut rules = Rules {
            death_mask: u32::MAX,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
            effects: Vec::new(),
            objects: Vec::new(),
            weapons: Vec::new(),
        };
        // Native field parsers append on every line, including null lookup results.
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens[0].as_str() {
                "DeathTypes" => rules.death_mask = super::death_weapons::mask(Some(line))?,
                "VeterancyLevels" => rules.veterancy_mask = veterancy(line)?,
                "ExemptStatus" => rules.exempt_status = status(line, rules.exempt_status)?,
                "RequiredStatus" => rules.required_status = status(line, rules.required_status)?,
                _ => {}
            }
            for name in &line.tokens[1..] {
                match line.tokens[0].as_str() {
                    "FX" => rules
                        .effects
                        .push(nullable(files, "FXList", name)?.map(|_| name.clone())),
                    "OCL" => rules.objects.push(
                        if nullable(files, "ObjectCreationList", name)?.is_some() {
                            Some(super::physical_ocl::load(files, name)?)
                        } else {
                            None
                        },
                    ),
                    "Weapon" => {
                        let weapon = if let Some(source) = nullable(files, "Weapon", name)? {
                            let mut weapon = WeaponRules::unarmed();
                            name.clone_into(&mut weapon.name);
                            weapons::parse(&mut weapon, &source.lines)?;
                            Some(weapon)
                        } else {
                            None
                        };
                        rules.weapons.push(weapon);
                    }
                    _ => {}
                }
            }
        }
        result.push(rules);
    }
    Ok(result)
}

pub(super) fn nullable<'a>(
    files: &'a [ContentFile],
    kind: &str,
    name: &str,
) -> Result<Option<&'a Declaration>, Box<dyn Error>> {
    if kind == "FXList" {
        return super::fx_definition(files, name);
    }
    if files
        .iter()
        .flat_map(|file| &file.declarations)
        .any(|item| item.kind == kind && item.name == name)
    {
        definition(files, kind, name).map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn veterancy(line: &ContentLine) -> Result<u8, Box<dyn Error>> {
    let mut mask = 0xf;
    for token in &line.tokens[1..] {
        let token = token.to_ascii_uppercase();
        if token == "ALL" {
            mask = 0xf;
            continue;
        }
        if token == "NONE" {
            mask = 0;
            continue;
        }
        let name = token.trim_start_matches(['+', '-']);
        let bit = match name {
            "REGULAR" => 1,
            "VETERAN" => 2,
            "ELITE" => 4,
            "HEROIC" => 8,
            _ => return Err(format!("invalid VeterancyLevels token {token}").into()),
        };
        if token.starts_with('-') {
            mask &= !bit;
        } else if token.starts_with('+') {
            mask |= bit;
        } else {
            return Err(format!("invalid VeterancyLevels token {token}").into());
        }
    }
    Ok(mask)
}

// BitFlagsIO.h: each line uses either replacement names or incremental +/- operations.
pub fn status(line: &ContentLine, mut mask: u64) -> Result<u64, Box<dyn Error>> {
    let mut normal = false;
    let mut incremental = false;
    for token in &line.tokens[1..] {
        let token = token.to_ascii_uppercase();
        if token == "NONE" {
            if normal || incremental {
                return Err("mixed DieMux status operations".into());
            }
            return Ok(0);
        }
        let signed = token.starts_with(['+', '-']);
        if (signed && normal) || (!signed && incremental) {
            return Err("mixed DieMux status operations".into());
        }
        let index = match token.trim_start_matches(['+', '-']) {
            "DESTROYED" => 1,
            "UNDER_CONSTRUCTION" => 3,
            "NO_COLLISIONS" => 5,
            "HIJACKED" => 10,
            "AFLAME" => 11,
            "BURNED" => 12,
            "STEALTHED" => 16,
            "DETECTED" => 17,
            "SOLD" => 19,
            "MASKED" => 22,
            "DISGUISED" => 43,
            _ => return Err(format!("unbound DieMux status producer {token}").into()),
        };
        if signed {
            incremental = true;
            if token.starts_with('-') {
                mask &= !(1 << index);
            } else {
                mask |= 1 << index;
            }
        } else {
            if !normal {
                mask = 0;
            }
            normal = true;
            mask |= 1 << index;
        }
    }
    Ok(mask)
}

#[cfg(test)]
mod tests;
