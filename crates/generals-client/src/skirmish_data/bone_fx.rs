//! Actual `BoneFXUpdate` fixed-range `OnlyOnce` parser; variable/repeating/OCL reject.
use super::{blocks, damage_type, definition, finite_number};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::bone_fx::{AtBone, Rules, Slot};
use num_traits::ToPrimitive;
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let has_damage = modules.iter().any(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "BoneFXDamage")
    });
    let mut result = Vec::new();
    for module in &modules {
        let family = module[0].tokens.get(1).map(String::as_str);
        if family == Some("BoneFXDamage") && module.len() != 2 {
            return Err("BoneFXDamage has unsupported authored fields".into());
        }
        if family != Some("BoneFXUpdate") {
            continue;
        }
        if !has_damage {
            return Err("BoneFXUpdate requires BoneFXDamage".into());
        }
        let mut rules = Rules {
            slots: std::array::from_fn(|_| {
                std::array::from_fn(|_| Slot {
                    fx: None,
                    particle: None,
                })
            }),
            damage_fx: u64::MAX,
            damage_particle: u64::MAX,
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            let key = line.tokens[0].as_str();
            match key {
                "DamageFXTypes" => rules.damage_fx = mask(line)?,
                "DamageParticleTypes" => rules.damage_particle = mask(line)?,
                "DamageOCLTypes" => {
                    mask(line)?;
                }
                _ => {
                    let (body, particle, index) = slot_key(key)?;
                    let at = slot(line, particle)?;
                    if !at.name.eq_ignore_ascii_case("NONE") {
                        if particle {
                            definition(files, "ParticleSystem", &at.name)?;
                        } else {
                            super::fx_definition(files, &at.name)?
                                .ok_or_else(|| format!("missing FXList {}", at.name))?;
                        }
                    }
                    let entry = &mut rules.slots[body][index];
                    *(if particle {
                        &mut entry.particle
                    } else {
                        &mut entry.fx
                    }) = Some(at);
                }
            }
        }
        result.push(rules);
    }
    if has_damage && result.is_empty() {
        return Err("BoneFXDamage requires BoneFXUpdate".into());
    }
    Ok(result)
}

fn slot_key(key: &str) -> Result<(usize, bool, usize), Box<dyn Error>> {
    for (body, prefix) in ["Pristine", "Damaged", "ReallyDamaged", "Rubble"]
        .into_iter()
        .enumerate()
    {
        let Some(rest) = key.strip_prefix(prefix) else {
            continue;
        };
        for (particle, family) in [(false, "FXList"), (true, "ParticleSystem")] {
            if let Some(index) = rest
                .strip_prefix(family)
                .and_then(|number| number.parse::<usize>().ok())
                .filter(|&index| (1..=8).contains(&index))
            {
                return Ok((body, particle, index - 1));
            }
        }
    }
    Err(format!(
        "unsupported BoneFX field {key}: original OCL/variable/repeating scheduler is not bound"
    )
    .into())
}

fn slot(line: &ContentLine, particle: bool) -> Result<AtBone, Box<dyn Error>> {
    let [_, bone, once, minimum, maximum, name] = line.tokens.as_slice() else {
        return Err("invalid BoneFX slot".into());
    };
    if !once.eq_ignore_ascii_case("OnlyOnce:Yes") {
        return Err("repeating BoneFX requires its original random scheduler".into());
    }
    let minimum = finite_number(minimum)? * (30.0 / 1000.0);
    let maximum = finite_number(maximum)? * (30.0 / 1000.0);
    if minimum.partial_cmp(&maximum) != Some(std::cmp::Ordering::Equal) {
        return Err(
            "variable BoneFX delay requires original client/logic random timer draws".into(),
        );
    }
    let delay = minimum
        .trunc()
        .to_i32()
        .filter(|&delay| delay >= 0)
        .ok_or("unsupported BoneFX duration")?;
    let bone = bone
        .strip_prefix("bone:")
        .filter(|bone| !bone.is_empty())
        .ok_or("invalid BoneFX bone token")?;
    let prefix = if particle { "PSys:" } else { "FXList:" };
    let name = name
        .strip_prefix(prefix)
        .filter(|name| !name.is_empty())
        .ok_or("invalid BoneFX template token")?;
    Ok(AtBone {
        bone: bone.into(),
        name: name.into(),
        delay,
    })
}

fn mask(line: &ContentLine) -> Result<u64, Box<dyn Error>> {
    let mut flags = 0_u64;
    for word in &line.tokens[1..] {
        if word.eq_ignore_ascii_case("ALL") {
            flags = u64::MAX;
            continue;
        }
        if word.eq_ignore_ascii_case("NONE") {
            flags = 0;
            continue;
        }
        let (remove, name) = word.strip_prefix('-').map_or_else(
            || (false, word.trim_start_matches('+')),
            |name| (true, name),
        );
        let kind =
            damage_type(name).ok_or_else(|| format!("unsupported native damage flag {name}"))?;
        let flag = 1_u64 << kind as u32;
        if remove {
            flags &= !flag;
        } else {
            flags |= flag;
        }
    }
    Ok(flags)
}

#[cfg(test)]
mod tests;
