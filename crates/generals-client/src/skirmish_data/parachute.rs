//! Original `ParachuteContain` data and FREEFALL locomotor; pristine attachment bones.
use super::{blocks, definition, field, finite_number, locomotor, scalar};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::parachute::Rules;
use std::{error::Error, f32::consts::PI};

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some(module) = modules.iter().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "ParachuteContain")
    }) else {
        return Ok(None);
    };
    let declaration = object
        .lines
        .iter()
        .find(|line| {
            line.depth == Some(0)
                && line.tokens.first().is_some_and(|key| key == "Locomotor")
                && line.tokens.get(1).is_some_and(|set| set == "SET_FREEFALL")
        })
        .ok_or("Parachute requires original FREEFALL locomotor set")?;
    if declaration.tokens.len() != 3 {
        return Err("Parachute FREEFALL set requires a single authored locomotor".into());
    }
    let freefall = definition(files, "Locomotor", &declaration.tokens[2])?;
    let globals = definition(files, "GameData", "")?;
    let freefall = locomotor::load(&freefall.lines, &globals.lines, &object.lines)?;
    let number = |key, fallback| -> Result<f32, Box<dyn Error>> {
        field(module, 1, key).map_or(Ok(fallback), |line| finite_number(scalar(line)?))
    };
    Ok(Some(Rules {
        freefall,
        angular_rate: [number("PitchRateMax", 0.0)?, number("RollRateMax", 0.0)?]
            .map(|value| value * ((1.0 / 30.0) * (PI / 180.0))),
        low_damping: number("LowAltitudeDamping", 0.2)?,
        open_distance: number("ParachuteOpenDist", 0.0)?,
        water_slop: number("KillWhenLandingInWaterSlop", 10.0)?,
        fall_damage: field(module, 1, "FreeFallDamagePercent").map_or(
            Ok(0.5),
            |line| -> Result<f32, Box<dyn Error>> {
                Ok(finite_number(scalar(line)?.trim_end_matches('%'))? / 100.0)
            },
        )?,
        open_sound: field(module, 1, "ParachuteOpenSound")
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into(),
    }))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::skirmish_data::{finish_type, normal_model, source_files, unit_type};
    use generals_assets::virtual_assets::VirtualAssets;
    use generals_sim::skirmish::{Faction, Kind};

    pub fn installed_unit(assets: &VirtualAssets) -> generals_sim::skirmish::UnitType {
        let files = source_files(assets).unwrap();
        let object = definition(&files, "Object", "AmericaParachute").unwrap();
        unit_type(&files, object, Faction::Usa, Kind::Effect)
            .and_then(|unit| finish_type(&files, object, unit))
            .unwrap()
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_parachute_object_admits_both_locomotors_and_original_model() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = source_files(&assets).unwrap();
        let object = definition(&files, "Object", "AmericaParachute").unwrap();
        let unit = installed_unit(&assets);
        let parachute = unit.parachute.as_ref().unwrap();
        assert_eq!(unit.preferred_height.to_bits(), (-1.0_f32).to_bits());
        assert!(unit.locomotor.flight.is_some());
        assert!(parachute.freefall.flight.is_some());
        assert_eq!(parachute.open_sound, "ParachuteOpen");
        assert_eq!(parachute.open_distance.to_bits(), 25.0_f32.to_bits());
        assert!(unit.contain.is_some());
        assert!(unit.crush.physics_module.is_some());
        assert_eq!(
            unit.locomotor.flight.as_ref().unwrap().mass.to_bits(),
            5.0_f32.to_bits()
        );
        let death = unit.death_lifecycle.as_ref().unwrap();
        assert_eq!(
            death.physics.as_ref().unwrap().mass.to_bits(),
            5.0_f32.to_bits()
        );
        let model = normal_model(&assets, object).unwrap();
        assert!(!model.meshes.is_empty());
        let mut types = vec![unit];
        let mut models = Vec::new();
        let mut animations = crate::model_animation::load(&assets, &mut models, &types).unwrap();
        animations.bind_logical_bones(&mut types);
        let draws = &types[0].pristine_draws;
        assert!(!draws.is_empty());
        assert!(
            draws.iter().flat_map(|draw| &draw.states).any(|state| {
                state.transforms.contains_key("para_attch")
                    && state.transforms.contains_key("para_cog")
            }),
            "original canopy public attachment bones must be bound"
        );
    }
}
