//! Immutable navigation views retain independent admission and source-cache state.
use super::*;
use crate::terrain::{BridgeGeometry, CellType, TerrainMap};
use std::collections::BTreeMap;

// An independently owned snapshot is the pre-sharing Clone contract. It must
// not inherit any Arc allocation from the grid whose isolation is being tested.
fn deep_snapshot(grid: &Grid) -> Grid {
    Grid {
        width: grid.width,
        height: grid.height,
        blocked: Arc::new((*grid.blocked).clone()),
        cliffs: Arc::new((*grid.cliffs).clone()),
        ground_heights: Arc::new((*grid.ground_heights).clone()),
        source_kinds: Arc::new((*grid.source_kinds).clone()),
        source_zones: grid
            .source_zones
            .as_ref()
            .map(|zones| Arc::new((**zones).clone())),
        bridge_ends: Arc::new((*grid.bridge_ends).clone()),
        pinched: Arc::new((*grid.pinched).clone()),
        obstacles: Arc::new((*grid.obstacles).clone()),
        ignored_obstacles: grid.ignored_obstacles.clone(),
        fences: Arc::new((*grid.fences).clone()),
        layers: Arc::new((*grid.layers).clone()),
        ground_connections: Arc::new((*grid.ground_connections).clone()),
    }
}

fn shared_backing(first: &Grid, second: &Grid) {
    assert!(Arc::ptr_eq(&first.blocked, &second.blocked));
    assert!(Arc::ptr_eq(&first.cliffs, &second.cliffs));
    assert!(Arc::ptr_eq(&first.ground_heights, &second.ground_heights));
    assert!(Arc::ptr_eq(&first.source_kinds, &second.source_kinds));
    assert!(Arc::ptr_eq(
        first.source_zones.as_ref().unwrap(),
        second.source_zones.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(&first.bridge_ends, &second.bridge_ends));
    assert!(Arc::ptr_eq(&first.pinched, &second.pinched));
    assert!(Arc::ptr_eq(&first.obstacles, &second.obstacles));
    assert!(Arc::ptr_eq(&first.fences, &second.fences));
    assert!(Arc::ptr_eq(&first.layers, &second.layers));
    assert!(Arc::ptr_eq(
        &first.ground_connections,
        &second.ground_connections
    ));
}

fn fixture() -> (Grid, TerrainMap, [LayerCell; 3]) {
    let mut terrain = TerrainMap::classify(40, 20, [Cell { x: 3, y: 4 }], []).unwrap();
    let layer = terrain
        .add_bridge(
            BridgeGeometry {
                from: [120.0, 100.0, 20.0],
                to: [280.0, 100.0, 20.0],
                corners: [
                    [120.0, 70.0, 20.0],
                    [120.0, 130.0, 20.0],
                    [280.0, 70.0, 20.0],
                    [280.0, 130.0, 20.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    let nodes = [
        LayerCell {
            cell: Cell { x: 5, y: 5 },
            layer: 1,
        },
        LayerCell {
            cell: Cell { x: 20, y: 10 },
            layer,
        },
        LayerCell {
            cell: Cell { x: 20, y: 9 },
            layer,
        },
    ];
    let mut grid = Grid::new(40, 20, terrain.blocked_ground_cells()).unwrap();
    grid.restore_ground_terrain(&terrain);
    grid = grid.with_navigation_obstacles([nodes[0].cell], &terrain, [nodes[1]], [nodes[2]]);
    grid.recalculate_source_zones();
    (grid, terrain, nodes)
}

fn same_snapshot(expected: &Grid, actual: &Grid) {
    same_zone_tables(expected, actual);
    assert_eq!(
        (expected.width, expected.height),
        (actual.width, actual.height)
    );
    assert!(expected.blocked == actual.blocked);
    assert!(expected.cliffs == actual.cliffs);
    assert!(expected.pinched == actual.pinched);
    assert!(expected.obstacles == actual.obstacles);
    assert!(expected.ignored_obstacles == actual.ignored_obstacles);
    assert!(expected.fences == actual.fences);
    assert!(expected.bridge_ends == actual.bridge_ends);
    assert!(expected.ground_connections == actual.ground_connections);
    assert!(
        expected
            .ground_heights
            .iter()
            .map(|v| v.to_bits())
            .eq(actual.ground_heights.iter().map(|v| v.to_bits()))
    );
    assert!(
        expected
            .source_kinds
            .iter()
            .map(std::mem::discriminant)
            .eq(actual.source_kinds.iter().map(std::mem::discriminant))
    );
    for y in 0..expected.height {
        for x in 0..expected.width {
            let cell = Cell { x, y };
            assert_eq!(expected.is_blocked(cell), actual.is_blocked(cell));
            assert_eq!(expected.is_pinched(cell), actual.is_pinched(cell));
            for layer in 1..=15 {
                let node = LayerCell { cell, layer };
                assert!(expected.resolve_layer(node) == actual.resolve_layer(node));
                assert!(expected.connection(node) == actual.connection(node));
                assert_eq!(expected.layer_blocked(node), actual.layer_blocked(node));
                assert_eq!(expected.layer_obstacle(node), actual.layer_obstacle(node));
                assert_eq!(expected.source_zone(node), actual.source_zone(node));
            }
            for surfaces in [1, 3, 5, 8, 17, 31] {
                for crusher in [false, true] {
                    assert_eq!(
                        expected.source_block_zone(surfaces, crusher, cell),
                        actual.source_block_zone(surfaces, crusher, cell)
                    );
                }
            }
        }
    }
}

fn same_zone_tables(expected: &Grid, actual: &Grid) {
    let maximum = expected.source_layer_zone(15) + 1;
    assert_eq!(maximum, actual.source_layer_zone(15) + 1);
    for layer in 0..=15 {
        assert_eq!(
            expected.source_layer_zone(layer),
            actual.source_layer_zone(layer)
        );
    }
    for zone in 0..maximum {
        assert_eq!(
            expected.source_terrain_zone(zone),
            actual.source_terrain_zone(zone)
        );
        for surfaces in [0, 1, 2, 3, 4, 5, 6, 7, 8, 16, 17, 19, 21, 31] {
            for crusher in [false, true] {
                assert_eq!(
                    expected.effective_source_zone(surfaces, crusher, zone),
                    actual.effective_source_zone(surfaces, crusher, zone)
                );
            }
        }
    }
}

fn same_search(expected: &SearchResult, actual: &SearchResult) {
    assert_eq!(expected.cells, actual.cells);
    for (left, right) in [
        (&expected.info_cleanup, &actual.info_cleanup),
        (&expected.retained_infos, &actual.retained_infos),
    ] {
        assert_eq!(left.len(), right.len());
        for ((left_node, left), (right_node, right)) in left.iter().zip(right) {
            assert!(left_node == right_node);
            assert!(left.coordinate == right.coordinate);
            assert_eq!((left.cost, left.total), (right.cost, right.total));
        }
    }
    match (&expected.route, &actual.route) {
        (None, None) => {}
        (Some(left), Some(right)) => {
            assert!(left.raw == right.raw);
            assert!(left.optimized == right.optimized);
            assert_eq!(left.raw_flags.len(), right.raw_flags.len());
            for (left, right) in left.raw_flags.iter().zip(&right.raw_flags) {
                assert_eq!(
                    (left.cliff, left.pinched, left.blocked_by_ally),
                    (right.cliff, right.pinched, right.blocked_by_ally)
                );
            }
            assert_eq!(left.optimization_blocked, right.optimization_blocked);
            assert_eq!(left.blocked_by_ally, right.blocked_by_ally);
            assert_eq!(
                left.exact_endpoint.map(|v| v.map(f32::to_bits)),
                right.exact_endpoint.map(|v| v.map(f32::to_bits))
            );
            assert_eq!(
                left.quick_start.map(|v| v.map(f32::to_bits)),
                right.quick_start.map(|v| v.map(f32::to_bits))
            );
        }
        _ => panic!("sharing changed route admission"),
    }
}

fn compare_searches(expected: &Grid, actual: &Grid, nodes: [LayerCell; 3]) {
    for available in [0, 1, CELL_INFO_CAPACITY] {
        let mut units = RouteUnits::new();
        units.info_available = available;
        units.info_records.insert(
            nodes[0].cell,
            CellInfo {
                coordinate: nodes[0],
                cost: 73,
                total: 999,
            },
        );
        units.fixed_allies.insert(Cell { x: 8, y: 5 });
        for start in [Cell { x: 2, y: 5 }, nodes[0].cell] {
            same_search(
                &expected.path_for_counted(1, start, nodes[0].cell, 0, true, &units),
                &actual.path_for_counted(1, start, nodes[0].cell, 0, true, &units),
            );
        }
        let mut upper = RouteUnits::new();
        upper.info_available = available;
        upper.info_records.insert(
            nodes[1].cell,
            CellInfo {
                coordinate: nodes[1],
                cost: 17,
                total: 54,
            },
        );
        let contexts = BTreeMap::from([(1, units), (nodes[1].layer, upper)]);
        let query = LayerQuery {
            start: nodes[1],
            goal: nodes[2],
            radius: 0,
            centered: true,
            closest_cost: None,
        };
        same_search(
            &expected.path_for_layers_counted(&query, &contexts, &BTreeMap::new()),
            &actual.path_for_layers_counted(&query, &contexts, &BTreeMap::new()),
        );
    }
}

#[test]
fn ignored_views_share_backing_but_keep_masks_types_zones_and_info_independent() {
    let (base, _, nodes) = fixture();
    // A nonexistent upper-layer cell resolves to the real ground obstacle.
    let fallback = LayerCell {
        layer: 15,
        ..nodes[0]
    };
    let non_obstacle = LayerCell {
        cell: Cell { x: 1, y: 1 },
        layer: 1,
    };
    let ignored = base.without_layer_obstacles([fallback, nodes[1], nodes[2], non_obstacle]);
    shared_backing(&base, &ignored);
    assert!(base.ignored_obstacles.is_empty());
    assert!(ignored.ignored_obstacles == nodes.into_iter().collect());
    let mut expected = deep_snapshot(&base);
    // The oracle installs the known resolved owner cells without calling the
    // ignored-view constructor being tested.
    expected.ignored_obstacles.extend(nodes);
    same_snapshot(&expected, &ignored);
    compare_searches(&expected, &ignored, nodes);
    let start = Cell { x: 2, y: 5 };
    let goal = nodes[0].cell;
    // classifyMap expands the source cliff (3,4) into its eight neighbors.
    // This start is CLIFF; ignoring the goal leaves their terrain zones apart.
    assert!(matches!(
        ignored.source_cell_kind(LayerCell {
            cell: start,
            layer: 1
        }),
        Some(CellType::Cliff)
    ));
    assert!(matches!(
        ignored.source_cell_kind(nodes[0]),
        Some(CellType::Obstacle)
    ));
    assert!(expected.destination_clear(goal, 0, true));
    assert!(ignored.destination_clear(goal, 0, true));
    assert!(!expected.quick_path_exists(1, start, goal));
    assert!(!ignored.quick_path_exists(1, start, goal));
    assert!(
        ignored
            .path_for_counted(1, start, nodes[0].cell, 0, true, &RouteUnits::new())
            .route
            .is_none()
    );
    let expected_direct =
        super::search::find_counted(&expected, start, goal, 0, true, false, &RouteUnits::new());
    let actual_direct =
        super::search::find_counted(&ignored, start, goal, 0, true, false, &RouteUnits::new());
    same_search(&expected_direct, &actual_direct);
    assert!(actual_direct.route.is_some());
    for node in nodes {
        assert!(ignored.layer_obstacle(node));
        assert!(!ignored.layer_blocked(node));
        assert!(matches!(
            ignored.source_cell_kind(node),
            Some(CellType::Obstacle)
        ));
        assert_eq!(base.source_zone(node), ignored.source_zone(node));
    }
    let other = base.without_layer_obstacles([nodes[0]]);
    assert!(!other.ignored_obstacle(nodes[1]));
    let combined = other.without_layer_obstacles([nodes[1]]);
    assert!(combined.ignored_obstacle(nodes[0]));
    assert!(combined.ignored_obstacle(nodes[1]));
    assert!(!other.ignored_obstacle(nodes[1]));
}

#[test]
fn ignored_destination_keeps_a_successful_route_from_clear_ground() {
    let (base, _, nodes) = fixture();
    // (1,1) lies outside both borders around the fixture's source cliff (3,4).
    let start = Cell { x: 1, y: 1 };
    let goal = nodes[0].cell;
    let ignored = base.without_layer_obstacles([nodes[0]]);
    let mut expected = deep_snapshot(&base);
    expected.ignored_obstacles.insert(nodes[0]);
    assert!(matches!(
        ignored.source_cell_kind(LayerCell {
            cell: start,
            layer: 1
        }),
        Some(CellType::Clear)
    ));
    assert!(matches!(
        ignored.source_cell_kind(nodes[0]),
        Some(CellType::Obstacle)
    ));
    assert!(!base.destination_clear(goal, 0, true));
    assert!(ignored.destination_clear(goal, 0, true));
    assert!(expected.quick_path_exists(1, start, goal));
    assert!(ignored.quick_path_exists(1, start, goal));
    let units = RouteUnits::new();
    let original = expected.path_for_counted(1, start, goal, 0, true, &units);
    let shared = ignored.path_for_counted(1, start, goal, 0, true, &units);
    same_search(&original, &shared);
    assert!(shared.route.is_some());
    shared_backing(&base, &ignored);
}

#[test]
fn live_and_ignored_navigation_mutations_detach_in_both_directions() {
    let (mut base, mut terrain, nodes) = fixture();
    let mut ignored = base.without_layer_obstacles(nodes);
    let expected_ignored = deep_snapshot(&ignored);
    let cliff = Cell { x: 3, y: 4 };
    base.clear_ground_barriers([cliff]);
    assert!(!Arc::ptr_eq(&base.blocked, &ignored.blocked));
    assert!(!Arc::ptr_eq(&base.cliffs, &ignored.cliffs));
    assert!(Arc::ptr_eq(&base.ground_heights, &ignored.ground_heights));
    same_snapshot(&expected_ignored, &ignored);
    terrain.set_ground_heights(|_| 3.0);
    base = base.with_navigation_obstacles([Cell { x: 7, y: 5 }], &terrain, [], []);
    base.recalculate_source_zones();
    same_snapshot(&expected_ignored, &ignored);
    compare_searches(&expected_ignored, &ignored, nodes);
    let expected_base = deep_snapshot(&base);
    let mut replacement = TerrainMap::classify(40, 20, [], [Cell { x: 9, y: 6 }]).unwrap();
    replacement.set_ground_heights(|_| 7.0);
    ignored.restore_ground_terrain(&replacement);
    ignored = ignored.with_navigation_obstacles([nodes[0].cell], &replacement, [], []);
    ignored.recalculate_source_zones();
    assert!(!Arc::ptr_eq(&base.layers, &ignored.layers));
    assert!(!Arc::ptr_eq(&base.ground_heights, &ignored.ground_heights));
    assert_eq!(
        base.ground_height_at(nodes[0].cell).to_bits(),
        3.0_f32.to_bits()
    );
    assert_eq!(
        ignored.ground_height_at(nodes[0].cell).to_bits(),
        7.0_f32.to_bits()
    );
    same_snapshot(&expected_base, &base);
    compare_searches(&expected_base, &base, nodes);
    let retained = base.clone();
    let removed = base.without_surface_barriers([cliff]);
    let added = base.with_surface_barriers([Cell { x: 6, y: 6 }]);
    assert!(!removed.is_blocked(cliff));
    assert!(added.is_blocked(Cell { x: 6, y: 6 }));
    same_snapshot(&expected_base, &retained);
    same_snapshot(&expected_base, &base);
}

#[test]
fn ordered_zone_replay_and_recalculation_leave_shared_snapshots_unchanged() {
    let original = Grid::new(20, 20, []).unwrap();
    let retained = original.clone();
    let expected = deep_snapshot(&original);
    let cell = Cell { x: 5, y: 5 };
    let node = LayerCell { cell, layer: 1 };
    let event = ZoneModification {
        update_bounds: Some([[5, 5], [5, 5]]),
        cells: vec![ZoneCellChange {
            cell,
            kind: CellType::Obstacle,
            fence: true,
            reset_zone: true,
        }],
        post_cells: vec![ZoneCellChange {
            cell,
            kind: CellType::Clear,
            fence: false,
            reset_zone: false,
        }],
        pinched: vec![(cell, true)],
        force_dirty: true,
    };
    let mut updated = original.clone();
    assert!(updated.inherit_source_zones(&original, &[event]));
    assert_eq!(updated.source_zone(node), 0);
    assert_ne!(retained.source_zone(node), 0);
    assert!(!Arc::ptr_eq(
        updated.source_zones.as_ref().unwrap(),
        retained.source_zones.as_ref().unwrap()
    ));
    same_snapshot(&expected, &original);
    same_snapshot(&expected, &retained);
    let partial = updated.clone();
    let expected_partial = deep_snapshot(&partial);
    updated.recalculate_source_zones();
    assert_ne!(updated.source_zone(node), 0);
    same_snapshot(&expected_partial, &partial);
    same_snapshot(&expected, &retained);
}

#[test]
fn navigation_snapshots_cross_threads_without_losing_independent_backing() {
    let (base, _, nodes) = fixture();
    let expected = deep_snapshot(&base);
    let view = base.without_layer_obstacles(nodes);
    // The loader returns its World across a thread boundary (Send), while a
    // scoped borrowed reader also enforces Grid's required Sync contract.
    let returned = std::thread::spawn(move || view.with_surface_barriers([Cell { x: 6, y: 6 }]))
        .join()
        .unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                assert!(base.layer_blocked(nodes[0]));
                assert!(!returned.layer_blocked(nodes[0]));
                assert!(returned.is_blocked(Cell { x: 6, y: 6 }));
            })
            .join()
            .unwrap();
    });
    assert!(Arc::ptr_eq(&base.layers, &returned.layers));
    assert!(!Arc::ptr_eq(&base.blocked, &returned.blocked));
    same_snapshot(&expected, &base);
}
