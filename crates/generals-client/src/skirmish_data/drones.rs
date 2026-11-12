//! Installed vehicle drone OCLs and native `SlavedUpdate` parameters.
use super::{blocks, definition, duration_frames, field, number, selected_pair, text};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::drones::{Repair, Rules, Upgrade};
use std::error::Error;

pub(super) fn supported(name: &str) -> bool {
    matches!(
        name,
        "Upgrade_AmericaBattleDrone" | "Upgrade_AmericaHellfireDrone" | "Upgrade_AmericaScoutDrone"
    )
}

pub(super) fn upgrades(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Upgrade>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "ObjectCreationUpgrade")
        {
            continue;
        }
        let trigger = text(module, 1, "TriggeredBy")?;
        if !supported(trigger) {
            continue;
        }
        let ocl = definition(
            files,
            "ObjectCreationList",
            text(module, 1, "UpgradeObject")?,
        )?;
        let creates = blocks(&ocl.lines, 0, "CreateObject")?;
        if creates.len() != 1 {
            return Err("drone OCL must create one native object".into());
        }
        let create = creates[0];
        if text(create, 1, "Count")? != "1"
            || text(create, 1, "Disposition")? != "LIKE_EXISTING"
            || text(create, 1, "IgnorePrimaryObstacle")? != "Yes"
            || text(create, 1, "RequiresLivePlayer")? != "Yes"
        {
            return Err("unsupported drone OCL creation policy".into());
        }
        let offset = creation_offset(create)?;
        result.push(Upgrade {
            trigger: trigger.into(),
            conflicts: field(module, 1, "ConflictsWith")
                .ok_or("missing drone conflicts")?
                .tokens[1..]
                .to_vec(),
            object: text(create, 1, "ObjectNames")?.into(),
            offset,
        });
    }
    Ok(result)
}

fn creation_offset(lines: &[ContentLine]) -> Result<[f32; 3], Box<dyn Error>> {
    let Some(line) = field(lines, 1, "Offset") else {
        return Ok([0.0; 3]);
    };
    let mut result = [0.0; 3];
    for (axis, prefix) in ["X:", "Y:", "Z:"].into_iter().enumerate() {
        let (index, value) = line
            .tokens
            .iter()
            .enumerate()
            .find_map(|(index, token)| token.strip_prefix(prefix).map(|value| (index, value)))
            .ok_or("missing drone offset component")?;
        let value = if value.is_empty() {
            line.tokens
                .get(index + 1)
                .map(String::as_str)
                .ok_or("missing drone offset value")?
        } else {
            value
        };
        let value: f32 = value.strip_suffix(['f', 'F']).unwrap_or(value).parse()?;
        if !value.is_finite() {
            return Err("nonfinite drone creation offset".into());
        }
        result[axis] = value;
    }
    Ok(result)
}

pub(super) fn rules(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let mut result = Vec::new();
    for module in modules.iter().copied().filter(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "SlavedUpdate")
    }) {
        result.push(slaved_rules(files, object, module)?);
    }
    Ok(result)
}

fn slaved_rules(
    files: &[ContentFile],
    object: &Declaration,
    module: &[ContentLine],
) -> Result<Rules, Box<dyn Error>> {
    let repair = repair(module)?;
    let mut panic = None;
    let mut panic_speed = 0.0;
    if let Some(name) = selected_pair(&object.lines, 0, "Locomotor", "SET_PANIC")? {
        let data = &definition(files, "Locomotor", name)?.lines;
        let globals = &definition(files, "GameData", "")?.lines;
        panic_speed = number(data, 0, "Speed")?;
        panic = Some(super::locomotor::load(data, globals, &object.lines)?);
    }
    Ok(Rules {
        guard_range: integer(module, "GuardMaxRange")?,
        guard_wander: integer(module, "GuardWanderRange")?,
        attack_range: integer(module, "AttackRange")?,
        attack_wander: integer(module, "AttackWanderRange")?,
        scout_range: integer(module, "ScoutRange")?,
        scout_wander: integer(module, "ScoutWanderRange")?,
        spotting_range: integer(module, "DistToTargetToGrantRangeBonus")?,
        repair,
        panic,
        panic_speed,
        stay_on_master_layer: field(module, 1, "StayOnSameLayerAsMaster")
            .map(super::scalar)
            .transpose()?
            .map_or(Ok(false), |value| {
                if value.eq_ignore_ascii_case("Yes") {
                    Ok(true)
                } else if value.eq_ignore_ascii_case("No") {
                    Ok(false)
                } else {
                    Err("invalid StayOnSameLayerAsMaster")
                }
            })?,
    })
}

fn integer(module: &[ContentLine], key: &str) -> Result<f32, Box<dyn Error>> {
    let Some(line) = field(module, 1, key) else {
        return Ok(0.0);
    };
    let value = super::scalar(line)?;
    value.parse::<i32>()?;
    Ok(value.parse()?)
}

fn repair(module: &[ContentLine]) -> Result<Option<Repair>, Box<dyn Error>> {
    if !module.iter().any(|line| {
        line.depth == Some(1)
            && line
                .tokens
                .first()
                .is_some_and(|key| key.starts_with("Repair"))
    }) {
        return Ok(None);
    }
    let frames = |key| field(module, 1, key).map_or(Ok(0), duration_frames);
    let real = |key| {
        field(module, 1, key).map_or(Ok(0.0), |line| super::finite_number(super::scalar(line)?))
    };
    let text = |key| {
        field(module, 1, key)
            .map(super::scalar)
            .transpose()
            .map(|value| value.unwrap_or_default().to_owned())
    };
    Ok(Some(Repair {
        range: integer(module, "RepairRange")?,
        altitude: [real("RepairMinAltitude")?, real("RepairMaxAltitude")?],
        per_second: real("RepairRatePerSecond")?,
        below_percent: field(module, 1, "RepairWhenBelowHealth%")
            .map(super::scalar)
            .transpose()?
            .unwrap_or("0")
            .parse()?,
        ready_frames: [frames("RepairMinReadyTime")?, frames("RepairMaxReadyTime")?],
        weld_frames: [frames("RepairMinWeldTime")?, frames("RepairMaxWeldTime")?],
        welding_system: text("RepairWeldingSys")?,
        welding_bone: text("RepairWeldingFXBone")?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str) -> ContentFile {
        generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "drones.ini".into(),
            origin: "authored drone field contracts".into(),
            bytes: source.as_bytes().to_vec(),
        })
        .unwrap()
    }

    const HELLFIRE: &str = "Object FixtureSlave
Behavior = SlavedUpdate Ranges
GuardMaxRange = 37
GuardWanderRange = 36
AttackRange = 77
AttackWanderRange = 12
ScoutRange = 81
ScoutWanderRange = 14
StayOnSameLayerAsMaster = Yes
End
Behavior = UpgradeDie Removal
DeathTypes = ALL
UpgradeToRemove = Upgrade_TestSlave OwnedSlot
End
End
";

    #[test]
    fn scout_creation_preserves_signed_local_offset_and_native_zero_default() {
        let source = "Object Parent\nBehavior = ObjectCreationUpgrade Drone\nTriggeredBy = Upgrade_AmericaScoutDrone\nConflictsWith = OtherFirst OtherSecond\nUpgradeObject = FixtureDroneCreation\nEnd\nEnd\nObjectCreationList FixtureDroneCreation\nCreateObject\nOffset = X:-8 Y:0 Z:10\nObjectNames = FixtureScout\nCount = 1\nRequiresLivePlayer = Yes\nDisposition = LIKE_EXISTING\nIgnorePrimaryObstacle = Yes\nEnd\nEnd\n";
        for (offset, expected) in [
            ("Offset = X:-8 Y:0 Z:10", [-8.0_f32, 0.0, 10.0]),
            ("Offset = X: -8f Y: 0 Z: 10", [-8.0, 0.0, 10.0]),
            ("", [0.0; 3]),
        ] {
            let files = [file(&source.replace("Offset = X:-8 Y:0 Z:10", offset))];
            let upgrades = upgrades(&files, &files[0].declarations[0]).unwrap();
            assert_eq!(
                upgrades[0].offset.map(f32::to_bits),
                expected.map(f32::to_bits)
            );
        }
        let files = [file(&source.replace("X:-8", "X:NaN"))];
        assert!(upgrades(&files, &files[0].declarations[0]).is_err());
    }

    #[test]
    fn empty_stinger_slaved_module_uses_native_zero_defaults_and_no_upgrade_die() {
        let file =
            file("Object GLAInfantryStingerSoldier\nBehavior = SlavedUpdate Slave\nEnd\nEnd\n");
        let files = [file];
        let object = definition(&files, "Object", "GLAInfantryStingerSoldier").unwrap();
        let rules = rules(&files, object).unwrap().into_iter().next().unwrap();
        assert_eq!(
            [
                rules.guard_range,
                rules.guard_wander,
                rules.attack_range,
                rules.attack_wander,
                rules.scout_range,
                rules.scout_wander
            ]
            .map(f32::to_bits),
            [0.0_f32.to_bits(); 6]
        );
        assert!(rules.repair.is_none());
        assert!(rules.panic.is_none());
    }

    #[test]
    fn slaved_ranges_are_signed_integers() {
        let files = [file(HELLFIRE)];
        let rules = rules(&files, &files[0].declarations[0])
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(rules.attack_range.to_bits(), 77.0f32.to_bits());
        assert!(rules.repair.is_none());
        let files = [file(
            &HELLFIRE.replace("GuardMaxRange = 37", "GuardMaxRange = -37"),
        )];
        let rules = super::rules(&files, &files[0].declarations[0]).unwrap();
        assert_eq!(rules[0].guard_range.to_bits(), (-37.0_f32).to_bits());
        let files = [file(
            &HELLFIRE.replace("GuardMaxRange = 37", "GuardMaxRange = 37.5"),
        )];
        assert!(super::rules(&files, &files[0].declarations[0]).is_err());
    }

    #[test]
    fn scout_spotting_range_is_loaded_as_native_integer_distance() {
        let source = HELLFIRE
            .replace("FixtureSlave", "FixtureScout")
            .replace("Upgrade_TestSlave", "Upgrade_TestScout")
            .replace(
                "GuardMaxRange = 37",
                "GuardMaxRange = 37\nDistToTargetToGrantRangeBonus = 20",
            );
        let files = [file(&source)];
        let rules = super::rules(&files, &files[0].declarations[0])
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(rules.spotting_range.to_bits(), 20.0_f32.to_bits());
    }

    #[test]
    fn drone_creation_requires_the_original_live_player_and_parent_obstacle_policy() {
        let source = "Object Tank
Behavior = ObjectCreationUpgrade Tag
TriggeredBy = Upgrade_AmericaBattleDrone
ConflictsWith = Upgrade_AmericaScoutDrone Upgrade_AmericaHellfireDrone
UpgradeObject = OriginalDrone
End
End
ObjectCreationList OriginalDrone
CreateObject
ObjectNames = AmericaVehicleBattleDrone
Count = 1
Disposition = LIKE_EXISTING
IgnorePrimaryObstacle = Yes
RequiresLivePlayer = Yes
Offset = X:0 Y:0 Z:10
End
End
";
        let files = [file(source)];
        let result = upgrades(&files, &files[0].declarations[0]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].object, "AmericaVehicleBattleDrone");
        assert_eq!(
            result[0].conflicts,
            ["Upgrade_AmericaScoutDrone", "Upgrade_AmericaHellfireDrone"]
        );
        for (before, after) in [
            ("RequiresLivePlayer = Yes", "RequiresLivePlayer = No"),
            ("IgnorePrimaryObstacle = Yes", "IgnorePrimaryObstacle = No"),
            ("Count = 1", "Count = 2"),
            ("Disposition = LIKE_EXISTING", "Disposition = NEUTRAL"),
        ] {
            let files = [file(&source.replace(before, after))];
            assert!(upgrades(&files, &files[0].declarations[0]).is_err());
        }
    }
}

// ProneUpdateModuleData: authored instances retain their default/last scalar value.
pub(super) fn prone_rules(object: &Declaration) -> Result<Vec<f32>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "ProneUpdate")
        })
        .map(|module| {
            field(module, 1, "DamageToFramesRatio")
                .map(|line| super::finite_number(super::scalar(line)?))
                .transpose()
                .map(|value| value.unwrap_or(1.0))
        })
        .collect()
}

#[cfg(test)]
mod prone_tests {
    use super::prone_rules;

    #[test]
    fn prone_instances_keep_defaults_and_last_authored_scalar() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "prone.ini".into(),
            origin: "ProneUpdate parser regression".into(),
            bytes: b"Object Soldier\nBehavior=ProneUpdate First\nEnd\nBehavior=ProneUpdate Second\nDamageToFramesRatio=2.5\nDamageToFramesRatio=1.75\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            prone_rules(&file.declarations[0])
                .unwrap()
                .iter()
                .copied()
                .map(f32::to_bits)
                .collect::<Vec<_>>(),
            [1.0_f32.to_bits(), 1.75_f32.to_bits()]
        );
    }
}
