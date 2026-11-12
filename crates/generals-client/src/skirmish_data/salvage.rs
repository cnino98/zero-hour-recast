//! Installed native crate templates and creation/collision modules.
use super::{
    blocks, definition, duration_frames, field, finite_number, scalar, selected_pair, text, weapons,
};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    WeaponRules,
    salvage::{Drop, Pickup, Rules},
};
use std::error::Error;

pub(super) fn load(files: &[ContentFile], object: &Declaration) -> Result<Rules, Box<dyn Error>> {
    let mut rules = Rules::none();
    for module in blocks(&object.lines, 0, "Behavior")? {
        match module[0].tokens.get(1).map(String::as_str) {
            Some("CreateCrateDie") => {
                if field(module, 1, "DeathTypes").is_some() {
                    return Err("filtered crate death not translated".into());
                }
                for line in module.iter().filter(|line| {
                    line.depth == Some(1)
                        && line.tokens.first().is_some_and(|key| key == "CrateData")
                }) {
                    let data = definition(files, "CrateData", scalar(line)?)?;
                    rules.drops.push(drop(&data.lines)?);
                }
            }
            Some("SalvageCrateCollide") => rules.pickup = Some(pickup(object, module)?),
            _ => {}
        }
    }
    for flag in ["CRATEUPGRADE_ONE", "CRATEUPGRADE_TWO"] {
        let set = blocks(&object.lines, 0, "WeaponSet")?
            .into_iter()
            .find(|set| field(set, 1, "Conditions").is_some_and(|line| line.tokens[1..] == [flag]));
        if let Some(set) = set {
            if selected_pair(set, 1, "Weapon", "TERTIARY")?
                .is_some_and(|name| !name.eq_ignore_ascii_case("None"))
            {
                return Err("salvage tertiary weapon set is not represented".into());
            }
            for (slot, destination) in [
                ("PRIMARY", &mut rules.weapons),
                ("SECONDARY", &mut rules.secondary),
            ] {
                let Some(name) = selected_pair(set, 1, "Weapon", slot)?
                    .filter(|name| !name.eq_ignore_ascii_case("None"))
                else {
                    continue;
                };
                let mut weapon = WeaponRules::unarmed();
                name.clone_into(&mut weapon.name);
                weapons::parse(&mut weapon, &definition(files, "Weapon", name)?.lines)?;
                destination.push(weapon);
            }
        }
    }
    Ok(rules)
}
fn drop(lines: &[ContentLine]) -> Result<Drop, Box<dyn Error>> {
    for key in ["VeterancyLevel", "IsOwnedByMaker"] {
        if field(lines, 0, key).is_some() {
            return Err(format!("unrepresented crate {key}").into());
        }
    }
    let entries = lines
        .iter()
        .filter(|line| {
            line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "CrateObject")
        })
        .collect::<Vec<_>>();
    if entries.len() != 1
        || entries[0].tokens.len() != 3
        || finite_number(&entries[0].tokens[2])?.to_bits() != 1.0_f32.to_bits()
    {
        return Err("selected crate requires its original one-entry weight table".into());
    }
    Ok(Drop {
        chance: field(lines, 0, "CreationChance")
            .map_or(Ok(0.0), |line| finite_number(scalar(line)?))?,
        killer_kinds: words(lines, 0, "KilledByType"),
        science: field(lines, 0, "KillerScience")
            .map(scalar)
            .transpose()?
            .map(str::to_owned),
        template: entries[0].tokens[1].clone(),
    })
}
fn pickup(object: &Declaration, module: &[ContentLine]) -> Result<Pickup, Box<dyn Error>> {
    for key in [
        "RequiredKindOf",
        "ForbidOwnerPlayer",
        "HumanOnly",
        "BuildingPickup",
        "ExecuteFX",
        "ExecuteAnimation",
    ] {
        if field(module, 1, key).is_some() {
            return Err(format!("unrepresented salvage {key}").into());
        }
    }
    let delete = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .find(|lines| {
            lines[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "DeletionUpdate")
        })
        .ok_or("crate lifetime absent")?;
    let draw = blocks(&object.lines, 0, "Draw")?
        .into_iter()
        .find(|lines| {
            lines[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "W3DScienceModelDraw")
        })
        .ok_or("crate science draw absent")?;
    Ok(Pickup {
        science: text(module, 1, "PickupScience")?.into(),
        visible_science: text(draw, 1, "RequiredScience")?.into(),
        forbidden: words(module, 1, "ForbiddenKindOf"),
        weapon_chance: percent(module, "WeaponChance")?,
        level_chance: percent(module, "LevelChance")?,
        money: [
            text(module, 1, "MinMoney")?.parse()?,
            text(module, 1, "MaxMoney")?.parse()?,
        ],
        lifetime: [
            duration_frames(field(delete, 1, "MinLifetime").ok_or("minimum lifetime absent")?)?,
            duration_frames(field(delete, 1, "MaxLifetime").ok_or("maximum lifetime absent")?)?,
        ],
    })
}
fn percent(lines: &[ContentLine], name: &str) -> Result<f32, Box<dyn Error>> {
    Ok(finite_number(
        text(lines, 1, name)?
            .strip_suffix('%')
            .ok_or("salvage percentage lacks suffix")?,
    )? / 100.0)
}
pub(super) fn words(lines: &[ContentLine], depth: u32, key: &str) -> Vec<String> {
    field(lines, depth, key).map_or_else(Vec::new, |line| line.tokens[1..].to_vec())
}
