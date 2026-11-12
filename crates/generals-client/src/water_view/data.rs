//! Effective Water.ini and source `GameData` rendering switches.
use crate::skirmish_data::{definition, field, scalar};
use generals_assets::{
    content::{self, ContentLine},
    virtual_assets::VirtualAssets,
};
use num_traits::ToPrimitive;
use std::error::Error;

pub struct Settings {
    pub diffuse: [f32; 4],
    pub texture: String,
    pub additive: bool,
    pub depth: f32,
    pub opacity: f32,
    pub soft: bool,
    pub feather: u32,
}

pub fn load(
    assets: &VirtualAssets,
    time: &str,
    light: [f32; 3],
) -> Result<Settings, Box<dyn Error>> {
    let files = [
        content::scan(assets.read("data/ini/water.ini")?)?,
        content::scan(assets.read("data/ini/gamedata.ini")?)?,
    ];
    let water = &definition(&files, "WaterTransparency", "")?.lines;
    let global = &definition(&files, "GameData", "")?.lines;
    let set = &definition(&files, "WaterSet", &time.to_ascii_uppercase())?.lines;
    let kind = real(global, "WaterType", 0.0)?;
    if kind != 0.0 && kind.to_bits() != 3.0_f32.to_bits() {
        return Err(
            "legacy sea water mode requires its separate original reflection backend".into(),
        );
    }
    let shade = color(water, "StandingWaterColor", [255.0; 4])?;
    let diffuse = color(set, "DiffuseColor", [255.0; 4])?;
    // drawTrapezoidWater extracts the low byte as red from packed AARRGGBB.
    let tint = if shade[..3] == [255.0; 3] {
        [
            light[0] * (diffuse[2] / 255.0) * 255.0,
            light[1] * (diffuse[1] / 255.0) * 255.0,
            light[2] * (diffuse[0] / 255.0) * 255.0,
        ]
    } else if shade[..3] == [0.0; 3] {
        [255.0; 3]
    } else {
        [shade[0], shade[1], shade[2]].map(|value| (value / 255.0) * 255.0)
    };
    // Preserve native packed-color truncation and channel spill, including light sums above one.
    let channels = tint.map(|value| {
        value
            .to_i32()
            .ok_or("water lighting exceeds original integer range")
    });
    let [red, green, blue] = channels;
    let packed = blue? | (green? << 8) | (red? << 16);
    let alpha = diffuse[3].to_i32().expect("byte water alpha");
    let packed = packed | (alpha << 24);
    let depth = real(water, "TransparentWaterDepth", 3.0)?;
    let opacity = real(water, "TransparentWaterMinOpacity", 1.0)?;
    if depth < 0.0 || !(0.0..=1.0).contains(&opacity) {
        return Err("invalid original water transparency range".into());
    }
    let feather = field(global, 0, "FeatherWater")
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse()?;
    Ok(Settings {
        diffuse: [16, 8, 0, 24].map(|shift| {
            f32::from(u8::try_from((packed >> shift) & 255).expect("masked water channel")) / 255.0
        }),
        texture: field(water, 0, "StandingWaterTexture")
            .map(scalar)
            .transpose()?
            .unwrap_or("TWWater01.tga")
            .into(),
        additive: boolean(water, "AdditiveBlending", false)?,
        depth,
        opacity,
        soft: boolean(global, "ShowSoftWaterEdge", true)?,
        feather,
    })
}

pub fn real(lines: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    let value = field(lines, 0, key)
        .map(scalar)
        .transpose()?
        .map(str::parse)
        .transpose()?
        .unwrap_or(default);
    if !f32::is_finite(value) {
        return Err(format!("nonfinite original {key}").into());
    }
    Ok(value)
}

pub fn boolean(lines: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    match field(lines, 0, key).map(scalar).transpose()? {
        None => Ok(default),
        Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
        _ => Err(format!("invalid original {key}").into()),
    }
}

fn color(
    lines: &[ContentLine],
    key: &str,
    mut defaults: [f32; 4],
) -> Result<[f32; 4], Box<dyn Error>> {
    if let Some(line) = field(lines, 0, key) {
        for (channel, prefix) in defaults.iter_mut().zip(["R:", "G:", "B:", "A:"]) {
            if let Some(value) = line
                .tokens
                .iter()
                .find_map(|token| token.strip_prefix(prefix))
            {
                *channel = f32::from(value.parse::<u8>()?);
            }
        }
    }
    Ok(defaults)
}
