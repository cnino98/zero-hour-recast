//! Authored Terrorist conversion module and placed civilian CARBOMB weapon sets.
use super::{blocks, definition, field, scalar, weapons};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{
    ConditionalWeapon, UnitType, WeaponRules, WeaponTemplateSet, car_bombs::Rules,
};
use std::error::Error;

pub(super) fn converter(object: &Declaration) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some((index, module)) = modules.iter().enumerate().find(|(_, module)| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "ConvertToCarBombCrateCollide")
    }) else {
        return Ok(None);
    };
    let position = |name| {
        modules
            .iter()
            .position(|module| module[0].tokens.get(1).is_some_and(|kind| kind == name))
    };
    if position("PhysicsBehavior").is_some_and(|physics| physics > index)
        || position("SquishCollide").is_some_and(|squish| squish < index)
    {
        return Err("untranslated car-bomb collision module ordering".into());
    }
    for key in [
        "ForbidOwnerPlayer",
        "BuildingPickup",
        "HumanOnly",
        "AllowMultiPickup",
        "PickupScience",
        "ExecuteFX",
        "ExecuteAnimation",
    ] {
        if field(module, 1, key).is_some() {
            return Err(format!("untranslated car-bomb crate {key}").into());
        }
    }
    Ok(Some(Rules {
        effect: field(module, 1, "FXList")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .to_owned(),
        required: super::salvage::words(module, 1, "RequiredKindOf"),
        forbidden: super::salvage::words(module, 1, "ForbiddenKindOf"),
    }))
}

pub(super) fn weapon_set(
    files: &[ContentFile],
    object: &Declaration,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    let sets = blocks(&object.lines, 0, "WeaponSet")?;
    let Some(set) = sets.into_iter().find(|set| {
        field(set, 1, "Conditions").is_some_and(|line| line.tokens[1..] == ["CARBOMB"])
    }) else {
        return Ok(());
    };
    let name = super::selected_pair(set, 1, "Weapon", "PRIMARY")?
        .ok_or("CARBOMB set lacks primary weapon")?;
    for slot in ["SECONDARY", "TERTIARY"] {
        if super::selected_pair(set, 1, "Weapon", slot)?.is_some() {
            return Err("untranslated extra car-bomb weapon slot".into());
        }
    }
    let mut rules = WeaponRules::unarmed();
    name.clone_into(&mut rules.name);
    weapons::parse(&mut rules, &definition(files, "Weapon", name)?.lines)?;
    let index = 7 + unit.weapon_set_rules.templates.len();
    unit.weapon_set_rules.templates.push(ConditionalWeapon {
        rules,
        flags: 8,
        slot: 0,
    });
    unit.weapon_set_rules.replacements.push(WeaponTemplateSet {
        flags: 0,
        weapons: [Some(0), None, None],
        share_lock: false,
    });
    unit.weapon_set_rules.replacements.push(WeaponTemplateSet {
        flags: 8,
        weapons: [Some(index), None, None],
        share_lock: field(set, 1, "WeaponLockSharedAcrossSets")
            .map(scalar)
            .transpose()?
            .is_some_and(|value| value.eq_ignore_ascii_case("Yes")),
    });
    Ok(())
}

#[cfg(test)]
mod tests;
