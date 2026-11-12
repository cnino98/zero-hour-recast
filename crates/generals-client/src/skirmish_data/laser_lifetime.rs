//! Original laser object constructors sample their lifetime in game logic.
use super::{blocks, definition, duration_frames, field, scalar};
use generals_assets::content::ContentFile;
use generals_sim::skirmish::UnitType;
use std::error::Error;

pub(super) fn bind(files: &[ContentFile], unit: &mut UnitType) -> Result<(), Box<dyn Error>> {
    for index in 0..unit.weapon_template_count() {
        let Some(weapon) = unit.weapon_template_mut(index) else {
            continue;
        };
        if !weapon.laser {
            continue;
        }
        let lines = &definition(files, "Weapon", &weapon.name)?.lines;
        let name = scalar(field(lines, 0, "LaserName").ok_or("laser object missing")?)?;
        let object = definition(files, "Object", name)?;
        let modules: Vec<_> = blocks(&object.lines, 0, "Behavior")?
            .into_iter()
            .filter(|module| {
                module[0].tokens.get(1).is_some_and(|name| {
                    matches!(name.as_str(), "LifetimeUpdate" | "DeletionUpdate")
                })
            })
            .collect();
        if modules.len() != 1 {
            return Err(format!("{name}: laser requires one original lifetime module").into());
        }
        let module = modules[0];
        let minimum = field(module, 1, "MinLifetime").map_or(Ok(0), duration_frames)?;
        let maximum = field(module, 1, "MaxLifetime").map_or(Ok(0), duration_frames)?;
        if minimum > maximum {
            return Err(format!("{name}: reversed laser lifetime range").into());
        }
        weapon.laser_lifetime = Some([minimum, maximum]);
    }
    Ok(())
}
