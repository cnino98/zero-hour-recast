//! SH W3DModelDraw.cpp569–710,3398: public bones in the first/final pristine frame.
use super::{Animations, State};
use crate::skirmish_data::{blocks, definition, field, finite_number, scalar};
use generals_assets::{content::ContentFile, w3d::Model};
use generals_sim::skirmish::{UnitType, model_bones};
use std::{collections::BTreeMap, error::Error};

type PristineBones = BTreeMap<String, [[f32; 4]; 3]>;

impl Animations {
    pub(super) fn prepare_logical_bones(
        &mut self,
        files: &[ContentFile],
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let globals = definition(files, "GameData", "")?;
        let standard: Vec<_> = globals
            .lines
            .iter()
            .filter(|line| {
                line.depth == Some(0)
                    && line
                        .tokens
                        .first()
                        .is_some_and(|key| key == "StandardPublicBone")
            })
            .flat_map(|line| line.tokens[1..].iter().cloned())
            .collect();
        let defaults = definition(files, "Object", "DefaultThingTemplate")?;
        let default_scale = field(&defaults.lines, 0, "Scale")
            .map(|line| finite_number(scalar(line)?))
            .transpose()?
            .unwrap_or(1.0);
        for (index, unit) in types.iter().enumerate() {
            let object = definition(files, "Object", &unit.name)?;
            let scale = field(&object.lines, 0, "Scale")
                .map(|line| finite_number(scalar(line)?))
                .transpose()?
                .unwrap_or(default_scale);
            let source_draws = blocks(&object.lines, 0, "Draw")?;
            let mut draws = Vec::new();
            for (draw, source) in self.draws[index].iter().zip(
                source_draws
                    .into_iter()
                    .enumerate()
                    .filter(|(ordinal, lines)| {
                        *ordinal == 0
                            || lines[0].tokens.get(1).is_some_and(|name| {
                                matches!(name.as_str(), "W3DModelDraw" | "W3DSupplyDraw")
                            })
                    })
                    .map(|(_, lines)| lines),
            ) {
                let mut public = standard.clone();
                public.extend(
                    source
                        .iter()
                        .filter(|line| {
                            line.depth == Some(1)
                                && line
                                    .tokens
                                    .first()
                                    .is_some_and(|key| key == "ExtraPublicBone")
                        })
                        .flat_map(|line| line.tokens[1..].iter().cloned()),
                );
                let mut states = Vec::new();
                for state in draw
                    .states
                    .iter()
                    .filter(|state| state.transition.is_none())
                {
                    states.push(model_bones::State {
                        conditions: state.conditions.clone(),
                        transforms: self.pristine_transforms(state, &public, models, scale)?,
                    });
                }
                draws.push(model_bones::Draw {
                    states,
                    ignored: draw.ignored.clone(),
                });
            }
            self.logical_bones.push(draws);
        }
        Ok(())
    }

    pub fn bind_logical_bones(&mut self, types: &mut [UnitType]) {
        assert_eq!(types.len(), self.logical_bones.len());
        for (unit, draws) in types.iter_mut().zip(self.logical_bones.drain(..)) {
            unit.pristine_draws = draws;
        }
    }

    fn pristine_transforms(
        &self,
        state: &State,
        public: &[String],
        models: &[Model],
        scale: f32,
    ) -> Result<PristineBones, Box<dyn Error>> {
        let mut result = BTreeMap::new();
        let Some(model) = state.model.and_then(|index| models.get(index)) else {
            return Ok(result);
        };
        let clip = state
            .motions
            .first()
            .and_then(|motion| self.clips.get(&motion.name));
        let frame = if state.flags.contains("PRISTINE_BONE_POS_IN_FINAL_FRAME") {
            clip.map_or(0.0, |clip| f32::from(clip.frame_count - 1))
        } else {
            0.0
        };
        let mut prefixes = public.to_vec();
        for tokens in &state.fields {
            let index = match tokens.first().map(String::as_str) {
                Some("Turret" | "TurretPitch" | "AltTurret" | "AltTurretPitch") => 1,
                Some(
                    "WeaponFireFXBone" | "WeaponRecoilBone" | "WeaponMuzzleFlash"
                    | "WeaponLaunchBone",
                ) => 2,
                _ => continue,
            };
            if let Some(name) = tokens.get(index) {
                prefixes.push(name.clone());
            }
        }
        for prefix in prefixes {
            for (name, index) in model.public_bone_indices(&prefix) {
                if let Some(position) =
                    model.posed_bone_matrix(&model.bone_names[index], clip, frame)?
                {
                    result.insert(name, position.map(|row| row.map(|value| value * scale)));
                }
            }
        }
        Ok(result)
    }
}
