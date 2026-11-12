//! Original shoreline tiles use the terrain renderer's actual diagonal.
use super::Terrain;

impl Terrain {
    pub(crate) fn shoreline_cells(&self) -> Vec<([[f32; 3]; 4], bool)> {
        let mut result = Vec::new();
        for y in 0..self.raw_height - 1 {
            for x in 0..self.raw_width - 1 {
                let index = self.index(x, y);
                let primary =
                    self.tiles.blends[usize::from(self.tiles.blend_tiles[index])].as_ref();
                let tile = primary.map_or(self.tiles.base_tiles[index], |blend| blend.tile);
                let uv = self
                    .tiles
                    .texture_uv(index, tile, &self.heights, self.adjust_cliff_textures)
                    .expect("validated terrain cell");
                let positions = self
                    .corners(x, y)
                    .map(|point| [point[0] * 10.0, -point[2] * 10.0, point[1] * 10.0]);
                result.push((positions, self.primary_flip(x, y, uv.stretched)));
            }
        }
        result
    }
}
