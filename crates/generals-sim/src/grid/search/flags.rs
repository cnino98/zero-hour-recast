//! Indexed cell metadata for one native search. Storage changes neither
//! neighbor traversal nor any cost, admission or open-list ordering.

use super::{Grid, RouteUnits};

pub(super) const BLOCKED: u8 = 1;
pub(super) const OBSTACLE: u8 = 2;
pub(super) const FIXED: u8 = 4;
pub(super) const MOVING: u8 = 8;
pub(super) const ENEMY: u8 = 16;
pub(super) const GOAL: u8 = 32;
pub(super) const TRANSIENT: u8 = 64;
pub(super) const INFO: u8 = 128;

pub(super) fn build(grid: &Grid, units: &RouteUnits) -> Vec<u8> {
    let width = usize::from(grid.width);
    let mut flags = vec![0; width * usize::from(grid.height)];
    for (cells, mask) in [
        (grid.blocked.as_ref(), BLOCKED),
        (grid.obstacles.as_ref(), OBSTACLE),
        (&units.fixed_allies, FIXED),
        (&units.moving_allies, MOVING),
        (&units.fixed_enemies, ENEMY),
        (&units.goal_flags, GOAL),
        (&units.transient_allies, TRANSIENT),
        (&units.transient_enemies, TRANSIENT),
    ] {
        for &cell in cells {
            if grid.contains(cell) {
                flags[usize::from(cell.y) * width + usize::from(cell.x)] |= mask;
            }
        }
    }
    for cell in units.retained_cells() {
        if grid.contains(cell) {
            flags[usize::from(cell.y) * width + usize::from(cell.x)] |= INFO;
        }
    }
    for &cell in grid.obstacles.iter() {
        if grid.contains(cell) {
            let index = usize::from(cell.y) * width + usize::from(cell.x);
            if grid.is_blocked(cell) {
                flags[index] |= BLOCKED;
            } else {
                flags[index] &= !BLOCKED;
            }
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::super::{Cell, Search};
    use super::*;

    #[test]
    fn indexed_flags_preserve_overlapping_native_classes_and_footprints() {
        let grid = Grid::new(8, 7, [Cell { x: 1, y: 1 }])
            .unwrap()
            .with_obstacles([Cell { x: 5, y: 4 }]);
        let mut units = RouteUnits::new();
        units
            .fixed_allies
            .extend([Cell { x: 2, y: 3 }, Cell { x: 4, y: 4 }]);
        units
            .moving_allies
            .extend([Cell { x: 2, y: 3 }, Cell { x: 3, y: 3 }]);
        units.fixed_enemies.insert(Cell { x: 4, y: 3 });
        units.goal_flags.insert(Cell { x: 2, y: 3 });
        units.transient_allies.insert(Cell { x: 5, y: 3 });
        units.transient_enemies.insert(Cell { x: 3, y: 4 });
        units
            .info_cells
            .extend([Cell { x: 2, y: 3 }, Cell { x: 9, y: 9 }]);
        for radius in 0..=2 {
            for centered in [false, true] {
                let search = Search::new(
                    &grid,
                    &units,
                    Cell { x: 0, y: 0 },
                    Cell { x: 7, y: 6 },
                    radius,
                    centered,
                    false,
                );
                for x in 0..grid.width {
                    for y in 0..grid.height {
                        let cell = Cell { x, y };
                        assert_eq!(search.has(cell, BLOCKED), grid.is_blocked(cell));
                        assert_eq!(search.has(cell, OBSTACLE), grid.obstacles.contains(&cell));
                        assert_eq!(search.has(cell, INFO), units.info_cells.contains(&cell));
                        let inside = |candidate: &Cell| {
                            let above = radius + i32::from(centered);
                            (i32::from(x) - radius..i32::from(x) + above)
                                .contains(&i32::from(candidate.x))
                                && (i32::from(y) - radius..i32::from(y) + above)
                                    .contains(&i32::from(candidate.y))
                        };
                        let occupied = search.occupied(cell);
                        assert_eq!(occupied.fixed, units.fixed_allies.iter().any(inside));
                        assert_eq!(occupied.moving, units.moving_allies.iter().any(inside));
                        assert_eq!(occupied.enemy, units.fixed_enemies.iter().any(inside));
                        assert_eq!(
                            search.goal_occupied(cell, radius, centered),
                            units.goal_flags.iter().any(inside)
                        );
                        assert_eq!(
                            search.transient_occupied(cell),
                            units
                                .transient_allies
                                .union(&units.transient_enemies)
                                .any(inside)
                        );
                    }
                }
            }
        }
    }
}
