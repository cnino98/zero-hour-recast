//! Original Tractor damage/salvage turrets, sprayer bones and toxin puddles.
use super::Animations;
use generals_assets::w3d::Model;
use generals_sim::skirmish::UnitType;
use std::error::Error;

impl Animations {
    pub fn verify_tractor_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let index = types
            .iter()
            .position(|unit| unit.name == "GLAVehicleToxinTruck")
            .ok_or("missing original Toxin Tractor")?;
        let unit = &types[index];
        let mut weapons = 0;
        for weapon_index in 0..unit.weapon_template_count() {
            let Some(weapon) = unit.weapon_template(weapon_index) else {
                continue;
            };
            if !weapon.name.starts_with("ToxinTruck") {
                continue;
            }
            weapons += 1;
            let (slot, flags) = unit.weapon_context(weapon_index);
            let turret = if flags & 4 != 0 {
                "TurretUP02"
            } else if flags & 2 != 0 {
                "TurretUP01"
            } else {
                "Turret"
            };
            for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
                let state = self
                    .firing_state(index, damage, weapon_index)
                    .ok_or("Tractor firing state absent")?;
                let expected = if damage == Some("REALLYDAMAGED") {
                    "uvtoxintrk_d"
                } else {
                    "uvtoxintrk"
                };
                let model = state.model.and_then(|index| models.get(index));
                if state.name != expected || model.is_none_or(|model| model.meshes.is_empty()) {
                    return Err(format!("{}: original damage model absent", weapon.name).into());
                }
                if !state.fields.iter().any(|line| {
                    line.first().is_some_and(|key| key == "Turret")
                        && line
                            .get(1)
                            .is_some_and(|name| name.eq_ignore_ascii_case(turret))
                }) {
                    return Err(format!("{}: wrong original salvage turret", weapon.name).into());
                }
                if slot == 1 {
                    let particle = if flags & 1 != 0 {
                        "AnthraxPuddleContinuous"
                    } else {
                        "ToxinPuddleContinuous"
                    };
                    if !state.fields.iter().any(|line| {
                        line.first().is_some_and(|key| key == "ParticleSysBone")
                            && line.get(2).is_some_and(|name| name == particle)
                    }) {
                        return Err(format!(
                            "{}: original contamination puddle absent",
                            weapon.name
                        )
                        .into());
                    }
                }
                let bank = match damage {
                    Some("REALLYDAMAGED") => 2,
                    Some("DAMAGED") => 1,
                    _ => 0,
                };
                if weapon
                    .motion
                    .as_ref()
                    .is_none_or(|motion| motion.launches[bank].is_empty())
                {
                    return Err(format!(
                        "{}: original stream/sprayer launch bone absent",
                        weapon.name
                    )
                    .into());
                }
            }
        }
        if weapons != 12 {
            return Err(format!(
                "Tractor imported {weapons} weapons; expected six original two-slot sets"
            )
            .into());
        }
        println!(
            "Original Tractor art: all six healthy/damaged salvage and Anthrax sets, stream/sprayer launch banks and contamination puddles passed"
        );
        Ok(())
    }
}
