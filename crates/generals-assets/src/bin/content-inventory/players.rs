//! Resolve only the source fields used by faction selection, in family load order.
use super::report::{row, table};
use generals_assets::{
    content::{ContentFile, ContentLine, Declaration},
    roster::{self, GeneralPersona, PlayerTemplate},
};
use std::{collections::BTreeMap, error::Error, io::Write, path::Path};

type Fields = BTreeMap<String, String>;

fn family<'a>(files: &'a [ContentFile], stem: &str) -> Vec<&'a ContentFile> {
    let filename = format!("{stem}.ini");
    let directory = format!("{stem}/");
    let mut result: Vec<_> = files
        .iter()
        .filter(|file| file.path == filename || file.path.starts_with(&directory))
        .collect();
    result.sort_by_key(|file| {
        let rank = if file.path == filename {
            0
        } else if file.path[directory.len()..].contains('/') {
            2
        } else {
            1
        };
        (rank, &file.path)
    });
    result
}

fn direct_fields(declaration: &Declaration, target: &mut Fields) -> Result<(), Box<dyn Error>> {
    for line in &declaration.lines {
        if line.depth == Some(0) {
            let key = line.tokens.first().ok_or("empty field")?.clone();
            if matches!(key.as_str(), "Side" | "StartingBuilding" | "OldFaction")
                || key.strip_prefix("StartingUnit").is_some_and(|suffix| {
                    suffix.len() == 1 && suffix.bytes().all(|byte| byte.is_ascii_digit())
                })
            {
                target.insert(key, scalar(line)?);
            }
        } else if line.depth.is_none() {
            return Err(
                format!("unresolved player template structure at line {}", line.line).into(),
            );
        }
    }
    Ok(())
}

fn scalar(line: &ContentLine) -> Result<String, Box<dyn Error>> {
    let token = line.tokens.get(1).map_or("", String::as_str);
    if token.starts_with('"') {
        return Err(format!(
            "quoted faction metadata at line {} needs the source string parser",
            line.line
        )
        .into());
    }
    // getNextAsciiString returns the first token for unquoted identifiers.
    Ok(token.to_owned())
}

fn boolean(value: Option<&String>) -> Result<bool, Box<dyn Error>> {
    match value.map(String::as_str) {
        None => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
        Some(value) => Err(format!("unsupported boolean: {value}").into()),
    }
}

fn personas(files: &[ContentFile]) -> Result<Vec<GeneralPersona>, Box<dyn Error>> {
    let mut slots = BTreeMap::<String, Fields>::new();
    for file in family(files, "data/ini/challengemode") {
        for declaration in &file.declarations {
            if !declaration.kind.eq_ignore_ascii_case("ChallengeGenerals") {
                continue;
            }
            let mut slot = None;
            for line in &declaration.lines {
                let Some(key) = line.tokens.first() else {
                    continue;
                };
                if line.depth == Some(0) {
                    slot = key
                        .strip_prefix("GeneralPersona")
                        .filter(|suffix| {
                            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                        })
                        .map(|_| key.clone());
                } else if line.depth == Some(1) {
                    if let Some(slot) = &slot
                        && matches!(key.as_str(), "PlayerTemplate" | "StartsEnabled")
                    {
                        slots
                            .entry(slot.clone())
                            .or_default()
                            .insert(key.clone(), scalar(line)?);
                    }
                } else if line.depth.is_none() {
                    return Err("unresolved ChallengeGenerals structure".into());
                }
            }
        }
    }
    let mut result = Vec::new();
    // Engine searches its fixed persona array by numeric slot, not lexical order.
    for index in 0..12 {
        if let Some(fields) = slots.get(&format!("GeneralPersona{index}"))
            && let Some(name) = fields.get("PlayerTemplate")
        {
            let starts_enabled = boolean(Some(
                fields
                    .get("StartsEnabled")
                    .ok_or("persona lacks explicit StartsEnabled")?,
            ))?;
            result.push(GeneralPersona {
                player_template: name.clone(),
                starts_enabled,
            });
        }
    }
    if result.is_empty() {
        return Err("no configured general personas found; faction roster unresolved".into());
    }
    Ok(result)
}

pub fn write(directory: &Path, files: &[ContentFile]) -> Result<(), Box<dyn Error>> {
    let mut definitions = Vec::<(String, String, Fields)>::new();
    for stem in ["data/ini/default/playertemplate", "data/ini/playertemplate"] {
        for file in family(files, stem) {
            for declaration in &file.declarations {
                if !declaration.kind.eq_ignore_ascii_case("PlayerTemplate") {
                    continue;
                }
                let index = definitions
                    .iter()
                    .position(|entry| entry.0 == legacy_lookup_name(&declaration.name))
                    .unwrap_or_else(|| {
                        definitions.push((
                            declaration.name.clone(),
                            format!("{}:{}", file.path, declaration.line),
                            Fields::new(),
                        ));
                        definitions.len() - 1
                    });
                direct_fields(declaration, &mut definitions[index].2)?;
                // parsePlayerTemplateDefinition sets the incoming key even when
                // findPlayerTemplate resolved a legacy alias to an existing row.
                definitions[index].0.clone_from(&declaration.name);
            }
        }
    }
    if definitions.is_empty() {
        return Err("no player templates found".into());
    }
    let players: Vec<_> = definitions
        .iter()
        .map(|(name, _, fields)| {
            Ok(PlayerTemplate {
                name: name.clone(),
                side: fields.get("Side").cloned().unwrap_or_default(),
                starting_building: fields.get("StartingBuilding").cloned().unwrap_or_default(),
                old_faction: boolean(fields.get("OldFaction"))?,
            })
        })
        .collect::<Result<_, Box<dyn Error>>>()?;
    let eligibility = roster::skirmish_eligibility(&players, &personas(files)?, false);
    let mut report = table(
        directory,
        "players.tsv",
        &[
            "name",
            "side",
            "eligibility",
            "starting_building",
            "starting_units",
            "source",
        ],
    )?;
    for ((player, status), (_, source, fields)) in
        players.iter().zip(&eligibility).zip(&definitions)
    {
        let units = (0..10)
            .filter_map(|index| fields.get(&format!("StartingUnit{index}")))
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        row(
            &mut report,
            &[
                &player.name,
                &player.side,
                &format!("{status:?}"),
                &player.starting_building,
                &units,
                source,
            ],
        )?;
    }
    report.flush()?;
    println!(
        "Selectable skirmish factions: {}",
        eligibility
            .iter()
            .filter(|value| **value == roster::Eligibility::Selectable)
            .count()
    );
    Ok(())
}

// PlayerTemplate.cpp::findPlayerTemplate remaps these exact NameKeys before lookup.
fn legacy_lookup_name(name: &str) -> &str {
    match name {
        "FactionAmericaChooseAGeneral"
        | "FactionAmericaTankCommand"
        | "FactionAmericaSpecialForces"
        | "FactionAmericaAirForce" => "FactionAmerica",
        "FactionChinaChooseAGeneral"
        | "FactionChinaRedArmy"
        | "FactionChinaSpecialWeapons"
        | "FactionChinaSecretPolice" => "FactionChina",
        "FactionGLAChooseAGeneral"
        | "FactionGLATerrorCell"
        | "FactionGLABiowarCommand"
        | "FactionGLAWarlordCommand" => "FactionGLA",
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::{family, legacy_lookup_name, personas};
    use generals_assets::{
        content::{ContentFile, scan},
        virtual_assets::Asset,
    };

    fn file(path: &str, text: &str) -> ContentFile {
        scan(Asset {
            path: path.into(),
            origin: "fixture".into(),
            bytes: text.as_bytes().to_vec(),
        })
        .expect("source fixture")
    }

    #[test]
    fn persona_slot_order_is_numeric_and_missing_enablement_is_rejected() {
        let input = file(
            "data/ini/challengemode.ini",
            "ChallengeGenerals\n GeneralPersona10\n PlayerTemplate = Boss\n StartsEnabled = no\n End\n GeneralPersona2\n PlayerTemplate = Tank\n StartsEnabled = yes\n End\nEnd\n",
        );
        let result = personas(&[input]).expect("configured personas");
        assert_eq!(result[0].player_template, "Tank");
        assert_eq!(result[1].player_template, "Boss");
        assert!(!result[1].starts_enabled);
        let missing = file(
            "data/ini/challengemode.ini",
            "ChallengeGenerals\n GeneralPersona0\n PlayerTemplate = Boss\n End\nEnd\n",
        );
        assert!(personas(&[missing]).is_err());
    }

    #[test]
    fn family_loads_main_file_then_direct_files_then_sorted_descendants() {
        let files = [
            "data/ini/playertemplate/a/z/a.ini",
            "data/ini/playertemplate/z.ini",
            "data/ini/playertemplate/a/b.ini",
            "data/ini/playertemplate.ini",
            "data/ini/other.ini",
        ]
        .map(|path| file(path, ""));
        let ordered = family(&files, "data/ini/playertemplate");
        assert_eq!(
            ordered
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            [
                files[3].path.as_str(),
                files[1].path.as_str(),
                files[2].path.as_str(),
                files[0].path.as_str()
            ]
        );
        assert_eq!(legacy_lookup_name("FactionChinaRedArmy"), "FactionChina");
        assert_eq!(
            legacy_lookup_name("FactionChinaTankGeneral"),
            "FactionChinaTankGeneral"
        );
        assert_eq!(
            legacy_lookup_name("factionchinaredarmy"),
            "factionchinaredarmy"
        );
    }
}
