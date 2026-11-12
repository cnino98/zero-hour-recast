//! Projectile and laser object dependencies from the selected object weapons.
use super::{definition, field, scalar};
use generals_assets::content::ContentFile;
use std::error::Error;

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut result = Vec::new();
    for name in selected {
        let object = definition(files, "Object", name)?;
        for line in &object.lines {
            let Some(key) = line.tokens.first() else {
                continue;
            };
            // WeaponSet slots and concrete module weapon references retain file order.
            let weapon = match key.as_str() {
                "Weapon" if line.tokens.len() == 3 => Some(line.tokens[2].as_str()),
                "Weapon"
                | "HowitzerWeaponTemplate"
                | "DetonationWeapon"
                | "DeathWeapon"
                | "WeaponTemplate"
                | "ReactionWeaponPristine"
                | "ReactionWeaponDamaged"
                | "ReactionWeaponReallyDamaged"
                | "ReactionWeaponRubble"
                | "ContinuousWeaponPristine"
                | "ContinuousWeaponDamaged"
                | "ContinuousWeaponReallyDamaged"
                | "ContinuousWeaponRubble"
                    if line.tokens.len() == 2 =>
                {
                    Some(line.tokens[1].as_str())
                }
                _ => None,
            };
            let Some(weapon) = weapon.filter(|name| !name.eq_ignore_ascii_case("NONE")) else {
                continue;
            };
            let weapon = if matches!(key.as_str(), "DetonationWeapon" | "HowitzerWeaponTemplate") {
                let Some(weapon) = super::instant_death::nullable(files, "Weapon", weapon)? else {
                    continue;
                };
                weapon
            } else {
                definition(files, "Weapon", weapon)?
            };
            for key in ["ProjectileObject", "LaserName"] {
                if let Some(line) = field(&weapon.lines, 0, key) {
                    let name = scalar(line)?;
                    if name.eq_ignore_ascii_case("NONE")
                        || selected.contains(&name)
                        || result.contains(&name)
                    {
                        continue;
                    }
                    let object = if key == "LaserName" {
                        // Weapon::createLaser returns when the named template is absent.
                        let Some(object) = super::instant_death::nullable(files, "Object", name)?
                        else {
                            continue;
                        };
                        object
                    } else {
                        definition(files, "Object", name)?
                    };
                    result.push(object.name.as_str());
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn selected_weapon_dependencies_keep_encounter_order_and_ignore_unselected_weapons() {
        let files = [generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Object.ini".into(), origin: "projectile dependency encounter order".into(),
            bytes: b"Object Shooter\nWeaponSet\nConditions=None\nWeapon=PRIMARY Rifle\nWeapon=SECONDARY Rocket\nEnd\nBehavior=FireWeaponWhenDeadBehavior Death\nDeathWeapon=Bomb\nEnd\nEnd\nObject Missile\nEnd\nObject Shell\nEnd\nObject Beam\nEnd\nWeapon Rifle\nLaserName=Beam\nProjectileObject=None\nEnd\nWeapon Rocket\nProjectileObject=Missile\nEnd\nWeapon Bomb\nProjectileObject=Shell\nLaserName=MissingOptionalBeam\nEnd\nWeapon Unused\nProjectileObject=Unknown\nEnd\n".to_vec(),
        }).unwrap()];
        assert_eq!(
            super::dependencies(&files, &["Shooter"]).unwrap(),
            ["Beam", "Missile", "Shell"]
        );
        assert_eq!(
            super::dependencies(&files, &["Shooter", "Missile"]).unwrap(),
            ["Beam", "Shell"]
        );
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_tank_shell_has_its_own_effective_modules_before_catalog_loading() {
        use generals_sim::skirmish::{Faction, Kind};
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let mut roster = vec![(
            "AmericaTankCrusader".to_owned(),
            Faction::Usa,
            Kind::Vehicle,
        )];
        let (files, metadata) =
            crate::skirmish_data::source_files::with_runtime_dependencies(&assets, &mut roster)
                .unwrap();
        let names: Vec<_> = roster.iter().map(|(name, _, _)| name.as_str()).collect();
        assert_eq!(
            super::dependencies(&files, &["AmericaTankCrusader"]).unwrap(),
            ["GenericTankShell"]
        );
        // The full roster also follows the destruction, pilot and drone upgrade
        // lists from the installed Crusader. Its shell keeps a distinct entry.
        assert_eq!(&names[..2], ["AmericaTankCrusader", "GenericTankShell"]);
        for name in [
            "DeadCrusaderHulk",
            "AmericaInfantryPilot",
            "AmericaParachute",
            "AmericaVehicleBattleDrone",
            "AmericaVehicleScoutDrone",
            "AmericaVehicleHellfireDrone",
        ] {
            assert!(names.contains(&name) && metadata.contains_key(name));
        }
        let shell = crate::skirmish_data::definition(&files, "Object", "GenericTankShell").unwrap();
        let modules = crate::skirmish_data::blocks(&shell.lines, 0, "Behavior").unwrap();
        assert!(
            modules
                .iter()
                .any(|module| module[0].tokens[1] == "DumbProjectileBehavior")
        );
        assert!(metadata.contains_key("GenericTankShell"));
        assert!(super::dependencies(&files, &names).unwrap().is_empty());
    }
}
