use super::*;

fn query(unavailable: &BTreeSet<Cell>) -> Attack<'_> {
    Attack {
        from: [15.25, 25.0, 17.0],
        target: [305.0, 25.0],
        range_targets: None,
        radius: 0,
        centered: true,
        weapon_range: [0.0, 40.0],
        geometry_radius: 0.0,
        distance: 40.0,
        vehicle: false,
        unavailable,
    }
}

#[test]
fn quick_attack_keeps_five_unit_probe_coordinate_instead_of_snapping() {
    let grid = Grid::new(40, 20, []).unwrap();
    let empty = BTreeSet::new();
    let mut attack = query(&empty);
    attack.target = [55.0, 25.0];
    let path = grid.quick_attack_path(&attack, |_| true).unwrap();
    assert!(path.exact_endpoint == Some([20.25, 25.0, 17.0]));
    assert!(attack.in_range([
        path.exact_endpoint.unwrap()[0],
        path.exact_endpoint.unwrap()[1]
    ]));
}

#[test]
fn attack_search_reaches_range_around_blocked_quick_approach() {
    let grid = Grid::new(40, 20, (0..9).map(|y| Cell { x: 3, y })).unwrap();
    let empty = BTreeSet::new();
    let attack = query(&empty);
    let path = grid
        .attack_search_counted(&attack, &RouteUnits::new(), |_| true)
        .route
        .unwrap();
    assert!(path.exact_endpoint.is_none());
    assert!(path.raw.iter().any(|cell| cell.cell.y >= 9));
    let last = *path.optimized.back().unwrap();
    assert!(attack.in_range(cell_position(last.cell, true)));
    assert!(
        path.raw
            .windows(2)
            .all(|pair| pair[0].cell.x.abs_diff(pair[1].cell.x) <= 1
                && pair[0].cell.y.abs_diff(pair[1].cell.y) <= 1)
    );
}

#[test]
fn attack_search_stops_expanding_at_native_allocation_budget() {
    let grid = Grid::new(4000, 3, []).unwrap();
    let empty = BTreeSet::new();
    let mut attack = query(&empty);
    attack.target = [39_985.0, 25.0];
    assert!(
        grid.attack_search_counted(&attack, &RouteUnits::new(), |_| true)
            .route
            .is_none()
    );
}

#[test]
fn weapon_goal_limits_preserve_both_quarter_cell_margins() {
    let empty = BTreeSet::new();
    let mut attack = query(&empty);
    attack.target = [0.0; 2];
    attack.weapon_range = [5.0, 100.0];
    attack.geometry_radius = 20.0;
    assert!(!attack.in_range([27.49, 0.0]));
    assert!(attack.in_range([27.5, 0.0]));
    assert!(attack.in_range([117.5, 0.0]));
    assert!(!attack.in_range([117.51, 0.0]));
}

#[test]
fn vehicle_tail_strip_yields_to_nonidle_goal_but_preserves_idle_ally_route() {
    let grid = Grid::new(25, 5, []).unwrap();
    let mut units = RouteUnits::new();
    let blocker = Cell { x: 14, y: 2 };
    units.goal_flags.insert(blocker);
    let end = Cell { x: 16, y: 2 };
    let start = Cell { x: 1, y: 2 };
    let populate = |search: &mut Search<'_>| {
        for x in 1..=16 {
            let parent = (x > 1).then_some(Cell { x: x - 1, y: 2 });
            search.insert(Cell { x, y: 2 }, parent, u32::from(x) * 10, 0);
        }
    };
    let mut search = Search::new(&grid, &units, start, end, 0, true, false);
    populate(&mut search);
    assert!(search.strip_attack_tail(end) == Cell { x: 13, y: 2 });
    units.idle_positions.insert(blocker);
    let mut search = Search::new(&grid, &units, start, end, 0, true, false);
    populate(&mut search);
    assert!(search.strip_attack_tail(end) == end);
}

#[test]
fn bridge_endpoint_range_preserves_native_first_point_and_minimum_range_order() {
    let unavailable = BTreeSet::new();
    let mut attack = query(&unavailable);
    attack.geometry_radius = 0.0;
    attack.weapon_range = [20.0, 80.0];
    attack.target = [60.0, 10.0];
    attack.range_targets = Some([[20.0, 10.0], [60.0, 10.0]]);
    // The closer heuristic endpoint cannot make native point one's minimum
    // range rejection pass by selecting point two instead.
    assert!(!attack.in_range([10.0, 10.0]));
    assert!(attack.in_range([120.0, 10.0]));
    assert!(!attack.in_range([300.0, 10.0]));
}

#[test]
fn attack_goal_allocation_failure_retains_reset_unlisted_start_info() {
    let grid = Grid::new(40, 20, []).unwrap();
    let empty = BTreeSet::new();
    let attack = query(&empty);
    let mut units = RouteUnits::new();
    units.info_available = 1;
    let result = grid.attack_search_counted(&attack, &units, |_| true);
    assert!(result.route.is_none());
    assert_eq!(result.cells, 0);
    assert!(result.info_cleanup.is_empty());
    assert_eq!(result.retained_infos.len(), 1);
    let (key, info) = result.retained_infos[0];
    assert_eq!((key.layer, key.cell.x, key.cell.y), (1, 2, 3));
    assert_eq!((info.cost, info.total), (0, 0));
}
