//! `W3DScorch::writeScorchToBuffer`: raw grid heights, atlas projection and source caps.
use super::Terrain;
use crate::scorch::Mark;
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use std::collections::VecDeque;

#[cfg(test)]
mod tests;

const MAX_VERTICES: usize = 32_768;
const MAX_INDICES: usize = 65_535;

struct Geometry {
    positions: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    indices: Vec<u16>,
}

impl Terrain {
    pub(crate) fn scorch_mesh(&self, marks: &VecDeque<Mark>, color: [f32; 4]) -> Mesh {
        let mut geometry = Geometry {
            positions: Vec::new(),
            uv: Vec::new(),
            indices: Vec::new(),
        };
        for mark in marks.iter().rev() {
            if !geometry.append(self, mark) {
                break;
            }
        }
        let count = geometry.positions.len();
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, geometry.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, geometry.uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![color; count])
        .with_inserted_indices(Indices::U16(geometry.indices))
    }

    fn scorch_flip(&self, x: u16, y: u16) -> bool {
        let index = self.index(x, y);
        let primary = self.tiles.blends[usize::from(self.tiles.blend_tiles[index])].as_ref();
        let tile = primary.map_or(self.tiles.base_tiles[index], |blend| blend.tile);
        let stretched = self
            .tiles
            .texture_uv(index, tile, &self.heights, self.adjust_cliff_textures)
            .expect("validated terrain cell")
            .stretched;
        self.primary_flip(x, y, stretched)
    }
}

impl Geometry {
    fn append(&mut self, terrain: &Terrain, mark: &Mark) -> bool {
        let [min_x, max_x] = bounds(
            mark.position[0],
            mark.radius,
            terrain.border,
            terrain.raw_width,
        );
        let [min_y, max_y] = bounds(
            mark.position[1],
            mark.radius,
            terrain.border,
            terrain.raw_height,
        );
        let width = max_x - min_x;
        let height = max_y - min_y;
        if width <= 0 || height <= 0 {
            return true;
        }
        let vertices = usize::try_from(width * height).expect("bounded scorch vertices");
        let indices =
            usize::try_from(6 * (width - 1) * (height - 1)).expect("bounded scorch indices");
        if self.positions.len() + vertices > MAX_VERTICES
            || self.indices.len() + indices > MAX_INDICES
        {
            return false;
        }
        let start = u16::try_from(self.positions.len()).expect("bounded vertex index");
        let stride = u16::try_from(width).expect("bounded scorch width");
        for y in min_y..max_y {
            for x in min_x..max_x {
                self.vertex(terrain, mark, [x, y]);
            }
        }
        // The orbital beam creates a zero-radius mark before its first growth update.
        // Retain its native buffer budget, but submit no area to the GPU.
        if mark.radius == 0.0 {
            self.indices.resize(self.indices.len() + indices, start);
            return true;
        }
        for y in min_y..max_y - 1 {
            for x in min_x..max_x - 1 {
                let raw = raw_cell(terrain, [x, y]);
                let first = start
                    + u16::try_from(y - min_y).expect("cell row") * stride
                    + u16::try_from(x - min_x).expect("cell column");
                let offsets = if terrain.scorch_flip(raw[0], raw[1]) {
                    [1, stride, 0, 1, 1 + stride, stride]
                } else {
                    [0, 1 + stride, stride, 0, 1, 1 + stride]
                };
                self.indices.extend(offsets.map(|offset| first + offset));
            }
        }
        true
    }

    fn vertex(&mut self, terrain: &Terrain, mark: &Mark, cell: [i32; 2]) {
        let raw = raw_cell(terrain, cell);
        let position =
            cell.map(|value| f32::from(i16::try_from(value).expect("bounded terrain cell")) * 10.0);
        let height = terrain.raw_height(raw[0], raw[1]).mul_add(0.625, 0.0625);
        self.positions
            .push([position[0] * 0.1, height * 0.1, -position[1] * 0.1]);
        if mark.radius == 0.0 {
            self.uv.push([0.0; 2]);
            return;
        }
        let offset = [mark.kind % 3, mark.kind / 3]
            .map(|value| f32::from(i16::try_from(value).expect("atlas index")) * 1.5);
        let uv = std::array::from_fn(|axis| {
            (offset[axis] + 0.5 + (position[axis] - mark.position[axis]) / (2.0 * mark.radius))
                / 4.0
        });
        self.uv.push(uv);
    }
}

fn bounds(position: f32, radius: f32, border: u16, extent: u16) -> [i32; 2] {
    let edges = Vec2::new(
        ((position - radius) / 10.0).floor(),
        ((position + radius) / 10.0).ceil(),
    )
    .as_ivec2();
    let minimum = edges.x;
    let maximum = edges.y;
    [
        minimum.max(-i32::from(border)),
        (maximum + 1).min(i32::from(extent - border)),
    ]
}

fn raw_cell(terrain: &Terrain, cell: [i32; 2]) -> [u16; 2] {
    cell.map(|value| {
        u16::try_from(value + i32::from(terrain.border)).expect("clipped terrain cell")
    })
}
