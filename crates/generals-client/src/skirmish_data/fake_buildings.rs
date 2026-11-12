//! Original fake-building replacement and Worker command-set upgrades.
use super::{blocks, definition, field, finite_number, scalar, selected_pair, text};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{Research, UnitType, fake_buildings::Replacement};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    if !unit.fake_building() && object.name != "GLAInfantryWorker" {
        return Ok(());
    }
    if unit.fake_building() {
        let sets = blocks(&object.lines, 0, "WeaponSet")?;
        let [set] = sets.as_slice() else {
            return Err("Fake building requires its single original suicide weapon set".into());
        };
        if field(set, 1, "AutoChooseSources")
            .is_none_or(|line| line.tokens[1..] != ["PRIMARY", "None"])
            || selected_pair(set, 1, "Weapon", "PRIMARY")? != Some("SuicideWeapon")
        {
            return Err("Fake building command weapon requires source admission".into());
        }
        // Ordinary auto-choice admission skips None; this explicit command still
        // owns its original PRIMARY weapon and parses the shared weapon rules.
        unit.weapon.name = "SuicideWeapon".into();
        super::weapons::parse(
            &mut unit.weapon,
            &definition(files, "Weapon", "SuicideWeapon")?.lines,
        )?;
        unit.weapon.auto_choose = 0;
    }
    for module in blocks(&object.lines, 0, "Behavior")? {
        match module[0].tokens.get(1).map(String::as_str) {
            Some("ReplaceObjectUpgrade") => {
                let upgrade = text(module, 1, "TriggeredBy")?.to_owned();
                let replacement = text(module, 1, "ReplaceObject")?.to_owned();
                definition(files, "Object", &replacement)?;
                unit.maintenance.replacement = Some(Replacement {
                    upgrade: upgrade.clone(),
                    object: replacement,
                });
                research(files, &upgrade, unit)?;
            }
            Some("CommandSetUpgrade") => {
                if field(module, 1, "TriggerAlt").is_some()
                    || field(module, 1, "CommandSetAlt").is_some()
                {
                    return Err("Worker command-set alternate upgrade requires translation".into());
                }
                let upgrade = text(module, 1, "TriggeredBy")?.to_owned();
                research(files, &upgrade, unit)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn research(files: &[ContentFile], name: &str, unit: &mut UnitType) -> Result<(), Box<dyn Error>> {
    if unit
        .maintenance
        .research
        .iter()
        .any(|upgrade| upgrade.name == name)
    {
        return Ok(());
    }
    let upgrade = &definition(files, "Upgrade", name)?.lines;
    if text(upgrade, 0, "Type")? != "OBJECT" {
        return Err("Fake-building upgrade must have authored OBJECT scope".into());
    }
    let frames = (finite_number(text(upgrade, 0, "BuildTime")?)? * 30.0).trunc();
    if frames > f32::from(u16::MAX) {
        return Err("Fake-building upgrade duration exceeds source representation".into());
    }
    unit.maintenance.research.push(Research {
        name: name.to_owned(),
        cost: scalar(field(upgrade, 0, "BuildCost").ok_or("missing original upgrade cost")?)?
            .parse()?,
        frames: frames.to_string().parse()?,
        player: false,
    });
    Ok(())
}

// ReplaceObjectUpgrade stores an authored name, resolved at execution, not an eager template pointer.
pub(super) fn replacement_names(object: &Declaration) -> Result<Vec<String>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "ReplaceObjectUpgrade")
        })
        .map(|module| {
            Ok(field(module, 1, "ReplaceObject")
                .map(scalar)
                .transpose()?
                .unwrap_or("")
                .into())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_data::{finish_type, normal_model, source_files, unit_type};
    use generals_assets::virtual_assets::VirtualAssets;
    use generals_sim::skirmish::{Faction, Kind, physical_ocl};

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_five_fakes_keep_original_paid_replacements_and_ordered_death_graphs() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let (files, metadata) =
            source_files::effective(&assets, &crate::fake_building_verification::NAMES).unwrap();
        for (name, cost, seconds, health, upgrade_cost, upgrade_frames) in [
            ("FakeGLACommandCenter", 500, 30.0_f32, 1250.0_f32, 1500, 900),
            ("FakeGLABarracks", 125, 5.0, 125.0, 375, 300),
            ("FakeGLASupplyStash", 375, 5.0, 250.0, 1125, 300),
            ("FakeGLAArmsDealer", 625, 10.0, 500.0, 1875, 300),
            ("FakeGLABlackMarket", 625, 20.0, 125.0, 1875, 600),
        ] {
            let object = definition(&files, "Object", name).unwrap();
            let mut unit = unit_type(&files, object, Faction::Gla, Kind::TechBuilding)
                .and_then(|unit| finish_type(&files, object, unit))
                .unwrap();
            load(&files, object, &mut unit).unwrap();
            unit.behavior_modules = metadata[name].runtime_behaviors(&files).unwrap();
            assert_eq!(unit.cost, cost);
            assert_eq!(unit.build_seconds.to_bits(), seconds.to_bits());
            assert_eq!(unit.max_health.to_bits(), health.to_bits());
            assert_eq!(unit.weapon.auto_choose, 0);
            assert_eq!(
                unit.maintenance.replacement.as_ref().unwrap().object,
                name.strip_prefix("Fake").unwrap()
            );
            let research = &unit.maintenance.research[0];
            assert_eq!(research.cost, upgrade_cost);
            assert_eq!(research.frames, upgrade_frames);
            assert!(!research.player);
            assert!(unit.maintenance.expose_hole.is_empty());
            let death = unit.death_lifecycle.as_ref().unwrap();
            assert!(death.physics.is_none());
            assert_eq!(death.slow[0].conditions.exempt_status, 1 << 3);
            assert_eq!(death.slow[0].destruction_delay, [0, 0]);
            assert_eq!(death.weapons[0].name, "FakeStructureDetonationWeapon");
            let order: Vec<_> = unit
                .behavior_modules
                .iter()
                .filter(|module| module.interfaces & 2 != 0)
                .map(|module| module.family.as_str())
                .collect();
            assert_eq!(unit.behavior.death_fx.len(), 1);
            assert_eq!(unit.behavior.destroy_die.len(), 1);
            let expected = match name {
                "FakeGLACommandCenter" => [
                    "ProductionUpdate",
                    "CreateObjectDie",
                    "FXListDie",
                    "SlowDeathBehavior",
                    "DestroyDie",
                ],
                "FakeGLABlackMarket" => [
                    "ProductionUpdate",
                    "SlowDeathBehavior",
                    "DestroyDie",
                    "CreateObjectDie",
                    "FXListDie",
                ],
                _ => [
                    "CreateObjectDie",
                    "FXListDie",
                    "ProductionUpdate",
                    "SlowDeathBehavior",
                    "DestroyDie",
                ],
            };
            assert_eq!(order, expected);
            for list in physical_ocl::lists(&unit) {
                for nugget in &list.nuggets {
                    for model in &nugget.names {
                        assert!(
                            !assets
                                .read(&format!("art/w3d/{model}.w3d"))
                                .unwrap()
                                .bytes
                                .is_empty()
                        );
                    }
                }
            }
            assert!(!normal_model(&assets, object).unwrap().meshes.is_empty());
        }
    }
}
