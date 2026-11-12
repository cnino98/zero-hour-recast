//! Pinned `W3DScorch`: original terrain marks, atlas and bounded newest-first buffers.
use bevy::{
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{collections::VecDeque, error::Error};

mod inspection;
mod render;
#[cfg(test)]
mod tests;
pub use inspection::Inspection;
pub use render::update;

pub struct Mark {
    pub position: [f32; 3],
    pub radius: f32,
    pub kind: i32,
}

struct Marks {
    entries: VecDeque<Mark>,
    deduplicate: bool,
    dirty: bool,
}

impl Marks {
    const fn new(deduplicate: bool) -> Self {
        Self {
            entries: VecDeque::new(),
            deduplicate,
            dirty: true,
        }
    }

    fn add(&mut self, position: [f32; 3], radius: f32, kind: i32) {
        let kind = if (0..9).contains(&kind) { kind } else { 0 };
        let limit = radius / 4.0;
        if self.deduplicate
            && self.entries.iter().any(|mark| {
                mark.kind == kind
                    && (position[0] - mark.position[0]).abs() < limit
                    && (position[1] - mark.position[1]).abs() < limit
                    && (radius - mark.radius).abs() < limit
            })
        {
            return;
        }
        if self.entries.len() >= 500 {
            self.entries.pop_front();
        }
        self.entries.push_back(Mark {
            position,
            radius,
            kind,
        });
        self.dirty = true;
    }
}

pub struct Scorches {
    marks: Marks,
    image: Option<Image>,
    texture: Option<Handle<Image>>,
    color: [f32; 4],
    index_count: usize,
}

impl Scorches {
    pub(crate) const fn new(image: Option<Image>) -> Self {
        Self {
            marks: Marks::new(true),
            image,
            texture: None,
            color: [1.0; 4],
            index_count: 0,
        }
    }

    pub(crate) fn load(assets: &VirtualAssets) -> Result<Self, Box<dyn Error>> {
        let file = content::scan(assets.read("data/ini/gamedata.ini")?)?;
        let files = [file];
        let lines = &crate::skirmish_data::definition(&files, "GameData", "")?.lines;
        let enabled = |name| -> Result<bool, Box<dyn Error>> {
            match crate::skirmish_data::field(lines, 0, name)
                .map(crate::skirmish_data::scalar)
                .transpose()?
            {
                Some("Yes" | "YES") => Ok(true),
                Some("No" | "NO") | None => Ok(false),
                _ => Err("invalid terrain texture filtering flag".into()),
            }
        };
        let trilinear = enabled("TrilinearTerrainTex")?;
        let linear = enabled("BilinearTerrainTex")? || trilinear;
        let mut image = crate::source_texture::load(assets, "EXScorch01.tga")?;
        image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            min_filter: if linear {
                ImageFilterMode::Linear
            } else {
                ImageFilterMode::Nearest
            },
            mag_filter: if linear {
                ImageFilterMode::Linear
            } else {
                ImageFilterMode::Nearest
            },
            mipmap_filter: if trilinear {
                ImageFilterMode::Linear
            } else {
                ImageFilterMode::Nearest
            },
            ..default()
        });
        Ok(Self::new(Some(image)))
    }

    pub(crate) fn add(&mut self, position: [f32; 3], radius: f32, kind: i32) {
        self.marks.add(position, radius, kind);
    }

    pub(crate) fn marks(&self) -> impl Iterator<Item = &Mark> {
        self.marks.entries.iter()
    }

    pub(crate) fn clear(&mut self) {
        self.marks.entries.clear();
        self.marks.dirty = true;
    }

    pub(crate) const fn set_color(&mut self, color: [f32; 4]) {
        self.color = color;
        self.marks.dirty = true;
    }
}
