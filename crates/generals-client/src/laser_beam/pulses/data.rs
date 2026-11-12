//! Installed weapon-owned `W3DLaserDraw` pulse templates.
use super::{Binding, Template};
use crate::skirmish_data::{blocks, definition, duration_frames, field, number, scalar, unsigned};
use generals_assets::{
    content::{self, ContentLine},
    virtual_assets::VirtualAssets,
};
use std::{collections::BTreeMap, error::Error};

pub(super) fn bindings(
    assets: &VirtualAssets,
    templates: &[Template],
) -> Result<BTreeMap<String, Binding>, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/weapon.ini")?)?;
    let mut bindings = BTreeMap::new();
    for declaration in file
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == "Weapon")
    {
        let Some(line) = field(&declaration.lines, 0, "LaserName") else {
            continue;
        };
        let name = scalar(line)?;
        let Some(template) = templates.iter().position(|template| template.name == name) else {
            continue;
        };
        bindings.insert(
            declaration.name.clone(),
            Binding {
                template,
                bone: field(&declaration.lines, 0, "LaserBoneName")
                    .map(scalar)
                    .transpose()?
                    .unwrap_or("")
                    .into(),
            },
        );
    }
    Ok(bindings)
}

pub(super) fn load(assets: &VirtualAssets) -> Result<Vec<Template>, Box<dyn Error>> {
    let files = [content::scan(
        assets.read("data/ini/object/weaponobjects.ini")?,
    )?];
    [
        "PointDefenseLaserBeam",
        "AvengerPointDefenseLaserBeam",
        "AvengerLaserBeam",
        "AvengerTargetingLaserBeam",
        "MicrowaveDisableStream",
        "ECMDisableStream",
    ]
    .into_iter()
    .map(|name| {
        let lines = &definition(&files, "Object", name)?.lines;
        let draw = module(lines, "Draw", "W3DLaserDraw")?;
        let update = module(lines, "ClientUpdate", "LaserUpdate")?;
        let lifetime = module(
            lines,
            "Behavior",
            if name == "MicrowaveDisableStream" {
                "DeletionUpdate"
            } else {
                "LifetimeUpdate"
            },
        )?;
        let image = crate::source_texture::load(
            assets,
            scalar(field(draw, 1, "Texture").ok_or("pulse texture missing")?)?,
        )?;
        let aspect =
            f32::from(u16::try_from(image.width())?) / f32::from(u16::try_from(image.height())?);
        let mut template = parse(name, draw, update, lifetime)?;
        template.aspect = aspect;
        template.image = Some(image);
        Ok(template)
    })
    .collect()
}

fn module<'a>(
    lines: &'a [ContentLine],
    category: &str,
    name: &str,
) -> Result<&'a [ContentLine], Box<dyn Error>> {
    blocks(lines, 0, category)?
        .into_iter()
        .find(|block| block[0].tokens.get(1).is_some_and(|value| value == name))
        .ok_or_else(|| format!("laser pulse lacks {name}").into())
}

fn parse(
    name: &str,
    draw: &[ContentLine],
    update: &[ContentLine],
    lifetime: &[ContentLine],
) -> Result<Template, Box<dyn Error>> {
    let count = u16::try_from(unsigned(draw, 1, "NumBeams")?)?;
    if count == 0
        || field(draw, 1, "Segments")
            .map(|_| unsigned(draw, 1, "Segments"))
            .transpose()?
            .unwrap_or(1)
            != 1
        || field(draw, 1, "ArcHeight").is_some()
        || field(draw, 1, "MaxIntensityLifetime").is_some()
        || field(draw, 1, "FadeLifetime").is_some()
        || scalar(field(draw, 1, "Tile").ok_or("pulse tiling missing")?)? != "Yes"
    {
        return Err(format!("unsupported laser pulse configuration {name}").into());
    }
    let widths = [
        number(draw, 1, "InnerBeamWidth")?,
        number(draw, 1, "OuterBeamWidth")?,
    ];
    if widths.iter().any(|width| *width <= 0.0) {
        return Err("nonpositive laser pulse width".into());
    }
    let inner =
        super::super::color(field(draw, 1, "InnerColor").ok_or("pulse inner color missing")?)?;
    let outer =
        super::super::color(field(draw, 1, "OuterColor").ok_or("pulse outer color missing")?)?;
    let min = duration_frames(field(lifetime, 1, "MinLifetime").ok_or("pulse lifetime missing")?)?;
    let max = duration_frames(field(lifetime, 1, "MaxLifetime").ok_or("pulse lifetime missing")?)?;
    if min > max {
        return Err("reversed weapon laser lifetime".into());
    }
    let mut particles = [String::new(), String::new()];
    for (destination, key) in particles
        .iter_mut()
        .zip(["MuzzleParticleSystem", "TargetParticleSystem"])
    {
        if let Some(line) = field(update, 1, key) {
            scalar(line)?.clone_into(destination);
        }
    }
    Ok(Template {
        name: name.into(),
        beams: (0..count)
            .rev()
            .map(|index| super::super::beam(index, count, widths, inner, outer))
            .collect(),
        particles,
        lifetime: [min, max],
        tiling: signed(draw, "TilingScalar")?,
        scroll: signed(draw, "ScrollRate")?,
        aspect: 1.0,
        image: None,
        material: None,
    })
}

fn signed(lines: &[ContentLine], key: &str) -> Result<f32, Box<dyn Error>> {
    let value: f32 = scalar(field(lines, 1, key).ok_or("laser pulse scalar missing")?)?.parse()?;
    if !value.is_finite() {
        return Err(format!("nonfinite laser pulse {key}").into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_microwave_beam_uses_original_defaults_and_fixed_deletion_lifetime() {
        let root = generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let templates = load(&assets).unwrap();
        let index = templates
            .iter()
            .position(|template| template.name == "MicrowaveDisableStream")
            .unwrap();
        let beam = &templates[index];
        assert_eq!(beam.beams.len(), 1);
        assert_eq!(beam.beams[0].width.to_bits(), 8.0_f32.to_bits());
        assert_eq!(beam.lifetime, [4, 4]);
        assert_eq!(beam.tiling.to_bits(), 0.5_f32.to_bits());
        assert_eq!(beam.scroll.to_bits(), (-10.0_f32).to_bits());
        assert_eq!(beam.particles, ["GenericGreenLaserFlare", "HackFlare"]);
        let bindings = bindings(&assets, &templates).unwrap();
        for weapon in [
            "MicrowaveTankBuildingDisabler",
            "MicrowaveTankBuildingClearer",
        ] {
            assert_eq!(bindings[weapon].template, index);
            assert_eq!(bindings[weapon].bone, "WEAPON02");
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_ecm_beam_preserves_three_layers_overlap_and_random_lifetime() {
        let root = generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let templates = load(&assets).unwrap();
        let index = templates
            .iter()
            .position(|template| template.name == "ECMDisableStream")
            .unwrap();
        let beam = &templates[index];
        assert_eq!(beam.beams.len(), 3);
        assert_eq!(beam.lifetime, [7, 20]);
        assert_eq!(beam.beams[0].width.to_bits(), 50.0_f32.to_bits());
        assert_eq!(
            beam.particles,
            ["GenericGreenLaserFlare", "GenericGreenLaserFlare"]
        );
        assert_eq!(
            bindings(&assets, &templates).unwrap()["ECMTankVehicleDisabler"].template,
            index
        );
    }

    #[test]
    fn pulse_preserves_negative_scroll_and_tiling_optional_target_particles_and_native_lifetime() {
        let file = content::scan(Asset {
            path: "pulse.ini".into(),
            origin: "original pulse fields".into(),
            bytes: b"Object Pulse\nDraw = W3DLaserDraw DrawTag\nNumBeams=4\nInnerBeamWidth=0.005\nOuterBeamWidth=10\nInnerColor=R:255 G:0 B:180 A:120\nOuterColor=R:100 G:0 B:100 A:100\nTile=Yes\nScrollRate=-2500\nSegments=1\nSegmentOverlapRatio=0\nTilingScalar=-3\nEnd\nClientUpdate=LaserUpdate UpdateTag\nMuzzleParticleSystem=PaladinPointDefenseLaserFlare\nEnd\nBehavior=LifetimeUpdate LifeTag\nMinLifetime=95\nMaxLifetime=95\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let lines = &file.declarations[0].lines;
        let template = parse(
            "Pulse",
            module(lines, "Draw", "W3DLaserDraw").unwrap(),
            module(lines, "ClientUpdate", "LaserUpdate").unwrap(),
            module(lines, "Behavior", "LifetimeUpdate").unwrap(),
        )
        .unwrap();
        assert_eq!(template.beams.len(), 4);
        assert_eq!(template.beams[0].width.to_bits(), 10.0_f32.to_bits());
        assert_eq!(template.tiling.to_bits(), (-3.0_f32).to_bits());
        assert_eq!(template.scroll.to_bits(), (-2500.0_f32).to_bits());
        assert_eq!(template.lifetime, [3, 3]);
        assert_eq!(template.particles, ["PaladinPointDefenseLaserFlare", ""]);
    }
}
