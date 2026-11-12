//! Installed `UndeadBody`, `SECOND_LIFE` armor and `BattleBusSlowDeath` fields.
use super::{armor_coefficients, blocks, definition, field, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::battle_bus::Rules;
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let Some(body) = blocks(&object.lines, 0, "Body")?.into_iter().find(|body| {
        body[0]
            .tokens
            .get(1)
            .is_some_and(|kind| kind == "UndeadBody")
    }) else {
        return Ok(None);
    };
    let real = |lines: &[ContentLine], key, default| -> Result<f32, Box<dyn Error>> {
        field(lines, 1, key).map_or(Ok(default), |line| super::finite_number(scalar(line)?))
    };
    let armor_sets = blocks(&object.lines, 0, "ArmorSet")?;
    let armor = |level, second| -> Result<_, Box<dyn Error>> {
        let upgrade = match level {
            0 => None,
            1 => Some("CRATE_UPGRADE_ONE"),
            _ => Some("CRATE_UPGRADE_TWO"),
        };
        let count = usize::from(second) + usize::from(upgrade.is_some());
        let set = armor_sets
            .iter()
            .find(|set| {
                field(set, 1, "Conditions").is_some_and(|line| {
                    line.tokens.len() == count + 1
                        && line.tokens[1..].iter().any(|flag| flag == "SECOND_LIFE") == second
                        && upgrade.is_none_or(|upgrade| {
                            line.tokens[1..].iter().any(|flag| flag == upgrade)
                        })
                })
            })
            .ok_or("Battle Bus lacks original crate/second-life armor set")?;
        let name = scalar(field(set, 1, "Armor").ok_or("Battle Bus set lacks armor")?)?;
        armor_coefficients(&definition(files, "Armor", name)?.lines)
    };
    let first_armor = [armor(1, false)?, armor(2, false)?];
    let second_armor = [armor(0, true)?, armor(1, true)?, armor(2, true)?];
    Ok(Some(Rules {
        second_health: real(body, "SecondLifeMaxHealth", 1.0)?,
        first_armor,
        second_armor,
    }))
}

pub(super) fn death(
    files: &[ContentFile],
    module: &[ContentLine],
) -> Result<generals_sim::skirmish::battle_bus::DeathRules, Box<dyn Error>> {
    let names = |key| -> Result<String, Box<dyn Error>> {
        Ok(field(module, 1, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into())
    };
    let list = |key| -> Result<_, Box<dyn Error>> {
        let name = names(key)?;
        if name.is_empty() || name.eq_ignore_ascii_case("NONE") {
            Ok(None)
        } else {
            Ok(Some(super::physical_ocl::load(files, &name)?))
        }
    };
    let real = |key, default| -> Result<f32, Box<dyn Error>> {
        field(module, 1, key).map_or(Ok(default), |line| super::finite_number(scalar(line)?))
    };
    let percent = field(module, 1, "PercentDamageToPassengers")
        .map(scalar)
        .transpose()?
        .unwrap_or("0%");
    Ok(generals_sim::skirmish::battle_bus::DeathRules {
        throw_force: real("ThrowForce", 1.0)?,
        passenger_damage: super::finite_number(percent.trim_end_matches('%'))? * 0.01,
        empty_delay: field(module, 1, "EmptyHulkDestructionDelay").map_or(Ok(0), |line| {
            super::source_numbers::duration_frames(scalar(line)?)
        })?,
        start_fx: names("FXStartUndeath")?,
        ground_fx: names("FXHitGround")?,
        start_ocl: list("OCLStartUndeath")?,
        ground_ocl: list("OCLHitGround")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};
    use std::fmt::Write;

    #[test]
    fn original_bus_loads_six_armor_sets_and_separate_second_health() {
        let mut ini = String::from(
            "Object GLAVehicleBattleBus\nBody = UndeadBody ModuleTag_02\nMaxHealth = 400\nInitialHealth = 400\nSecondLifeMaxHealth = 650\nEnd\nBehavior = BattleBusSlowDeathBehavior ModuleTag_08\nDeathTypes = ALL -CRUSHED -SPLATTED -EXTRA_4\nProbabilityModifier = 5\nThrowForce = 100\nPercentDamageToPassengers = 50%\nEmptyHulkDestructionDelay = 1000\nEnd\n",
        );
        for (conditions, name) in [
            ("None", "Truck"),
            ("SECOND_LIFE", "Hulk"),
            ("CRATE_UPGRADE_ONE", "Truck1"),
            ("CRATE_UPGRADE_ONE SECOND_LIFE", "Hulk1"),
            ("CRATE_UPGRADE_TWO", "Truck2"),
            ("CRATE_UPGRADE_TWO SECOND_LIFE", "Hulk2"),
        ] {
            writeln!(
                ini,
                "ArmorSet\nConditions = {conditions}\nArmor = {name}\nEnd"
            )
            .unwrap();
        }
        ini.push_str("End\n");
        for (name, default, radiation) in [
            ("Truck", 100, 50),
            ("Hulk", 100, 1),
            ("Truck1", 90, 45),
            ("Hulk1", 90, 1),
            ("Truck2", 80, 40),
            ("Hulk2", 80, 1),
        ] {
            writeln!(
                ini,
                "Armor {name}\nArmor = DEFAULT {default}%\nArmor = RADIATION {radiation}%\nEnd"
            )
            .unwrap();
        }
        let files = [content::scan(Asset {
            path: "battle-bus.ini".into(),
            origin: "original Battle Bus fields".into(),
            bytes: ini.into_bytes(),
        })
        .unwrap()];
        let rules = load(
            &files,
            definition(&files, "Object", "GLAVehicleBattleBus").unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(rules.second_health.to_bits(), 650.0_f32.to_bits());
        let object = definition(&files, "Object", "GLAVehicleBattleBus").unwrap();
        let modules = blocks(&object.lines, 0, "Behavior").unwrap();
        let derived = death(&files, modules[0]).unwrap();
        assert_eq!(derived.throw_force.to_bits(), 100.0_f32.to_bits());
        assert_eq!(derived.passenger_damage.to_bits(), 0.5_f32.to_bits());
        assert_eq!(derived.empty_delay, 30);
        assert_eq!(
            rules.first_armor[0][generals_sim::skirmish::DamageType::Radiation as usize].to_bits(),
            0.45_f32.to_bits()
        );
        assert_eq!(
            rules.second_armor[0][generals_sim::skirmish::DamageType::Radiation as usize].to_bits(),
            0.01_f32.to_bits()
        );
        assert_eq!(
            super::super::death_weapons::mask(field(modules[0], 1, "DeathTypes")).unwrap()
                & (1 << generals_sim::skirmish::DeathType::Extra4 as u32),
            0
        );
    }
}
