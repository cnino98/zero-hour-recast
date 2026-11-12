//! Native `FireWeaponWhenDamagedBehavior` data and declaration order.
use crate::skirmish_data::{blocks, damage_type, definition, field, number, scalar, weapons};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{WeaponRules, fire::when_damaged::Rules};
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
            .is_none_or(|family| family != "FireWeaponWhenDamagedBehavior")
        {
            continue;
        }
        let load = |key| -> Result<_, Box<dyn Error>> {
            let name = field(module, 1, key)
                .map(scalar)
                .transpose()?
                .filter(|name| !name.eq_ignore_ascii_case("NONE"));
            name.map(|name| -> Result<_, Box<dyn Error>> {
                let mut rules = WeaponRules::unarmed();
                name.clone_into(&mut rules.name);
                weapons::parse(&mut rules, &definition(files, "Weapon", name)?.lines)?;
                Ok(rules)
            })
            .transpose()
        };
        result.push(Rules {
            reaction: [
                load("ReactionWeaponPristine")?,
                load("ReactionWeaponDamaged")?,
                load("ReactionWeaponReallyDamaged")?,
                load("ReactionWeaponRubble")?,
            ],
            continuous: [
                load("ContinuousWeaponPristine")?,
                load("ContinuousWeaponDamaged")?,
                load("ContinuousWeaponReallyDamaged")?,
                load("ContinuousWeaponRubble")?,
            ],
            starts_active: boolean(module, "StartsActive", false)?,
            damage_types: damage_flags(module)?,
            amount: field(module, 1, "DamageAmount")
                .map_or(Ok(0.0), |_| number(module, 1, "DamageAmount"))?,
            triggers: crate::skirmish_data::salvage::words(module, 1, "TriggeredBy"),
            conflicts: crate::skirmish_data::salvage::words(module, 1, "ConflictsWith"),
            requires_all: boolean(module, "RequiresAllTriggers", false)?,
            removals: crate::skirmish_data::salvage::words(module, 1, "RemovesUpgrades"),
            upgrade_fx: field(module, 1, "FXListUpgrade")
                .map(scalar)
                .transpose()?
                .unwrap_or("")
                .into(),
        });
    }
    Ok(result)
}

fn boolean(module: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    field(module, 1, key)
        .map(scalar)
        .transpose()?
        .map_or(Ok(default), |value| {
            if value.eq_ignore_ascii_case("Yes") {
                Ok(true)
            } else if value.eq_ignore_ascii_case("No") {
                Ok(false)
            } else {
                Err(format!("invalid native {key} boolean").into())
            }
        })
}

fn damage_flags(module: &[ContentLine]) -> Result<u64, Box<dyn Error>> {
    let Some(line) = field(module, 1, "DamageTypes") else {
        return Ok(u64::MAX);
    };
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
