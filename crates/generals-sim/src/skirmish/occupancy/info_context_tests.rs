use crate::grid::{Cell, CellInfo, LayerCell, SearchResult};
use crate::skirmish::{Kind, Skirmish, tests};
use std::collections::BTreeMap;
use std::sync::Arc;

fn fixture() -> (Skirmish, [usize; 3]) {
    let mut game = tests::game();
    let layer = game
        .movement_terrain
        .add_bridge(
            crate::terrain::BridgeGeometry {
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
    game.rebuild_navigation();
    let actors = [Kind::Builder, Kind::Infantry, Kind::Tank].map(|kind| {
        let kind = game.type_for(0, kind).unwrap();
        game.spawn(kind, 0, [185.0, 255.0], true)
    });
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[infantry].crush.crushable_level = 1;
    game.spawn(infantry, 1, [285.0, 255.0], true);
    let ally = game.spawn(infantry, 0, [265.0, 255.0], true);
    game.move_to(ally, [365.0, 255.0]).unwrap();
    let tank = game.entities[actors[2]].type_index;
    game.catalog[tank].crush.crusher_level = 2;
    game.refresh_occupancy();
    // Historical path INFO carries allocation coordinates/costs independently
    // of the current actor-relative claim classes, including the bridge deck.
    for y in 0..40 {
        for x in 0..50 {
            for current in [1, layer] {
                let node = LayerCell {
                    cell: Cell { x, y },
                    layer: current,
                };
                if game.navigation.resolve_layer(node) == node {
                    game.occupancy.claim_info(node);
                }
            }
        }
    }
    let retained = LayerCell {
        cell: Cell { x: 45, y: 25 },
        layer: 1,
    };
    Arc::make_mut(&mut game.occupancy.infos)
        .get_mut(&retained)
        .unwrap()
        .cost = 77;
    Arc::make_mut(&mut game.occupancy.infos)
        .get_mut(&retained)
        .unwrap()
        .total = 91;
    (game, actors)
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
        (None, None) => {}
        (Some(left), Some(right)) => {
            assert!(left.raw == right.raw && left.optimized == right.optimized);
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
        _ => panic!("INFO sharing changed route admission"),
    }
}

fn same_retained(left: &BTreeMap<LayerCell, CellInfo>, right: &BTreeMap<LayerCell, CellInfo>) {
    assert_eq!(left.len(), right.len());
    for ((left_node, left), (right_node, right)) in left.iter().zip(right) {
        assert!(left_node == right_node);
        assert!(left.coordinate == right.coordinate);
        assert_eq!((left.cost, left.total), (right.cost, right.total));
    }
}

fn owned_route_context(game: &Skirmish, actor: usize) -> BTreeMap<u8, crate::grid::RouteUnits> {
    let (mut full, _) = game.layer_route_context(actor);
    // Independent old producer: materialize each layer from authoritative
    // INFO with the original filters, preserving actor-relative flags.
    for (&layer, units) in &mut full {
        units.shared_info = None;
        units.info_cells = game
            .occupancy
            .infos
            .keys()
            .filter(|node| node.layer == layer)
            .map(|node| node.cell)
            .collect();
        units.info_records = game
            .occupancy
            .infos
            .iter()
            .filter_map(|(&node, &info)| (node.layer == layer).then_some((node.cell, info)))
            .collect();
    }
    full
}

#[test]
fn info_only_context_preserves_source_costs_hierarchy_and_cleanup_for_real_claim_classes() {
    let (mut game, actors) = fixture();
    for actor in actors {
        for (from, to) in [
            ([185.0, 255.0, 0.0], [455.0, 255.0, 0.0]),
            ([150.0, 100.0, 20.0], [250.0, 100.0, 20.0]),
        ] {
            let full = owned_route_context(&game, actor);
            let narrow = game.layer_path_info();
            assert!(
                full[&1].fixed_allies.len()
                    + full[&1].moving_allies.len()
                    + full[&1].fixed_enemies.len()
                    > 0
            );
            assert!(full.keys().eq(narrow.keys()));
            for (&layer, info) in &narrow {
                assert_eq!(full[&layer].info_available, info.info_available);
                assert!(
                    full[&layer]
                        .info_cells
                        .iter()
                        .copied()
                        .eq(info.retained_cells())
                );
                for cell in info.retained_cells() {
                    let expected = full[&layer].info_records[&cell];
                    let actual = info.info_record(cell).unwrap();
                    assert!(expected.coordinate == actual.coordinate);
                    assert_eq!((expected.cost, expected.total), (actual.cost, actual.total));
                }
                assert!(
                    info.fixed_allies.is_empty()
                        && info.moving_allies.is_empty()
                        && info.fixed_enemies.is_empty()
                );
            }
            let query = game.hierarchy_query(actor, from, to);
            let expected = game.navigation.hierarchical_corridor(&query, &full);
            let actual = game.navigation.hierarchical_corridor(&query, &narrow);
            assert!(
                expected.blocks.is_some(),
                "fixture must exercise an admitted hierarchy corridor"
            );
            assert!(expected.blocks == actual.blocks);
            same_search(&expected.search, &actual.search);
            let expected_cost = game
                .navigation
                .path_cost_counted(query.start, query.goal, &full);
            let actual_cost = game
                .navigation
                .path_cost_counted(query.start, query.goal, &narrow);
            assert!(expected_cost.cost.is_some());
            assert_eq!(expected_cost.cost, actual_cost.cost);
            same_search(&expected_cost.search, &actual_cost.search);
            let (shared_full, unavailable) = game.layer_route_context(actor);
            let movement = crate::grid::LayerQuery {
                start: query.start,
                goal: query.goal,
                radius: 0,
                centered: true,
                closest_cost: None,
            };
            same_search(
                &game
                    .navigation
                    .path_for_layers_counted(&movement, &full, &unavailable),
                &game
                    .navigation
                    .path_for_layers_counted(&movement, &shared_full, &unavailable),
            );
            same_search(
                &game
                    .navigation
                    .group_path_counted(query.start, query.goal, &full),
                &game
                    .navigation
                    .group_path_counted(query.start, query.goal, &shared_full),
            );
            drop(shared_full);
            drop(narrow);
            let before = Arc::clone(&game.occupancy.infos);
            game.finish_path_search(expected.search);
            let committed = Arc::clone(&game.occupancy.infos);
            game.occupancy.infos = before;
            game.finish_path_search(actual.search);
            same_retained(&committed, &game.occupancy.infos);
            // The next query snapshots the newly committed INFO, never a cache
            // from before source releaseInfo/retention callbacks.
            let refreshed = game.layer_path_info();
            assert_eq!(
                refreshed[&1].info_available,
                crate::grid::CELL_INFO_CAPACITY - game.occupancy.infos.len()
            );
        }
    }
}

#[test]
fn retained_info_snapshot_is_shared_immutable_and_next_query_is_fresh() {
    let (mut game, _) = fixture();
    let node = LayerCell {
        cell: Cell { x: 45, y: 25 },
        layer: 1,
    };
    let previous = game.layer_path_info();
    let snapshot = previous[&1].shared_info.as_ref().unwrap();
    assert!(Arc::ptr_eq(&snapshot.1, &game.occupancy.infos));
    assert!(previous[&1].info_cells.is_empty() && previous[&1].info_records.is_empty());
    let available = previous[&1].info_available;
    let mut changed = game.occupancy.infos[&node];
    changed.coordinate.cell.x -= 1;
    changed.cost = 123;
    changed.total = 456;
    // A retained result can outlive a query. COW protects that exact snapshot;
    // ordinary search entry points release theirs before applying these writes.
    game.finish_path_search(SearchResult {
        route: None,
        cells: 0,
        info_cleanup: Vec::new(),
        retained_infos: vec![(node, changed)],
    });
    let next = game.layer_path_info();
    assert_eq!(previous[&1].info_record(node.cell).unwrap().cost, 77);
    let actual = next[&1].info_record(node.cell).unwrap();
    assert!(actual.coordinate == changed.coordinate);
    assert_eq!((actual.cost, actual.total), (123, 456));
    assert_eq!(next[&1].info_available, available);
    drop(next);
    game.finish_path_search(SearchResult {
        route: None,
        cells: 0,
        info_cleanup: vec![(node, changed)],
        retained_infos: Vec::new(),
    });
    let after_release = game.layer_path_info();
    assert!(!after_release[&1].has_info(node.cell));
    assert!(previous[&1].has_info(node.cell));
    assert_eq!(after_release[&1].info_available, available + 1);
}

#[test]
fn ordinary_queries_release_info_snapshots_before_source_cleanup() {
    let (mut game, actors) = fixture();
    let original = Arc::as_ptr(&game.occupancy.infos);
    let query = game.hierarchy_query(actors[2], [185.0, 255.0, 0.0], [455.0, 255.0, 0.0]);
    let result = game.hierarchical_path(&query);
    assert!(result.blocks.is_some());
    assert_eq!(Arc::strong_count(&game.occupancy.infos), 1);
    assert_eq!(Arc::as_ptr(&game.occupancy.infos), original);
    let result = game.movement_path_counted(actors[2], [185.0, 255.0], [455.0, 255.0]);
    assert!(result.route.is_some());
    assert_eq!(Arc::strong_count(&game.occupancy.infos), 1);
    assert_eq!(Arc::as_ptr(&game.occupancy.infos), original);
    // Force the native shared-ground-path branch without changing its search.
    game.ai_rules.min_distance_for_group = 0.0;
    game.ai_rules.distance_requires_group = 0.0;
    game.group_move_to(&actors, [455.0, 255.0], false).unwrap();
    assert_eq!(Arc::strong_count(&game.occupancy.infos), 1);
    assert_eq!(Arc::as_ptr(&game.occupancy.infos), original);
}

#[test]
fn shared_info_matches_owned_single_layer_search_at_capacity_boundaries() {
    use crate::grid::{Grid, RouteUnits};
    let grid = Grid::new(24, 18, [Cell { x: 12, y: 8 }]).unwrap();
    let start = Cell { x: 2, y: 8 };
    let goal = Cell { x: 21, y: 8 };
    let mut records = BTreeMap::new();
    for cell in [start, goal, Cell { x: 3, y: 8 }] {
        let node = LayerCell { cell, layer: 1 };
        let mut info = CellInfo::new(node);
        info.cost = 30;
        info.total = 80;
        records.insert(node, info);
    }
    // Another layer at the same cell must not overwrite this layer's INFO.
    records.insert(
        LayerCell {
            cell: start,
            layer: 2,
        },
        CellInfo {
            coordinate: LayerCell {
                cell: goal,
                layer: 2,
            },
            cost: 999,
            total: 1000,
        },
    );
    let records = Arc::new(records);
    for available in [0, 1, crate::grid::CELL_INFO_CAPACITY] {
        let mut owned = RouteUnits::new();
        owned.info_available = available;
        owned.info_cells = records
            .keys()
            .filter(|node| node.layer == 1)
            .map(|node| node.cell)
            .collect();
        owned.info_records = records
            .iter()
            .filter_map(|(&node, &info)| (node.layer == 1).then_some((node.cell, info)))
            .collect();
        let mut shared = RouteUnits::new();
        shared.info_available = available;
        shared.shared_info = Some((1, Arc::clone(&records)));
        same_search(
            &grid.path_for_counted(1, start, goal, 0, true, &owned),
            &grid.path_for_counted(1, start, goal, 0, true, &shared),
        );
        assert!(owned.retained_cells().eq(shared.retained_cells()));
    }
}

#[test]
fn group_path_construction_keeps_width_flags_when_info_snapshot_is_released() {
    use crate::grid::{Grid, RouteUnits};
    let grid = Grid::new(32, 24, []).unwrap();
    let start = LayerCell {
        cell: Cell { x: 5, y: 12 },
        layer: 1,
    };
    let goal = LayerCell {
        cell: Cell { x: 26, y: 12 },
        layer: 1,
    };
    let info = Arc::new(BTreeMap::from([(start, CellInfo::new(start))]));
    let mut context = RouteUnits::new();
    context.shared_info = Some((1, Arc::clone(&info)));
    context.width_blockers.insert(Cell { x: 15, y: 12 });
    let mut contexts = BTreeMap::from([(1, context)]);
    let route = grid
        .group_path_counted(start, goal, &contexts)
        .route
        .unwrap();
    let position = |node: LayerCell| {
        [
            f32::from(node.cell.x) * 10.0,
            f32::from(node.cell.y) * 10.0,
            0.0,
        ]
    };
    let expected = grid.construct_group_path(
        &route.raw,
        &route.raw_flags,
        position(start),
        &contexts
            .iter()
            .map(|(&layer, units)| (layer, units))
            .collect(),
        position,
    );
    contexts.get_mut(&1).unwrap().shared_info = None;
    assert_eq!(Arc::strong_count(&info), 1);
    assert!(contexts[&1].width_blockers.contains(&Cell { x: 15, y: 12 }));
    let actual = grid.construct_group_path(
        &route.raw,
        &route.raw_flags,
        position(start),
        &contexts
            .iter()
            .map(|(&layer, units)| (layer, units))
            .collect(),
        position,
    );
    assert!(expected.optimized == actual.optimized);
    assert_eq!(expected.nodes.len(), actual.nodes.len());
    for (expected, actual) in expected.nodes.iter().zip(&actual.nodes) {
        assert_eq!(
            expected.position.map(f32::to_bits),
            actual.position.map(f32::to_bits)
        );
        assert_eq!(
            (
                expected.layer,
                expected.previous_layer,
                expected.can_optimize
            ),
            (actual.layer, actual.previous_layer, actual.can_optimize)
        );
    }
}
