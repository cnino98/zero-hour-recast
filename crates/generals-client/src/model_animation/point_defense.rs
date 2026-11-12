//! Installed Paladin, Avenger and independent turret source-art acceptance.
use super::Animations;
use generals_assets::w3d::Model;
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn verify_point_defense_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        for (name, healthy, damaged, bones) in [
            (
                "AmericaTankPaladin",
                "avpaladin",
                "avpaladin_d",
                &["LASER", "TurretMS"][..],
            ),
            (
                "AmericaTankAvenger",
                "avavnger",
                "avavnger_d",
                &["LazerSpot01", "LazerSpot02", "TurretFX03", "FIREPOINT01"][..],
            ),
            (
                "AmericaTankAvengerLaserTurret",
                "avavnger_g",
                "avavnger_gd",
                &["TurretFX01", "TurretFX02", "TURRET01", "TURRETEL01"][..],
            ),
        ] {
            let index = types
                .iter()
                .position(|unit| unit.name == name)
                .ok_or_else(|| format!("missing {name}"))?;
            for condition in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
                let flags: BTreeSet<_> = condition.into_iter().map(str::to_owned).collect();
                let draw = self.draws[index]
                    .first()
                    .ok_or("missing defense unit draw")?;
                let state = &draw.states[draw.select(&flags)];
                if state.name
                    != if condition == Some("REALLYDAMAGED") {
                        damaged
                    } else {
                        healthy
                    }
                {
                    return Err(format!("{name}: wrong original {condition:?} model").into());
                }
                let model = state
                    .model
                    .and_then(|index| models.get(index))
                    .ok_or("missing original defense model")?;
                for bone in bones {
                    if model.public_bone_indices(bone).is_empty() {
                        return Err(format!(
                            "{name}: missing original laser/attachment bone {bone}"
                        )
                        .into());
                    }
                }
                if name == "AmericaTankAvenger" {
                    let mut turret_seen = false;
                    for mesh in &model.meshes {
                        if mesh
                            .name
                            .rsplit('.')
                            .next()
                            .is_some_and(|name| name.eq_ignore_ascii_case("TURRET01"))
                        {
                            turret_seen = true;
                            if mesh.attributes & 0x1000 == 0 {
                                return Err(
                                    "Avenger chassis leaks duplicate controlled turret".into()
                                );
                            }
                        }
                    }
                    if !turret_seen {
                        return Err("Avenger chassis lacks authored turret subobject".into());
                    }
                }
                if !model
                    .meshes
                    .iter()
                    .any(|mesh| mesh.attributes & 0x1000 == 0)
                {
                    return Err(format!("{name}: original model is wholly hidden").into());
                }
            }
        }
        println!(
            "Original point-defense art: Paladin/Avenger healthy and damaged models, independent turret, hidden duplicate turret and authored laser/attachment bones passed"
        );
        Ok(())
    }
}
