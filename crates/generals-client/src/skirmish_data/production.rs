//! `BuildAssistant::isPossibleToMakeUnit` scans the producer's authored command set.
use super::{definition, field, scalar};
use generals_assets::content::{ContentFile, Declaration};
use std::error::Error;

type Quantities = Vec<(String, u32)>;

fn modules(
    object: &Declaration,
) -> Result<Vec<&[generals_assets::content::ContentLine]>, Box<dyn Error>> {
    Ok(super::blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "ProductionUpdate")
        })
        .collect())
}

pub(super) fn additional(
    object: &Declaration,
) -> Result<Vec<generals_sim::skirmish::ProductionRules>, Box<dyn Error>> {
    modules(object)?
        .into_iter()
        .skip(1)
        .map(module_rules)
        .collect()
}

fn module_rules(
    module: &[generals_assets::content::ContentLine],
) -> Result<generals_sim::skirmish::ProductionRules, Box<dyn Error>> {
    let mut quantities = Vec::new();
    for line in module.iter().filter(|line| {
        line.depth == Some(1)
            && line
                .tokens
                .first()
                .is_some_and(|key| key == "QuantityModifier")
    }) {
        let [_, name, count] = line.tokens.as_slice() else {
            return Err("invalid original production quantity".into());
        };
        let count = count.parse::<u32>()?;
        if count == 0 {
            return Err("zero original production quantity".into());
        }
        quantities.push((name.clone(), count));
    }
    Ok(generals_sim::skirmish::ProductionRules {
        quantities,
        queue_limit: field(module, 1, "MaxQueueEntries")
            .map(scalar)
            .transpose()?
            .unwrap_or("9")
            .parse()?,
        disabled_types: module_disabled_types(module)?,
        doors: module_doors(module)?,
    })
}

pub(super) fn queue_limit(object: &Declaration) -> Result<i32, Box<dyn Error>> {
    Ok(modules(object)?
        .first()
        .map(|module| module_rules(module))
        .transpose()?
        .map_or(9, |rules| rules.queue_limit))
}

pub(super) fn quantities(object: &Declaration) -> Result<Option<Quantities>, Box<dyn Error>> {
    Ok(modules(object)?
        .first()
        .map(|module| module_rules(module))
        .transpose()?
        .map(|rules| rules.quantities))
}

pub(super) fn exit_frames(
    module: &[generals_assets::content::ContentLine],
) -> Result<u32, Box<dyn Error>> {
    field(module, 1, "ExitDelay").map_or(Ok(0), super::duration_frames)
}

pub(super) fn objects(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<String>, Box<dyn Error>> {
    let Some(line) = field(&object.lines, 0, "CommandSet") else {
        return Ok(Vec::new());
    };
    objects_for_set(files, scalar(line)?)
}

pub(super) fn command_sets(
    files: &[ContentFile],
) -> Result<std::collections::BTreeMap<String, Vec<String>>, Box<dyn Error>> {
    let mut sets = std::collections::BTreeMap::new();
    for declaration in files
        .iter()
        .flat_map(|file| &file.declarations)
        .filter(|declaration| declaration.kind == "CommandSet")
    {
        sets.insert(
            declaration.name.clone(),
            objects_for_set(files, &declaration.name)?,
        );
    }
    Ok(sets)
}

fn objects_for_set(files: &[ContentFile], name: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let mut products = Vec::new();
    for button in buttons_for_set(files, name)?.into_iter().flatten() {
        let command = field(&button.lines, 0, "Command").map(scalar).transpose()?;
        if !matches!(command, Some("UNIT_BUILD" | "DOZER_CONSTRUCT")) {
            continue;
        }
        if let Some(line) = field(&button.lines, 0, "Object") {
            products.push(scalar(line)?.into());
        }
    }
    Ok(products)
}

pub(super) fn location_weapons(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<u8, Box<dyn Error>> {
    let mut slots = 0;
    for button in buttons(files, object)?.into_iter().flatten() {
        if field(&button.lines, 0, "Command").map(scalar).transpose()? != Some("FIRE_WEAPON") {
            continue;
        }
        if !field(&button.lines, 0, "Options")
            .is_some_and(|line| line.tokens.iter().any(|word| word == "NEED_TARGET_POS"))
        {
            continue;
        }
        // This command switches to the conditional DISARM set before entering
        // target mode; it does not use the projectile attack-position consumer.
        if field(&button.lines, 0, "Options").is_some_and(|line| {
            line.tokens
                .iter()
                .any(|word| word == "USES_MINE_CLEARING_WEAPONSET")
        }) {
            continue;
        }
        slots |= 1 << super::command_button_hunt::weapon_slot(button)?;
    }
    Ok(slots)
}

fn buttons<'a>(
    files: &'a [ContentFile],
    object: &Declaration,
) -> Result<[Option<&'a Declaration>; 18], Box<dyn Error>> {
    let Some(line) = field(&object.lines, 0, "CommandSet") else {
        return Ok([None; 18]);
    };
    buttons_for_set(files, scalar(line)?)
}

fn buttons_for_set<'a>(
    files: &'a [ContentFile],
    name: &str,
) -> Result<[Option<&'a Declaration>; 18], Box<dyn Error>> {
    if name.eq_ignore_ascii_case("NONE") {
        return Ok([None; 18]);
    }
    let set = definition(files, "CommandSet", name)?;
    // CommandSet keeps eighteen slots, including four script-only slots. A
    // repeated assignment replaces the earlier button before build admission.
    let mut buttons = [None; 18];
    for line in set.lines.iter().filter(|line| line.depth == Some(0)) {
        let Some(slot) = line
            .tokens
            .first()
            .and_then(|key| key.parse::<usize>().ok())
        else {
            continue;
        };
        let index = slot
            .checked_sub(1)
            .filter(|&index| index < buttons.len())
            .ok_or("original command slot outside 1..=18")?;
        buttons[index] = Some(definition(files, "CommandButton", scalar(line)?)?);
    }
    Ok(buttons)
}

pub(super) fn factory_bib(
    lines: &[generals_assets::content::ContentLine],
) -> Result<generals_sim::skirmish::FactoryBib, Box<dyn std::error::Error>> {
    Ok(generals_sim::skirmish::FactoryBib {
        exit_width: super::optional_number(lines, "FactoryExitWidth", 0.0)?,
        extra_width: super::optional_number(lines, "FactoryExtraBibWidth", 0.0)?,
    })
}

pub(super) fn doors(
    object: &Declaration,
) -> Result<generals_sim::skirmish::parking::DoorRules, Box<dyn Error>> {
    modules(object)?.first().map_or(
        Ok(generals_sim::skirmish::parking::DoorRules::NONE),
        |module| module_doors(module),
    )
}

fn module_doors(
    module: &[generals_assets::content::ContentLine],
) -> Result<generals_sim::skirmish::parking::DoorRules, Box<dyn Error>> {
    let count = field(module, 1, "NumDoorAnimations")
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse::<i32>()?;
    let duration = |key| field(module, 1, key).map_or(Ok(0), super::duration_frames);
    Ok(generals_sim::skirmish::parking::DoorRules {
        count,
        opening: duration("DoorOpeningTime")?,
        waiting: duration("DoorWaitOpenTime")?,
        closing: duration("DoorCloseTime")?,
        complete: duration("ConstructionCompleteDuration")?,
    })
}

pub(super) fn disabled_types(object: &Declaration) -> Result<u32, Box<dyn Error>> {
    modules(object)?
        .first()
        .map_or(Ok(1 << 3), |module| module_disabled_types(module))
}

fn module_disabled_types(
    module: &[generals_assets::content::ContentLine],
) -> Result<u32, Box<dyn Error>> {
    const NAMES: [&str; 13] = [
        "DEFAULT",
        "DISABLED_HACKED",
        "DISABLED_EMP",
        "DISABLED_HELD",
        "DISABLED_PARALYZED",
        "DISABLED_UNMANNED",
        "DISABLED_UNDERPOWERED",
        "DISABLED_FREEFALL",
        "DISABLED_AWESTRUCK",
        "DISABLED_BRAINWASHED",
        "DISABLED_SUBDUED",
        "DISABLED_SCRIPT_DISABLED",
        "DISABLED_SCRIPT_UNDERPOWERED",
    ];
    let mut mask = 1 << 3;
    for line in module.iter().filter(|line| {
        line.depth == Some(1)
            && line
                .tokens
                .first()
                .is_some_and(|key| key == "DisabledTypesToProcess")
    }) {
        let mut normal = false;
        let mut modifying = false;
        for token in line.tokens.iter().skip(1) {
            if token.eq_ignore_ascii_case("NONE") {
                if normal || modifying {
                    return Err("mixed ProductionUpdate disabled-mask operations".into());
                }
                mask = 0;
                break;
            }
            let (name, change) = match token.as_bytes().first() {
                Some(b'+') => (&token[1..], Some(true)),
                Some(b'-') => (&token[1..], Some(false)),
                _ => (token.as_str(), None),
            };
            let index = NAMES
                .iter()
                .position(|value| value.eq_ignore_ascii_case(name))
                .ok_or("invalid ProductionUpdate disabled type")?;
            if let Some(set) = change {
                if normal {
                    return Err("mixed ProductionUpdate disabled-mask operations".into());
                }
                modifying = true;
                if set {
                    mask |= 1 << index;
                } else {
                    mask &= !(1 << index);
                }
            } else {
                if modifying {
                    return Err("mixed ProductionUpdate disabled-mask operations".into());
                }
                if !normal {
                    mask = 0;
                }
                normal = true;
                mask |= 1 << index;
            }
        }
    }
    Ok(mask)
}

#[cfg(test)]
mod tests {
    #[test]
    fn production_door_count_retains_native_signed_integer_semantics() {
        use generals_assets::{content, virtual_assets::Asset};
        for count in [-1, 0, 1, 5] {
            let file = content::scan(Asset {
                path: "production.ini".into(), origin: "ProductionUpdate door count".into(),
                bytes: format!("Object Factory\nBehavior=ProductionUpdate P\nNumDoorAnimations={count}\nEnd\nEnd\n").into_bytes(),
            }).unwrap();
            assert_eq!(super::doors(&file.declarations[0]).unwrap().count, count);
        }
    }

    #[test]
    fn production_disabled_mask_keeps_native_defaults_and_relative_operations() {
        use generals_assets::{content, virtual_assets::Asset};
        let file = content::scan(Asset {
            path: "production.ini".into(), origin: "ProductionUpdate disabled mask".into(),
            bytes: b"Object Default\nBehavior=ProductionUpdate P\nEnd\nEnd\nObject Relative\nBehavior=ProductionUpdate P\nDisabledTypesToProcess=+DISABLED_HACKED -DISABLED_HELD\nEnd\nEnd\nObject HeldAndFreefall\nBehavior=ProductionUpdate P\nDisabledTypesToProcess=DISABLED_HELD DISABLED_FREEFALL\nEnd\nEnd\nObject None\nBehavior=ProductionUpdate P\nDisabledTypesToProcess=NONE\nEnd\nEnd\nObject Invalid\nBehavior=ProductionUpdate P\nDisabledTypesToProcess=DISABLED_HELD +DISABLED_HACKED\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        for (index, mask) in [(0, 1 << 3), (1, 1 << 1), (2, (1 << 3) | (1 << 7)), (3, 0)] {
            assert_eq!(
                super::disabled_types(&file.declarations[index]).unwrap(),
                mask
            );
        }
        assert!(super::disabled_types(&file.declarations[4]).is_err());
    }

    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn named_command_sets_keep_unreferenced_upgrade_sets_and_native_slot_order() {
        let files = [content::scan(Asset {
            path: "commands.ini".into(),
            origin: "ControlBar live override table".into(),
            bytes: b"Object Worker
CommandSet=RealSet
End
CommandSet RealSet
1=Real
End
CommandSet FakeSet
18=FakeB
3=FakeA
1=Real
1=Upgrade
End
CommandSet AlternateSet
7=FakeB
End
CommandButton Real
Command=DOZER_CONSTRUCT
Object=RealBuilding
End
CommandButton FakeA
Command=DOZER_CONSTRUCT
Object=FakeA
End
CommandButton FakeB
Command=UNIT_BUILD
Object=FakeB
End
CommandButton Upgrade
Command=OBJECT_UPGRADE
Object=UpgradeName
End
"
            .to_vec(),
        })
        .unwrap()];
        let sets = command_sets(&files).unwrap();
        assert_eq!(sets["RealSet"], ["RealBuilding"]);
        assert_eq!(sets["FakeSet"], ["FakeA", "FakeB"]);
        assert_eq!(sets["AlternateSet"], ["FakeB"]);
        assert_eq!(
            objects(&files, definition(&files, "Object", "Worker").unwrap()).unwrap(),
            ["RealBuilding"]
        );
    }

    #[test]
    fn original_queue_limit_retains_default_and_signed_authored_values() {
        let file = content::scan(Asset {
            path: "production.ini".into(),
            origin: "ProductionUpdateModuleData MaxQueueEntries".into(),
            bytes: b"Object Default\nBehavior=ProductionUpdate Factory\nEnd\nEnd\nObject One\nBehavior=ProductionUpdate Factory\nMaxQueueEntries=1\nEnd\nEnd\nObject Blocked\nBehavior=ProductionUpdate Factory\nMaxQueueEntries=-1\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(queue_limit(&file.declarations[0]).unwrap(), 9);
        assert_eq!(queue_limit(&file.declarations[1]).unwrap(), 1);
        assert_eq!(queue_limit(&file.declarations[2]).unwrap(), -1);
    }

    #[test]
    fn object_target_pilot_command_is_not_a_location_weapon() {
        let files = [content::scan(Asset {
            path: "production.ini".into(),
            origin: "native object versus position weapon options".into(),
            bytes: b"Object Hero\nCommandSet=Commands\nEnd\nCommandSet Commands\n1=Pilot\n2=Ground\nEnd\nCommandButton Pilot\nCommand=FIRE_WEAPON\nWeaponSlot=SECONDARY\nOptions=OK_FOR_MULTI_SELECT NEED_TARGET_ENEMY_OBJECT CONTEXTMODE_COMMAND\nEnd\nCommandButton Ground\nCommand=FIRE_WEAPON\nWeaponSlot=TERTIARY\nOptions=OK_FOR_MULTI_SELECT NEED_TARGET_POS\nEnd\n".to_vec(),
        }).unwrap()];
        assert_eq!(
            location_weapons(&files, definition(&files, "Object", "Hero").unwrap()).unwrap(),
            4
        );
    }

    #[test]
    fn empty_production_update_remains_distinct_from_no_production_interface() {
        let file = content::scan(Asset {
            path: "production.ini".into(), origin: "native factory registration fixture".into(),
            bytes: b"Object Scenery\nEnd\nObject Factory\nBehavior=ProductionUpdate Produce\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        assert!(quantities(&file.declarations[0]).unwrap().is_none());
        assert_eq!(quantities(&file.declarations[1]).unwrap(), Some(Vec::new()));
    }

    #[test]
    fn native_quantity_order_and_exit_delay_are_loaded_without_deduplicating() {
        let file = content::scan(Asset {
            path: "production.ini".into(), origin: "queue fixture".into(),
            bytes: b"Object Barracks\nBehavior=ProductionUpdate Produce\nQuantityModifier=Redguard 2\nQuantityModifier=Redguard 3\nEnd\nBehavior=QueueProductionExitUpdate Exit\nUnitCreatePoint=X:0 Y:0 Z:0\nNaturalRallyPoint=X:0 Y:20 Z:0\nExitDelay=300\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let object = &file.declarations[0];
        assert_eq!(
            quantities(object).unwrap(),
            Some(vec![("Redguard".into(), 2), ("Redguard".into(), 3)])
        );
        let exit = super::super::production_exit(object, generals_sim::skirmish::Kind::Barracks)
            .unwrap()
            .unwrap();
        let queue = exit.queue.unwrap();
        assert_eq!(queue.delay_frames, 9);
        assert_eq!(queue.initial_burst, 0);
    }

    #[test]
    fn only_original_build_commands_admit_products_not_object_upgrades_or_exit_buttons() {
        let file = content::scan(Asset { path: "production.ini".into(), origin: "command-set fixture".into(), bytes: b"Object Factory\nCommandSet = FactorySet\nEnd\nCommandSet FactorySet\n1 = BuildTank\n2 = DroneUpgrade\n3 = Exit\n4 = BuildTower\nEnd\nCommandButton BuildTank\nCommand = UNIT_BUILD\nObject = Tank\nEnd\nCommandButton DroneUpgrade\nCommand = OBJECT_UPGRADE\nObject = Drone\nEnd\nCommandButton Exit\nCommand = EXIT_CONTAINER\nObject = Passenger\nEnd\nCommandButton BuildTower\nCommand = DOZER_CONSTRUCT\nObject = Tower\nEnd\n".to_vec() }).unwrap();
        let files = [file];
        assert_eq!(
            objects(&files, definition(&files, "Object", "Factory").unwrap()).unwrap(),
            ["Tank", "Tower"]
        );
    }

    #[test]
    fn command_slots_replace_prior_buttons_and_keep_script_only_products() {
        let files = [content::scan(Asset {
            path: "production.ini".into(),
            origin: "native eighteen-slot command set".into(),
            bytes: b"Object Factory\nCommandSet=FactorySet\nEnd\nCommandSet FactorySet\n1=BuildTank\n18=BuildTank\n1=Upgrade\nEnd\nCommandButton BuildTank\nCommand=UNIT_BUILD\nObject=Tank\nEnd\nCommandButton Upgrade\nCommand=OBJECT_UPGRADE\nObject=Drone\nEnd\n".to_vec(),
        }).unwrap()];
        assert_eq!(
            objects(&files, definition(&files, "Object", "Factory").unwrap()).unwrap(),
            ["Tank"]
        );
        for slot in [0, 19] {
            let invalid = content::scan(Asset {
                path: "production.ini".into(),
                origin: "invalid command slot".into(),
                bytes: format!("Object Factory\nCommandSet=FactorySet\nEnd\nCommandSet FactorySet\n{slot}=BuildTank\nEnd\nCommandButton BuildTank\nCommand=UNIT_BUILD\nObject=Tank\nEnd\n").into_bytes(),
            }).unwrap();
            let files = [invalid];
            assert!(objects(&files, definition(&files, "Object", "Factory").unwrap()).is_err());
        }
    }
}

#[cfg(test)]
mod instance_tests;
