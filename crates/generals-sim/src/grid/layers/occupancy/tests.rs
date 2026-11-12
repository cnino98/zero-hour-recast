use super::super::Search;
use super::*;
use crate::{
    grid::{Cell, CellInfo, LayerQuery, SearchResult},
    terrain::{BridgeGeometry, TerrainMap},
};

fn bridge() -> (Grid, u8) {
    let water = (0..32).flat_map(|y| (20..28).map(move |x| Cell { x, y }));
    let mut terrain = TerrainMap::classify(48, 32, [], water).unwrap();
    let layer = terrain
        .add_bridge(
            BridgeGeometry {
                from: [120.0, 120.0, 20.0],
                to: [360.0, 120.0, 20.0],
                corners: [
                    [120.0, 80.0, 20.0],
                    [120.0, 160.0, 20.0],
                    [360.0, 80.0, 20.0],
                    [360.0, 160.0, 20.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    let grid = Grid::new(48, 32, terrain.blocked_ground_cells())
        .unwrap()
        .with_bridge_layers(&terrain);
    (grid, layer)
}

fn query(start: LayerCell, goal: LayerCell, radius: i32, centered: bool) -> LayerQuery {
    LayerQuery {
        start,
        goal,
        radius,
        centered,
        closest_cost: None,
    }
}

#[test]
fn indexed_layer_flags_match_old_footprint_queries_with_overlap_and_ground_fallback() {
    let (grid, layer) = bridge();
    for crusher in [false, true] {
        let mut ground = RouteUnits::new();
        for y in 0..32 {
            for x in 0..48 {
                let cell = Cell { x, y };
                let index = usize::from(y) * 48 + usize::from(x);
                if index % 2 == 0 {
                    ground.fixed_allies.insert(cell);
                }
                if index % 3 == 0 {
                    ground.moving_allies.insert(cell);
                }
                // canCrushOrSquish has already filtered this actor-relative set.
                if !crusher && index % 5 == 0 {
                    ground.fixed_enemies.insert(cell);
                }
            }
        }
        assert!(ground.fixed_allies.len() > 512);
        ground.fixed_allies.insert(Cell { x: 500, y: 500 });
        let mut upper = RouteUnits::new();
        upper
            .fixed_allies
            .extend([Cell { x: 24, y: 12 }, Cell { x: 1, y: 1 }]);
        upper.moving_allies.insert(Cell { x: 24, y: 12 });
        upper.fixed_enemies.insert(Cell { x: 24, y: 12 });
        let mut arbitrary = RouteUnits::new();
        arbitrary.fixed_enemies.insert(Cell { x: 1, y: 1 });
        let units = BTreeMap::from([(1, ground), (layer, upper), (255, arbitrary)]);
        for radius in 0..=2 {
            for centered in [false, true] {
                let start = LayerCell {
                    cell: Cell { x: 10, y: 12 },
                    layer: 1,
                };
                let search = Search::new(
                    &grid,
                    &query(start, start, radius, centered),
                    &units,
                    false,
                    false,
                )
                .unwrap();
                for requested in [0, 1, layer, 15, 254, 255] {
                    for y in 0..32 {
                        for x in 0..48 {
                            let node = LayerCell {
                                cell: Cell { x, y },
                                layer: requested,
                            };
                            assert_eq!(
                                search.occupied_at(node, radius, centered),
                                screen(&grid, &units, node, radius, centered)
                            );
                        }
                    }
                }
                let overlap = LayerCell {
                    cell: Cell { x: 24, y: 12 },
                    layer,
                };
                assert_eq!(search.occupied_at(overlap, 0, true), (true, true, true));
                let fallback = LayerCell {
                    cell: Cell { x: 1, y: 1 },
                    layer: 255,
                };
                assert_eq!(search.occupied_at(fallback, 0, true), (false, false, false));
            }
        }
    }
}

#[test]
fn empty_contexts_allocate_no_cell_masks_and_cold_screen_matches_search_short_circuit() {
    let (grid, layer) = bridge();
    let empty = BTreeMap::from([(1, RouteUnits::new()), (255, RouteUnits::new())]);
    assert!(build(&grid, &empty).is_empty());
    let mut occupied = RouteUnits::new();
    occupied.fixed_enemies.insert(Cell { x: 10, y: 12 });
    let units = BTreeMap::from([(1, occupied), (255, RouteUnits::new())]);
    let masks = build(&grid, &units);
    assert_eq!(masks.len(), 2);
    assert!(masks[0].is_none());
    assert!(masks[1].is_some());
    for radius in 0..=2 {
        for centered in [false, true] {
            for node in [
                LayerCell {
                    cell: Cell { x: 10, y: 12 },
                    layer: 1,
                },
                LayerCell {
                    cell: Cell { x: 24, y: 12 },
                    layer,
                },
                LayerCell {
                    cell: Cell { x: 0, y: 0 },
                    layer: 1,
                },
            ] {
                for units in [&empty, &units] {
                    let search = Search::new(
                        &grid,
                        &query(node, node, radius, centered),
                        units,
                        true,
                        false,
                    )
                    .unwrap();
                    assert_eq!(
                        grid.initial_tunneling(node, radius, centered, units),
                        search.tunneling
                    );
                }
            }
        }
    }
}

fn same_infos(left: &[(LayerCell, CellInfo)], right: &[(LayerCell, CellInfo)]) {
    assert_eq!(left.len(), right.len());
    for ((left_node, left), (right_node, right)) in left.iter().zip(right) {
        assert!(left_node == right_node);
        assert!(left.coordinate == right.coordinate);
        assert_eq!((left.cost, left.total), (right.cost, right.total));
    }
}

fn same_search(left: &SearchResult, right: &SearchResult) {
    assert_eq!(left.cells, right.cells);
    same_infos(&left.info_cleanup, &right.info_cleanup);
    same_infos(&left.retained_infos, &right.retained_infos);
    match (&left.route, &right.route) {
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
        (None, None) => {}
        _ => panic!("equivalent occupancy changed route admission"),
    }
}

#[test]
fn bridge_routes_costs_and_info_records_ignore_unreachable_context_cells() {
    let (grid, layer) = bridge();
    let start = LayerCell {
        cell: Cell { x: 10, y: 12 },
        layer: 1,
    };
    let goal = LayerCell {
        cell: Cell { x: 40, y: 12 },
        layer: 1,
    };
    let mut upper = RouteUnits::new();
    upper.moving_allies.insert(Cell { x: 23, y: 12 });
    upper.fixed_allies.insert(Cell { x: 24, y: 12 });
    upper.fixed_enemies.insert(Cell { x: 25, y: 12 });
    let mut units = BTreeMap::from([(1, RouteUnits::new()), (layer, upper)]);
    let expected =
        grid.path_for_layers_counted(&query(start, goal, 0, true), &units, &BTreeMap::new());
    let route = expected.route.as_ref().unwrap();
    assert!(route.raw.iter().any(|node| node.layer == layer));
    assert!(route.raw.first() == Some(&start));
    assert!(route.raw.last() == Some(&goal));
    let expected_cost = grid.path_cost_counted(start, goal, &units);
    assert!(expected_cost.cost.is_some());
    // Upper-layer entries outside an allocated deck resolve to ground and
    // must never borrow occupancy from that absent physical layer.
    let upper = units.get_mut(&layer).unwrap();
    upper.fixed_allies.insert(Cell { x: 1, y: 1 });
    upper.moving_allies.insert(Cell { x: 1, y: 1 });
    upper.fixed_enemies.insert(Cell { x: 1, y: 1 });
    upper.fixed_enemies.insert(Cell { x: 500, y: 500 });
    let mut arbitrary = RouteUnits::new();
    arbitrary
        .fixed_enemies
        .extend((0..32).flat_map(|y| (0..48).map(move |x| Cell { x, y })));
    units.insert(255, arbitrary);
    let actual =
        grid.path_for_layers_counted(&query(start, goal, 0, true), &units, &BTreeMap::new());
    same_search(&expected, &actual);
    let actual_cost = grid.path_cost_counted(start, goal, &units);
    assert_eq!(expected_cost.cost, actual_cost.cost);
    same_search(&expected_cost.search, &actual_cost.search);
}
