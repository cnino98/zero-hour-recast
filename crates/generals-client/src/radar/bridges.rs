//! Native bridge-center radar colors and queued terrain refresh.
use super::Radar;
use crate::terrain::Terrain;
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::Skirmish;
use std::error::Error;

struct Bridge {
    layer: u8,
    color: [f32; 3],
    pixels: Vec<(usize, [u8; 3])>,
    working: bool,
}

pub(super) struct Paint {
    bridges: Vec<Bridge>,
    pub(super) pixels: Vec<u8>,
    generation: Option<u32>,
}

impl Paint {
    pub(super) fn new() -> Self {
        Self {
            bridges: Vec::new(),
            pixels: vec![0; 128 * 128 * 4],
            generation: None,
        }
    }

    fn refresh(&mut self, game: &Skirmish) {
        for bridge in &mut self.bridges {
            bridge.working = game.radar_bridge_working(bridge.layer);
        }
        self.paint();
    }

    pub(super) fn terrain_color(&self, base: &[u8], index: usize) -> [u8; 3] {
        let pixels = if self.pixels[index + 3] == 0 {
            base
        } else {
            &self.pixels
        };
        [pixels[index], pixels[index + 1], pixels[index + 2]]
    }

    fn paint(&mut self) {
        self.pixels.fill(0);
        for bridge in &self.bridges {
            if bridge.working {
                for &(index, color) in &bridge.pixels {
                    self.pixels[index..index + 3].copy_from_slice(&color);
                    self.pixels[index + 3] = 255;
                }
            }
        }
    }
}

impl Radar {
    pub(crate) fn configure_bridges(
        &mut self,
        assets: &VirtualAssets,
        terrain: &Terrain,
        game: &Skirmish,
        generation: u32,
    ) -> Result<(), Box<dyn Error>> {
        let colors = crate::world_data::bridge_radar_colors(assets)?;
        let mut paint = Paint::new();
        for y in 0..128_u16 {
            for x in 0..128_u16 {
                let center = [
                    f32::from(x) * self.world.x / 128.0,
                    f32::from(y) * self.world.y / 128.0,
                ];
                let Some((layer, name, geometry)) = game.radar_bridge_at(center) else {
                    continue;
                };
                let group = if let Some(index) = paint
                    .bridges
                    .iter()
                    .position(|bridge| bridge.layer == layer)
                {
                    index
                } else {
                    let color = *colors
                        .get(name)
                        .ok_or_else(|| format!("Missing radar Bridge template {name}"))?;
                    paint.bridges.push(Bridge {
                        layer,
                        color: terrain.radar_bridge_color(geometry, color, self.average_z),
                        pixels: Vec::new(),
                        working: game.radar_bridge_working(layer),
                    });
                    paint.bridges.len() - 1
                };
                let index = (usize::from(127 - y) * 128 + usize::from(x)) * 4;
                let color = pixel_color(paint.bridges[group].color, [x, y]);
                paint.bridges[group].pixels.push((index, color));
            }
        }
        paint.paint();
        paint.generation = Some(generation);
        self.bridges = paint;
        Ok(())
    }

    pub(super) fn refresh_bridge_terrain(&mut self, game: &mut Skirmish, generation: u32) {
        let due = game.take_radar_terrain_refresh();
        if self.bridges.generation != Some(generation) || due {
            self.bridges.generation = Some(generation);
            self.bridges.refresh(game);
        }
    }
}

fn pixel_color(color: [f32; 3], [x, y]: [u16; 2]) -> [u8; 3] {
    // Every valid neighboring sample uses the chosen center bridge's same color.
    // Keep repeated addition and final division: multiplication changes rounding.
    let mut sample = [0.0; 3];
    let mut count = 0_u16;
    for _row in y.saturating_sub(1)..=(y + 1).min(127) {
        for _column in x.saturating_sub(1)..=(x + 1).min(127) {
            for (sum, value) in sample.iter_mut().zip(color) {
                *sum += value;
            }
            count += 1;
        }
    }
    sample.map(|value| super::byte(value / f32::from(count) * 255.0))
}

#[cfg(test)]
mod tests;
