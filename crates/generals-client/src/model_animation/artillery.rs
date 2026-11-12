//! Installed artillery draw states, launch bones and researched barrel banks.
use super::Animations;
use generals_assets::w3d::Model;
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn verify_artillery_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        for (name, normal, damaged) in [
            ("AmericaVehicleTomahawk", "avtomahawk", "avtomahawk_d"),
            ("GLAVehicleRocketBuggy", "uvrockbug", "uvrockbug_d"),
            ("GLAVehicleScudLauncher", "uvscudlchr", "uvscudlchr_d"),
        ] {
            let index = types
                .iter()
                .position(|unit| unit.name == name)
                .ok_or("missing artillery unit")?;
            let unit = &types[index];
            for condition in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
                let flags: BTreeSet<_> = condition.into_iter().map(str::to_owned).collect();
                let draw = self.draws[index].first().ok_or("missing artillery draw")?;
                let state = &draw.states[draw.select(&flags)];
                if state.name
                    != if condition == Some("REALLYDAMAGED") {
                        damaged
                    } else {
                        normal
                    }
                {
                    return Err(format!("{name}: wrong original artillery damage model").into());
                }
                if state.model.and_then(|index| models.get(index)).is_none() {
                    return Err(format!("{name}: artillery geometry absent").into());
                }
                for slot in 0..unit.weapon_template_count() {
                    let Some(weapon) = unit.weapon_template(slot) else {
                        continue;
                    };
                    let model = self
                        .firing_model(models, index, condition, slot)
                        .ok_or("missing artillery firing model")?;
                    let motion = weapon
                        .motion
                        .as_ref()
                        .ok_or("artillery lacks projectile launch binding")?;
                    let damage_index = match condition {
                        Some("REALLYDAMAGED") => 2,
                        Some("DAMAGED") => 1,
                        _ => 0,
                    };
                    if motion.launches[damage_index].is_empty() || model.meshes.is_empty() {
                        return Err(format!(
                            "{name}: missing original launch bank for {}",
                            weapon.name
                        )
                        .into());
                    }
                }
            }
        }
        println!(
            "Original artillery art: healthy/damaged Tomahawk, Rocket Buggy and SCUD models plus every researched/salvaged launch bank passed"
        );
        Ok(())
    }
}
