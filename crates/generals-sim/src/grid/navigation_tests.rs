//! Unpublished navigation snapshots must equal the former eager constructor chain.
use super::{
    Cell, CellInfo, Grid, LayerCell, LayerQuery, SearchResult, ZoneCellChange, ZoneModification,
};
use crate::terrain::{BridgeGeometry, CellType, TerrainMap};
use std::collections::BTreeMap;

const MASKS: [u8; 14] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 16, 17, 19, 21, 31];

fn terrain() -> (TerrainMap, u8) {
    let water = (0..20).flat_map(|y| (15..25).map(move |x| Cell { x, y }));
    let cliffs = (14..18).flat_map(|y| (5..8).map(move |x| Cell { x, y }));
    let mut terrain = TerrainMap::classify(40, 20, cliffs, water).unwrap();
    terrain.add_path_rubble(Cell { x: 6, y: 5 });
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
    (terrain, layer)
}

fn snapshots(
    terrain: &TerrainMap,
    layer: u8,
    cliff: bool,
    phase: u8,
    classify: bool,
) -> (Grid, Grid) {
    let mut base = Grid::new(40, 20, terrain.blocked_ground_cells()).unwrap();
    base.restore_ground_terrain(terrain);
    if cliff {
        base = base.without_surface_barriers(terrain.cliff_cells());
    }
    // Overlapping owners project to one obstacle; later phases remove one
    // claimant, move the other footprint, then release its final cell.
    let obstacles = match phase {
        0 => vec![Cell { x: 4, y: 6 }, Cell { x: 4, y: 6 }],
        1 => vec![Cell { x: 4, y: 6 }, Cell { x: 5, y: 6 }],
        2 => vec![Cell { x: 5, y: 6 }],
        _ => Vec::new(),
    };
    let layers = [
        LayerCell {
            cell: Cell { x: 20, y: 10 },
            layer,
        },
        LayerCell {
            cell: Cell { x: 4, y: 5 },
            layer,
        },
    ];
    let fences = [
        LayerCell {
            cell: Cell { x: 20, y: 9 },
            layer,
        },
        LayerCell {
            cell: Cell { x: 3, y: 4 },
            layer,
        },
    ];
    let eager = base
        .with_obstacles(obstacles.iter().copied())
        .with_bridge_layers(terrain)
        .with_layer_obstacles(layers)
        .with_fence_cells(fences);
    let mut batched = base.with_navigation_obstacles(obstacles, terrain, layers, fences);
    if classify {
        batched.recalculate_source_zones();
    }
    (eager, batched)
}

fn same_grid(expected: &Grid, actual: &Grid) {
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
    assert!(expected.layers.keys().eq(actual.layers.keys()));
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
            for layer in 0..=15 {
                let node = LayerCell { cell, layer };
                assert!(expected.resolve_layer(node) == actual.resolve_layer(node));
                assert!(expected.connection(node) == actual.connection(node));
                assert_eq!(expected.layer_blocked(node), actual.layer_blocked(node));
                assert_eq!(expected.layer_obstacle(node), actual.layer_obstacle(node));
                assert_eq!(expected.source_zone(node), actual.source_zone(node));
            }
        }
    }
    same_zone_tables(expected, actual);
}

fn same_zone_tables(expected: &Grid, actual: &Grid) {
    let left = expected.source_zones.as_ref().unwrap();
    let right = actual.source_zones.as_ref().unwrap();
    let maximum = left.upper(15) + 1;
    assert_eq!(maximum, right.upper(15) + 1);
    for layer in 0..=15 {
        assert_eq!(left.upper(layer), right.upper(layer));
    }
    for zone in 0..maximum {
        assert_eq!(left.terrain(zone), right.terrain(zone));
    }
    for crusher in [false, true] {
        for mask in MASKS {
            for zone in 0..maximum {
                assert_eq!(
                    left.effective_global(mask, crusher, zone),
                    right.effective_global(mask, crusher, zone)
                );
            }
            for y in 0..expected.height {
                for x in 0..expected.width {
                    let cell = Cell { x, y };
                    assert_eq!(
                        left.effective_block(expected, mask, crusher, cell),
                        right.effective_block(actual, mask, crusher, cell)
                    );
                    assert_eq!(
                        expected.bridge_block_interacts(cell),
                        actual.bridge_block_interacts(cell)
                    );
                }
            }
        }
    }
}

fn same_infos(expected: &[(LayerCell, CellInfo)], actual: &[(LayerCell, CellInfo)]) {
    assert_eq!(expected.len(), actual.len());
    for ((left_node, left), (right_node, right)) in expected.iter().zip(actual) {
        assert!(left_node == right_node);
        assert!(left.coordinate == right.coordinate);
        assert_eq!((left.cost, left.total), (right.cost, right.total));
    }
}

fn same_search(expected: &SearchResult, actual: &SearchResult) {
    assert_eq!(expected.cells, actual.cells);
    same_infos(&expected.info_cleanup, &actual.info_cleanup);
    same_infos(&expected.retained_infos, &actual.retained_infos);
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
        _ => panic!("batching changed route admission"),
    }
}

fn crossing(expected: &Grid, actual: &Grid) -> bool {
    let start = LayerCell {
        cell: Cell { x: 10, y: 10 },
        layer: 1,
    };
    let goal = LayerCell {
        cell: Cell { x: 30, y: 10 },
        layer: 1,
    };
    let units = BTreeMap::new();
    let query = LayerQuery {
        start,
        goal,
        radius: 0,
        centered: true,
        closest_cost: None,
    };
    let left = expected.path_for_layers_counted(&query, &units, &BTreeMap::new());
    let right = actual.path_for_layers_counted(&query, &units, &BTreeMap::new());
    same_search(&left, &right);
    let left_cost = expected.path_cost_counted(start, goal, &units);
    let right_cost = actual.path_cost_counted(start, goal, &units);
    assert_eq!(left_cost.cost, right_cost.cost);
    same_search(&left_cost.search, &right_cost.search);
    left.route.is_some()
}

#[test]
fn unpublished_navigation_batch_preserves_cells_zones_and_routes_through_bridge_changes() {
    let (mut terrain, layer) = terrain();
    for phase in 0..4 {
        terrain.set_bridge_destroyed(layer, phase == 1).unwrap();
        for cliff in [false, true] {
            let (eager, batched) = snapshots(&terrain, layer, cliff, phase, true);
            same_grid(&eager, &batched);
            assert_eq!(crossing(&eager, &batched), phase != 1);
            assert_eq!(batched.is_blocked(Cell { x: 4, y: 6 }), phase < 2);
            assert!(batched.layer_obstacle(LayerCell {
                cell: Cell { x: 3, y: 4 },
                layer
            }));
            assert!(matches!(
                batched.source_cell_kind(LayerCell {
                    cell: Cell { x: 4, y: 5 },
                    layer: 1
                }),
                Some(CellType::Obstacle)
            ));
        }
    }
}

#[test]
fn navigation_batch_retains_ordered_partial_zone_inheritance_after_obstacle_removal() {
    let (terrain, layer) = terrain();
    for cliff in [false, true] {
        let (previous_eager, previous_batched) = snapshots(&terrain, layer, cliff, 0, true);
        let (mut eager, mut batched) = snapshots(&terrain, layer, cliff, 2, true);
        let events = [ZoneModification {
            update_bounds: Some([[4, 6], [5, 6]]),
            cells: vec![
                ZoneCellChange {
                    cell: Cell { x: 4, y: 6 },
                    kind: CellType::Clear,
                    fence: false,
                    reset_zone: true,
                },
                ZoneCellChange {
                    cell: Cell { x: 5, y: 6 },
                    kind: CellType::Obstacle,
                    fence: false,
                    reset_zone: true,
                },
            ],
            post_cells: Vec::new(),
            pinched: Vec::new(),
            force_dirty: true,
        }];
        assert_eq!(
            eager.inherit_source_zones(&previous_eager, &events),
            batched.inherit_source_zones(&previous_batched, &events)
        );
        same_grid(&eager, &batched);
        assert!(crossing(&eager, &batched));
        eager.recalculate_source_zones();
        batched.recalculate_source_zones();
        same_grid(&eager, &batched);
    }
}

#[test]
fn live_navigation_inherits_previous_cache_without_discarded_full_classification() {
    let (mut terrain, layer) = terrain();
    let (previous, _) = snapshots(&terrain, layer, false, 0, true);
    // Live normal and cliff snapshots both inherit the previous normal cache,
    // preserving partial source-zone changes until the scheduled full rebuild.
    for phase in 0..4 {
        terrain.set_bridge_destroyed(layer, phase == 1).unwrap();
        for cliff in [false, true] {
            let (mut formerly_classified, mut inherited) =
                snapshots(&terrain, layer, cliff, phase, false);
            let modifications = [
                ZoneModification {
                    cells: vec![ZoneCellChange {
                        cell: Cell { x: 4, y: 6 },
                        kind: if phase < 2 {
                            CellType::Obstacle
                        } else {
                            CellType::Clear
                        },
                        fence: false,
                        reset_zone: true,
                    }],
                    update_bounds: Some([[4, 6], [5, 6]]),
                    post_cells: vec![ZoneCellChange {
                        cell: Cell { x: 5, y: 6 },
                        kind: if phase == 1 || phase == 2 {
                            CellType::Obstacle
                        } else {
                            CellType::Clear
                        },
                        fence: false,
                        reset_zone: false,
                    }],
                    pinched: vec![(Cell { x: 7, y: 15 }, true)],
                    force_dirty: true,
                },
                ZoneModification {
                    cells: Vec::new(),
                    update_bounds: None,
                    post_cells: Vec::new(),
                    pinched: vec![(Cell { x: 7, y: 15 }, false)],
                    force_dirty: false,
                },
            ];
            assert_eq!(
                formerly_classified.inherit_source_zones(&previous, &modifications),
                inherited.inherit_source_zones(&previous, &modifications),
            );
            same_grid(&formerly_classified, &inherited);
            assert_eq!(crossing(&formerly_classified, &inherited), phase != 1);
            formerly_classified.recalculate_source_zones();
            inherited.recalculate_source_zones();
            same_grid(&formerly_classified, &inherited);
            assert_eq!(crossing(&formerly_classified, &inherited), phase != 1);
        }
    }
}

#[test]
fn obsolete_cache_removal_preserves_empty_and_low_bridge_states() {
    use crate::terrain::BridgeGeometry;
    for deck in [5.0, 20.0] {
        let mut terrain = TerrainMap::classify(40, 20, [], []).unwrap();
        let layer = terrain
            .add_bridge(
                BridgeGeometry {
                    from: [80.0, 100.0, deck],
                    to: [320.0, 100.0, deck],
                    corners: [
                        [80.0, 60.0, deck],
                        [80.0, 140.0, deck],
                        [320.0, 60.0, deck],
                        [320.0, 140.0, deck],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        for destroyed in [false, true] {
            terrain.set_bridge_destroyed(layer, destroyed).unwrap();
            for occupied in [false, true] {
                // Ground was classified before this bridge was installed. The
                // low deck therefore inserts new blocked cells in the setter.
                let base = Grid::new(40, 20, []).unwrap();
                let ground: Vec<_> = occupied
                    .then_some(Cell { x: 4, y: 6 })
                    .into_iter()
                    .collect();
                let upper: Vec<_> = occupied
                    .then_some(LayerCell {
                        cell: Cell { x: 20, y: 10 },
                        layer,
                    })
                    .into_iter()
                    .collect();
                let fences: Vec<_> = occupied
                    .then_some(LayerCell {
                        cell: Cell { x: 3, y: 4 },
                        layer,
                    })
                    .into_iter()
                    .collect();
                let eager = base
                    .with_obstacles(ground.iter().copied())
                    .with_bridge_layers(&terrain)
                    .with_layer_obstacles(upper.iter().copied())
                    .with_fence_cells(fences.iter().copied());
                let mut unpublished =
                    base.with_navigation_obstacles(ground, &terrain, upper, fences);
                unpublished.recalculate_source_zones();
                same_grid(&eager, &unpublished);
                assert_eq!(unpublished.is_blocked(Cell { x: 19, y: 10 }), deck < 10.0);
                crossing(&eager, &unpublished);
            }
        }
    }
}
