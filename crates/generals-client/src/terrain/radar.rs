//! Native radar tile mip colors, height modulation and nine-sample averaging.

use super::Terrain;
use crate::source_texture;
use bevy::{prelude::Vec2, render::render_resource::TextureFormat};
use generals_assets::{content, map::PolygonTrigger, virtual_assets::VirtualAssets};
use std::{collections::BTreeMap, error::Error};

impl Terrain {
    pub(crate) fn radar_bridge_color(
        &self,
        geometry: &generals_sim::terrain::BridgeGeometry,
        color: [f32; 3],
        average: f32,
    ) -> [f32; 3] {
        let corners = geometry.corners;
        let height = (corners[0][2] + corners[1][2] + corners[2][2] + corners[3][2]) / 4.0;
        let low = f32::from(*self.heights.iter().min().expect("loaded terrain heights")) * 0.625;
        let high = f32::from(*self.heights.iter().max().expect("loaded terrain heights")) * 0.625;
        height_color(color, height, average, high, low)
    }

    pub fn radar_pixels(
        &self,
        assets: &VirtualAssets,
        water: &[PolygonTrigger],
    ) -> Result<(Vec<u8>, f32), Box<dyn Error>> {
        let tile_colors = self.radar_tile_colors(assets)?;
        let (mut total, mut count) = (0.0, 0_u16);
        let mut heights = Vec::with_capacity(128 * 128);
        let mut waters = Vec::with_capacity(128 * 128);
        for y in 0..128_u16 {
            for x in 0..128_u16 {
                let point = Vec2::new(
                    f32::from(x) * f32::from(self.width) / 128.0,
                    f32::from(y) * f32::from(self.height) / 128.0,
                );
                let height = self.sample(point) * 10.0;
                let polygon = super::movement::water_height(
                    water,
                    [
                        i32::from(x) * i32::from(self.width) * 10 / 128,
                        i32::from(y) * i32::from(self.height) * 10 / 128,
                    ],
                )?
                .filter(|&level| height < level);
                if x % 2 == 0 && y % 2 == 0 && polygon.is_none() {
                    total += height;
                    count += 1;
                }
                heights.push(height);
                waters.push(polygon);
            }
        }
        let average = total / f32::from(count.max(1));
        let low = f32::from(*self.heights.iter().min().ok_or("empty radar heights")?) * 0.625;
        let high = f32::from(*self.heights.iter().max().ok_or("empty radar heights")?) * 0.625;
        let water_color = if waters.iter().any(Option::is_some) {
            load_water_color(assets)?
        } else {
            [0.0; 3]
        };
        let mut samples = Vec::with_capacity(128 * 128);
        for y in 0..128_u16 {
            for x in 0..128_u16 {
                let index = usize::from(y) * 128 + usize::from(x);
                let raw_x = u16::try_from(u32::from(x) * u32::from(self.width) / 128)
                    .expect("sample within map")
                    + self.border;
                let raw_y = u16::try_from(u32::from(y) * u32::from(self.height) / 128)
                    .expect("sample within map")
                    + self.border;
                let tile = self.tiles.base_tiles[self.index(raw_x, raw_y)] >> 2;
                let color = tile_colors
                    .get(&tile)
                    .ok_or("radar source tile absent")?
                    .map(|value| f32::from(value) / 255.0);
                samples.push(height_color(color, heights[index], average, high, low));
            }
        }
        let mut pixels = vec![0; 128 * 128 * 4];
        for y in 0..128_u16 {
            for x in 0..128_u16 {
                let index = usize::from(y) * 128 + usize::from(x);
                let mut color = [0.0; 3];
                let mut count = 0_u16;
                for row in y.saturating_sub(1)..=(y + 1).min(127) {
                    for column in x.saturating_sub(1)..=(x + 1).min(127) {
                        let sample = usize::from(row) * 128 + usize::from(column);
                        if waters[index].is_none() || waters[sample].is_some() {
                            let sample_color = waters[index].map_or_else(
                                || samples[sample],
                                |level| {
                                    height_color(water_color, heights[sample], level, level, low)
                                },
                            );
                            for (target, value) in color.iter_mut().zip(sample_color) {
                                *target += value;
                            }
                            count += 1;
                        }
                    }
                }
                let index = (usize::from(127 - y) * 128 + usize::from(x)) * 4;
                for (channel, value) in color.into_iter().enumerate() {
                    pixels[index + channel] = byte(value / f32::from(count.max(1)) * 255.0);
                }
                pixels[index + 3] = 255;
            }
        }
        Ok((pixels, average))
    }

    fn radar_tile_colors(
        &self,
        assets: &VirtualAssets,
    ) -> Result<BTreeMap<u16, [u8; 3]>, Box<dyn Error>> {
        let definitions = content::scan(assets.read("data/ini/terrain.ini")?)?;
        let mut colors = BTreeMap::new();
        for class in &self.tiles.texture_classes {
            let definition = definitions
                .declarations
                .iter()
                .find(|item| item.kind == "Terrain" && item.name == class.name)
                .ok_or("radar terrain class missing")?;
            let name = crate::skirmish_data::text(&definition.lines, 0, "Texture")?;
            let image = source_texture::load_terrain(
                assets,
                &format!("art/terrain/{name}"),
                u32::from(class.width) * 64,
            )?;
            let bytes =
                rgba_pixels(&image).map_err(|error| format!("radar terrain {name}: {error}"))?;
            let extent = usize::from(class.width) * 64;
            for tile in 0..class.tile_count {
                let left = usize::from(tile % class.width) * 64;
                let bottom = usize::from(tile / class.width) * 64;
                let mut pixels = Vec::with_capacity(64 * 64);
                for y in 0..64 {
                    for x in 0..64 {
                        let index = ((extent - 1 - bottom - y) * extent + left + x) * 4;
                        pixels.push([bytes[index], bytes[index + 1], bytes[index + 2]]);
                    }
                }
                colors.insert(class.first_tile + tile, mip(pixels));
            }
        }
        Ok(colors)
    }
}

fn rgba_pixels(image: &bevy::prelude::Image) -> Result<&[u8], &'static str> {
    if !matches!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
    ) {
        return Err("original terrain decoder did not produce RGBA8 pixels");
    }
    // load_terrain already decoded the TGA; preserve encoded channel values.
    // Image::convert does not accept Rgba8Unorm as an output format.
    image.data.as_deref().ok_or("terrain lacks CPU pixels")
}

fn mip(mut pixels: Vec<[u8; 3]>) -> [u8; 3] {
    let mut width = 64;
    while width > 1 {
        let mut next = Vec::with_capacity(pixels.len() / 4);
        for y in (0..width).step_by(2) {
            for x in (0..width).step_by(2) {
                let index = y * width + x;
                next.push(std::array::from_fn(|channel| {
                    let sum = [index, index + 1, index + width, index + width + 1]
                        .map(|index| u16::from(pixels[index][channel]))
                        .into_iter()
                        .sum::<u16>();
                    u8::try_from((sum + 2) / 4).expect("four byte mip")
                }));
            }
        }
        pixels = next;
        width /= 2;
    }
    pixels[0]
}

fn load_water_color(assets: &VirtualAssets) -> Result<[f32; 3], Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/water.ini")?)?;
    let line = file
        .declarations
        .iter()
        .flat_map(|item| &item.lines)
        .rev()
        .find(|line| {
            line.tokens
                .first()
                .is_some_and(|key| key == "RadarWaterColor")
        })
        .ok_or("missing source RadarWaterColor")?;
    let mut color = [0.0; 3];
    for (target, prefix) in color.iter_mut().zip(["R:", "G:", "B:"]) {
        *target = f32::from(
            line.tokens
                .iter()
                .find_map(|value| value.strip_prefix(prefix))
                .ok_or("invalid radar water color")?
                .parse::<u8>()?,
        ) / 255.0;
    }
    Ok(color)
}

fn height_color(
    color: [f32; 3],
    height: f32,
    mut high: f32,
    middle: f32,
    mut low: f32,
) -> [f32; 3] {
    if high.to_bits() == middle.to_bits() {
        high = middle + 0.1;
    }
    if middle.to_bits() == low.to_bits() {
        low = middle - 0.1;
    }
    if high.to_bits() == low.to_bits() {
        high = low + 0.2;
    }
    let (ratio, target) = if height >= middle {
        (
            (height - middle) / (high - middle),
            color.map(|value| {
                let increment = (1.0 - value) * 0.95;
                value + increment
            }),
        )
    } else {
        (
            (middle - height) / (middle - low),
            color.map(|value| {
                let increment = -value * 0.60;
                value + increment
            }),
        )
    };
    std::array::from_fn(|index| {
        let increment = (target[index] - color[index]) * ratio;
        (color[index] + increment).clamp(0.0, 1.0)
    })
}

fn byte(value: f32) -> u8 {
    let (mut low, mut high) = (0_u8, 255_u8);
    while low < high {
        let middle = low + (high - low) / 2 + 1;
        if f32::from(middle) <= value {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_radar_uses_four_corner_height_and_original_height_color_operation_order() {
        let mut terrain = super::super::tests::terrain().unwrap();
        terrain.heights[0] = 160;
        let geometry = generals_sim::terrain::BridgeGeometry {
            from: [0.0, 0.0, 10.0],
            to: [100.0, 0.0, 70.0],
            corners: [
                [0.0, 10.0, 0.0],
                [0.0, -10.0, 20.0],
                [100.0, 10.0, 60.0],
                [100.0, -10.0, 80.0],
            ],
        };
        // Original call order is average=hiZ, map-high=midZ, map-low=loZ.
        // Four-corner mean40, bounds0..100, average25 produce this native float.
        let color = terrain.radar_bridge_color(&geometry, [192.0 / 255.0; 3], 25.0);
        assert_eq!(color.map(f32::to_bits), [0x3ef6_b948; 3]);
    }

    #[test]
    fn radar_reads_original_decoded_tga_bytes_without_unsupported_format_conversion() {
        use bevy::{
            asset::RenderAssetUsages,
            image::{CompressedImageFormats, ImageSampler, ImageType},
            prelude::Image,
        };
        let mut tga = vec![0; 18];
        tga[2] = 2;
        tga[12] = 2;
        tga[14] = 2;
        tga[16] = 24;
        for _ in 0..4 {
            tga.extend_from_slice(&[30, 20, 10]);
        }
        let image = Image::from_buffer(
            &tga,
            ImageType::Extension("tga"),
            CompressedImageFormats::NONE,
            false,
            ImageSampler::nearest(),
            RenderAssetUsages::default(),
        )
        .unwrap();
        assert!(matches!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8Unorm
        ));
        assert_eq!(rgba_pixels(&image).unwrap(), [10, 20, 30, 255].repeat(4));
    }

    #[test]
    fn native_tile_mips_round_each_level_not_only_the_final_average() {
        let mut pixels = vec![[0; 3]; 64 * 64];
        for y in (0..64).step_by(2) {
            for x in (0..64).step_by(2) {
                pixels[y * 64 + x] = [2; 3];
            }
        }
        assert_eq!(mip(pixels), [1; 3]);
    }
}
