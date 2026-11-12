//! `SnowManager` defaults followed by effective Weather.ini/map override fields.
use crate::{
    skirmish_data::{definition, field, scalar},
    water_view::data::{boolean, real},
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use num_traits::ToPrimitive;
use std::error::Error;

// Reload preparation retains cached manager values until a Weather declaration
// invokes updateIniSettings; loading may still fail before replacing the map.
#[derive(Clone)]
pub struct Settings {
    pub enabled: bool,
    pub texture: String,
    pub frequency: [f32; 2],
    pub amplitude: f32,
    pub point_size: f32,
    pub point_bounds: [f32; 2],
    pub quad_size: f32,
    pub dimension: f32,
    pub density: f32,
    pub velocity: f32,
    pub points: bool,
}
pub fn load(assets: &VirtualAssets) -> Result<Settings, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/weather.ini")?)?];
    let lines = &definition(&files, "Weather", "")?.lines;
    let settings = Settings {
        enabled: boolean(lines, "SnowEnabled", false)?,
        texture: field(lines, 0, "SnowTexture")
            .map(scalar)
            .transpose()?
            .unwrap_or("EXSnowFlake.tga")
            .into(),
        frequency: [
            real(lines, "SnowFrequencyScaleX", 0.0533)?,
            real(lines, "SnowFrequencyScaleY", 0.0275)?,
        ],
        amplitude: real(lines, "SnowAmplitude", 5.0)?,
        point_size: real(lines, "SnowPointSize", 1.0)?,
        point_bounds: [
            real(lines, "SnowMinPointSize", 0.0)?,
            real(lines, "SnowMaxPointSize", 64.0)?,
        ],
        quad_size: real(lines, "SnowQuadSize", 0.5)?,
        dimension: real(lines, "SnowBoxDimensions", 200.0)?,
        density: real(lines, "SnowBoxDensity", 1.0)?,
        velocity: real(lines, "SnowVelocity", 4.0)?,
        points: boolean(lines, "SnowPointSprites", true)?,
    };
    if settings.dimension.to_i32().is_none()
        || (settings.dimension * settings.density * 0.5)
            .to_i32()
            .is_none()
        || settings.dimension < 1.0
        || settings.density <= 0.0
        || settings.velocity <= 0.0
        || settings.amplitude < 0.0
        || settings.point_size < 0.0
        || settings.quad_size < 0.0
        || settings.point_bounds[0] < 0.0
        || settings.point_bounds[1] < settings.point_bounds[0]
    {
        return Err("invalid original weather geometry or timing".into());
    }
    Ok(settings)
}
