//! Selected player progression and power modules from installed source definitions.
//! Field defaults follow pinned `Player`, `Science`, `RankInfo` and `CashBountyPower`.
use std::error::Error;
#[cfg(test)]
mod identity_tests;

use generals_assets::{
    content::{self, ContentFile, ContentLine},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{
    Faction, FactionProgression, ProgressionRules, Rank, Science, UnitProgression, UnitType,
};

use crate::skirmish_data::{blocks, definition, field, finite_number, scalar, text, unsigned};

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

    unit_modules(&files, objects, catalog)?;
    Ok(ProgressionRules {
        multiplayer_colors: multiplayer_colors(&files[4])?,
        ranks,
        sciences,
        factions,
        powers: Vec::new(),
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
) -> Result<(), Box<dyn Error>> {
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
            }
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{int_list, science_words};
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
}
