use super::*;
use std::sync::Arc;

#[test]
fn native_hierarchy_cost_rounds_euclidean_distance_after_double_sqrt() {
    assert_eq!(hier_cost(Cell { x: 0, y: 0 }, Cell { x: 3, y: 4 }), 50);
    assert_eq!(hier_cost(Cell { x: 0, y: 0 }, Cell { x: 1, y: 1 }), 14);
    assert_eq!(hier_cost(Cell { x: 9, y: 4 }, Cell { x: 9, y: 5 }), 10);
}

fn fixture(mode: u8) -> (Grid, HierarchyQuery, BTreeMap<u8, RouteUnits>) {
    let mut grid = Grid::new(30, 10, []).unwrap();
    let mut units = RouteUnits::new();
    let mut query = HierarchyQuery {
        start: LayerCell {
            cell: Cell { x: 5, y: 5 },
            layer: 1,
        },
        goal: LayerCell {
            cell: Cell { x: 25, y: 5 },
            layer: 1,
        },
        surfaces: 1,
        crusher: false,
        closest: matches!(mode, 2 | 3),
        logical_bounds: Some([Cell { x: 0, y: 0 }, Cell { x: 29, y: 9 }]),
        start_expansion: [Cell { x: 0, y: 0 }, Cell { x: 15, y: 9 }],
        goal_is_origin: mode == 6,
    };
    if matches!(mode, 1 | 4 | 13 | 14 | 15) {
        existing(&mut units, 9, Cell { x: 9, y: 5 }, 0, 0);
        existing(&mut units, 19, Cell { x: 19, y: 5 }, 0, 0);
    }
    if matches!(mode, 3 | 8) {
        existing(&mut units, 9, Cell { x: 9, y: 5 }, 0, 0);
    }
    if mode == 4 {
        existing(&mut units, 10, Cell { x: 12, y: 6 }, 77, 999);
    }
    if mode == 5 {
        query.goal.cell.x = 8;
    }
    if mode == 6 {
        query.goal.cell = Cell { x: 0, y: 0 };
    }
    if mode == 7 {
        units.info_available = 1;
    }
    if mode == 8 {
        units.info_available = 2;
    }
    if matches!(mode, 9 | 10 | 11 | 12 | 16) {
        grid = bridge_fixture();
        query.start.cell.x = 2;
        query.goal.cell.x = 29;
        query.start_expansion[1].x = 12;
        if mode == 9 {
            existing(&mut units, 5, Cell { x: 5, y: 5 }, 700, 999);
        }
        if mode == 10 {
            query.start = LayerCell {
                cell: Cell { x: 15, y: 5 },
                layer: 2,
            };
            query.start_expansion = [Cell { x: 5, y: 0 }, Cell { x: 25, y: 9 }];
        }
        if mode == 11 {
            existing(&mut units, 5, Cell { x: 5, y: 5 }, 700, 999);
            units.info_available = 2;
        }
        if mode == 12 {
            units.info_available = 2;
        }
        if mode == 16 {
            query.goal = LayerCell {
                cell: Cell { x: 15, y: 5 },
                layer: 2,
            };
        }
    }
    if mode == 13 {
        existing(&mut units, 9, Cell { x: 8, y: 4 }, 1234, 420);
    }
    if mode == 14 {
        Arc::make_mut(&mut grid.pinched).insert(Cell { x: 9, y: 5 });
    }
    if mode == 15 {
        query.logical_bounds.as_mut().unwrap()[1].y = 4;
    }
    if mode == 17 {
        grid = Grid::new(30, 10, (0..10).map(|y| Cell { x: 20, y })).unwrap();
    }
    (grid, query, BTreeMap::from([(1, units)]))
}

fn records(records: impl IntoIterator<Item = (LayerCell, CellInfo)>) -> Vec<u32> {
    let records: BTreeMap<_, _> = records
        .into_iter()
        .map(|(key, info)| ((key.layer, key.cell.x, key.cell.y), info))
        .collect();
    let mut result = vec![u32::try_from(records.len()).unwrap()];
    for ((layer, x, y), info) in records {
        result.extend([
            u32::from(layer),
            u32::from(x),
            u32::from(y),
            u32::from(info.coordinate.cell.x),
            u32::from(info.coordinate.cell.y),
            info.cost,
            info.total,
        ]);
    }
    result
}

#[test]
fn pinned_hierarchy_boundaries_bridges_and_allocation_cleanup_match_native() {
    let fixture_text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/reference/hierarchy-reference.txt"
    ))
    .unwrap();
    let mut lines = fixture_text.lines();
    let mut compared = 0;
    while let Some(header) = lines.next() {
        let parse = |line: &str| {
            line.split_whitespace()
                .map(|value| value.parse::<u32>().unwrap())
                .collect::<Vec<_>>()
        };
        let header = parse(header);
        let expected_blocks = parse(lines.next().unwrap());
        let expected_cleanup = parse(lines.next().unwrap());
        let expected_retained = parse(lines.next().unwrap());
        let mode = u8::try_from(header[0]).unwrap();
        let (grid, query, units) = fixture(mode);
        let result = grid.hierarchical_corridor(&query, &units);
        assert_eq!(
            u32::from(result.blocks.is_some()),
            header[1],
            "mode {mode} success"
        );
        assert_eq!(
            u32::try_from(result.search.cells).unwrap(),
            header[2],
            "mode {mode} budget"
        );
        let mut actual_blocks =
            vec![u32::try_from(result.blocks.as_ref().map_or(0, BTreeSet::len)).unwrap()];
        if let Some(blocks) = result.blocks {
            for block in blocks {
                actual_blocks.extend([u32::from(block.x), u32::from(block.y)]);
            }
        }
        assert_eq!(actual_blocks, expected_blocks, "mode {mode} corridor");
        assert_eq!(
            records(result.search.info_cleanup),
            expected_cleanup,
            "mode {mode} cleanup"
        );
        assert_eq!(
            records(result.search.retained_infos),
            expected_retained,
            "mode {mode} unlisted INFO"
        );
        compared += 1;
    }
    assert_eq!(compared, 18);
}

#[test]
fn boundary_requires_existing_info_and_allocation_failure_keeps_native_cleanup() {
    let (grid, query, units) = fixture(0);
    let bare = grid.hierarchical_corridor(&query, &units);
    assert!(bare.blocks.is_none());
    assert_eq!(bare.search.cells, 1);
    let (grid, query, units) = fixture(1);
    assert!(grid.hierarchical_corridor(&query, &units).blocks.is_some());
    let (grid, query, units) = fixture(12);
    let exhausted = grid.hierarchical_corridor(&query, &units);
    assert!(exhausted.blocks.is_none());
    assert_eq!(exhausted.search.cells, 0);
    assert_eq!(exhausted.search.retained_infos.len(), 1);
    assert!(exhausted.search.retained_infos[0].0 == query.start);
}

#[test]
fn group_corridor_margin_checks_only_four_native_corners() {
    let grid = Grid::new(40, 40, []).unwrap();
    let blocks = BTreeSet::from([Cell { x: 1, y: 1 }]);
    assert!(group_passable(&grid, Some(&blocks), Cell { x: 7, y: 7 }));
    assert!(!group_passable(&grid, Some(&blocks), Cell { x: 6, y: 7 }));
    assert!(!passable(Some(&blocks), Cell { x: 7, y: 7 }));
    assert!(passable(Some(&blocks), Cell { x: 10, y: 10 }));
}

fn existing(units: &mut RouteUnits, x: u16, coordinate: Cell, cost: u32, total: u32) {
    let cell = Cell { x, y: 5 };
    units.info_cells.insert(cell);
    units.info_records.insert(
        cell,
        CellInfo {
            coordinate: LayerCell {
                cell: coordinate,
                layer: 1,
            },
            cost,
            total,
        },
    );
}

fn bridge_fixture() -> Grid {
    let mut terrain = crate::terrain::TerrainMap::classify(30, 10, [], []).unwrap();
    terrain
        .add_bridge(
            crate::terrain::BridgeGeometry {
                from: [60.0, 50.0, 20.0],
                to: [250.0, 50.0, 20.0],
                corners: [
                    [60.0, 40.0, 20.0],
                    [60.0, 60.0, 20.0],
                    [250.0, 40.0, 20.0],
                    [250.0, 60.0, 20.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    let mut grid = Grid::new(30, 10, []).unwrap().with_bridge_layers(&terrain);
    // This fixture supplies clear ground zone inputs independently of the
    // separately source-tested terrain classification underneath a bridge.
    Arc::make_mut(&mut grid.source_kinds).fill(crate::terrain::CellType::Clear);
    grid.pinched = Arc::new(std::collections::BTreeSet::new());
    grid.recalculate_source_zones();
    assert_eq!(
        grid.bridge_endpoints(2)
            .map(|ends| ends.map(|cell| (cell.x, cell.y))),
        Some([(5, 5), (25, 5)])
    );

    grid
}
