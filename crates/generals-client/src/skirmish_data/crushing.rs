//! Source module admission/order and sequential CRUSH armor coefficients.
use super::{blocks, field, scalar};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::CrushRules;
use std::error::Error;

pub(super) fn rules(lines: &[ContentLine]) -> Result<CrushRules, Box<dyn Error>> {
    let modules = blocks(lines, 0, "Behavior")?;
    let module = |name| {
        modules
            .iter()
            .position(|lines| lines[0].tokens.get(1).is_some_and(|value| value == name))
    };
    let squish = module("SquishCollide");
    let physics = module("PhysicsBehavior");
    Ok(CrushRules {
        // ThingTemplate initializes levels to 0 and 255.
        crusher_level: field(lines, 0, "CrusherLevel")
            .map(scalar)
            .transpose()?
            .unwrap_or("0")
            .parse()?,
        crushable_level: field(lines, 0, "CrushableLevel")
            .map(scalar)
            .transpose()?
            .unwrap_or("255")
            .parse()?,
        squish_module: squish,
        physics_module: physics,
        hijacker: module("HijackerUpdate").is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn collision_module_order_and_level_defaults_follow_the_object_definition() {
        let file = content::scan(Asset {
            path: "object.ini".into(),
            origin: "crushing parser fixture".into(),
            bytes: b"Object Soldier\nBehavior = SquishCollide Squish\nEnd\nBehavior = PhysicsBehavior Physics\nEnd\nBehavior = HijackerUpdate Hijacker\nEnd\nEnd\nObject Tank\nCrusherLevel=2\nCrushableLevel=1\nBehavior = PhysicsBehavior Physics\nEnd\nBehavior = SquishCollide Squish\nEnd\nEnd\n".to_vec(),
        }).expect("source object declarations");
        let soldier = rules(&file.declarations[0].lines).expect("source modules");
        assert!(soldier.hijacker);
        assert_eq!(soldier.squish_module, Some(0));
        assert_eq!(soldier.physics_module, Some(1));
        assert_eq!((soldier.crusher_level, soldier.crushable_level), (0, 255));
        let tank = rules(&file.declarations[1].lines).expect("source tank modules");
        assert!(!tank.hijacker);
        assert_eq!(tank.squish_module, Some(1));
        assert_eq!(tank.physics_module, Some(0));
        assert_eq!((tank.crusher_level, tank.crushable_level), (2, 1));
    }

    #[test]
    fn default_armor_resets_prior_crush_resistance_and_later_entries_override_it() {
        for (declaration, expected) in [
            ("Armor=CRUSH 0%\nArmor=DEFAULT 75%", 0.75_f32),
            ("Armor=DEFAULT 75%\nArmor=CRUSH 0%", 0.0),
            ("Armor=SMALL_ARMS 10%", 1.0),
        ] {
            let file = content::scan(Asset {
                path: "armor.ini".into(),
                origin: "crush armor fixture".into(),
                bytes: format!("Armor Soldier\n{declaration}\nEnd\n").into_bytes(),
            })
            .expect("source armor");
            assert_eq!(
                super::super::armor_coefficients(&file.declarations[0].lines).expect("coefficient")
                    [generals_sim::skirmish::DamageType::Crush as usize]
                    .to_bits(),
                expected.to_bits()
            );
        }
    }
}
