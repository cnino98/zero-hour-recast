//! SH AIPathfind.cpp11094–11298, findSafePath with no destination heuristic.
use super::{BTreeMap, BTreeSet, Grid, LayerQuery, Reverse, RouteUnits, Search, SearchResult};
use crate::grid::search::{cell_position, squared_distance};

impl Grid {
    pub(crate) fn safe_path_counted(
        &self,
        query: &LayerQuery,
        repulsors: [[f32; 2]; 2],
        safe_range: f32,
        units: &BTreeMap<u8, RouteUnits>,
        unavailable: &BTreeMap<u8, BTreeSet<super::Cell>>,
    ) -> SearchResult {
        let Some(mut search) = Search::new(self, query, units, false, false) else {
            return SearchResult::empty();
        };
        search.use_heuristic = false;
        if !search.insert(query.start, None, 0, 0) {
            return search.result(None, false);
        }
        let mut allocations = 0;
        let mut farthest_squared = 0.0;
        let safe_squared = safe_range * safe_range;
        while let Some(Reverse((total, insertion, node))) = search.open.pop() {
            if search.nodes[&node].insertion != insertion {
                continue;
            }
            let coordinate = search.coordinate(node);
            let point = cell_position(coordinate.cell, query.centered);
            let squared =
                squared_distance(point, repulsors[0]).min(squared_distance(point, repulsors[1]));
            let mut acceptable =
                squared > safe_squared || search.open.is_empty() && allocations > 0;
            if squared > farthest_squared {
                farthest_squared = squared;
                if allocations > 2000 {
                    acceptable = true;
                }
            }
            if acceptable
                && self.layer_destination_clear(coordinate, query.radius, query.centered)
                && !unavailable
                    .get(&coordinate.layer)
                    .is_some_and(|cells| cells.contains(&coordinate.cell))
            {
                return search.result(Some(node), true);
            }
            search.connect(node, total);
            allocations += search.expand_neighbors(node);
        }
        search.result(None, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Cell, LayerCell};

    #[test]
    fn safe_search_checks_both_repulsors_and_does_not_take_a_claimed_destination() {
        let grid = Grid::new(21, 21, []).unwrap();
        let start = LayerCell {
            cell: Cell { x: 10, y: 10 },
            layer: 1,
        };
        let query = LayerQuery {
            start,
            goal: start,
            radius: 0,
            centered: true,
            closest_cost: None,
        };
        let units = BTreeMap::from([(1, RouteUnits::new())]);
        let repulsors = [[85.0, 105.0], [125.0, 105.0]];
        let first = grid
            .safe_path_counted(&query, repulsors, 40.0, &units, &BTreeMap::new())
            .route
            .unwrap();
        let endpoint = *first.raw.last().unwrap();
        let point = cell_position(endpoint.cell, true);
        assert!(
            repulsors
                .iter()
                .all(|&repulsor| squared_distance(point, repulsor) > 1600.0)
        );
        let unavailable = BTreeMap::from([(1, BTreeSet::from([endpoint.cell]))]);
        let second = grid
            .safe_path_counted(&query, repulsors, 40.0, &units, &unavailable)
            .route
            .unwrap();
        assert!(second.raw.last().unwrap().cell != endpoint.cell);
    }

    #[test]
    fn safe_search_accepts_the_single_remaining_frontier_before_expanding_it() {
        // SH findSafePath checks open-list exhaustion after popping the cell,
        // before examineNeighboringCells. A one-cell frontier is accepted even
        // when a legal, more distant cell has not yet been discovered.
        for width in [3, 5] {
            let grid = Grid::new(width, 1, []).unwrap();
            let start = LayerCell {
                cell: Cell { x: 0, y: 0 },
                layer: 1,
            };
            let next = LayerCell {
                cell: Cell { x: 1, y: 0 },
                layer: 1,
            };
            let query = LayerQuery {
                start,
                goal: start,
                radius: 0,
                centered: true,
                closest_cost: None,
            };
            let units = BTreeMap::from([(1, RouteUnits::new())]);
            let result =
                grid.safe_path_counted(&query, [[5.0, 5.0]; 2], 1000.0, &units, &BTreeMap::new());
            let route = result.route.unwrap();
            assert!(route.raw == [start, next]);
            assert!(squared_distance(cell_position(next.cell, true), [5.0, 5.0]) < 1_000_000.0);
            assert_eq!(result.cells, 1);
            assert_eq!(result.info_cleanup.len(), 2);
            assert!(
                result
                    .info_cleanup
                    .iter()
                    .all(|(node, _)| *node == start || *node == next)
            );
            assert!(grid.layer_destination_clear(
                LayerCell {
                    cell: Cell { x: 2, y: 0 },
                    layer: 1
                },
                0,
                true,
            ));
        }
    }

    #[test]
    fn allocation_failure_stops_before_later_neighbor_with_retained_info() {
        let grid = Grid::new(3, 3, []).unwrap();
        let start = LayerCell {
            cell: Cell { x: 1, y: 1 },
            layer: 1,
        };
        let north = Cell { x: 1, y: 2 };
        let query = LayerQuery {
            start,
            goal: start,
            radius: 0,
            centered: true,
            closest_cost: None,
        };
        let mut context = RouteUnits::new();
        context.info_available = 0;
        context.info_cells.extend([start.cell, north]);
        let units = BTreeMap::from([(1, context)]);
        let result =
            grid.safe_path_counted(&query, [[15.0, 15.0]; 2], 1000.0, &units, &BTreeMap::new());
        assert!(result.route.is_none());
        assert!(
            !result
                .info_cleanup
                .iter()
                .any(|(node, _)| node.cell == north)
        );
    }
}
