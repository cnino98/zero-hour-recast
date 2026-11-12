//! Selected Object weapons and OCL modules discover real cleanup-hazard Object templates.
use crate::skirmish_data::{blocks, definition, field, scalar};
use generals_assets::content::{ContentFile, ContentLine};
use std::{collections::BTreeSet, error::Error};

pub(in crate::skirmish_data) fn field_templates<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut objects = Vec::new();
    let mut found = BTreeSet::new();
    for name in selected {
        let object = definition(files, "Object", name)?;
        let mut lists = Vec::new();
        let mut weapons = Vec::new();
        collect_references(object, &mut lists, &mut weapons)?;
        for (name, nullable) in weapons {
            // SlowDeath parseWeapon retains a null pointer for any absent name.
            // Existing declarations still require ordinary unambiguous resolution.
            if nullable
                && !files.iter().any(|file| {
                    file.declarations
                        .iter()
                        .any(|declaration| declaration.kind == "Weapon" && declaration.name == name)
                })
            {
                continue;
            }
            let weapon = definition(files, "Weapon", name)?;
            for key in ["FireOCL", "ProjectileDetonationOCL"] {
                if let Some(line) = field(&weapon.lines, 0, key) {
                    lists.push(scalar(line)?);
                }
            }
        }
        for name in lists {
            if name.eq_ignore_ascii_case("NONE") {
                continue;
            }
            let list = definition(files, "ObjectCreationList", name)?;
            for create in blocks(&list.lines, 0, "CreateObject")? {
                let Some(line) = field(create, 1, "ObjectNames") else {
                    continue;
                };
                for name in &line.tokens[1..] {
                    let object = definition(files, "Object", name)?;
                    if field(&object.lines, 0, "KindOf").is_some_and(|line| {
                        line.tokens[1..].iter().any(|flag| flag == "CLEANUP_HAZARD")
                    }) && !selected.contains(&name.as_str())
                        && found.insert(name.as_str())
                    {
                        objects.push(object.name.as_str());
                    }
                }
            }
        }
    }
    Ok(objects)
}

fn collect_references<'a>(
    object: &'a generals_assets::content::Declaration,
    lists: &mut Vec<&'a str>,
    weapons: &mut Vec<(&'a str, bool)>,
) -> Result<(), Box<dyn Error>> {
    for set in blocks(&object.lines, 0, "WeaponSet")? {
        for line in set.iter().filter(|line| {
            line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "Weapon")
        }) {
            if let Some(name) = line.tokens.get(2)
                && !name.eq_ignore_ascii_case("NONE")
            {
                weapons.push((name.as_str(), false));
            }
        }
    }
    for module in blocks(&object.lines, 0, "Behavior")? {
        let phased = module[0].tokens.get(1).is_some_and(|family| {
            matches!(
                family.as_str(),
                "SlowDeathBehavior"
                    | "JetSlowDeathBehavior"
                    | "HelicopterSlowDeathBehavior"
                    | "BattleBusSlowDeathBehavior"
                    | "NeutronMissileSlowDeathBehavior"
            )
        });
        if phased {
            weapons.extend(
                slow_death_weapons(module)?
                    .into_iter()
                    .map(|name| (name, true)),
            );
        }
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|family| family == "DemoTrapUpdate")
            && let Some(line) = field(module, 1, "DetonationWeapon")
        {
            let name = scalar(line)?;
            weapons.push((name, true));
        }
        for key in [
            "Weapon",
            "DeathWeapon",
            "CollideWeapon",
            "ReactionWeaponPristine",
            "ReactionWeaponDamaged",
            "ReactionWeaponReallyDamaged",
            "ReactionWeaponRubble",
            "ContinuousWeaponPristine",
            "ContinuousWeaponDamaged",
            "ContinuousWeaponReallyDamaged",
            "ContinuousWeaponRubble",
        ] {
            if phased && key == "Weapon" {
                continue;
            }
            if let Some(line) = field(module, 1, key) {
                let name = scalar(line)?;
                if !name.eq_ignore_ascii_case("NONE") {
                    weapons.push((name, false));
                }
            }
        }
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|family| family == "FireOCLAfterWeaponCooldownUpdate")
            && let Some(line) = field(module, 1, "OCL")
        {
            lists.push(scalar(line)?);
        }
    }
    Ok(())
}

fn slow_death_weapons(module: &[ContentLine]) -> Result<Vec<&str>, Box<dyn Error>> {
    // SlowDeathBehavior::parseWeapon consumes the phase, then every remaining
    // weapon name. All five families share its field parser.
    let mut names = Vec::new();
    for line in module.iter().filter(|line| {
        line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "Weapon")
    }) {
        if line.tokens.len() < 3
            || !["INITIAL", "MIDPOINT", "FINAL"]
                .iter()
                .any(|phase| line.tokens[1].eq_ignore_ascii_case(phase))
        {
            return Err("invalid SlowDeath weapon phase or missing weapon".into());
        }
        names.extend(
            line.tokens[2..]
                .iter()
                .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                .map(String::as_str),
        );
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_shell_death_weapon_discovers_field_without_visiting_unselected_ocl() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "field-dependencies.ini".into(), origin: "SCUD and source poison field dependency".into(),
            bytes: b"Object SCUDShell\nBehavior=FireWeaponWhenDeadBehavior Death\nDeathWeapon=SCUDPoisonPayload\nEnd\nEnd\nWeapon SCUDPoisonPayload\nFireOCL=OCL_PoisonFieldMedium\nEnd\nObjectCreationList OCL_PoisonFieldMedium\nCreateObject\nObjectNames=PoisonFieldMedium\nDisposition=ON_GROUND_ALIGNED\nEnd\nEnd\nObject PoisonFieldMedium\nKindOf=IMMOBILE CLEANUP_HAZARD INERT NO_COLLIDE\nBody=ActiveBody Body\nMaxHealth=100\nInitialHealth=100\nEnd\nEnd\nObjectCreationList UnselectedMissionOnly\nCreateObject\nObjectNames=MissingCampaignObject\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            field_templates(&[file], &["SCUDShell"]).unwrap(),
            ["PoisonFieldMedium"]
        );
    }

    #[test]
    fn slow_death_weapons_consume_phase_and_all_names_before_discovering_hazard_fields() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "field-dependencies.ini".into(),
            origin: "FactionBuilding.ini SlowDeath phase grammar".into(),
            bytes: b"Object DemoTrap\nBehavior=SlowDeathBehavior Death\nWeapon=INITIAL None Ordinary Payload\nWeapon=FINAL DemoTrapDetonationWeapon\nEnd\nEnd\nWeapon Ordinary\nEnd\nWeapon Payload\nFireOCL=OCL_PoisonFieldMedium\nEnd\nWeapon DemoTrapDetonationWeapon\nFireOCL=OCL_PoisonFieldSmall\nEnd\nObjectCreationList OCL_PoisonFieldMedium\nCreateObject\nObjectNames=PoisonFieldMedium\nEnd\nEnd\nObjectCreationList OCL_PoisonFieldSmall\nCreateObject\nObjectNames=PoisonFieldSmall\nEnd\nEnd\nObject PoisonFieldMedium\nKindOf=CLEANUP_HAZARD\nEnd\nObject PoisonFieldSmall\nKindOf=CLEANUP_HAZARD\nEnd\n".to_vec(),
        })
        .unwrap();
        assert_eq!(
            field_templates(&[file], &["DemoTrap"]).unwrap(),
            ["PoisonFieldMedium", "PoisonFieldSmall"]
        );
    }

    #[test]
    fn missing_phase_weapon_is_nullable_but_ordinary_weapon_remains_required() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "nullable-phase.ini".into(),
            origin: "SlowDeath parseWeapon nullable lookup".into(),
            bytes: b"Object NullableDeath\nBehavior=SlowDeathBehavior Death\nWeapon=INITIAL MissingPhaseWeapon Payload\nWeapon=FINAL AnotherMissingPhaseWeapon\nEnd\nEnd\nObject RequiredWeapon\nBehavior=FireWeaponUpdate Fire\nWeapon=MissingPhaseWeapon\nEnd\nEnd\nWeapon Payload\nFireOCL=PoisonList\nEnd\nObjectCreationList PoisonList\nCreateObject\nObjectNames=PoisonField\nEnd\nEnd\nObject PoisonField\nKindOf=CLEANUP_HAZARD\nEnd\n".to_vec(),
        })
        .unwrap();
        let files = [file];
        assert_eq!(
            field_templates(&files, &["NullableDeath"]).unwrap(),
            ["PoisonField"]
        );
        let error = field_templates(&files, &["RequiredWeapon"]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("missing Weapon MissingPhaseWeapon")
        );
    }
}
