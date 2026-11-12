//! `findAttackPath` layer connectors, quick probes, range goals and INFO limits.
use super::{
    BTreeMap, BTreeSet, Grid, LayerCell, LayerQuery, Reverse, Route, RouteUnits, Search,
    SearchResult,
};
use crate::grid::search::attack::{Attack, AttackPoint, truncate, world_cell};
use crate::grid::search::{cell_position, squared_distance};

impl Grid {
    pub(crate) fn attack_search_layers_counted(
        &self,
        attack: &Attack<'_>,
        query: &LayerQuery,
        units: &BTreeMap<u8, RouteUnits>,
        unavailable: &BTreeMap<u8, BTreeSet<super::Cell>>,
        mut view: impl FnMut(AttackPoint) -> bool,
    ) -> SearchResult {
        let shift = if attack.centered { 5.0 } else { 0.0 };
        let position = [attack.from[0] + shift, attack.from[1] + shift];
        let Some(cell) = world_cell(self, position.map(|value| value.max(0.0)), true) else {
            return SearchResult::empty();
        };
        let start = self.resolve_layer(LayerCell {
            cell,
            layer: query.start.layer,
        });
        // findAttackPath reserves its heuristic destination INFO on GROUND,
        // even when the victim and candidate firing positions use a bridge.
        let goal = LayerCell {
            cell: query.goal.cell,
            layer: 1,
        };
        let Some(mut search) = Search::new(
            self,
            &LayerQuery {
                start,
                goal,
                radius: attack.radius,
                centered: attack.centered,
                closest_cost: None,
            },
            units,
            false,
            false,
        ) else {
            return SearchResult::empty();
        };
        search.attack_cost = Some((
            truncate(f64::from(attack.distance)) + 30,
            if attack.vehicle { 30 } else { 10 },
        ));
        if !search.insert(start, None, 0, 0) {
            return search.result(None, false);
        }
        if !self.contains(goal.cell) || !search.reserve_info(goal) {
            return SearchResult {
                route: None,
                cells: 0,
                info_cleanup: Vec::new(),
                retained_infos: search.info_cleanup(),
            };
        }
        let mut allocations = 0;
        while let Some(Reverse((total, insertion, node))) = search.open.pop() {
            if search.nodes[&node].insertion != insertion {
                continue;
            }
            let coordinate = search.coordinate(node);
            let point = cell_position(coordinate.cell, attack.centered);
            if node != start
                && squared_distance(point, position) >= 25.0
                && attack.in_range(point)
                && self.layer_destination_clear(coordinate, attack.radius, attack.centered)
                && !unavailable
                    .get(&node.layer)
                    .is_some_and(|cells| cells.contains(&coordinate.cell))
                && view(AttackPoint {
                    layer: node.layer,
                    position: point,
                    on_grid: true,
                })
            {
                let endpoint = if attack.vehicle {
                    search.strip_attack_tail(node)
                } else {
                    node
                };
                return search.result(Some(endpoint), true);
            }
            if allocations < 2500 {
                search.connect(node, total);
                allocations += search.expand_neighbors(node);
            }
        }
        search.result(None, false)
    }
    pub(crate) fn quick_attack_layers(
        &self,
        attack: &Attack<'_>,
        query: &LayerQuery,
        unavailable: &BTreeMap<u8, BTreeSet<super::Cell>>,
        view: &mut impl FnMut(AttackPoint) -> bool,
    ) -> Option<Route> {
        let delta = [
            attack.target[0] - attack.from[0],
            attack.target[1] - attack.from[1],
        ];
        let length = squared_distance(attack.target, [attack.from[0], attack.from[1]]).sqrt();
        let step = if length > 0.0 {
            delta.map(|value| value / length * 10.0)
        } else {
            [0.0; 2]
        };
        for index in 1..10_u16 {
            let offset = step.map(|step| step * f32::from(index) * 0.5);
            let point = [attack.from[0] + offset[0], attack.from[1] + offset[1]];
            let Some(cell) = world_cell(self, point, false) else {
                break;
            };
            let node = self.resolve_layer(LayerCell {
                cell,
                layer: query.start.layer,
            });
            if !self.layer_destination_clear(node, attack.radius, attack.centered)
                || unavailable
                    .get(&node.layer)
                    .is_some_and(|cells| cells.contains(&cell))
            {
                break;
            }
            if attack.in_range(point)
                && view(AttackPoint {
                    layer: node.layer,
                    position: point,
                    on_grid: false,
                })
            {
                let start = world_cell(self, [attack.from[0], attack.from[1]], false)?;
                let mut route = Route::direct_on(
                    LayerCell {
                        cell: start,
                        layer: query.start.layer,
                    },
                    node,
                );
                route.exact_endpoint = Some([point[0], point[1], attack.from[2]]);
                route.quick_start = Some(attack.from);
                return Some(route);
            }
        }
        None
    }
}

impl Search<'_> {
    fn strip_attack_tail(&self, end: LayerCell) -> LayerCell {
        let mut current = Some(end);
        let mut last_blocked = None;
        let mut large = false;
        for _ in 0..12 {
            let Some(node) = current else {
                break;
            };
            let (radius, centered) = if large {
                (self.radius, self.centered)
            } else {
                (0, true)
            };
            let occupied = self.occupied_at(node, radius, centered);
            let idle = self
                .units
                .get(&node.layer)
                .is_some_and(|units| units.idle_positions.contains(&node.cell));
            let ally = (occupied.0 || self.goal_occupied(node, radius, centered)) && !idle;
            let above = radius + i32::from(centered);
            let inside = i32::from(node.cell.x) >= radius
                && i32::from(node.cell.y) >= radius
                && i32::from(node.cell.x) + above <= i32::from(self.grid.width)
                && i32::from(node.cell.y) + above <= i32::from(self.grid.height);
            large = !inside || occupied.2 || ally;
            if large {
                last_blocked = Some(node);
            }
            current = self.nodes[&node].parent;
        }
        last_blocked.map_or(end, |node| self.nodes[&node].parent.unwrap_or(node))
    }
}
