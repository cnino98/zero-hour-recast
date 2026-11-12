//! `WorldHeightMap::getUVForNdx/getUVForTileIndex`, using texture-class local
//! coordinates instead of the legacy 2048-wide packed atlas. The final affine
//! coordinate conversion preserves the source tile quadrants and cliff stretch.

use super::TerrainTiles;

pub struct TextureUv {
    pub texture_class: usize,
    pub corners: [[f32; 2]; 4],
    pub stretched: bool,
}

impl TerrainTiles {
    /// Return the four source UV corners for a raw sample-array index.
    /// Returns `None` for an out-of-bounds cell or unmatched texture class.
    #[must_use]
    pub fn texture_uv(
        &self,
        index: usize,
        tile: u16,
        heights: &[u16],
        adjust_cliffs: bool,
    ) -> Option<TextureUv> {
        let width = usize::try_from(self.width).ok()?;
        if index % width + 1 >= width || index.checked_add(width + 1)? >= heights.len() {
            return None;
        }
        let texture_class = self.class_for_tile(tile)?;
        let class = &self.texture_classes[texture_class];
        let local_tile = (tile >> 2) - class.first_tile;
        let class_width = f32::from(class.width);
        let mut min_u = f32::from(local_tile % class.width) / class_width;
        let mut max_v = 1.0 - f32::from(local_tile / class.width) / class_width;
        let mut max_u = min_u + 1.0 / class_width;
        let mut min_v = max_v - 1.0 / class_width;
        if tile & 1 != 0 {
            min_u = f32::midpoint(min_u, max_u);
        } else {
            max_u = f32::midpoint(min_u, max_u);
        }
        if tile & 2 != 0 {
            max_v = f32::midpoint(min_v, max_v);
        } else {
            min_v = f32::midpoint(min_v, max_v);
        }
        let mut corners = [
            [min_u, max_v],
            [max_u, max_v],
            [max_u, min_v],
            [min_u, min_v],
        ];
        if !adjust_cliffs {
            return Some(TextureUv {
                texture_class,
                corners,
                stretched: false,
            });
        }
        let cliff = self
            .cliff_tiles
            .get(index)
            .and_then(|&id| self.cliffs.get(usize::from(id)))
            .and_then(Option::as_ref);
        if let Some(cliff) =
            cliff.filter(|cliff| self.class_for_tile(cliff.tile) == Some(texture_class))
        {
            let scale = 32.0 / class_width; // Legacy TEXTURE_WIDTH / class pixel width.
            for (corner, stored) in corners.iter_mut().zip(cliff.uv) {
                let scaled_v = stored[1] * scale;
                *corner = [stored[0] * scale, scaled_v + 1.0];
            }
            return Some(TextureUv {
                texture_class,
                corners,
                stretched: cliff.flip,
            });
        }
        let samples = [
            heights[index],
            heights[index + 1],
            heights[index + width + 1],
            heights[index + width],
        ];
        let stretched = stretch_cliff(&mut corners, samples);
        Some(TextureUv {
            texture_class,
            corners,
            stretched,
        })
    }
}

fn stretch_cliff(uv: &mut [[f32; 2]; 4], heights: [u16; 4]) -> bool {
    // Source DO_OLD_UV remains enabled at the pinned revision; its thresholds
    // are measured in terrain-cell lengths (raw height * 0.625 / 10).
    let minimum = *heights.iter().min().expect("four corner heights");
    let maximum = *heights.iter().max().expect("four corner heights");
    let delta = maximum - minimum;
    let stretch = f32::from(delta) * 0.0625;
    if stretch < 1.5 {
        return false;
    }
    let below_limit = minimum + (2 * delta + 1) / 3;
    let above_limit = minimum + (delta + 1) / 3;
    let below = heights
        .iter()
        .filter(|&&height| height < below_limit)
        .count();
    let above = heights
        .iter()
        .filter(|&&height| height > above_limit)
        .count();
    if above != 1 && below != 1 && (above != 2 || below != 2) && stretch < 2.4 {
        return false;
    }
    let divisor = (4.0 / stretch).clamp(1.0, 4.0);
    let min_v = uv[2][1];
    let max_v = uv[0][1];
    if below == 1 || above > below {
        adjust_peak(uv, heights, minimum, min_v, max_v, divisor);
    } else if above == 1 || below > above {
        adjust_peak(uv, heights, maximum, min_v, max_v, divisor);
    } else {
        if stretch < 2.0 {
            return false;
        }
        adjust_side(uv, heights);
    }
    let raise_v = uv.iter().map(|point| -point[1]).fold(0.0_f32, f32::max);
    for point in &mut *uv {
        point[1] += raise_v;
    }
    let lower_u = uv
        .iter()
        .map(|point| point[0] - 1.0)
        .fold(0.0_f32, f32::max);
    let lower_v = uv
        .iter()
        .map(|point| point[1] - 1.0)
        .fold(0.0_f32, f32::max);
    for point in uv {
        point[0] -= lower_u;
        point[1] -= lower_v;
    }
    true
}

fn adjust_peak(
    uv: &mut [[f32; 2]; 4],
    heights: [u16; 4],
    extreme: u16,
    min_v: f32,
    max_v: f32,
    divisor: f32,
) {
    if let Some(index) = heights.iter().position(|&height| height == extreme) {
        uv[index][1] = if index < 2 {
            min_v + 1.0 / divisor
        } else {
            max_v - 1.0 / divisor
        };
    }
}

fn edge_stretch(a: u16, b: u16) -> f32 {
    let slope = (f32::from(a) - f32::from(b)) * 0.0625;
    let slope_squared = slope * slope;
    let length = (1.0 + slope_squared).sqrt();
    if length < 1.5 { 1.0 } else { length.min(4.0) }
}

fn adjust_side(uv: &mut [[f32; 2]; 4], heights: [u16; 4]) {
    let [h0, h1, h2, h3] = heights;
    let min_u = uv[0][0];
    let min_v = uv[2][1];
    let delta_u = uv[1][0] - min_u;
    let delta_v = uv[0][1] - min_v;
    let dx = edge_stretch(h3, h2) * delta_u;
    let dy = edge_stretch(h3, h0) * delta_v;
    *uv = [
        [min_u, min_v + dy],
        [min_u + dx, min_v + dy],
        [min_u + dx, min_v],
        [min_u, min_v],
    ];
    let horizontal_length = edge_stretch(h1, h0) * delta_u;
    let vertical_length = edge_stretch(h2, h1) * delta_v;
    uv[1][0] = min_u + horizontal_length;
    uv[1][1] = min_v + vertical_length;
}
