//! Admission of the original standalone, uncolored, fixed-state emitter draw.
use super::{Binding, Definitions};
use crate::skirmish_data::{field, scalar};
use generals_assets::{
    content::{ContentLine, Declaration},
    virtual_assets::VirtualAssets,
    w3d::{Model, emitter::Definition},
};
use generals_sim::skirmish::UnitType;
use num_traits::ToPrimitive;
use std::{error::Error, sync::Arc};

pub fn validate(definition: &Definition) -> Result<(), Box<dyn Error>> {
    let emission = &definition.emission;
    // Signed Random3's minimum nonzero magnitude is 2^-31. These bounds
    // keep squared local radii inside the x87 helper's normal-intermediate
    // binary32 domain, including the half-radius and estimate-squared steps.
    let normal_extents = f32::from_bits(0x3080_0000)..=f32::from_bits(0x4e80_0000);
    if definition.motion.outward != 0.0
        && definition
            .motion
            .position_box
            .iter()
            .any(|extent| *extent != 0.0 && !normal_extents.contains(extent))
    {
        return Err("emitter outward volume requires extended-exponent x87 arithmetic".into());
    }

    let interval = (1000.0 / emission.rate)
        .to_u32()
        .ok_or("invalid emitter interval")?;
    let burst = emission
        .burst
        .max(1)
        .to_f32()
        .ok_or("invalid emitter burst")?;
    let count = burst * emission.rate;
    let capacity = count * (emission.lifetime + 1.0);
    if interval == 0
        || emission.maximum != 0.0
        || emission.lifetime <= 0.0
        || !capacity.is_finite()
        || !(2.0..=65_536.0).contains(&capacity)
    {
        return Err("unsupported emitter timing, completion cap or buffer capacity".into());
    }
    let age = (emission.lifetime * 1000.0)
        .to_u32()
        .ok_or("invalid emitter lifetime")?;
    if age == 0 {
        return Err("submillisecond emitter lifetime".into());
    }
    for keys in [&definition.properties.size, &definition.properties.opacity] {
        let mut previous = 0;
        for key in &keys[1..] {
            let time = (key.time * 1000.0)
                .to_u32()
                .ok_or("invalid emitter key time")?;
            if time <= previous {
                return Err("emitter key times collapse to the same millisecond".into());
            }
            if time >= age {
                break;
            }
            previous = time;
        }
    }
    Ok(())
}

pub fn validate_draw(draw: &[ContentLine], definition: &Definition) -> Result<(), Box<dyn Error>> {
    validate(definition)?;
    if draw[0]
        .tokens
        .get(1)
        .is_none_or(|family| family != "W3DModelDraw")
        || draw
            .iter()
            .filter(|line| {
                line.tokens
                    .first()
                    .is_some_and(|key| key == "DefaultConditionState")
            })
            .count()
            != 1
        || draw.iter().any(|line| {
            line.tokens.first().is_some_and(|key| {
                !matches!(
                    key.as_str(),
                    "Draw" | "DefaultConditionState" | "Model" | "End"
                )
            })
        })
    {
        return Err("emitter draw requires its original single default state without animation, color or model replacement".into());
    }
    Ok(())
}

pub fn validate_object(object: &Declaration) -> Result<(), Box<dyn Error>> {
    if field(&object.lines, 0, "Scale")
        .map(scalar)
        .transpose()?
        .is_some_and(|scale| scale != "1" && scale != "1.0")
    {
        return Err("scaled emitter draw requires native clone-cache support".into());
    }
    let immobile = field(&object.lines, 0, "KindOf")
        .is_some_and(|line| line.tokens.iter().any(|flag| flag == "IMMOBILE"));
    if !immobile {
        return Err("moving emitter draw requires its model-condition lifecycle".into());
    }
    Ok(())
}

pub fn load(
    assets: &VirtualAssets,
    types: &[UnitType],
    models: &[Model],
) -> Result<Definitions, Box<dyn Error>> {
    if models.len() < types.len() {
        return Err("missing catalog models for emitter binding".into());
    }
    reject_map_overrides(assets, types, models)?;
    let mut bindings = Vec::with_capacity(types.len());
    let mut images = Vec::new();
    for (stats, model) in types.iter().zip(models) {
        let Some(definition) = &model.emitter else {
            bindings.push(None);
            continue;
        };
        validate(definition)?;
        let mut image = crate::source_texture::load(assets, &definition.texture)?;
        image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
        let texture = images.len();
        images.push(image);
        bindings.push(Some(Binding {
            name: stats.name.clone(),
            definition: Arc::new(definition.clone()),
            texture,
        }));
    }
    Ok(Definitions { bindings, images })
}

fn reject_map_overrides(
    assets: &VirtualAssets,
    types: &[UnitType],
    models: &[Model],
) -> Result<(), Box<dyn Error>> {
    for asset in assets.map_ini_sources() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: asset.path.clone(),
            origin: asset.origin.clone(),
            bytes: asset.bytes.clone(),
        })?;
        for object in &file.declarations {
            if !matches!(object.kind.as_str(), "Object" | "ObjectReskin") {
                continue;
            }
            if types
                .iter()
                .zip(models)
                .any(|(stats, model)| model.emitter.is_some() && stats.name == object.name)
            {
                return Err(
                    "map-local emitter templates require native validation-data lifetime support"
                        .into(),
                );
            }
        }
    }
    Ok(())
}
