use super::*;
use crate::grid::Cell;
use std::sync::Arc;

#[test]
fn native_path_cost_info_exhaustion_returns_examined_count() {
    let grid = Grid::new(7, 7, []).unwrap();
    let start = LayerCell {
        cell: Cell { x: 3, y: 3 },
        layer: 1,
    };
    let goal = LayerCell {
        cell: Cell { x: 5, y: 3 },
        layer: 1,
    };
    let mut units = RouteUnits::new();
    units.info_available = 0;
    units.info_cells.extend([start.cell, goal.cell]);
    let result = grid.path_cost_counted(start, goal, &BTreeMap::from([(1, units)]));
    assert_eq!(result.cost, Some(0));
    assert_eq!(result.search.cells, 1);
    assert_eq!(result.search.info_cleanup.len(), 2);
}

#[test]
fn native_path_cost_limits_existing_info_examinations_to_500() {
    let grid = Grid::new(150, 3, []).unwrap();
    let start = LayerCell {
        cell: Cell { x: 1, y: 1 },
        layer: 1,
    };
    let goal = LayerCell {
        cell: Cell { x: 148, y: 1 },
        layer: 1,
    };
    let mut units = RouteUnits::new();
    units.info_available = 0;
    units.info_cells = (0..150)
        .flat_map(|x| (0..3).map(move |y| Cell { x, y }))
        .collect();
    let result = grid.path_cost_counted(start, goal, &BTreeMap::from([(1, units)]));
    assert!(result.cost.is_none());
    assert!(result.search.cells > 1);
    assert!(result.search.cells < 150 * 3);
}

#[test]
fn shared_ground_cleanup_releases_terminal_info_outside_budget_count() {
    let grid = Grid::new(30, 30, []).unwrap();
    let start = LayerCell {
        cell: Cell { x: 5, y: 5 },
        layer: 1,
    };
    let goal = LayerCell {
        cell: Cell { x: 25, y: 25 },
        layer: 1,
    };
    let result = grid.group_path_counted(start, goal, &BTreeMap::from([(1, RouteUnits::new())]));
    assert!(result.route.is_some());
    assert!(result.info_cleanup.iter().any(|&(node, _)| node == start));
    assert!(result.info_cleanup.iter().any(|&(node, _)| node == goal));
    assert_eq!(result.cells + 1, result.info_cleanup.len());
}

#[test]
fn pinned_native_full_path_cost_queries_match_values_and_cleanup_budgets() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/reference/path-cost-reference.txt"
    ));
    let mut cases = 0;
    for line in fixture.lines() {
        let values: Vec<u32> = line
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect();
        let mode = values[0];
        let mut grid = Grid::new(150, 3, []).unwrap();
        let start = LayerCell {
            cell: Cell { x: 1, y: 1 },
            layer: 1,
        };
        let goal = LayerCell {
            cell: Cell {
                x: if mode == 0 { 148 } else { 3 },
                y: 1,
            },
            layer: 1,
        };
        let mut units = RouteUnits::new();
        units.info_available = if mode == 3 {
            super::super::CELL_INFO_CAPACITY - 450
        } else {
            0
        };
        if mode == 1 {
            units.info_cells.extend([start.cell, goal.cell]);
        } else {
            units.info_cells = (0..150)
                .flat_map(|x| (0..3).map(move |y| Cell { x, y }))
                .collect();
        }
        if mode == 2 {
            Arc::make_mut(&mut grid.pinched).insert(Cell { x: 2, y: 1 });
        }
        let result = grid.path_cost_counted(start, goal, &BTreeMap::from([(1, units)]));
        assert_eq!(
            result.cost.unwrap_or(0x7fff_0000),
            values[1],
            "native path-cost mode {mode}"
        );
        assert_eq!(
            result.search.cells,
            usize::try_from(values[2]).unwrap(),
            "native cleanup mode {mode}"
        );
        cases += 1;
    }
    assert_eq!(cases, 4);
}
