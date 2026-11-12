use super::{blocks, definition, field, finite_number, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{StartingVeterancy, UnitType, VeterancyRules};
use std::error::Error;

/// Consume a reconciled object: surviving modules at depth zero, fields at depth one.
/// Raw declaration fixtures must materialize inheritance through `module_metadata` first.
pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    let global = &definition(files, "GameData", "")?.lines;
    let mut rules = VeterancyRules::none();
    rules.experience_value = integers(&object.lines, "ExperienceValue")?;
    rules.required = integers(&object.lines, "ExperienceRequired")?;
    rules.trainable = boolean(&object.lines, 0, "IsTrainable", false)?;
    for (index, name) in ["Veteran", "Elite", "Heroic"].into_iter().enumerate() {
        let key = format!("HealthBonus_{name}");
        rules.health_bonus[index + 1] = percent(scalar(
            field(global, 0, &key).ok_or("missing health bonus")?,
        )?)?;
    }
    for key in ["WeaponSet", "ArmorSet"] {
        for set in blocks(&object.lines, 0, key)? {
            if field(set, 1, "Conditions")
                .is_some_and(|line| line.tokens.iter().skip(1).any(|name| level(name).is_some()))
            {
                return Err(format!("veterancy-specific {key} needs its native set switch").into());
            }
        }
    }
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "VeterancyGainCreate")
        {
            let starting = field(module, 1, "StartingLevel")
                .map(scalar)
                .transpose()?
                .unwrap_or("REGULAR");
            rules.starting.push(StartingVeterancy {
                level: level(starting).ok_or("invalid create veterancy level")?,
                science: field(module, 1, "ScienceRequired")
                    .map(scalar)
                    .transpose()?
                    .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                    .map(str::to_owned),
            });
        }
    }
    unit.veterancy = rules;
    Ok(())
}

fn integers(lines: &[ContentLine], key: &str) -> Result<[i32; 4], Box<dyn Error>> {
    let Some(line) = field(lines, 0, key) else {
        // ThingTemplate's constructor clears all four values.
        return Ok([0; 4]);
    };
    let [_, regular, veteran, elite, heroic] = line.tokens.as_slice() else {
        return Err(format!("{key} must contain four source levels").into());
    };
    let signed = |value: &str| {
        super::source_numbers::unsigned(value).map(|value| i32::from_ne_bytes(value.to_ne_bytes()))
    };
    Ok([
        signed(regular)?,
        signed(veteran)?,
        signed(elite)?,
        signed(heroic)?,
    ])
}

fn level(name: &str) -> Option<u8> {
    match name {
        "REGULAR" => Some(0),
        "VETERAN" => Some(1),
        "ELITE" => Some(2),
        "HERO" | "HEROIC" => Some(3),
        _ => None,
    }
}

fn percent(value: &str) -> Result<f32, Box<dyn Error>> {
    Ok(finite_number(
        value
            .strip_suffix('%')
            .ok_or("bonus lacks percent suffix")?,
    )? / 100.0)
}

fn boolean(
    lines: &[ContentLine],
    depth: u32,
    key: &str,
    default: bool,
) -> Result<bool, Box<dyn Error>> {
    let Some(line) = field(lines, depth, key) else {
        return Ok(default);
    };
    match scalar(line)?.to_ascii_lowercase().as_str() {
        "yes" => Ok(true),
        "no" => Ok(false),
        _ => Err(format!("invalid {key} boolean").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    fn object(fields: &str) -> ContentFile {
        content::scan(Asset {
            path: "test.ini".into(),
            origin: "fixture".into(),
            bytes: format!("Object Test\n{fields}\nEnd\n").into_bytes(),
        })
        .unwrap()
    }

    #[test]
    fn experience_tables_keep_signed_native_int_prefixes_and_narrowing() {
        let file = object("ExperienceRequired=0 20 40");
        assert!(integers(&file.declarations[0].lines, "ExperienceRequired").is_err());
        let file = object("ExperienceRequired=9 -20 40tail 4294967295\nExperienceValue=-5 0 10 20");
        assert_eq!(
            integers(&file.declarations[0].lines, "ExperienceRequired").unwrap(),
            [9, -20, 40, -1]
        );
        assert_eq!(
            integers(&file.declarations[0].lines, "ExperienceValue").unwrap(),
            [-5, 0, 10, 20]
        );
    }

    #[test]
    fn ordered_default_and_research_healers_keep_separate_data() {
        let file = object(
            "Behavior=AutoHealBehavior Default\nHealingAmount=10\nHealingDelay=1000\nTriggeredBy=Upgrade_Veterancy_ELITE Upgrade_Veterancy_HEROIC\nStartHealingDelay=5000\nEnd\nBehavior=AutoHealBehavior Junk\nHealingAmount=2\nHealingDelay=2000\nTriggeredBy=Upgrade_GLAJunkRepair\nEnd",
        );
        let modules =
            crate::skirmish_data::support::auto_heal(&file.declarations[0].lines).unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].damage_delay, 150);
        assert_eq!(modules[1].damage_delay, 0);
        assert_eq!(modules[0].delay, 30);
        assert_eq!(modules[1].delay, 60);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_trainable_healing_consumes_surviving_removed_and_replaced_modules() {
        use generals_assets::virtual_assets::VirtualAssets;
        use generals_sim::skirmish::{Faction, Kind};
        let root = &generals_assets::game_root().unwrap();
        for (override_text, expected, trainable) in [
            ("", Some((10.0_f32, 30, 150, 0b1100)), true),
            (
                "Object AmericaTankCrusader\nIsTrainable=No\nEnd\n",
                Some((10.0, 30, 150, 0b1100)),
                false,
            ),
            (
                "Object AmericaTankCrusader\nRemoveModule=ModuleTag_DefaultAutoHealBehavior\nEnd\n",
                None,
                true,
            ),
            (
                "Object AmericaTankCrusader\nReplaceModule=ModuleTag_DefaultAutoHealBehavior\nBehavior=AutoHealBehavior MapVeterancyHeal\nHealingAmount=7\nHealingDelay=2000\nStartHealingDelay=3000\nTriggeredBy=Upgrade_Veterancy_ELITE\nEnd\nEnd\nEnd\n",
                Some((7.0, 60, 90, 0b0100)),
                true,
            ),
        ] {
            let assets = VirtualAssets::for_zero_hour(
                root.join("GameData/Command and Conquer Generals"),
                root.join("GameData/Command and Conquer Generals Zero Hour"),
            )
            .unwrap();
            let sources = if override_text.is_empty() {
                Vec::new()
            } else {
                vec![Asset {
                    path: "maps/healing/map.ini".into(),
                    origin: "raw healing regression".into(),
                    bytes: override_text.as_bytes().to_vec(),
                }]
            };
            let assets = assets.with_map_ini_provenance(Vec::new(), sources).unwrap();
            let (files, metadata) =
                crate::skirmish_data::source_files::effective(&assets, &["AmericaTankCrusader"])
                    .unwrap();
            let object = definition(&files, "Object", "AmericaTankCrusader").unwrap();
            let mut unit =
                crate::skirmish_data::unit_type(&files, object, Faction::Usa, Kind::Vehicle)
                    .unwrap();
            load(&files, object, &mut unit).unwrap();
            assert_eq!(unit.veterancy.trainable, trainable);
            let actual = unit.maintenance.auto_heal.first().map(|rule| {
                (
                    rule.amount.to_bits(),
                    rule.delay,
                    rule.damage_delay,
                    rule.triggers.iter().fold(0_u8, |mask, name| {
                        mask | match name.as_str() {
                            "Upgrade_Veterancy_ELITE" => 4,
                            "Upgrade_Veterancy_HEROIC" => 8,
                            _ => 0,
                        }
                    }),
                )
            });
            assert_eq!(
                actual,
                expected.map(|(amount, delay, damage, levels)| (
                    amount.to_bits(),
                    delay,
                    damage,
                    levels
                ))
            );
            let healers: Vec<_> = metadata["AmericaTankCrusader"].modules[0]
                .iter()
                .filter(|entry| entry.family == "AutoHealBehavior")
                .collect();
            assert_eq!(healers.len(), usize::from(expected.is_some()));
            if !override_text.is_empty() && trainable && expected.is_some() {
                assert_eq!(healers[0].tag, "MapVeterancyHeal");
                assert_eq!(healers[0].source.path, "maps/healing/map.ini");
                assert_eq!(healers[0].source.origin, "raw healing regression");
                assert_eq!(healers[0].source.line, 3);
                assert!(!healers[0].copied_from_default);
            }
        }
    }
}
