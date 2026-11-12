//! `Pathfinder::getMoveAwayFromPath` and `TerrainLogic::LineInRegion`.
use super::{Cell, Grid, RouteUnits, search};
use std::collections::BTreeSet;

pub struct Avoidance<'a> {
    pub start: Cell,
    pub indexed_start: Cell,
    pub radius: i32,
    pub centered: bool,
    pub half_width: f32,
    pub paths: [&'a [[f32; 2]]; 2],
    pub unavailable: &'a BTreeSet<Cell>,
}

pub fn find_counted(grid: &Grid, units: &RouteUnits, query: &Avoidance<'_>) -> super::SearchResult {
    search::away_counted(
        grid,
        query.start,
        query.indexed_start,
        query.radius,
        query.centered,
        units,
        |cell| {
            if cell == query.indexed_start
                || query.unavailable.contains(&cell)
                || !grid.destination_clear(cell, query.radius, query.centered)
            {
                return false;
            }
            let offset = if query.centered { 0.5 } else { 0.05 };
            let center = [
                (f32::from(cell.x) + offset) * 10.0,
                (f32::from(cell.y) + offset) * 10.0,
            ];
            let low = center.map(|coordinate| coordinate - query.half_width);
            let high = center.map(|coordinate| coordinate + query.half_width);
            !query.paths.iter().any(|path| {
                path.windows(2)
                    .any(|pair| intersects(pair[0], pair[1], low, high))
            })
        },
    )
}

fn clip_code(point: [f32; 2], low: [f32; 2], high: [f32; 2]) -> u8 {
    let mut code = 0;
    if point[0] < low[0] {
        code = 1;
    } else if point[0] > high[0] {
        code = 2;
    }
    if point[1] < low[1] {
        code |= 8;
    } else if point[1] > high[1] {
        code |= 4;
    }
    code
}

// Preserve the source endpoint order and intermediate rounding. This is not
// replaced by a mathematically equivalent slab/intersection implementation.
pub(super) fn intersects(
    mut first: [f32; 2],
    mut second: [f32; 2],
    low: [f32; 2],
    high: [f32; 2],
) -> bool {
    let first_code = clip_code(first, low, high);
    let second_code = clip_code(second, low, high);
    if first_code | second_code == 0 {
        return true;
    }
    if first_code & second_code != 0 {
        return false;
    }
    if first_code != 0 {
        if first_code & 0x0c != 0 {
            let boundary = if first_code & 8 != 0 { low[1] } else { high[1] };
            let difference = second[1] - first[1];
            if difference == 0.0 {
                return false;
            }
            let displacement = (second[0] - first[0]) * (boundary - first[1]) / difference;
            first[0] += displacement;
            first[1] = boundary;
        }
        if first[0] > high[0] || first[0] < low[0] {
            let boundary = if first[0] > high[0] { high[0] } else { low[0] };
            let difference = second[0] - first[0];
            if difference == 0.0 {
                return false;
            }
            let displacement = (second[1] - first[1]) * (boundary - first[0]) / difference;
            first[1] += displacement;
            first[0] = boundary;
        }
    }
    if second_code != 0 {
        if second_code & 0x0c != 0 {
            let boundary = if second_code & 8 != 0 {
                low[1]
            } else {
                high[1]
            };
            let difference = second[1] - first[1];
            if difference == 0.0 {
                return false;
            }
            let displacement = (second[0] - first[0]) * (boundary - second[1]) / difference;
            second[0] += displacement;
            second[1] = boundary;
        }
        if second[0] > high[0] || second[0] < low[0] {
            let boundary = if second[0] > high[0] { high[0] } else { low[0] };
            let difference = second[0] - first[0];
            if difference == 0.0 {
                return false;
            }
            let displacement = (second[1] - first[1]) * (boundary - second[0]) / difference;
            second[1] += displacement;
            second[0] = boundary;
        }
    }
    first
        .into_iter()
        .zip(low)
        .zip(high)
        .all(|((value, low), high)| value >= low && value <= high)
        && second
            .into_iter()
            .zip(low)
            .zip(high)
            .all(|((value, low), high)| value >= low && value <= high)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corridor_boundary_contact_is_an_overlap() {
        assert!(intersects([-10.0, 5.0], [10.0, 5.0], [-5.0; 2], [5.0; 2]));
        assert!(!intersects(
            [-10.0, 5.001],
            [10.0, 5.001],
            [-5.0; 2],
            [5.0; 2]
        ));
        assert!(intersects([8.0, 8.0], [0.0, 0.0], [-5.0; 2], [5.0; 2]));
        assert!(!intersects([8.0, 8.0], [5.1, 5.1], [-5.0; 2], [5.0; 2]));
    }

    #[test]
    fn move_aside_clears_both_routes_and_does_not_choose_occupied_goal() {
        let grid = Grid::new(20, 20, []).unwrap();
        let unavailable = BTreeSet::from([Cell { x: 10, y: 12 }]);
        let first = [[0.0, 105.0], [195.0, 105.0]];
        let second = [[105.0, 0.0], [105.0, 195.0]];
        let query = Avoidance {
            start: Cell { x: 10, y: 10 },
            indexed_start: Cell { x: 10, y: 10 },
            radius: 0,
            centered: true,
            half_width: 7.5,
            paths: [&first, &second],
            unavailable: &unavailable,
        };
        let path = find_counted(&grid, &RouteUnits::new(), &query)
            .route
            .expect("room outside both corridors");
        let goal = *path.optimized.back().unwrap();
        assert!(goal.cell.x != 10 && goal.cell.y != 10);
        assert!(!unavailable.contains(&goal.cell));
    }
    #[test]
    fn an_existing_cell_info_keeps_its_original_index_when_start_is_shifted() {
        let grid = Grid::new(20, 20, []).unwrap();
        let unavailable = BTreeSet::new();
        let query = Avoidance {
            start: Cell { x: 5, y: 5 },
            indexed_start: Cell { x: 6, y: 5 },
            radius: 1,
            centered: false,
            half_width: 15.0,
            paths: [&[], &[]],
            unavailable: &unavailable,
        };
        let mut units = RouteUnits::new();
        let unallocated = find_counted(&grid, &units, &query).route.unwrap();
        assert!(unallocated.raw.len() > 1);
        units.info_cells.insert(query.start);
        let allocated = find_counted(&grid, &units, &query).route.unwrap();
        assert!(
            allocated.raw
                == [crate::grid::LayerCell {
                    cell: query.start,
                    layer: 1
                }]
        );
    }
}
