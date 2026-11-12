//! Selected player progression and power modules from installed source definitions.
//! Field defaults follow pinned `Player`, `Science`, `RankInfo`, `CashBountyPower`,
//! `CashHackSpecialPower`, `OCLSpecialPower` and `AutoHealBehavior`.
use std::error::Error;
#[cfg(test)]
mod identity_tests;

use generals_assets::{
    content::{self, ContentFile, ContentLine, Declaration},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{
    Faction, FactionProgression, PowerDefinition, PowerEffect, ProgressionRules, Rank, RepairBurst,
    Science, UnitProgression, UnitType,
};

use crate::skirmish_data::{
    blocks, definition, duration_frames, field, finite_number, number, scalar, text, unsigned,
};

pub fn load(
    assets: &VirtualAssets,
    objects: &[ContentFile],
    catalog: &mut [UnitType],
) -> Result<ProgressionRules, Box<dyn Error>> {
    let mut files = Vec::new();
    for name in [
        "rank",
        "science",
        "playertemplate",
        "specialpower",
        "objectcreationlist",
        "object/system",
        "multiplayer",
    ] {
        files.push(content::scan(
            assets.read(&format!("data/ini/{name}.ini"))?,
        )?);
    }
    let mut ranks = Vec::new();
    for declaration in files[0]
        .declarations
        .iter()
        .filter(|decl| decl.kind == "Rank")
    {
        if declaration.name.parse::<usize>()? != ranks.len() + 1 {
            return Err("Rank definitions must be consecutive from one".into());
        }
        ranks.push(Rank {
            skill_points: unsigned(&declaration.lines, 0, "SkillPointsNeeded")?,
            science_points: optional_unsigned(
                &declaration.lines,
                0,
                "SciencePurchasePointsGranted",
                0,
            )?,
            sciences: science_words(&declaration.lines, "SciencesGranted"),
        });
    }
    let mut sciences = Vec::new();
    for declaration in files[1]
        .declarations
        .iter()
        .filter(|decl| decl.kind == "Science")
    {
        sciences.push(Science {
            name: declaration.name.clone(),
            prerequisites: science_words(&declaration.lines, "PrerequisiteSciences"),
            cost: optional_unsigned(&declaration.lines, 0, "SciencePurchasePointCost", 0)?,
        });
    }
    let mut factions = Vec::new();
    for (name, faction) in [
        ("FactionAmerica", Faction::Usa),
        ("FactionChina", Faction::China),
        ("FactionGLA", Faction::Gla),
        ("FactionCivilian", Faction::Civilian),
    ] {
        let lines = &definition(&files, "PlayerTemplate", name)?.lines;
        factions.push(player_template(lines, faction)?);
    }

    let powers = unit_modules(&files, objects, catalog)?;
    Ok(ProgressionRules {
        multiplayer_colors: multiplayer_colors(&files[6])?,
        ranks,
        sciences,
        factions,
        powers,
    })
}

fn player_template(
    lines: &[ContentLine],
    faction: Faction,
) -> Result<FactionProgression, Box<dyn Error>> {
    Ok(FactionProgression {
        faction,
        side: field(lines, 0, "Side")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .to_owned(),
        preferred_color: packed_rgb(lines, "PreferredColor", 0xff00_0000)?,
        playable_side: match field(lines, 0, "PlayableSide")
            .map(scalar)
            .transpose()?
            .unwrap_or("No")
            .to_ascii_lowercase()
            .as_str()
        {
            "yes" => true,
            "no" => false,
            _ => return Err("invalid PlayerTemplate PlayableSide".into()),
        },
        intrinsic_points: optional_unsigned(lines, 0, "IntrinsicSciencePurchasePoints", 0)?,
        intrinsic_sciences: science_words(lines, "IntrinsicSciences"),
    })
}

fn multiplayer_colors(file: &ContentFile) -> Result<Vec<u32>, Box<dyn Error>> {
    file.declarations
        .iter()
        .filter(|declaration| declaration.kind == "MultiplayerColor")
        .map(|declaration| packed_rgb(&declaration.lines, "RGBColor", 0xffff_ffff))
        .collect()
}

fn packed_rgb(lines: &[ContentLine], name: &str, default: u32) -> Result<u32, Box<dyn Error>> {
    let Some(line) = field(lines, 0, name) else {
        return Ok(default);
    };
    let mut color = 0xff00_0000;
    for (name, shift) in [("R:", 16), ("G:", 8), ("B:", 0)] {
        let value = line
            .tokens
            .iter()
            .find_map(|token| token.strip_prefix(name))
            .ok_or("missing source color channel")?
            .parse::<u8>()?;
        color |= u32::from(value) << shift;
    }
    Ok(color)
}

fn unit_modules(
    files: &[ContentFile],
    objects: &[ContentFile],
    catalog: &mut [UnitType],
) -> Result<Vec<PowerDefinition>, Box<dyn Error>> {
    let mut powers = Vec::new();
    for unit in catalog {
        let object = definition(objects, "Object", &unit.name)?;
        let kind = words(&object.lines, 0, "KindOf");
        let has = |name: &str| kind.iter().any(|flag| flag == name);
        let experience = int_list(&object.lines, "ExperienceValue", [0; 4])?;
        unit.progression = UnitProgression {
            skill_point_value: int_list(&object.lines, "SkillPointValue", experience)?,
            ignored_in_gui: has("IGNORED_IN_GUI"),
            vehicle: has("VEHICLE"),
            cash_hack_target: has("STRUCTURE")
                && has("CAPTURABLE")
                && has("CASH_GENERATOR")
                && !has("REBUILD_HOLE"),
            ..UnitProgression::default()
        };
        for module in blocks(&object.lines, 0, "Behavior")? {
            let Some(module_type) = module[0].tokens.get(1) else {
                return Err("Behavior has no module type".into());
            };
            let Some(template) = field(module, 1, "SpecialPowerTemplate") else {
                continue;
            };
            let name = scalar(template)?;
            if module_type == "CashBountyPower" {
                let power = definition(files, "SpecialPower", name)?;
                let percent = text(module, 1, "Bounty")?
                    .strip_suffix('%')
                    .ok_or("Bounty requires source percent")?;
                unit.progression.cash_bounties.push((
                    text(&power.lines, 0, "RequiredScience")?.to_owned(),
                    finite_number(percent)? / 100.0,
                ));
            } else if name == "SuperweaponCashHack" || name == "SuperweaponEmergencyRepair" {
                let power = active_power(files, module, module_type, name)?;
                if let Some(previous) = powers
                    .iter()
                    .find(|previous: &&PowerDefinition| previous.name == power.name)
                {
                    if !same_power(previous, &power) {
                        return Err(
                            format!("Conflicting per-object power modules for {name}").into()
                        );
                    }
                } else {
                    powers.push(power);
                }
                unit.progression.powers.push(name.to_owned());
            }
        }
    }
    Ok(powers)
}

fn active_power(
    files: &[ContentFile],
    module: &[ContentLine],
    module_type: &str,
    name: &str,
) -> Result<PowerDefinition, Box<dyn Error>> {
    validate_base_module(module)?;
    let effect = if name == "SuperweaponCashHack" && module_type == "CashHackSpecialPower" {
        let upgrades = pairs(module, "UpgradeMoneyAmount")?
            .into_iter()
            .map(|(science, value)| Ok((science, value.parse()?)))
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        PowerEffect::CashHack {
            amount: unsigned(module, 1, "MoneyAmount")?,
            upgrades,
        }
    } else if name == "SuperweaponEmergencyRepair" && module_type == "OCLSpecialPower" {
        if text(module, 1, "CreateLocation")? != "CREATE_AT_LOCATION" {
            return Err("Unsupported repair OCL placement".into());
        }
        let burst = repair_burst(files, text(module, 1, "OCL")?)?;
        let upgrades = pairs(module, "UpgradeOCL")?
            .into_iter()
            .map(|(science, ocl)| Ok((science, repair_burst(files, &ocl)?)))
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        PowerEffect::EmergencyRepair { burst, upgrades }
    } else {
        return Err(format!("Unsupported module {module_type} for {name}").into());
    };
    let template = definition(files, "SpecialPower", name)?;
    let expected_enum = if name == "SuperweaponCashHack" {
        "SPECIAL_CASH_HACK"
    } else {
        "SPECIAL_REPAIR_VEHICLES"
    };
    if text(&template.lines, 0, "Enum")? != expected_enum {
        return Err("Selected power has an unsupported source enum".into());
    }
    if text(&template.lines, 0, "SharedSyncedTimer")? != "Yes" {
        return Err("Only source shared-timer powers are translated".into());
    }
    Ok(PowerDefinition {
        name: name.to_owned(),
        required_science: text(&template.lines, 0, "RequiredScience")?.to_owned(),
        reload_frames: duration_frames(
            field(&template.lines, 0, "ReloadTime").ok_or("Power missing ReloadTime")?,
        )?,
        effect,
    })
}

fn science_words(lines: &[ContentLine], key: &str) -> Vec<String> {
    let values = words(lines, 0, key);
    // Core/INI.cpp::parseScienceVector: None anywhere clears the entire list.
    if values
        .iter()
        .any(|value| value.eq_ignore_ascii_case("None"))
    {
        Vec::new()
    } else {
        values
    }
}

fn words(lines: &[ContentLine], depth: u32, key: &str) -> Vec<String> {
    field(lines, depth, key).map_or_else(Vec::new, |line| line.tokens[1..].to_vec())
}

fn optional_unsigned(
    lines: &[ContentLine],
    depth: u32,
    key: &str,
    default: u32,
) -> Result<u32, Box<dyn Error>> {
    field(lines, depth, key).map_or(Ok(default), |line| Ok(scalar(line)?.parse()?))
}

fn int_list(
    lines: &[ContentLine],
    key: &str,
    default: [u32; 4],
) -> Result<[u32; 4], Box<dyn Error>> {
    let Some(line) = field(lines, 0, key) else {
        return Ok(default);
    };
    let [_, regular, veteran, elite, heroic] = line.tokens.as_slice() else {
        return Err(format!("Expected four {key} entries").into());
    };
    Ok([
        regular.parse()?,
        veteran.parse()?,
        elite.parse()?,
        heroic.parse()?,
    ])
}

fn pairs(lines: &[ContentLine], key: &str) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let mut result = Vec::new();
    for line in lines.iter().filter(|line| {
        line.depth == Some(1) && line.tokens.first().is_some_and(|value| value == key)
    }) {
        let [_, science, value] = line.tokens.as_slice() else {
            return Err(format!("Malformed {key}").into());
        };
        result.push((science.clone(), value.clone()));
    }
    Ok(result)
}

fn validate_base_module(lines: &[ContentLine]) -> Result<(), Box<dyn Error>> {
    for key in [
        "StartsPaused",
        "StartsReady",
        "UpdateModuleStartsAttack",
        "ScriptedSpecialPowerOnly",
    ] {
        if field(lines, 1, key)
            .is_some_and(|line| line.tokens.get(1).is_none_or(|value| value != "No"))
        {
            return Err(format!("Unsupported selected special-power flag {key}").into());
        }
    }
    Ok(())
}

fn repair_burst(files: &[ContentFile], ocl: &str) -> Result<RepairBurst, Box<dyn Error>> {
    let creation = definition(files, "ObjectCreationList", ocl)?;
    let created = blocks(&creation.lines, 0, "CreateObject")?;
    if created.len() != 1 || unsigned(created[0], 1, "Count")? != 1 {
        return Err("Repair requires exactly one source marker".into());
    }
    let marker = definition(files, "Object", text(created[0], 1, "ObjectNames")?)?;
    let auto_heal = behavior(marker, "AutoHealBehavior")?;
    let deletion = behavior(marker, "DeletionUpdate")?;
    if text(auto_heal, 1, "StartsActive")? != "Yes"
        || text(auto_heal, 1, "KindOf")? != "VEHICLE"
        || text(auto_heal, 1, "SingleBurst")? != "Yes"
        || field(auto_heal, 1, "ForbiddenKindOf").is_some()
        || unsigned(deletion, 1, "MinLifetime")? != 0
        || unsigned(deletion, 1, "MaxLifetime")? != 0
    {
        return Err("Unsupported repair marker behavior".into());
    }
    // INI::scanReal uses atof; installed Radius contains the optional C float suffix.
    let radius = finite_number(text(auto_heal, 1, "Radius")?.trim_end_matches('f'))?;
    let delay_frames = duration_frames(
        field(auto_heal, 1, "HealingDelay").ok_or("Repair missing healing delay")?,
    )?;
    Ok(RepairBurst {
        amount: number(auto_heal, 1, "HealingAmount")?,
        radius,
        delay_frames,
    })
}

fn behavior<'a>(object: &'a Declaration, name: &str) -> Result<&'a [ContentLine], Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let mut selected = modules
        .into_iter()
        .filter(|module| module[0].tokens.get(1).is_some_and(|kind| kind == name));
    let module = selected
        .next()
        .ok_or_else(|| format!("Missing {name} module"))?;
    if selected.next().is_some() {
        return Err(format!("Ambiguous {name} module").into());
    }
    Ok(module)
}

fn same_power(left: &PowerDefinition, right: &PowerDefinition) -> bool {
    if left.required_science != right.required_science || left.reload_frames != right.reload_frames
    {
        return false;
    }
    match (&left.effect, &right.effect) {
        (
            PowerEffect::CashHack {
                amount: left,
                upgrades: lu,
            },
            PowerEffect::CashHack {
                amount: right,
                upgrades: ru,
            },
        ) => left == right && lu == ru,
        (
            PowerEffect::EmergencyRepair {
                burst: left,
                upgrades: lu,
            },
            PowerEffect::EmergencyRepair {
                burst: right,
                upgrades: ru,
            },
        ) => {
            same_burst(left, right)
                && lu.len() == ru.len()
                && lu
                    .iter()
                    .zip(ru)
                    .all(|((ls, lb), (rs, rb))| ls == rs && same_burst(lb, rb))
        }
        _ => false,
    }
}

const fn same_burst(left: &RepairBurst, right: &RepairBurst) -> bool {
    left.amount.to_bits() == right.amount.to_bits()
        && left.radius.to_bits() == right.radius.to_bits()
        && left.delay_frames == right.delay_frames
}

#[cfg(test)]
mod tests {
    use super::{int_list, repair_burst, science_words};
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn source_none_resets_science_prerequisites_including_earlier_tokens() {
        let file = content::scan(Asset {
            path: "science.ini".into(),
            origin: "fixture".into(),
            bytes: b"Science TEST\nPrerequisiteSciences=SCIENCE_Rank3 None SCIENCE_CHINA\nEnd\n"
                .to_vec(),
        })
        .unwrap();
        assert!(science_words(&file.declarations[0].lines, "PrerequisiteSciences").is_empty());
    }

    #[test]
    fn malformed_skill_point_lists_do_not_silently_change_kill_experience() {
        let file = content::scan(Asset {
            path: "object.ini".into(),
            origin: "fixture".into(),
            bytes: b"Object TEST\nSkillPointValue=20 40 60\nEnd\n".to_vec(),
        })
        .unwrap();
        assert!(int_list(&file.declarations[0].lines, "SkillPointValue", [10; 4]).is_err());
    }

    #[test]
    fn repair_loader_resolves_original_marker_but_rejects_changed_target_kind() {
        let fixture = "ObjectCreationList REPAIR\nCreateObject\nObjectNames=MARKER\nCount=1\nEnd\nEnd\nObject MARKER\nBehavior=AutoHealBehavior Heal\nHealingAmount=100\nHealingDelay=1\nRadius=100.0f\nStartsActive=Yes\nKindOf=VEHICLE\nSingleBurst=Yes\nEnd\nBehavior=DeletionUpdate Delete\nMinLifetime=0\nMaxLifetime=0\nEnd\nEnd\n";
        let scan = |bytes: Vec<u8>| {
            content::scan(Asset {
                path: "fixture.ini".into(),
                origin: "fixture".into(),
                bytes,
            })
            .unwrap()
        };
        let files = [scan(fixture.as_bytes().to_vec())];
        let burst = repair_burst(&files, "REPAIR").unwrap();
        assert_eq!(burst.delay_frames, 1);
        assert!((burst.radius - 100.0).abs() < f32::EPSILON);
        let files = [scan(
            fixture
                .replace("KindOf=VEHICLE", "KindOf=INFANTRY")
                .into_bytes(),
        )];
        assert!(repair_burst(&files, "REPAIR").is_err());
    }
}
