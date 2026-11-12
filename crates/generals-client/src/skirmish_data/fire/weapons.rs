use super::OBJECTS;
use crate::skirmish_data::{
    blocks, definition, duration_frames, field, number, scalar, text, unsigned,
};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{
    UnitType,
    fire::{Creation, Disposition, Historic, Weapon, history},
};
use std::error::Error;

pub(in crate::skirmish_data) fn load(
    files: &[ContentFile],
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    for index in 0..unit.weapon_template_count() {
        let Some(weapon) = unit.weapon_template_mut(index) else {
            continue;
        };
        if weapon.name.is_empty() {
            continue;
        }
        let definition = definition(files, "Weapon", &weapon.name)?;
        weapon.incendiary = parse(files, definition)?;
    }
    Ok(())
}

fn parse(files: &[ContentFile], weapon: &Declaration) -> Result<Option<Weapon>, Box<dyn Error>> {
    let impact = field(&weapon.lines, 0, "ProjectileDetonationOCL")
        .or_else(|| field(&weapon.lines, 0, "FireOCL"))
        .map(scalar)
        .transpose()?
        .filter(|name| {
            name.starts_with("OCL_FireWall")
                || matches!(
                    *name,
                    "OCL_FireFieldSmall"
                        | "OCL_FireFieldUpgradedSmall"
                        | "OCL_FirestormSmall"
                        | "OCL_BlackNapalmFirestormSmall"
                )
        })
        .map(|name| creation(files, name))
        .transpose()?;
    let death = field(&weapon.lines, 0, "ProjectileObject")
        .map(scalar)
        .transpose()?
        .filter(|name| matches!(*name, "InfernoTankShell" | "InfernoTankShellUpgraded"))
        .map(|name| shell_death(files, name))
        .transpose()?;
    let historic = field(&weapon.lines, 0, "HistoricBonusWeapon")
        .map(scalar)
        .transpose()?
        .filter(|name| {
            matches!(
                *name,
                "FirestormSmallCreationWeapon" | "BlackNapalmFirestormSmallCreationWeapon"
            )
        })
        .map(|name| -> Result<_, Box<dyn Error>> {
            Ok(Historic {
                rules: history::Rules {
                    frames: duration_frames(
                        field(&weapon.lines, 0, "HistoricBonusTime")
                            .ok_or("missing historic time")?,
                    )?,
                    count: usize::try_from(unsigned(&weapon.lines, 0, "HistoricBonusCount")?)?,
                    radius: number(&weapon.lines, 0, "HistoricBonusRadius")?,
                    weapon: name.into(),
                },
                creation: creation_weapon(files, name)?,
            })
        })
        .transpose()?;
    Ok(
        (impact.is_some() || death.is_some() || historic.is_some()).then_some(Weapon {
            impact,
            death,
            historic,
        }),
    )
}

fn shell_death(files: &[ContentFile], name: &str) -> Result<Creation, Box<dyn Error>> {
    let shell = definition(files, "Object", name)?;
    let modules = blocks(&shell.lines, 0, "Behavior")?;
    let module = modules
        .iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "FireWeaponWhenDeadBehavior")
        })
        .ok_or("Inferno shell lacks fire on death")?;
    creation_weapon(files, text(module, 1, "DeathWeapon")?)
}

fn creation_weapon(files: &[ContentFile], name: &str) -> Result<Creation, Box<dyn Error>> {
    let weapon = definition(files, "Weapon", name)?;
    // WeaponTemplate's constructor initializes primary damage to zero; the
    // original firestorm creation weapons deliberately omit the field.
    let damage = field(&weapon.lines, 0, "PrimaryDamage")
        .map_or(Ok(0.0), |_| number(&weapon.lines, 0, "PrimaryDamage"))?;
    if damage != 0.0 || field(&weapon.lines, 0, "ProjectileObject").is_some() {
        return Err("fire creation weapon has additional damage or projectile".into());
    }
    creation(files, text(&weapon.lines, 0, "FireOCL")?)
}

fn creation(files: &[ContentFile], name: &str) -> Result<Creation, Box<dyn Error>> {
    let object = definition(files, "ObjectCreationList", name)?;
    let creates = blocks(&object.lines, 0, "CreateObject")?;
    if creates.len() != 1 {
        return Err("fire OCL requires one original object nugget".into());
    }
    let module = creates[0];
    let names = crate::skirmish_data::salvage::words(module, 1, "ObjectNames");
    if names.is_empty() || names.iter().any(|name| !OBJECTS.contains(&name.as_str())) {
        return Err("untranslated fire OCL template".into());
    }
    let flags = crate::skirmish_data::salvage::words(module, 1, "Disposition");
    let disposition = match flags
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["ON_GROUND_ALIGNED"] => Disposition::GroundAligned,
        ["LIKE_EXISTING", "INHERIT_VELOCITY"] => Disposition::ExistingWithVelocity,
        _ => return Err("untranslated fire OCL disposition".into()),
    };
    let count = field(module, 1, "Count").map_or(Ok(1), |_| unsigned(module, 1, "Count"))?;
    Ok(Creation {
        names,
        count,
        disposition,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_creation_weapon_omits_damage_but_keeps_its_real_ocl() {
        let source = "Weapon FirestormSmallCreationWeapon\nDamageType=FLAME\nDeathType=BURNED\nFireOCL=OCL_FirestormSmall\nEnd\nObjectCreationList OCL_FirestormSmall\nCreateObject\nObjectNames=FirestormSmall\nDisposition=ON_GROUND_ALIGNED\nEnd\nEnd\n";
        let files = [
            generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "fire.ini".into(),
                origin: "installed firestorm creation fields".into(),
                bytes: source.as_bytes().to_vec(),
            })
            .unwrap(),
        ];
        let creation = creation_weapon(&files, "FirestormSmallCreationWeapon").unwrap();
        assert_eq!(creation.names, ["FirestormSmall"]);
        assert_eq!(creation.count, 1);
        assert!(matches!(creation.disposition, Disposition::GroundAligned));
    }
}
