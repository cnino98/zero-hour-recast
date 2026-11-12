//! SH `PathfindLayer` wall allocation/classification and `Pathfinder` piece IDs.
use super::{BridgeCell, Cell, CellType, TerrainMap};
use crate::{collision_geometry::Shape, skirmish::Footprint};
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

pub(super) struct Layer {
    pieces: Vec<Option<usize>>,
    pub(super) height: f32,
    pub(super) bounds: Option<[Cell; 2]>,
    pub(super) cells: BTreeMap<Cell, BridgeCell>,
}

impl Layer {
    pub(super) const fn new() -> Self {
        Self {
            pieces: Vec::new(),
            height: 0.0,
            bounds: None,
            cells: BTreeMap::new(),
        }
    }
}

impl TerrainMap {
    pub(crate) fn wall_piece_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.walls.pieces.iter().flatten().copied()
    }

    pub(crate) fn add_wall_piece(&mut self, id: usize) {
        // MAX_WALL_PIECES is 128; the native insertion reserves the last slot.
        if self.walls.pieces.len() < 127 {
            self.walls.pieces.push(Some(id));
        }
    }

    pub(crate) fn remove_wall_piece(&mut self, id: usize) {
        if let Some(index) = self
            .walls
            .pieces
            .iter()
            .position(|piece| *piece == Some(id))
        {
            self.walls.pieces.swap_remove(index);
        }
    }

    pub(crate) fn invalidate_wall_piece(&mut self, id: usize) {
        for piece in &mut self.walls.pieces {
            if *piece == Some(id) {
                *piece = None;
            }
        }
    }

    pub(crate) const fn wall_height(&self) -> f32 {
        self.walls.height
    }

    pub(crate) fn is_point_on_wall(&self, point: [f32; 2]) -> bool {
        if self.walls.pieces.is_empty() {
            return false;
        }
        let Some(x) = (point[0] / 10.0).floor().to_u16() else {
            return false;
        };
        let Some(y) = (point[1] / 10.0).floor().to_u16() else {
            return false;
        };
        self.walls
            .cells
            .get(&Cell { x, y })
            .is_some_and(|cell| matches!(cell.kind, CellType::Clear))
    }

    pub(crate) fn allocate_wall_layer(&mut self, height: f32, pieces: &[Footprint]) {
        self.walls.height = height;
        self.walls.bounds = None;
        self.walls.cells.clear();
        let Some(first) = pieces.first() else {
            return;
        };
        let mut region = bounds(first);
        for piece in &pieces[1..] {
            let next = bounds(piece);
            for axis in 0..2 {
                region[0][axis] = region[0][axis].min(next[0][axis]);
                region[1][axis] = region[1][axis].max(next[1][axis]);
            }
        }
        let origin = region[0].map(|value| (((value - 0.1) / 10.0).floor() - 1.0).max(0.0));
        let maximum: [f32; 2] = std::array::from_fn(|axis| {
            let limit = if axis == 0 { self.width } else { self.height };
            (((region[1][axis] + 0.1) / 10.0).ceil() + 1.0).min(f32::from(limit - 1))
        });
        if maximum[0] <= origin[0] || maximum[1] <= origin[1] {
            return;
        }
        let [x, y] = origin.map(|value| value.to_u16().expect("bounded wall origin"));
        let [end_x, end_y] = maximum.map(|value| value.to_u16().expect("bounded wall extent"));
        self.walls.bounds = Some([Cell { x, y }, Cell { x: end_x, y: end_y }]);
        self.classify_wall_cells(pieces);
    }

    pub(crate) fn classify_wall_cells(&mut self, pieces: &[Footprint]) {
        let Some([origin, maximum]) = self.walls.bounds else {
            return;
        };
        for x in origin.x..maximum.x {
            for y in origin.y..maximum.y {
                let left = f32::from(x) * 10.0;
                let top = f32::from(y) * 10.0;
                let right = left + 10.0;
                let bottom = top + 10.0;
                let count = [[left, top], [left, bottom], [right, bottom], [right, top]]
                    .into_iter()
                    .filter(|point| pieces.iter().any(|piece| contains(piece, *point)))
                    .count();
                let kind = match count {
                    4 => CellType::Clear,
                    0 => CellType::Impassable,
                    _ => CellType::BridgeImpassable,
                };
                self.walls.cells.insert(
                    Cell { x, y },
                    BridgeCell {
                        kind,
                        connect_ground: false,
                    },
                );
            }
        }
    }
}

/// Unlike a footprint circle test, wall spheres use their enclosing square.
pub fn contains(piece: &Footprint, point: [f32; 2]) -> bool {
    let minor = if matches!(piece.shape, Shape::Sphere) {
        piece.major
    } else {
        piece.minor
    };
    let cosine = (-piece.angle).cos();
    let sine = (-piece.angle).sin();
    let delta = [point[0] - piece.position[0], point[1] - piece.position[1]];
    let products = [
        delta[0] * cosine,
        delta[1] * sine,
        delta[0] * sine,
        delta[1] * cosine,
    ];
    (products[0] - products[1]).abs() <= piece.major && (products[2] + products[3]).abs() <= minor
}

fn bounds(piece: &Footprint) -> [[f32; 2]; 2] {
    let [x, y] = piece.position;
    if !matches!(piece.shape, Shape::Box) {
        return [
            [x - piece.major, y - piece.major],
            [x + piece.major, y + piece.major],
        ];
    }
    let cosine = piece.angle.cos();
    let sine = piece.angle.sin();
    let major_cosine = piece.major * cosine;
    let minor_cosine = piece.minor * cosine;
    let major_sine = piece.major * sine;
    let minor_sine = piece.minor * sine;
    let corners = [
        [x - major_cosine - minor_sine, y + minor_cosine - major_sine],
        [x + major_cosine - minor_sine, y + minor_cosine + major_sine],
        [x + major_cosine + minor_sine, y - minor_cosine + major_sine],
        [x - major_cosine + minor_sine, y - minor_cosine - major_sine],
    ];
    let mut result = [corners[0]; 2];
    for corner in &corners[1..] {
        for axis in 0..2 {
            result[0][axis] = result[0][axis].min(corner[axis]);
            result[1][axis] = result[1][axis].max(corner[axis]);
        }
    }
    result
}

#[cfg(test)]
mod tests;
