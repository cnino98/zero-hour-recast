use super::Animations;
use generals_assets::w3d::Model;
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn verify_hacker_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let index = types
            .iter()
            .position(|unit| unit.name == "ChinaInfantryHacker")
            .ok_or("missing Hacker type")?;
        let draw = &self.draws[index][0];
        for (flag, name) in [
            ("UNPACKING", "NIHCKR_SKL.NIHCKR_ATB1"),
            ("FIRING_A", "NIHCKR_SKL.NIHCKR_ATB2"),
            ("PACKING", "NIHCKR_SKL.NIHCKR_ATB3"),
        ] {
            let flags = BTreeSet::from([flag.to_owned()]);
            let state = &draw.states[draw.select(&flags)];
            let motion = state
                .motions
                .first()
                .filter(|motion| motion.name.eq_ignore_ascii_case(name))
                .ok_or("Hacker state selected the wrong original clip")?;
            let clip = self
                .clips
                .get(&motion.name)
                .ok_or("Hacker animation not preloaded")?;
            let model = &models[state.model.ok_or("Hacker state model missing")?];
            for frame in [0.0, f32::from(clip.frame_count - 1)] {
                let posed = model.pose(clip, frame)?;
                if !posed.iter().any(|mesh| mesh.visible)
                    || posed
                        .iter()
                        .flat_map(|mesh| mesh.positions.iter().flatten())
                        .any(|value| !value.is_finite())
                {
                    return Err(
                        "Hacker original animation has invisible or nonfinite posed geometry"
                            .into(),
                    );
                }
            }
        }
        println!(
            "Installed Hacker art: original unpack/hack/pack model and clips selected; six endpoint poses finite and visible"
        );
        Ok(())
    }
}
