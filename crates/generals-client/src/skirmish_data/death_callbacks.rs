//! SH `DieMux` fields and the nullable FXListDie/DestroyDie module data.
use super::{blocks, death_weapons, instant_death, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::death_callbacks::{Conditions, CrushRules, FxRules, UpgradeRules};
use std::error::Error;

pub(super) fn fx(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<FxRules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "FXListDie")
        {
            continue;
        }
        let mut rules = FxRules {
            conditions: conditions(module)?,
            effect: String::new(),
            orient_to_object: true,
            starts_active: true,
            triggers: Vec::new(),
            conflicts: Vec::new(),
            requires_all: false,
            removals: Vec::new(),
            upgrade_fx: String::new(),
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens[0].as_str() {
                "DeathFX" => rules.effect = effect(files, line)?,
                "FXListUpgrade" => rules.upgrade_fx = effect(files, line)?,
                "StartsActive" => rules.starts_active = boolean(line)?,
                "OrientToObject" => rules.orient_to_object = boolean(line)?,
                "RequiresAllTriggers" => rules.requires_all = boolean(line)?,
                "TriggeredBy" => rules.triggers = line.tokens[1..].to_vec(),
                "ConflictsWith" => rules.conflicts = line.tokens[1..].to_vec(),
                "RemovesUpgrades" => rules.removals = line.tokens[1..].to_vec(),
                _ => {}
            }
        }
        result.push(rules);
    }
    Ok(result)
}

pub(super) fn upgrade(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<UpgradeRules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "UpgradeDie")
        {
            continue;
        }
        let name = super::field(module, 1, "UpgradeToRemove")
            .and_then(|line| line.tokens.get(1))
            .map_or("", String::as_str);
        // UpgradeCenter is immutable during a match; unknown names remain a nullable lookup.
        let upgrade = files
            .iter()
            .flat_map(|file| &file.declarations)
            .find(|item| item.kind == "Upgrade" && item.name == name)
            .map(|item| item.name.clone());
        result.push(UpgradeRules {
            conditions: conditions(module)?,
            upgrade,
        });
    }
    Ok(result)
}

pub(super) fn destroy(object: &Declaration) -> Result<Vec<Conditions>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|family| family == "DestroyDie")
        })
        .map(conditions)
        .collect()
}

pub(super) fn crush(object: &Declaration) -> Result<Vec<CrushRules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "CrushDie")
        {
            continue;
        }
        let mut rules = CrushRules {
            conditions: conditions(module)?,
            sounds: std::array::from_fn(|_| String::new()),
            percentages: [100; 3],
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            let key = line.tokens[0].as_str();
            let sound = match key {
                "TotalCrushSound" => Some(0),
                "BackEndCrushSound" => Some(1),
                "FrontEndCrushSound" => Some(2),
                _ => None,
            };
            if let Some(index) = sound {
                let name = scalar(line)?;
                // INI::parseAudioEventRTS leaves the prior event intact for NoSound.
                if !name.eq_ignore_ascii_case("NoSound") {
                    rules.sounds[index] = name.into();
                }
                continue;
            }
            let percent = match key {
                "TotalCrushSoundPercent" => Some(0),
                "BackEndCrushSoundPercent" => Some(1),
                "FrontEndCrushSoundPercent" => Some(2),
                _ => None,
            };
            if let Some(index) = percent {
                rules.percentages[index] = scalar(line)?.parse()?;
            }
        }
        result.push(rules);
    }
    Ok(result)
}

pub fn conditions(module: &[ContentLine]) -> Result<Conditions, Box<dyn Error>> {
    let mut result = Conditions {
        death_mask: u32::MAX,
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 0,
    };
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        match line.tokens[0].as_str() {
            "DeathTypes" => result.death_mask = death_weapons::mask(Some(line))?,
            "VeterancyLevels" => result.veterancy_mask = instant_death::veterancy(line)?,
            "ExemptStatus" => {
                result.exempt_status = instant_death::status(line, result.exempt_status)?;
            }
            "RequiredStatus" => {
                result.required_status = instant_death::status(line, result.required_status)?;
            }
            _ => {}
        }
    }
    Ok(result)
}

fn effect(files: &[ContentFile], line: &ContentLine) -> Result<String, Box<dyn Error>> {
    let name = scalar(line)?;
    Ok(
        if instant_death::nullable(files, "FXList", name)?.is_some() {
            name.to_owned()
        } else {
            String::new()
        },
    )
}

fn boolean(line: &ContentLine) -> Result<bool, Box<dyn Error>> {
    match scalar(line)? {
        value if value.eq_ignore_ascii_case("Yes") => Ok(true),
        value if value.eq_ignore_ascii_case("No") => Ok(false),
        value => Err(format!("invalid FXListDie boolean {value}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn defaults_nullable_lookups_and_repeated_fields_follow_native_module_data() {
        let files = [content::scan(Asset {
            path: "death.ini".into(), origin: "native module regression".into(),
            bytes: b"FXList RealFX\nEnd\nObject Arbitrary\nBehavior=FXListDie First\nDeathFX=RealFX\nEnd\nBehavior=FXListDie Second\nDeathFX=RealFX\nDeathFX=MissingFX\nStartsActive=No\nOrientToObject=No\nTriggeredBy=Discarded\nTriggeredBy=One Two\nRemovesUpgrades=Discarded\nRemovesUpgrades=OldOne OldTwo\nRequiresAllTriggers=Yes\nRequiredStatus=UNDER_CONSTRUCTION\nRequiredStatus=+AFLAME\nEnd\nBehavior=DestroyDie Last\nDeathTypes=NONE +LASERED\nVeterancyLevels=NONE +HEROIC\nExemptStatus=UNDER_CONSTRUCTION\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let object = &files[0].declarations[1];
        let effects = fx(&files, object).unwrap();
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0].effect, "RealFX");
        assert!(effects[0].starts_active && effects[0].orient_to_object);
        assert!(effects[1].effect.is_empty());
        assert!(!effects[1].starts_active && !effects[1].orient_to_object);
        assert_eq!(effects[1].triggers, ["One", "Two"]);
        assert!(effects[1].requires_all);
        assert_eq!(effects[1].removals, ["OldOne", "OldTwo"]);
        assert_eq!(effects[1].conditions.required_status, (1 << 3) | (1 << 11));
        let destruction = destroy(object).unwrap();
        assert_eq!(destruction.len(), 1);
        assert_eq!(
            destruction[0].death_mask,
            1 << generals_sim::skirmish::DeathType::Lasered as u32
        );
        assert_eq!(destruction[0].veterancy_mask, 8);
        assert_eq!(destruction[0].exempt_status, 1 << 3);
    }
    #[test]
    fn crush_sound_defaults_repetition_and_signed_percentages_follow_native_parser() {
        let files = [content::scan(Asset {
            path: "crush.ini".into(), origin: "native CrushDie parser".into(),
            bytes: b"Object CivilianCar\nBehavior=CrushDie First\nDeathTypes=NONE +CRUSHED\nTotalCrushSound=CarAlarm\nTotalCrushSound=NoSound\nBackEndCrushSound=AlarmBack\nFrontEndCrushSound=AlarmFront\nTotalCrushSoundPercent=-1\nBackEndCrushSoundPercent=101\nEnd\nBehavior=CrushDie Second\nFrontEndCrushSound=NoSound\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let rules = crush(&files[0].declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].sounds, ["CarAlarm", "AlarmBack", "AlarmFront"]);
        assert_eq!(rules[0].percentages, [-1, 101, 100]);
        assert_eq!(
            rules[0].conditions.death_mask,
            1 << generals_sim::skirmish::DeathType::Crushed as u32
        );
        assert!(rules[1].sounds.iter().all(String::is_empty));
        assert_eq!(rules[1].percentages, [100; 3]);
    }
}

#[cfg(test)]
mod upgrade_tests {
    #[test]
    fn upgrade_die_retains_nullable_instances_and_native_first_token() {
        let files = [generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "upgrade-death.ini".into(), origin: "UpgradeDie source contracts".into(),
            bytes: b"Object Drone\nBehavior=UpgradeDie First\nUpgradeToRemove=DroneUpgrade ExtraToken\nEnd\nBehavior=UpgradeDie Missing\nUpgradeToRemove=Unknown\nEnd\nEnd\nUpgrade DroneUpgrade\nEnd\n".to_vec(),
        }).unwrap()];
        let rules = super::upgrade(&files, &files[0].declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].upgrade.as_deref(), Some("DroneUpgrade"));
        assert!(rules[1].upgrade.is_none());
    }
}
