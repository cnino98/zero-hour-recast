use super::{ContentFile, ContentLine, Error, boolean, definition, field, locomotor, scalar};
use generals_sim::skirmish::{
    DamageType,
    physical_ocl::{CollisionRules, CrashWeapon},
};

pub(in crate::skirmish_data) fn collision_rules(
    files: &[ContentFile],
    physics: &[ContentLine],
) -> Result<CollisionRules, Box<dyn Error>> {
    let global = &definition(files, "GameData", "")?.lines;
    let weapon = |key, default| -> Result<Option<CrashWeapon>, Box<dyn Error>> {
        let name = field(physics, 1, key)
            .map(scalar)
            .transpose()?
            .unwrap_or(default);
        if name.eq_ignore_ascii_case("NONE") {
            return Ok(None);
        }
        let lines = &definition(files, "Weapon", name)?.lines;
        let damage = field(lines, 0, "DamageType")
            .map(scalar)
            .transpose()?
            .unwrap_or("EXPLOSION");
        let damage_type =
            super::super::damage_type(damage).ok_or("unsupported crash weapon damage type")?;
        if !matches!(damage_type, DamageType::Explosion | DamageType::Crush) {
            return Err("untranslated special crash damage callbacks".into());
        }
        Ok(Some(CrashWeapon {
            damage: locomotor::signed_real(lines, 0, "PrimaryDamage", 0.0)?,
            damage_type,
            death_type: super::super::death_weapons::death_type(
                field(lines, 0, "DeathType")
                    .map(scalar)
                    .transpose()?
                    .unwrap_or("NORMAL"),
            )?,
            effects: effects(lines)?,
        }))
    };
    Ok(CollisionRules {
        stiffness: locomotor::signed_real(global, 0, "StructureStiffness", 0.5)?,
        rubble_height: locomotor::signed_real(global, 0, "DefaultStructureRubbleHeight", 1.0)?,
        allow_force: boolean(physics, "AllowCollideForce", true)?,
        weapons: [
            weapon(
                "VehicleCrashesIntoBuildingWeaponTemplate",
                "VehicleCrashesIntoBuildingWeapon",
            )?,
            weapon(
                "VehicleCrashesIntoNonBuildingWeaponTemplate",
                "VehicleCrashesIntoNonBuildingWeapon",
            )?,
        ],
    })
}

fn effects(lines: &[ContentLine]) -> Result<[String; 4], Box<dyn Error>> {
    let mut result = std::array::from_fn(|_| String::new());
    for line in lines.iter().filter(|line| line.depth == Some(0)) {
        match line.tokens.first().map(String::as_str) {
            Some("FireFX") => {
                let value = scalar(line)?;
                for effect in &mut result {
                    *effect = if value.eq_ignore_ascii_case("NONE") {
                        String::new()
                    } else {
                        value.into()
                    };
                }
            }
            Some("VeterancyFireFX") => {
                let [_, level, value] = line.tokens.as_slice() else {
                    return Err("invalid crash VeterancyFireFX".into());
                };
                let level = match level.as_str() {
                    "REGULAR" => 0,
                    "VETERAN" => 1,
                    "ELITE" => 2,
                    "HEROIC" => 3,
                    _ => return Err("invalid crash weapon veterancy level".into()),
                };
                result[level] = if value.eq_ignore_ascii_case("NONE") {
                    String::new()
                } else {
                    value.clone()
                };
            }
            _ => {}
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn crash_fire_fx_applies_native_declaration_order_and_per_level_overrides() {
        let file = content::scan(Asset {
            path: "crash.ini".into(),
            origin: "native crash FireFX parser fixture".into(),
            bytes: b"Weapon Crash\nVeterancyFireFX=HEROIC Replaced\nFireFX=All\nVeterancyFireFX=VETERAN Veteran\nVeterancyFireFX=HEROIC NONE\nEnd\n".to_vec(),
        }).unwrap();
        let files = [file];
        let object = &definition(&files, "Weapon", "Crash").unwrap().lines;
        assert_eq!(effects(object).unwrap(), ["All", "Veteran", "All", ""]);
    }
}
