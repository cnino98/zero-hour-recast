//! `internal_classifyObjectFootprint`: local post-zone removal and pinched flags.
use super::{BTreeSet, Cell, Claims, Grid, LayerCell};
use crate::{
    grid::ZoneCellChange,
    terrain::{CellType, TerrainMap},
};
use num_traits::ToPrimitive;

pub(super) fn bounds(position: [f32; 2], sampled: &BTreeSet<Cell>, fence: bool) -> [[i32; 2]; 2] {
    let center = position.map(|coordinate| (coordinate + 0.5) / 10.0);
    let low = center.map(|coordinate| coordinate.floor().to_i32().expect("native map coordinate"));
    let high = if fence {
        center.map(|coordinate| coordinate.ceil().to_i32().expect("native map coordinate"))
    } else {
        low
    };
    let mut bounds = [low, high];
    for cell in sampled {
        bounds[0][0] = bounds[0][0].min(i32::from(cell.x));
        bounds[0][1] = bounds[0][1].min(i32::from(cell.y));
        bounds[1][0] = bounds[1][0].max(i32::from(cell.x));
        bounds[1][1] = bounds[1][1].max(i32::from(cell.y));
    }
    bounds
}

pub(super) fn finish(
    claims: &Claims,
    terrain: &mut TerrainMap,
    grid: &Grid,
    bounds: [[i32; 2]; 2],
    removing: bool,
) -> (Vec<ZoneCellChange>, Vec<(Cell, bool)>) {
    let low = bounds[0].map(|coordinate| (coordinate - 2).max(0));
    let high = [
        (bounds[1][0] + 2).min(i32::from(grid.width()) - 1),
        (bounds[1][1] + 2).min(i32::from(grid.height()) - 1),
    ];
    let mut changes = Vec::new();
    if removing {
        for y in low[1]..=high[1] {
            for x in low[0]..=high[0] {
                let cell = cell(x, y);
                if matches!(kind(claims, terrain, cell), CellType::Impassable) {
                    terrain.clear_path_obstacle(cell);
                    changes.push(ZoneCellChange {
                        cell,
                        kind: CellType::Clear,
                        fence: false,
                        reset_zone: false,
                    });
                }
            }
        }
    }
    let mut pinched = Vec::new();
    for y in low[1]..=high[1] {
        for x in low[0]..=high[0] {
            let cell = cell(x, y);
            let pinched_here = matches!(kind(claims, terrain, cell), CellType::Clear)
                && is_pinched(claims, terrain, grid, x, y);
            terrain.set_path_pinched(cell, pinched_here);
            pinched.push((cell, pinched_here));
        }
    }
    (changes, pinched)
}

fn is_pinched(claims: &Claims, terrain: &TerrainMap, grid: &Grid, x: i32, y: i32) -> bool {
    let mut orthogonal = 0;
    let mut total = 0;
    let mut obstacle_adjacent = false;
    for adjacent_x in (x - 1).max(0)..=(x + 1).min(i32::from(grid.width()) - 1) {
        for adjacent_y in (y - 1).max(0)..=(y + 1).min(i32::from(grid.height()) - 1) {
            if adjacent_x == x && adjacent_y == y {
                continue;
            }
            let adjacent = kind(claims, terrain, cell(adjacent_x, adjacent_y));
            if matches!(adjacent, CellType::Clear) {
                total += 1;
                if adjacent_x == x || adjacent_y == y {
                    orthogonal += 1;
                }
            }
            if (adjacent_x == x || adjacent_y == y) && matches!(adjacent, CellType::Obstacle) {
                obstacle_adjacent = true;
            }
        }
    }
    // The fixed profile never converts pinched cells into IMPASSABLE, so the
    // following source obstacle-expansion pass reads the same cell types.
    orthogonal < 2 || total < 4 || obstacle_adjacent
}

fn kind(claims: &Claims, terrain: &TerrainMap, cell: Cell) -> CellType {
    if claims.at(LayerCell { cell, layer: 1 }).is_some() {
        CellType::Obstacle
    } else {
        terrain.path_kind(cell).expect("bounded footprint cell")
    }
}

fn cell(x: i32, y: i32) -> Cell {
    Cell {
        x: u16::try_from(x).expect("clipped cell"),
        y: u16::try_from(y).expect("clipped cell"),
    }
}
