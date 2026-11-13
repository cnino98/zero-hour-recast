//! Original upgrade modules for the explicitly enabled ordinary research commands.
use super::{
    armor_coefficients, blocks, definition, field, number, optional_number, scalar, selected_pair,
    text,
};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    ArmorUpgrade, ConditionUpgrade, ExperienceUpgrade, HealthUpgrade, MaintenanceRules,
    MovementUpgrade, PowerUpgrade, WeaponUpgrade,
};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
    rules: &mut MaintenanceRules,
) -> Result<(), Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        if !matches!(
            module[0].tokens.get(1).map(String::as_str),
            Some(
                "CommandSetUpgrade"
                    | "StealthUpgrade"
                    | "PowerPlantUpgrade"
                    | "ExperienceScalarUpgrade"
                    | "MaxHealthUpgrade"
                    | "ArmorUpgrade"
                    | "WeaponBonusUpgrade"
                    | "LocomotorSetUpgrade"
                    | "ModelConditionUpgrade"
            )
        ) {
            continue;
        }
        // The real shared UpgradeMuxData owns compound masks and eligibility.
        // This string remains the existing command/asset data association only.
        let trigger = field(module, 1, "TriggeredBy")
            .and_then(|line| line.tokens.get(1))
            .cloned()
            .unwrap_or_default();
        load_module(files, object, module, trigger, rules)?;
    }
    Ok(())
}

fn load_module(
    files: &[ContentFile],
    object: &Declaration,
    module: &[ContentLine],
    trigger: String,
    rules: &mut MaintenanceRules,
) -> Result<(), Box<dyn Error>> {
    match module[0].tokens.get(1).map(String::as_str) {
        Some("CommandSetUpgrade") => {
            rules
                .command_sets
                .push(command_set(files, module, trigger)?);
        }
        Some("StealthUpgrade") => {
            // StealthUpdate owns CAN_STEALTH and the original spawned-weapon
            // propagation. Its UpgradeMux executes at normal upgrade activation.
            if super::stealth::load(files, object)?.is_none() {
                return Err("stealth upgrade has no original StealthUpdate".into());
            }
        }
        Some("WeaponSetUpgrade" | "SpyVisionUpdate" | "AutoHealBehavior") => {
            // weapons/spy_vision own their modules; support::auto_heal owns each UpgradeMux.
        }
        Some("PowerPlantUpgrade") => {
            rules.power_upgrade = Some(PowerUpgrade {
                trigger,
                bonus: field(&object.lines, 0, "EnergyBonus")
                    .map(scalar)
                    .transpose()?
                    .unwrap_or("0")
                    .parse()?,
            });
        }
        Some("ExperienceScalarUpgrade") => {
            rules.experience_upgrades.push(ExperienceUpgrade {
                trigger,
                scalar: field(module, 1, "AddXPScalar")
                    .map_or(Ok(0.0), |_| number(module, 1, "AddXPScalar"))?,
            });
        }
        Some("MaxHealthUpgrade") => {
            let change_type = match field(module, 1, "ChangeType")
                .map(scalar)
                .transpose()?
                .unwrap_or("SAME_CURRENTHEALTH")
            {
                "PRESERVE_RATIO" => generals_sim::skirmish::MaxHealthChange::PreserveRatio,
                "ADD_CURRENT_HEALTH_TOO" => {
                    generals_sim::skirmish::MaxHealthChange::AddCurrentHealthToo
                }
                "SAME_CURRENTHEALTH" => generals_sim::skirmish::MaxHealthChange::SameCurrentHealth,
                "FULLY_HEAL" => generals_sim::skirmish::MaxHealthChange::FullyHeal,
                _ => return Err("unknown native maximum-health change type".into()),
            };
            rules.health_upgrades.push(HealthUpgrade {
                change_type,
                trigger,
                amount: field(module, 1, "AddMaxHealth")
                    .map_or(Ok(0.0), |_| number(module, 1, "AddMaxHealth"))?,
            });
        }
        Some("ArmorUpgrade") => {
            rules.armor_upgrade = Some(ArmorUpgrade {
                trigger,
                coefficients: upgraded_armor(files, object)?,
            });
        }
        Some("WeaponBonusUpgrade") => {
            rules.weapon_upgrade = Some(WeaponUpgrade { trigger });
        }
        Some("LocomotorSetUpgrade") => {
            rules.movement_upgrade = Some(movement(files, object, trigger)?);
        }
        Some("ModelConditionUpgrade") => {
            rules.condition_upgrades.push(ConditionUpgrade {
                trigger,
                condition: text(module, 1, "ConditionFlag")?.into(),
            });
        }
        _ => return Err(format!("unported module for enabled upgrade {trigger}").into()),
    }
    Ok(())
}

fn upgraded_armor(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<[f32; generals_sim::skirmish::DAMAGE_TYPE_COUNT], Box<dyn Error>> {
    // ArmorUpgrade sets PLAYER_UPGRADE; SparseMatchFinder chooses the best set,
    // including ordinary armor when the template has no distinct upgraded set.
    let flags = 1 << 3;
    let mut selected = None;
    let mut best_matches = 0;
    let mut best_extra = u32::MAX;
    for set in blocks(&object.lines, 0, "ArmorSet")? {
        let conditions = super::damage_fx::armor_set_conditions(set)?;
        let matches = (flags & conditions).count_ones();
        let extra = (!flags & conditions).count_ones();
        if matches > best_matches || (matches == best_matches && extra < best_extra) {
            selected = Some(set);
            best_matches = matches;
            best_extra = extra;
        }
    }
    let set = selected.ok_or("armor upgrade object has no inherited ArmorSet")?;
    let name = field(set, 1, "Armor").map(scalar).transpose()?;
    if let Some(name) = name.filter(|name| !name.eq_ignore_ascii_case("NONE")) {
        armor_coefficients(&definition(files, "Armor", name)?.lines)
    } else {
        Ok([1.0; generals_sim::skirmish::DAMAGE_TYPE_COUNT])
    }
}

fn command_set(
    files: &[ContentFile],
    module: &[ContentLine],
    trigger: String,
) -> Result<generals_sim::skirmish::fake_buildings::CommandSet, Box<dyn Error>> {
    let command_set = text(module, 1, "CommandSet")?.to_owned();
    definition(files, "CommandSet", &command_set)?;
    let alternate = match (
        field(module, 1, "TriggerAlt"),
        field(module, 1, "CommandSetAlt"),
    ) {
        (None, None) => None,
        (Some(upgrade), Some(command)) => {
            let command = scalar(command)?.to_owned();
            definition(files, "CommandSet", &command)?;
            Some((scalar(upgrade)?.to_owned(), command))
        }
        _ => return Err("incomplete command-set alternate upgrade".into()),
    };
    Ok(generals_sim::skirmish::fake_buildings::CommandSet {
        upgrade: trigger,
        command_set,
        removes: Vec::new(),
        alternate,
    })
}

pub(super) fn supported(name: &str) -> bool {
    [
        "Upgrade_AmericaRangerFlashBangGrenade",
        "Upgrade_AmericaTOWMissile",
        "Upgrade_AmericaAdvancedTraining",
        "Upgrade_AmericaCompositeArmor",
        "Upgrade_AmericaChemicalSuits",
        "Upgrade_AmericaSupplyLines",
        "Upgrade_AmericaDroneArmor",
        "Upgrade_AmericaSentryDroneGun",
        "Upgrade_ComancheRocketPods",
        "Upgrade_AmericaLaserMissiles",
        "Upgrade_AmericaCountermeasures",
        "Upgrade_ChinaAircraftArmor",
        "Upgrade_ChinaMines",
        "Upgrade_ChinaSatelliteHackOne",
        "Upgrade_ChinaSatelliteHackTwo",
        "Upgrade_GLAToxinShells",
        "Upgrade_GLABuggyAmmo",
        "Upgrade_GLAAPRockets",
        "Upgrade_GLAAnthraxBeta",
        "Upgrade_GLABombTruckBioBomb",
        "Upgrade_GLABombTruckHighExplosiveBomb",
        "Upgrade_GLAScorpionRocket",
        "Upgrade_AmericaAdvancedControlRods",
        "Upgrade_ChinaChainGuns",
        "Upgrade_ChinaBlackNapalm",
        "Upgrade_GLAFortifiedStructure",
        "Upgrade_GLAJunkRepair",
        "Upgrade_GLAAPBullets",
        "Upgrade_AmericaBunkerBusters",
        "Upgrade_GLAArmTheMob",
        "Upgrade_GLAWorkerShoes",
        "Upgrade_GLACamoNetting",
    ]
    .contains(&name)
}

fn movement(
    files: &[ContentFile],
    object: &Declaration,
    trigger: String,
) -> Result<MovementUpgrade, Box<dyn Error>> {
    let name = selected_pair(&object.lines, 0, "Locomotor", "SET_NORMAL_UPGRADED")?
        .ok_or("upgrade lacks original SET_NORMAL_UPGRADED locomotor")?;
    let lines = &definition(files, "Locomotor", name)?.lines;
    let normal_name = selected_pair(&object.lines, 0, "Locomotor", "SET_NORMAL")?
        .ok_or("upgrade lacks normal locomotor")?;
    let normal = &definition(files, "Locomotor", normal_name)?.lines;
    if super::locomotor_surface_mask(lines)? != super::locomotor_surface_mask(normal)?
        || text(lines, 0, "Appearance")? != text(normal, 0, "Appearance")?
        || optional_number(lines, "PreferredHeight", 0.0)? != 0.0
    {
        return Err("upgrade changes unported pathfinder movement class".into());
    }
    Ok(MovementUpgrade {
        trigger,
        speed: number(lines, 0, "Speed")?,
        locomotor: super::locomotor::load(
            lines,
            &definition(files, "GameData", "")?.lines,
            &object.lines,
        )?,
    })
}

pub(super) fn sub_objects(
    object: &Declaration,
) -> Result<Vec<generals_sim::skirmish::SubObjectUpgrade>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "SubObjectsUpgrade")
        {
            continue;
        }
        let names = |key| {
            module
                .iter()
                .filter(|line| line.depth == Some(1) && line.tokens[0] == key)
                .flat_map(|line| line.tokens[1..].iter().cloned())
                .collect()
        };
        result.push(generals_sim::skirmish::SubObjectUpgrade {
            show: names("ShowSubObjects"),
            hide: names("HideSubObjects"),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    fn object(fields: &str) -> ContentFile {
        content::scan(Asset {
            path: "upgrades.ini".into(),
            origin: "fixture".into(),
            bytes: format!("Object Test\n{fields}\nEnd\n").into_bytes(),
        })
        .unwrap()
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_worker_supply_boost_is_owned_by_worker_ai() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        let object = definition(&files, "Object", "GLAInfantryWorker").unwrap();
        let mut unit = super::super::unit_type(
            &files,
            object,
            generals_sim::skirmish::Faction::Gla,
            generals_sim::skirmish::Kind::Builder,
        )
        .unwrap();
        let model = super::super::normal_model(&assets, object).unwrap();
        crate::supply_data::load(object, &model, &mut unit).unwrap();
        let boost = unit.maintenance.collector_boost.unwrap();
        assert_eq!(boost.trigger, "Upgrade_GLAWorkerShoes");
        assert_eq!(boost.amount, 8);
        assert_eq!(
            unit.maintenance.movement_upgrade.unwrap().speed.to_bits(),
            30.0_f32.to_bits()
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_nuclear_tanks_load_full_upgraded_locomotors_without_worker_ai() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        for (name, speed, damaged, acceleration) in [
            ("ChinaTankBattleMaster", 35.0_f32, 32.0_f32, 1000.0_f32),
            ("ChinaTankOverlord", 30.0, 30.0, 30.0),
        ] {
            let object = definition(&files, "Object", name).unwrap();
            assert!(
                !blocks(&object.lines, 0, "Behavior")
                    .unwrap()
                    .iter()
                    .any(|module| {
                        module[0]
                            .tokens
                            .get(1)
                            .is_some_and(|family| family == "WorkerAIUpdate")
                    })
            );
            let mut rules = MaintenanceRules::none();
            load(&files, object, &mut rules).unwrap();
            let upgrade = rules.movement_upgrade.unwrap();
            assert_eq!(upgrade.trigger, "Upgrade_ChinaNuclearTanks");
            assert_eq!(upgrade.speed.to_bits(), speed.to_bits());
            assert_eq!(upgrade.locomotor.speed_damaged.to_bits(), damaged.to_bits());
            assert_eq!(upgrade.locomotor.legal_surfaces, 1);
            let ground = upgrade.locomotor.ground.unwrap();
            assert!(matches!(
                ground.appearance,
                generals_sim::skirmish::GroundAppearance::Treads
            ));
            assert_eq!(
                ground.acceleration.to_bits(),
                (acceleration * ((1.0_f32 / 30.0) * (1.0_f32 / 30.0))).to_bits()
            );
        }
    }

    #[test]
    fn armor_upgrade_selects_native_best_set_without_requiring_exact_upgrade_conditions() {
        let file = content::scan(Asset {
            path: "armor-upgrade.ini".into(),
            origin: "SparseMatchFinder regression".into(),
            bytes: b"Armor Ordinary\nArmor=DEFAULT 50%\nEnd\nArmor Upgraded\nArmor=DEFAULT 25%\nEnd\nObject Test\nArmorSet\nConditions=VETERAN\nArmor=Upgraded\nEnd\nArmorSet\nConditions=NONE\nArmor=Ordinary\nEnd\nBehavior=ArmorUpgrade Armor\nTriggeredBy=Upgrade_Mines\nEnd\nEnd\nObject Combined\nArmorSet\nConditions=NONE\nArmor=Ordinary\nEnd\nArmorSet\nConditions=PLAYER_UPGRADE VETERAN\nArmor=Upgraded\nEnd\nEnd\nObject Empty\nArmorSet\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let files = [file];
        let object = definition(&files, "Object", "Test").unwrap();
        let mut rules = MaintenanceRules::none();
        load(&files, object, &mut rules).unwrap();
        let upgrade = rules.armor_upgrade.unwrap();
        assert_eq!(upgrade.trigger, "Upgrade_Mines");
        assert!(
            upgrade
                .coefficients
                .iter()
                .all(|value| value.to_bits() == 0.5_f32.to_bits())
        );
        for (name, expected) in [("Combined", 0.25_f32), ("Empty", 1.0)] {
            let object = definition(&files, "Object", name).unwrap();
            assert!(
                upgraded_armor(&files, object)
                    .unwrap()
                    .iter()
                    .all(|value| value.to_bits() == expected.to_bits())
            );
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_helix_dependencies_validate_their_enabled_upgrade_consumers() {
        use crate::skirmish_data::{finish_type, source_files, unit_type};
        use generals_assets::virtual_assets::VirtualAssets;
        use generals_sim::skirmish::{Faction, Kind};
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = source_files(&assets).unwrap();
        for (name, faction, kind) in [
            ("ChinaVehicleHelix", Faction::China, Kind::Vehicle),
            ("ChinaHelixGattlingCannon", Faction::China, Kind::Vehicle),
            ("ChinaHelixPropagandaTower", Faction::China, Kind::Vehicle),
            ("ChinaHelixBattleBunker", Faction::China, Kind::Vehicle),
            ("HelixRubbleHull", Faction::China, Kind::Effect),
            ("NapalmBomb", Faction::China, Kind::Effect),
            ("AmericaParachute", Faction::Usa, Kind::Effect),
            ("GLAInfantryHijacker", Faction::Gla, Kind::Infantry),
        ] {
            let object = definition(&files, "Object", name).unwrap();
            let unit = unit_type(&files, object, faction, kind)
                .and_then(|unit| finish_type(&files, object, unit))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            if name == "NapalmBomb" {
                assert_eq!(unit.behavior.death_weapons.len(), 2);
                let normal = &unit.behavior.death_weapons[0];
                let black = &unit.behavior.death_weapons[1];
                assert_eq!(normal.weapon.as_ref().unwrap().name, "NapalmBombWeapon");
                assert!(normal.starts_active);
                assert!(normal.triggers.is_empty());
                assert_eq!(normal.conflicts, ["Upgrade_ChinaBlackNapalm"]);
                assert_eq!(black.weapon.as_ref().unwrap().name, "BlackNapalmBombWeapon");
                assert!(!black.starts_active);
                assert_eq!(black.triggers, ["Upgrade_ChinaBlackNapalm"]);
                assert!(black.conflicts.is_empty());
            }
        }
    }

    #[test]
    fn auto_heal_loader_keeps_compound_trigger_and_conflict_fields() {
        let file = object(
            "Behavior=AutoHealBehavior Tag\nTriggeredBy=Upgrade_GLAJunkRepair Upgrade_Another\nConflictsWith=Upgrade_Conflict\nRequiresAllTriggers=Yes\nHealingAmount=2\nHealingDelay=1000\nRadius=100\nEnd",
        );
        let healers =
            crate::skirmish_data::support::auto_heal(&file.declarations[0].lines).unwrap();
        assert_eq!(healers.len(), 1);
        assert_eq!(
            healers[0].triggers,
            ["Upgrade_GLAJunkRepair", "Upgrade_Another"]
        );
        assert_eq!(healers[0].conflicts, ["Upgrade_Conflict"]);
        assert!(healers[0].requires_all);
        assert_eq!(healers[0].delay, 30);
        assert_eq!(healers[0].targets.radius.to_bits(), 100.0_f32.to_bits());
    }
    #[test]
    fn strategy_research_preserves_duplicate_health_modules_and_experience_addition() {
        let file = object(
            "Behavior=ExperienceScalarUpgrade Xp\nTriggeredBy=Upgrade_AmericaAdvancedTraining\nAddXPScalar=1.0\nEnd\nBehavior=MaxHealthUpgrade First\nTriggeredBy=Upgrade_AmericaCompositeArmor\nAddMaxHealth=100\nChangeType=ADD_CURRENT_HEALTH_TOO\nEnd\nBehavior=MaxHealthUpgrade Second\nTriggeredBy=Upgrade_AmericaCompositeArmor\nAddMaxHealth=100\nChangeType=ADD_CURRENT_HEALTH_TOO\nEnd",
        );
        let mut rules = MaintenanceRules::none();
        load(
            std::slice::from_ref(&file),
            &file.declarations[0],
            &mut rules,
        )
        .unwrap();
        assert_eq!(rules.experience_upgrades.len(), 1);
        assert_eq!(
            rules.experience_upgrades[0].scalar.to_bits(),
            1.0_f32.to_bits()
        );
        assert_eq!(rules.health_upgrades.len(), 2);
        assert!(
            rules
                .health_upgrades
                .iter()
                .all(|module| module.amount.to_bits() == 100.0_f32.to_bits())
        );
    }

    #[test]
    fn object_camo_requires_a_real_stealth_module_and_preserves_the_original_trigger() {
        let file = object(
            "Behavior=StealthUpdate Cloak\nStealthDelay=2500\nStealthForbiddenConditions=ATTACKING USING_ABILITY TAKING_DAMAGE\nInnateStealth=No\nEnd\nBehavior=StealthUpgrade Camo\nTriggeredBy=Upgrade_GLACamoNetting\nEnd",
        );
        load(
            std::slice::from_ref(&file),
            &file.declarations[0],
            &mut MaintenanceRules::none(),
        )
        .unwrap();
        let stealth =
            super::super::stealth::load(std::slice::from_ref(&file), &file.declarations[0])
                .unwrap()
                .unwrap();
        assert_eq!(stealth.upgrades, ["Upgrade_GLACamoNetting"]);
        assert_eq!(stealth.delay, 75);
        assert!(!stealth.innate);
        let missing =
            object("Behavior=StealthUpgrade Camo\nTriggeredBy=Upgrade_GLACamoNetting\nEnd");
        assert!(
            load(
                std::slice::from_ref(&missing),
                &missing.declarations[0],
                &mut MaintenanceRules::none()
            )
            .is_err()
        );
    }
}
