//! Authored `GenerateMinefieldBehavior` and `MinefieldBehavior` module data.
use super::{blocks, definition, field, scalar, source_numbers, source_real, weapons};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    WeaponRules,
    minefield::{GeneratorRules, MineRules, Placement},
};
use std::error::Error;

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut names = Vec::new();
    for name in selected {
        let object = definition(files, "Object", name)?;
        for module in blocks(&object.lines, 0, "Behavior")? {
            if module[0]
                .tokens
                .get(1)
                .is_some_and(|family| family == "GenerateMinefieldBehavior")
            {
                for key in ["MineName", "UpgradedMineName"] {
                    if let Some(line) = field(module, 1, key) {
                        let name = scalar(line)?;
                        if let Some(object) = files
                            .iter()
                            .flat_map(|file| &file.declarations)
                            .find(|object| object.kind == "Object" && object.name == name)
                        {
                            names.push(object.name.as_str());
                        }
                    }
                }
            }
        }
        for mine in mines(files, object)? {
            if let Some(list) = mine.creation {
                for nugget in list.objects {
                    for name in nugget
                        .names
                        .into_iter()
                        .chain((!nugget.container.is_empty()).then_some(nugget.container))
                    {
                        names.push(definition(files, "Object", &name)?.name.as_str());
                    }
                }
            }
        }
    }
    names.retain(|name| !selected.contains(name));
    names.sort_unstable();
    names.dedup();
    Ok(names)
}

pub(super) fn generators(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<GeneratorRules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "GenerateMinefieldBehavior")
        {
            continue;
        }
        let global = &definition(files, "GameData", "")?.lines;
        let mut rules = GeneratorRules {
            mine_name: String::new(),
            upgraded_name: String::new(),
            generation_fx: None,
            emp_upgrade: nullable_name(files, "Upgrade", "Upgrade_ChinaEMPMines"),
            distance: field(global, 0, "StandardMinefieldDistance")
                .map_or(Ok(40.0), |line| source_real(scalar(line)?))?,
            density: field(global, 0, "StandardMinefieldDensity")
                .map_or(Ok(0.01), |line| source_real(scalar(line)?))?,
            placement: Placement::Border,
            on_death: false,
            circular: false,
            upgradable: false,
            jitter: 0.0,
            skip_under_structure: 0.33,
        };
        let mut smart = false;
        let mut border = true;
        let mut skip_interior = true;
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            let key = line.tokens.first().map_or("", String::as_str);
            match key {
                "MineName" => rules.mine_name = scalar(line)?.into(),
                "UpgradedMineName" => rules.upgraded_name = scalar(line)?.into(),
                // The source stores this string but update hardcodes Upgrade_ChinaEMPMines.
                "UpgradedTriggeredBy" => {
                    let _ = scalar(line)?;
                }
                "GenerationFX" => {
                    let name = scalar(line)?;
                    rules.generation_fx = super::fx_definition(files, name)?.map(|_| name.into());
                }
                "DistanceAroundObject" => rules.distance = source_real(scalar(line)?)?,
                "MinesPerSquareFoot" => rules.density = source_real(scalar(line)?)?,
                "GenerateOnlyOnDeath" => rules.on_death = boolean(line)?,
                "BorderOnly" => border = boolean(line)?,
                "SmartBorder" => smart = boolean(line)?,
                "SmartBorderSkipInterior" => skip_interior = boolean(line)?,
                "AlwaysCircular" => rules.circular = boolean(line)?,
                "Upgradable" => rules.upgradable = boolean(line)?,
                "RandomJitter" => rules.jitter = percent(line)?,
                "SkipIfThisMuchUnderStructure" => rules.skip_under_structure = percent(line)?,
                _ => {}
            }
        }
        rules.placement = if smart {
            Placement::Smart { skip_interior }
        } else if border {
            Placement::Border
        } else {
            Placement::Footprint
        };
        result.push(rules);
    }
    Ok(result)
}

pub(super) fn mines(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<MineRules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "MinefieldBehavior")
        {
            continue;
        }
        let mut rules = MineRules {
            weapon: None,
            detonated_by: 3,
            stop_regeneration: true,
            regenerates: false,
            workers_detonate: false,
            creator_check_frames: 30,
            scoot_frames: 0,
            virtual_mines: 1,
            repeat_move_threshold: 1.0,
            drain_percent: 0.0,
            creation: None,
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("DetonationWeapon") => {
                    rules.weapon = if let Some(name) = nullable_name(files, "Weapon", scalar(line)?)
                    {
                        let mut weapon = WeaponRules::unarmed();
                        name.clone_into(&mut weapon.name);
                        weapons::parse(&mut weapon, &definition(files, "Weapon", &name)?.lines)?;
                        Some(weapon)
                    } else {
                        None
                    };
                }
                Some("DetonatedBy") => rules.detonated_by = relations(line)?,
                Some("StopsRegenAfterCreatorDies") => rules.stop_regeneration = boolean(line)?,
                Some("Regenerates") => rules.regenerates = boolean(line)?,
                Some("WorkersDetonate") => rules.workers_detonate = boolean(line)?,
                Some("CreatorDeathCheckRate") => {
                    rules.creator_check_frames = source_numbers::duration_frames(scalar(line)?)?;
                }
                Some("ScootFromStartingPointTime") => {
                    rules.scoot_frames = source_numbers::duration_frames(scalar(line)?)?;
                }
                Some("NumVirtualMines") => {
                    rules.virtual_mines = source_numbers::unsigned(scalar(line)?)?;
                }
                Some("RepeatDetonateMoveThresh") => {
                    rules.repeat_move_threshold = source_real(scalar(line)?)?;
                }
                Some("DegenPercentPerSecondAfterCreatorDies") => {
                    rules.drain_percent = percent(line)?;
                }
                Some("CreationList") => {
                    rules.creation = nullable_name(files, "ObjectCreationList", scalar(line)?)
                        .map(|name| {
                            definition(files, "ObjectCreationList", &name)
                                .and_then(super::object_creation::parse_list)
                        })
                        .transpose()?;
                }
                _ => {}
            }
        }
        result.push(rules);
    }
    Ok(result)
}

fn boolean(line: &ContentLine) -> Result<bool, Box<dyn Error>> {
    match scalar(line)?.to_ascii_uppercase().as_str() {
        "YES" => Ok(true),
        "NO" => Ok(false),
        _ => Err("invalid minefield boolean".into()),
    }
}

fn percent(line: &ContentLine) -> Result<f32, Box<dyn Error>> {
    Ok(source_real(scalar(line)?)? * 0.01)
}

fn nullable_name(files: &[ContentFile], kind: &str, name: &str) -> Option<String> {
    files
        .iter()
        .flat_map(|file| &file.declarations)
        .find(|entry| entry.kind == kind && entry.name == name)
        .map(|entry| entry.name.clone())
}

fn relations(line: &ContentLine) -> Result<u8, Box<dyn Error>> {
    let mut mask = 0;
    for word in &line.tokens[1..] {
        if word.eq_ignore_ascii_case("ALL") {
            mask = 7;
            continue;
        }
        if word.eq_ignore_ascii_case("NONE") {
            mask = 0;
            continue;
        }
        let remove = word.starts_with('-');
        let name = word.trim_start_matches(['+', '-']);
        let flag = match name {
            "ENEMIES" => 1,
            "NEUTRAL" => 2,
            "ALLIES" => 4,
            _ => return Err(format!("invalid native relationship {name}").into()),
        };
        if remove {
            mask &= !flag;
        } else {
            mask |= flag;
        }
    }
    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn generation_defaults_and_upgrade_lookup_follow_native_module_data() {
        let file = content::scan(Asset {
            origin: "native generator defaults".into(), path: "mines.ini".into(),
            bytes: b"GameData\nEnd\nObject Generator\nBehavior=GenerateMinefieldBehavior Mines\nMineName=LandMine\nUpgradedMineName=Missing\nUpgradedTriggeredBy=IgnoredUpgrade\nSmartBorder=Yes\nBorderOnly=No\nRandomJitter=25%\nEnd\nEnd\nObject LandMine\nEnd\nUpgrade Upgrade_ChinaEMPMines\nEnd\n".to_vec(),
        }).unwrap();
        let files = [file];
        let object = definition(&files, "Object", "Generator").unwrap();
        let parsed = generators(&files, object).unwrap();
        assert_eq!(parsed[0].distance.to_bits(), 40.0_f32.to_bits());
        assert_eq!(parsed[0].density.to_bits(), 0.01_f32.to_bits());
        assert_eq!(parsed[0].jitter.to_bits(), 0.25_f32.to_bits());
        assert_eq!(
            parsed[0].emp_upgrade.as_deref(),
            Some("Upgrade_ChinaEMPMines")
        );
        assert!(matches!(
            parsed[0].placement,
            Placement::Smart {
                skip_interior: true
            }
        ));
        assert_eq!(dependencies(&files, &["Generator"]).unwrap(), ["LandMine"]);
    }

    #[test]
    fn land_mine_preserves_instances_native_duration_and_null_references() {
        let file = content::scan(Asset {
            origin: "native mine module fields".into(), path: "mines.ini".into(),
            bytes: b"Object Mine\nBehavior=MinefieldBehavior First\nDetonationWeapon=Missing\nDetonatedBy=ALL -ALLIES\nCreatorDeathCheckRate=50\nScootFromStartingPointTime=1000\nNumVirtualMines=4\nDegenPercentPerSecondAfterCreatorDies=50%\nCreationList=Missing\nEnd\nBehavior=MinefieldBehavior Second\nWorkersDetonate=Yes\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let parsed = mines(std::slice::from_ref(&file), &file.declarations[0]).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].detonated_by, 3);
        assert_eq!(parsed[0].creator_check_frames, 2);
        assert_eq!(parsed[0].scoot_frames, 30);
        assert_eq!(parsed[0].virtual_mines, 4);
        assert_eq!(parsed[0].drain_percent.to_bits(), 0.5_f32.to_bits());
        assert!(parsed[0].creation.is_none() && parsed[0].weapon.is_none());
        assert!(parsed[1].workers_detonate);
        assert_eq!(parsed[1].virtual_mines, 1);
    }
}
