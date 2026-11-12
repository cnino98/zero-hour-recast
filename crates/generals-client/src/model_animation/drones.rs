use super::Animations;
use generals_sim::skirmish::drones::Weld;
use std::collections::BTreeSet;

impl Animations {
    pub fn verify_drone_art(
        &self,
        models: &[generals_assets::w3d::Model],
        types: &[generals_sim::skirmish::UnitType],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (name, healthy, damaged, count) in [
            ("AmericaVehicleBattleDrone", "avbattledr", "avbattledr_d", 2),
            (
                "AmericaVehicleHellfireDrone",
                "avhelldrone",
                "avhelldrone_d",
                1,
            ),
        ] {
            let index = types
                .iter()
                .position(|unit| unit.name == name)
                .ok_or("missing installed drone")?;
            let draws = &self.draws[index];
            if draws.len() != count {
                return Err(format!("{name}: original draw modules missing").into());
            }
            for damage in [None, Some("REALLYDAMAGED")] {
                let flags = damage.into_iter().map(str::to_owned).collect();
                let state = &draws[0].states[draws[0].select(&flags)];
                if state.name != if damage.is_some() { damaged } else { healthy } {
                    return Err(format!("{name}: original damage model differs").into());
                }
                let model = state
                    .model
                    .and_then(|index| models.get(index))
                    .ok_or("missing drone model")?;
                let bone = if count == 2 { "Muzzle01" } else { "WeaponA" };
                if model.public_bone_indices(bone).is_empty() {
                    return Err(format!("{name}: original firing bone missing").into());
                }
            }
            if count == 2 {
                let arm = &draws[1];
                for (flag, clip, mode) in [
                    ("UNPACKING", "avbattledr_a1.avbattledr_a1", "ONCE"),
                    ("PACKING", "avbattledr_a1.avbattledr_a1", "ONCE_BACKWARDS"),
                    ("FIRING_B", "avbattledr_a2.avbattledr_a2", "ONCE"),
                    ("FIRING_C", "avbattledr_a2.avbattledr_a2", "ONCE_BACKWARDS"),
                ] {
                    let flags = BTreeSet::from([flag.into()]);
                    let state = &arm.states[arm.select(&flags)];
                    if state.mode != mode
                        || !state
                            .motions
                            .iter()
                            .any(|motion| motion.name.eq_ignore_ascii_case(clip))
                    {
                        return Err(format!("{name}: original {flag} arm animation missing").into());
                    }
                    if !self.clips.contains_key(clip) {
                        return Err(format!("missing installed arm clip {clip}").into());
                    }
                }
            }
        }
        println!(
            "Original drone art: both bodies/damage models, firing bones and four Battle Drone arm conditions passed"
        );
        Ok(())
    }

    pub fn drone_weld_position(&self, type_index: usize, bone: &str, weld: &Weld) -> [f32; 3] {
        let mut flags = BTreeSet::from(["FIRING_B".into()]);
        let [damaged, really_damaged] = self.damage_thresholds;
        if weld.health_ratio <= damaged && weld.health_ratio > really_damaged {
            flags.insert("DAMAGED".into());
        } else if weld.health_ratio <= really_damaged && weld.health_ratio > 0.0 {
            flags.insert("REALLYDAMAGED".into());
        }
        let name = bone.to_ascii_lowercase();
        let offset = self
            .draws
            .get(type_index)
            .and_then(|draws| {
                draws.iter().find_map(|draw| {
                    let model = self.models.get(draw.states[draw.select(&flags)].model?)?;
                    let index = *model.public_bone_indices(bone).get(&name)?;
                    model.bone_position(&model.bone_names[index])
                })
            })
            .unwrap_or([0.0; 3]);
        // SlavedUpdate adds pristine model-space coordinates directly; it does
        // not rotate this offset by the drone's facing or attach the emitter.
        std::array::from_fn(|axis| weld.position[axis] + offset[axis])
    }
}
