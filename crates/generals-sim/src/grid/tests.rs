use super::{Cell, Grid, RouteUnits};

#[test]
fn clear_diagonal_uses_one_optimized_segment() {
    let grid = Grid::new(30, 30, []).unwrap();
    let start = Cell { x: 3, y: 4 };
    let goal = Cell { x: 24, y: 20 };
    assert!(grid.path(start, goal).unwrap() == [start, goal]);
}

#[test]
fn native_shared_ground_corridor_keeps_a_wall_opening_after_corner_optimization() {
    let walls = (24..=26).flat_map(|x| {
        (0..40)
            .filter(|y| !(20..=30).contains(y))
            .map(move |y| Cell { x, y })
    });
    let grid = Grid::new(50, 40, walls).unwrap();
    let start = Cell { x: 10, y: 10 };
    let goal = Cell { x: 40, y: 10 };
    let path = grid
        .group_path_counted(
            super::LayerCell {
                cell: start,
                layer: 1,
            },
            super::LayerCell {
                cell: goal,
                layer: 1,
            },
            &std::collections::BTreeMap::from([(1, super::RouteUnits::new())]),
        )
        .route
        .map(|route| {
            route
                .optimized
                .into_iter()
                .map(|node| node.cell)
                .collect::<Vec<_>>()
        })
        .unwrap();
    assert!(path.first() == Some(&start));
    assert!(path.last() == Some(&goal));
    assert!(path.iter().any(|point| point.y >= 20));
    for pair in path.windows(2) {
        assert!(
            grid.line_cells(pair[0], pair[1])
                .into_iter()
                .all(|cell| !grid.is_blocked(cell))
        );
    }
    assert!(
        path == grid
            .group_path_counted(
                super::LayerCell {
                    cell: start,
                    layer: 1
                },
                super::LayerCell {
                    cell: goal,
                    layer: 1
                },
                &std::collections::BTreeMap::from([(1, super::RouteUnits::new())])
            )
            .route
            .map(|route| route
                .optimized
                .into_iter()
                .map(|node| node.cell)
                .collect::<Vec<_>>())
            .unwrap()
    );
}

#[test]
fn diagonal_cannot_escape_two_closed_sides() {
    let grid = Grid::new(2, 2, [Cell { x: 1, y: 0 }, Cell { x: 0, y: 1 }]).unwrap();
    assert!(
        grid.path(Cell { x: 0, y: 0 }, Cell { x: 1, y: 1 })
            .is_none()
    );
}

#[test]
fn wall_detour_keeps_the_opening_after_optimization() {
    let grid = Grid::new(16, 14, (0..10).map(|y| Cell { x: 8, y })).unwrap();
    let start = Cell { x: 3, y: 4 };
    let goal = Cell { x: 12, y: 4 };
    let path = grid.path(start, goal).unwrap();
    assert!(path.len() > 2);
    assert!(path.iter().any(|cell| cell.y >= 10));
    assert!(path.iter().all(|&cell| !grid.is_blocked(cell)));
    assert!(path.front() == Some(&start));
    assert!(path.back() == Some(&goal));
    assert!(path == grid.path(start, goal).unwrap());
}

#[test]
fn destination_checks_even_and_odd_native_footprints() {
    let grid = Grid::new(12, 12, [Cell { x: 5, y: 5 }]).unwrap();
    let goal = Cell { x: 6, y: 6 };
    assert!(
        grid.path_for_counted(1, Cell { x: 2, y: 2 }, goal, 0, true, &RouteUnits::new())
            .route
            .is_some()
    );
    assert!(
        grid.path_for_counted(1, Cell { x: 2, y: 2 }, goal, 1, false, &RouteUnits::new())
            .route
            .is_none()
    );
    assert!(
        grid.path_for_counted(1, Cell { x: 2, y: 2 }, goal, 1, true, &RouteUnits::new())
            .route
            .is_none()
    );
    assert!(!grid.destination_clear(Cell { x: 0, y: 4 }, 1, false));
    assert!(!grid.destination_clear(Cell { x: 11, y: 4 }, 1, true));
}

#[test]
fn occupied_start_can_leave_but_not_reenter_an_obstacle_wall() {
    let terrain = Grid::new(16, 16, []).unwrap();
    let obstacles = (4..=6).flat_map(|x| (4..=6).map(move |y| Cell { x, y }));
    let grid = terrain.with_obstacles(obstacles);
    let start = Cell { x: 5, y: 5 };
    let goal = Cell { x: 12, y: 5 };
    assert!(grid.path(start, goal).is_none());
    let path = grid.path_from_occupied(start, goal).unwrap();
    assert!(path.front() == Some(&start));
    assert!(path.back() == Some(&goal));
    assert!(grid.path_from_occupied(goal, start).is_none());
}

#[test]
fn fixed_enemy_cannot_be_bypassed_by_line_expansion_or_corner_optimization() {
    let grid = Grid::new(30, 20, []).unwrap();
    let start = Cell { x: 3, y: 8 };
    let goal = Cell { x: 25, y: 8 };
    let mut units = RouteUnits::new();
    units
        .fixed_enemies
        .extend((6..=10).map(|y| Cell { x: 14, y }));
    let mut path = grid
        .path_for_counted(1, start, goal, 0, true, &units)
        .route
        .unwrap()
        .optimized;
    assert!(path.len() > 2);
    assert!(path.front().map(|node| node.cell) == Some(start));
    assert!(path.back().map(|node| node.cell) == Some(goal));
    for pair in path.make_contiguous().windows(2) {
        assert!(
            grid.line_cells(pair[0].cell, pair[1].cell)
                .into_iter()
                .all(|cell| { !units.fixed_enemies.contains(&cell) })
        );
    }
}

#[test]
fn fixed_ally_remains_traversable_when_the_only_route_requires_move_aside() {
    let walls = (0..20).flat_map(|x| [Cell { x, y: 0 }, Cell { x, y: 2 }]);
    let grid = Grid::new(20, 3, walls).unwrap();
    let start = Cell { x: 2, y: 1 };
    let goal = Cell { x: 17, y: 1 };
    let mut units = RouteUnits::new();
    units.fixed_allies.insert(Cell { x: 9, y: 1 });
    // Native path search accepts a fixed ally at its source cost; actual yielding
    // is a later AI state, not a hard obstacle or invented displacement here.
    assert!(
        grid.path_for_counted(1, start, goal, 0, true, &units)
            .route
            .is_some()
    );
    units.fixed_allies.clear();
    units.fixed_enemies.insert(Cell { x: 9, y: 1 });
    assert!(
        grid.path_for_counted(1, start, goal, 0, true, &units)
            .route
            .is_none()
    );
}

#[test]
fn route_checks_the_complete_body_footprint_for_a_fixed_enemy() {
    let walls = (0..24).flat_map(|x| [Cell { x, y: 0 }, Cell { x, y: 5 }]);
    let grid = Grid::new(24, 6, walls).unwrap();
    let mut units = RouteUnits::new();
    units
        .fixed_enemies
        .extend((1..=4).map(|y| Cell { x: 11, y }));
    assert!(
        grid.path_for_counted(
            1,
            Cell { x: 4, y: 2 },
            Cell { x: 20, y: 2 },
            1,
            true,
            &units
        )
        .route
        .is_none()
    );
}

#[test]
fn frame_accounting_counts_listed_cells_and_excludes_the_popped_success_cell() {
    let grid = Grid::new(3, 1, []).unwrap();
    let start = Cell { x: 0, y: 0 };
    let goal = Cell { x: 2, y: 0 };
    let mut units = RouteUnits::new();
    let path = grid.path_for_counted(1, start, goal, 0, true, &units);
    assert!(path.route.is_some());
    assert_eq!(path.cells, 2);
    // Persistent INFO changes allocation-cap accounting, but every open/closed
    // list member still contributes to processPathfindQueue's frame budget.
    units.info_cells.extend([start, Cell { x: 1, y: 0 }, goal]);
    let allocated = grid.path_for_counted(1, start, goal, 0, true, &units);
    assert_eq!(allocated.cells, path.cells);
    let same_cell = grid.path_for_counted(1, start, start, 0, true, &units);
    assert!(same_cell.route.is_some());
    assert_eq!(same_cell.cells, 0);
}

#[test]
fn failed_search_and_closest_rejection_preserve_source_cleanup_accounting() {
    let grid = Grid::new(3, 1, []).unwrap();
    let start = Cell { x: 0, y: 0 };
    let goal = Cell { x: 2, y: 0 };
    let mut units = RouteUnits::new();
    units.fixed_enemies.insert(Cell { x: 1, y: 0 });
    let failed = grid.path_for_counted(1, start, goal, 0, true, &units);
    assert!(failed.route.is_none());
    assert_eq!(failed.cells, 1);
    units.fixed_enemies.clear();
    let closest = grid.closest_path_counted(
        start,
        goal,
        0,
        true,
        &std::collections::BTreeSet::from([goal]),
        &units,
    );
    let route = closest.route.unwrap();
    assert!(route.optimized.back().map(|node| node.cell) == Some(Cell { x: 1, y: 0 }));
    // The rejected goal is removed from open and bypasses putOnClosedList.
    assert_eq!(closest.cells, 2);
}

#[test]
fn bounded_info_pool_reserves_goal_and_releases_search_allocations_between_queries() {
    let grid = Grid::new(3, 1, []).unwrap();
    let start = Cell { x: 0, y: 0 };
    let middle = Cell { x: 1, y: 0 };
    let goal = Cell { x: 2, y: 0 };
    let mut units = RouteUnits::new();
    units.info_cells.extend([start, goal]);
    units.info_available = 0;
    let failed = grid.path_for_counted(1, start, goal, 0, true, &units);
    assert!(failed.route.is_none());
    assert_eq!(failed.cells, 1);
    units.info_available = 1;
    for _ in 0..2 {
        let found = grid.path_for_counted(1, start, goal, 0, true, &units);
        assert!(found.route.is_some());
        assert_eq!(found.cells, 2);
    }
    units.info_cells.remove(&goal);
    // findPath allocates the destination's INFO before opening its start.
    assert!(
        grid.path_for_counted(1, start, goal, 0, true, &units)
            .route
            .is_none()
    );
    units.info_cells.insert(middle);
    assert!(
        grid.path_for_counted(1, start, goal, 0, true, &units)
            .route
            .is_some()
    );
}
