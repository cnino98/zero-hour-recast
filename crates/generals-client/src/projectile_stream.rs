//! Original `W3DProjectileStreamDraw`: projectile points, gaps and tiled `SegLines`.
mod inspection;
mod render;
#[cfg(test)]
mod tests;
pub use inspection::ProjectileStreamInspection;
pub use render::{Visual as ProjectileStreamVisual, update};

use crate::skirmish_data::{blocks, definition, number, text, unsigned};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeMap, error::Error};

#[derive(Resource)]
pub struct Streams {
    templates: BTreeMap<String, Template>,
    active: BTreeMap<(usize, usize), Vec<Line>>,
    generation: u32,
    now_ms: u32,
}

struct Template {
    width: f32,
    tile: f32,
    scroll: f32,
    maximum: usize,
    image: Option<Image>,
    material: Option<Handle<crate::source_material::SourceMaterial>>,
}

struct Line {
    offset: f32,
    used_ms: u32,
}

impl Line {
    const fn new(now: u32) -> Self {
        Self {
            offset: 0.0,
            used_ms: now,
        }
    }

    fn advance(&mut self, now: u32, scroll: f32) -> f32 {
        let bytes = now.wrapping_sub(self.used_ms).to_le_bytes();
        let upper = f32::from(u16::from_le_bytes([bytes[2], bytes[3]])) * 65_536.0;
        let milliseconds = upper + f32::from(u16::from_le_bytes([bytes[0], bytes[1]]));
        let per_millisecond = scroll * 0.001;
        let change = per_millisecond * milliseconds;
        self.offset += change;
        self.offset -= self.offset.floor();
        self.used_ms = now;
        self.offset
    }
}

pub fn load(assets: &VirtualAssets, types: &[UnitType]) -> Result<Streams, Box<dyn Error>> {
    let files = [content::scan(
        assets.read("data/ini/object/weaponobjects.ini")?,
    )?];
    let mut templates = BTreeMap::new();
    for name in types.iter().flat_map(|unit| {
        (0..unit.weapon_template_count())
            .filter_map(|index| unit.weapon_template(index)?.projectile.stream.as_deref())
    }) {
        if templates.contains_key(name) {
            continue;
        }
        let object = definition(&files, "Object", name)?;
        let draws = blocks(&object.lines, 0, "Draw")?;
        if draws.len() != 1
            || draws[0][0]
                .tokens
                .get(1)
                .is_none_or(|kind| kind != "W3DProjectileStreamDraw")
        {
            return Err(format!("{name}: unsupported projectile stream draw modules").into());
        }
        let draw = draws[0];
        let width = number(draw, 1, "Width")?;
        if width <= 0.0 {
            return Err("projectile stream width must be positive".into());
        }
        templates.insert(
            name.into(),
            Template {
                width,
                tile: number(draw, 1, "TileFactor")?.clamp(0.0, 50.0),
                scroll: number(draw, 1, "ScrollRate")?,
                maximum: usize::try_from(unsigned(draw, 1, "MaxSegments")?)?,
                image: Some(crate::source_texture::load(
                    assets,
                    text(draw, 1, "Texture")?,
                )?),
                material: None,
            },
        );
    }
    Ok(Streams {
        templates,
        active: BTreeMap::new(),
        generation: 0,
        now_ms: 0,
    })
}

fn runs(points: &[[f32; 3]], maximum: usize) -> Vec<&[[f32; 3]]> {
    let first = if maximum == 0 {
        0
    } else {
        points.len().saturating_sub(maximum)
    };
    points[first..]
        .split(|point| point.iter().all(|&value| value == 0.0))
        .filter(|run| run.len() > 1)
        .collect()
}

fn logic_milliseconds(tick: u64, fraction: std::time::Duration) -> u32 {
    // WW3D::Get_Logic_Time_Milliseconds includes the fractional logic step.
    let full_steps = u128::from(tick) * 1_000_000_000 / 30;
    let milliseconds = (full_steps + fraction.as_nanos()) / 1_000_000;
    u32::try_from(milliseconds & u128::from(u32::MAX)).expect("bounded source milliseconds")
}
