//! Native `getCell` layer fallback and `checkChangeLayers`, SH 2bfaea60.
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::sync::Arc;

use super::search::{NEIGHBORS, heuristic, line_cells, line_cells_iter, step_cost};
use super::{Cell, Grid, LayerCell, Route, RouteUnits, SearchResult};

mod attack;
mod occupancy;
pub(super) mod patch;
mod safe;

#[derive(Clone)]
pub(super) struct Layer {
    cells: BTreeSet<Cell>,
    blocked: BTreeSet<Cell>,
    connections: BTreeSet<Cell>,
    obstacles: BTreeSet<Cell>,
}

pub struct LayerQuery {
    pub start: LayerCell,
    pub goal: LayerCell,
    pub radius: i32,
    pub centered: bool,
    pub closest_cost: Option<f32>,
}

impl Grid {
    /// Prepare one unpublished navigation snapshot. These ordered mutations
    /// read cells/layers, never the intermediate source-zone cache. The caller
    /// must classify or inherit source zones before publishing the snapshot.
    pub(crate) fn with_navigation_obstacles(
        mut self,
        obstacles: impl IntoIterator<Item = Cell>,
        terrain: &crate::terrain::TerrainMap,
        layers: impl IntoIterator<Item = LayerCell>,
        fences: impl IntoIterator<Item = LayerCell>,
    ) -> Self {
        self.set_obstacles(obstacles);
        self.set_bridge_layers(terrain);
        self.set_layer_obstacles(layers);
        self.set_fence_cells(fences);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_bridge_layers(mut self, terrain: &crate::terrain::TerrainMap) -> Self {
        self.set_bridge_layers(terrain);
        self.recalculate_source_zones();
        self
    }

    fn set_bridge_layers(&mut self, terrain: &crate::terrain::TerrainMap) {
        self.pinched = Arc::new(BTreeSet::new());
        self.bridge_ends = Arc::new(BTreeMap::new());
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = Cell { x, y };
                if let Some(data) = terrain.cell(cell) {
                    let kind = if self.obstacles.contains(&cell) {
                        crate::terrain::CellType::Obstacle
                    } else {
                        terrain.path_kind(cell).unwrap_or(data.kind)
                    };
                    let index = self.source_index(cell);
                    Arc::make_mut(&mut self.source_kinds)[index] = kind;
                    if matches!(data.kind, crate::terrain::CellType::BridgeImpassable) {
                        Arc::make_mut(&mut self.blocked).insert(cell);
                    }
                    Arc::make_mut(&mut self.ground_heights)
                        [usize::from(y) * usize::from(self.width) + usize::from(x)] = data.height;
                    if data.pinched {
                        Arc::make_mut(&mut self.pinched).insert(cell);
                    }
                }
            }
        }
        self.layers = Arc::new(BTreeMap::new());
        self.ground_connections = Arc::new(BTreeMap::new());
        for layer in terrain.path_layers() {
            if terrain.bridge_layer_active(layer)
                && let Some(ends) = terrain.bridge_endpoint_cells(layer)
            {
                Arc::make_mut(&mut self.bridge_ends).insert(layer, ends);
            }
            let mut data = Layer {
                cells: BTreeSet::new(),
                blocked: BTreeSet::new(),
                connections: BTreeSet::new(),
                obstacles: BTreeSet::new(),
            };
            for y in 0..self.height {
                for x in 0..self.width {
                    let cell = Cell { x, y };
                    if let Some(upper) = terrain.bridge_cell(layer, cell)
                        && !matches!(upper.kind, crate::terrain::CellType::Impassable)
                    {
                        // PathfindLayer::getCell returns null for allocated
                        // IMPASSABLE padding, so getCell falls back to ground.
                        data.cells.insert(cell);
                        if !matches!(upper.kind, crate::terrain::CellType::Clear) {
                            data.blocked.insert(cell);
                        }
                        if upper.connect_ground {
                            data.connections.insert(cell);
                        }
                    }
                    if let Some(bridge) = terrain.ground_bridge(cell) {
                        Arc::make_mut(&mut self.ground_connections).insert(cell, bridge);
                    }
                }
            }
            Arc::make_mut(&mut self.layers).insert(layer, data);
        }
    }

    pub(crate) fn initial_tunneling(
        &self,
        start: LayerCell,
        radius: i32,
        centered: bool,
        units: &BTreeMap<u8, RouteUnits>,
    ) -> bool {
        self.layer_obstacle(start)
            || self.layer_blocked(start)
            || !self.layer_movement_bounds(start, radius, centered)
            || occupancy::screen(self, units, start, radius, centered).2
    }

    pub(crate) fn has_bridge_layers(&self) -> bool {
        !self.layers.is_empty()
    }

    fn set_layer_obstacles(&mut self, nodes: impl IntoIterator<Item = LayerCell>) {
        for node in nodes {
            let node = self.resolve_layer(node);
            if node.layer == 1 {
                Arc::make_mut(&mut self.obstacles).insert(node.cell);
                let index = self.source_index(node.cell);
                Arc::make_mut(&mut self.source_kinds)[index] = crate::terrain::CellType::Obstacle;
            } else if let Some(layer) = Arc::make_mut(&mut self.layers).get_mut(&node.layer) {
                layer.obstacles.insert(node.cell);
            }
        }
    }

    fn set_fence_cells(&mut self, nodes: impl IntoIterator<Item = LayerCell>) {
        self.fences = Arc::new(
            nodes
                .into_iter()
                .map(|node| self.resolve_layer(node))
                .collect(),
        );
    }

    // Former eager stages are the test-only oracle for unpublished snapshots.
    #[cfg(test)]
    pub(crate) fn with_layer_obstacles(&self, nodes: impl IntoIterator<Item = LayerCell>) -> Self {
        let mut grid = self.clone();
        grid.set_layer_obstacles(nodes);
        grid.recalculate_source_zones();
        grid
    }

    #[cfg(test)]
    pub(crate) fn with_fence_cells(mut self, nodes: impl IntoIterator<Item = LayerCell>) -> Self {
        self.set_fence_cells(nodes);
        self.recalculate_source_zones();
        self
    }

    pub(crate) fn without_layer_obstacles(
        &self,
        nodes: impl IntoIterator<Item = LayerCell>,
    ) -> Self {
        let mut grid = self.clone();
        for node in nodes {
            let node = grid.resolve_layer(node);
            // ignoreObstacle changes validMovementPosition, not CELL_OBSTACLE.
            // Its INFO retention and +1000 search cost remain authoritative.
            if grid.layer_obstacle(node) {
                grid.ignored_obstacles.insert(node);
            }
        }
        grid
    }

    pub(crate) fn layer_destination_clear(
        &self,
        node: LayerCell,
        radius: i32,
        centered: bool,
    ) -> bool {
        if node.layer == 1 {
            self.destination_clear(node.cell, radius, centered)
        } else {
            self.layer_clear(node, radius, centered)
        }
    }

    pub(crate) fn resolve_layer(&self, node: LayerCell) -> LayerCell {
        if node.layer == 1
            || self
                .layers
                .get(&node.layer)
                .is_some_and(|layer| layer.cells.contains(&node.cell))
        {
            node
        } else {
            LayerCell {
                cell: node.cell,
                layer: 1,
            }
        }
    }

    /// Canonical `PathfindCell` type after the source upper-layer fallback.
    pub(crate) fn source_cell_kind(&self, node: LayerCell) -> Option<crate::terrain::CellType> {
        if !self.contains(node.cell) {
            return None;
        }
        let node = self.resolve_layer(node);
        Some(if self.layer_obstacle(node) {
            crate::terrain::CellType::Obstacle
        } else if node.layer == 1 {
            self.source_kinds[self.source_index(node.cell)]
        } else if self.layers.get(&node.layer)?.blocked.contains(&node.cell) {
            crate::terrain::CellType::BridgeImpassable
        } else {
            crate::terrain::CellType::Clear
        })
    }

    pub(crate) fn ignored_obstacle(&self, node: LayerCell) -> bool {
        self.ignored_obstacles.contains(&self.resolve_layer(node))
    }

    pub(crate) fn layer_blocked(&self, node: LayerCell) -> bool {
        let node = self.resolve_layer(node);
        if self.ignored_obstacles.contains(&node) {
            false
        } else if node.layer == 1 {
            self.is_blocked(node.cell)
        } else {
            self.layers.get(&node.layer).is_none_or(|layer| {
                layer.blocked.contains(&node.cell) || layer.obstacles.contains(&node.cell)
            })
        }
    }

    pub(crate) fn layer_obstacle(&self, node: LayerCell) -> bool {
        let node = self.resolve_layer(node);
        if self.fences.contains(&node) {
            true
        } else if node.layer == 1 {
            self.obstacles.contains(&node.cell)
        } else {
            self.layers
                .get(&node.layer)
                .is_some_and(|layer| layer.obstacles.contains(&node.cell))
        }
    }

    pub(super) fn connection(&self, node: LayerCell) -> Option<LayerCell> {
        if node.layer == 1 {
            self.ground_connections
                .get(&node.cell)
                .map(|&layer| LayerCell {
                    cell: node.cell,
                    layer,
                })
        } else {
            self.layers
                .get(&node.layer)?
                .connections
                .contains(&node.cell)
                .then_some(LayerCell {
                    cell: node.cell,
                    layer: 1,
                })
        }
    }

    pub(crate) fn layer_line_passable(
        &self,
        from: LayerCell,
        to: LayerCell,
        radius: i32,
        centered: bool,
        mut fixed: impl FnMut(LayerCell) -> bool,
    ) -> bool {
        from.layer == to.layer
            && line_cells(self, from.cell, to.cell)
                .into_iter()
                .all(|cell| {
                    let node = self.resolve_layer(LayerCell {
                        cell,
                        layer: from.layer,
                    });
                    if self.layer_blocked(node)
                        || !self.layer_movement_bounds(node, radius, centered)
                    {
                        return false;
                    }
                    let above = radius + i32::from(centered);
                    (i32::from(cell.x) - radius..i32::from(cell.x) + above).all(|x| {
                        (i32::from(cell.y) - radius..i32::from(cell.y) + above).all(|y| {
                            self.signed_cell(x, y).is_some_and(|cell| {
                                !fixed(self.resolve_layer(LayerCell {
                                    cell,
                                    layer: from.layer,
                                }))
                            })
                        })
                    })
                })
    }

    fn layer_movement_bounds(&self, node: LayerCell, radius: i32, centered: bool) -> bool {
        let above = radius + i32::from(centered);
        i32::from(node.cell.x) >= radius
            && i32::from(node.cell.y) >= radius
            && i32::from(node.cell.x) + above <= i32::from(self.width)
            && i32::from(node.cell.y) + above <= i32::from(self.height)
    }

    fn layer_clear(&self, node: LayerCell, radius: i32, centered: bool) -> bool {
        let above = radius + i32::from(centered);
        !self.layer_blocked(node)
            && (i32::from(node.cell.x) - radius..i32::from(node.cell.x) + above).all(|x| {
                (i32::from(node.cell.y) - radius..i32::from(node.cell.y) + above).all(|y| {
                    self.signed_cell(x, y).is_some_and(|cell| {
                        !self.layer_blocked(LayerCell {
                            cell,
                            layer: node.layer,
                        })
                    })
                })
            })
    }

    pub(crate) fn path_for_layers_counted(
        &self,
        query: &LayerQuery,
        units: &BTreeMap<u8, RouteUnits>,
        unavailable: &BTreeMap<u8, BTreeSet<Cell>>,
    ) -> SearchResult {
        let LayerQuery {
            start,
            goal,
            radius,
            centered,
            closest_cost,
        } = *query;
        if !self.contains(start.cell) || !self.contains(goal.cell) {
            return SearchResult::empty();
        }
        let start = self.resolve_layer(start);
        let goal = self.resolve_layer(goal);
        let Some(mut search) = Search::new(
            self,
            &LayerQuery {
                start,
                goal,
                radius,
                centered,
                closest_cost,
            },
            units,
            true,
            true,
        ) else {
            return SearchResult::empty();
        };
        let started_stuck = search.tunneling;
        if closest_cost.is_none()
            && (!self.layer_destination_clear(goal, radius, centered)
                || unavailable
                    .get(&goal.layer)
                    .is_some_and(|cells| cells.contains(&goal.cell)))
        {
            return search.result(None, false);
        }
        if !search.insert(
            start,
            None,
            0,
            heuristic(search.coordinate(start).cell, search.coordinate(goal).cell),
        ) {
            return search.result(None, false);
        }
        let mut closest = None;
        let mut closest_score = f32::MAX;
        let mut closest_screen = f32::MAX;
        let mut found_goal = false;
        let mut neighbor_count = 0;
        let corridor = units.get(&1).and_then(|units| units.corridor.as_ref());
        while let Some(Reverse((total, insertion, node))) = search.open.pop() {
            if search.nodes[&node].insertion != insertion {
                continue;
            }
            let coordinate = search.coordinate(node);
            let clear = self.layer_clear(coordinate, radius, centered)
                && !unavailable
                    .get(&node.layer)
                    .is_some_and(|cells| cells.contains(&coordinate.cell))
                && !search.occupied(coordinate).2;
            if node == goal {
                if closest_cost.is_some() && closest.is_some() && !clear {
                    found_goal = true;
                    search.listed -= 1;
                    continue;
                }
                return search.result(Some(node), true);
            }
            if let Some(factor) = closest_cost {
                let dx = f32::from(coordinate.cell.x.abs_diff(search.coordinate(goal).cell.x));
                let dy = f32::from(coordinate.cell.y.abs_diff(search.coordinate(goal).cell.y));
                let xx = dx * dx;
                let yy = dy * dy;
                let distance = xx + yy;
                if clear && !search.tunneling && (!started_stuck || !self.layer_blocked(node)) {
                    closest_screen = closest_screen.min(distance);
                    let cost = super::search::source_real(search.nodes[&node].cost);
                    let weighted = (cost * (cost * (0.1_f32 * 0.1_f32))) * factor;
                    let score = distance + weighted;
                    if score < closest_score {
                        closest = Some(node);
                        closest_score = score;
                    }
                }
                if distance > closest_screen * 4.0
                    && (corridor.is_none() || neighbor_count > 2000 || closest_screen < 1000.0)
                {
                    continue;
                }
            }
            if found_goal {
                continue;
            }
            search.connect(node, total);
            if !search.tunneling && !search.units.values().any(|units| units.downhill_only) {
                search.expand_line(node);
            }
            neighbor_count += search.expand_neighbors(node);
        }
        search.result(closest, false)
    }
}

struct Node {
    coordinate: LayerCell,
    parent: Option<LayerCell>,
    cost: u32,
    total: u32,
    insertion: u64,
}
struct Search<'a> {
    grid: &'a Grid,
    goal: LayerCell,
    start: LayerCell,
    radius: i32,
    centered: bool,
    units: &'a BTreeMap<u8, RouteUnits>,
    occupancy: Vec<Option<Vec<u8>>>,
    nodes: BTreeMap<LayerCell, Node>,
    open: BinaryHeap<Reverse<(u32, u64, LayerCell)>>,
    sequence: u64,
    available: usize,
    allocated: usize,
    reserved: Option<LayerCell>,
    tunneling: bool,
    listed: usize,
    attack_cost: Option<(u32, u32)>,
    use_heuristic: bool,
}

impl<'a> Search<'a> {
    fn new(
        grid: &'a Grid,
        query: &LayerQuery,
        units: &'a BTreeMap<u8, RouteUnits>,
        tunneling: bool,
        reserve: bool,
    ) -> Option<Self> {
        let available = units
            .values()
            .map(|units| units.info_available)
            .min()
            .unwrap_or(super::CELL_INFO_CAPACITY);
        let mut search = Self {
            grid,
            goal: query.goal,
            start: query.start,
            radius: query.radius,
            centered: query.centered,
            units,
            occupancy: occupancy::build(grid, units),
            nodes: BTreeMap::new(),
            open: BinaryHeap::new(),
            sequence: 0,
            available,
            allocated: 0,
            reserved: None,
            tunneling: false,
            listed: 0,
            attack_cost: None,
            use_heuristic: true,
        };
        if reserve && !search.reserve_info(query.goal) {
            return None;
        }
        if tunneling {
            search.tunneling = grid.layer_obstacle(query.start)
                || grid.layer_blocked(query.start)
                || !grid.layer_movement_bounds(query.start, query.radius, query.centered)
                || search.occupied(query.start).2;
        }
        Some(search)
    }
    fn connect(&mut self, node: LayerCell, total: u32) {
        // Source inserts an equal-cost connector before examining XY neighbors.
        if let Some(next) = self.grid.connection(node)
            && !self.nodes.contains_key(&next)
        {
            self.insert(
                next,
                Some(node),
                self.nodes[&node].cost,
                total - self.nodes[&node].cost,
            );
        }
    }

    fn coordinate(&self, node: LayerCell) -> LayerCell {
        self.nodes.get(&node).map_or_else(
            || LayerCell {
                cell: self
                    .units
                    .get(&node.layer)
                    .and_then(|units| units.info_record(node.cell))
                    .map_or(node.cell, |info| info.coordinate.cell),
                layer: node.layer,
            },
            |record| record.coordinate,
        )
    }

    fn stored_cost(&self, node: LayerCell) -> u32 {
        self.nodes
            .get(&node)
            .map(|record| record.cost)
            .or_else(|| {
                self.units
                    .get(&node.layer)
                    .and_then(|units| units.info_record(node.cell))
                    .map(|info| info.cost)
            })
            .expect("native line predecessor has allocated INFO")
    }

    fn has_info(&self, node: LayerCell) -> bool {
        self.units
            .get(&node.layer)
            .is_some_and(|units| units.has_info(node.cell))
    }

    fn reserve_info(&mut self, node: LayerCell) -> bool {
        if !self.nodes.contains_key(&node) && !self.has_info(node) {
            if self.allocated == self.available {
                return false;
            }
            self.allocated += 1;
        }
        self.reserved = Some(node);
        true
    }

    fn goal_occupied(&self, node: LayerCell, radius: i32, centered: bool) -> bool {
        let above = radius + i32::from(centered);
        (i32::from(node.cell.x) - radius..i32::from(node.cell.x) + above).any(|x| {
            (i32::from(node.cell.y) - radius..i32::from(node.cell.y) + above).any(|y| {
                self.grid.signed_cell(x, y).is_some_and(|cell| {
                    let resolved = self.grid.resolve_layer(LayerCell {
                        cell,
                        layer: node.layer,
                    });
                    self.units
                        .get(&resolved.layer)
                        .is_some_and(|units| units.goal_flags.contains(&cell))
                })
            })
        })
    }

    fn insert(
        &mut self,
        node: LayerCell,
        parent: Option<LayerCell>,
        cost: u32,
        remaining: u32,
    ) -> bool {
        if !self.nodes.contains_key(&node) {
            if Some(node) != self.reserved && !self.has_info(node) {
                if self.allocated == self.available {
                    return false;
                }
                self.allocated += 1;
            }
            self.listed += 1;
        }
        self.sequence += 1;
        self.nodes.insert(
            node,
            Node {
                coordinate: self.coordinate(node),
                parent,
                cost,
                total: cost + remaining,
                insertion: self.sequence,
            },
        );
        self.open
            .push(Reverse((cost + remaining, self.sequence, node)));
        true
    }

    fn occupied(&self, node: LayerCell) -> (bool, bool, bool) {
        self.occupied_at(node, self.radius, self.centered)
    }

    fn occupied_at(&self, node: LayerCell, radius: i32, centered: bool) -> (bool, bool, bool) {
        let mut flags = 0;
        let above = radius + i32::from(centered);
        for x in i32::from(node.cell.x) - radius..i32::from(node.cell.x) + above {
            for y in i32::from(node.cell.y) - radius..i32::from(node.cell.y) + above {
                if let Some(cell) = self.grid.signed_cell(x, y) {
                    let node = self.grid.resolve_layer(LayerCell {
                        cell,
                        layer: node.layer,
                    });
                    if let Some(Some(cells)) = self.occupancy.get(usize::from(node.layer)) {
                        flags |= cells[self.grid.source_index(cell)];
                    }
                }
            }
        }
        (
            flags & occupancy::FIXED != 0,
            flags & occupancy::MOVING != 0,
            flags & occupancy::ENEMY != 0,
        )
    }

    fn expand_line(&mut self, start: LayerCell) {
        let mut cells = line_cells_iter(
            self.grid,
            self.coordinate(start).cell,
            self.coordinate(self.goal).cell,
        );
        let Some(mut previous) = cells.next() else {
            return;
        };
        for to in cells {
            let pair = [previous, to];
            previous = to;
            let from = self.grid.resolve_layer(LayerCell {
                cell: pair[0],
                layer: start.layer,
            });
            let to = self.grid.resolve_layer(LayerCell {
                cell: pair[1],
                layer: start.layer,
            });
            if (to.layer == 1
                && !super::hierarchy::passable(
                    self.units.get(&1).and_then(|units| units.corridor.as_ref()),
                    to.cell,
                ))
                || self.grid.layer_blocked(to)
                || !self
                    .grid
                    .layer_movement_bounds(to, self.radius, self.centered)
                || (to.layer == 1
                    && (self.grid.cliffs.contains(&to.cell)
                        || self.grid.pinched.contains(&to.cell)))
                || self.occupied(to).0
                || self.occupied(to).2
            {
                break;
            }
            let cost = self.stored_cost(from) + 5;
            if self.nodes.get(&to).is_some_and(|node| node.cost <= cost) {
                continue;
            }
            if !self.insert(
                to,
                Some(from),
                cost,
                heuristic(self.coordinate(to).cell, self.coordinate(self.goal).cell),
            ) {
                break;
            }
        }
    }

    fn expand_neighbors(&mut self, parent: LayerCell) -> u32 {
        let mut created = 0;
        let mut admitted = [false; 8];
        let coordinate = self.nodes[&parent].coordinate;
        let previous = self.nodes[&parent]
            .parent
            .map(|node| self.nodes[&node].coordinate.cell);
        for (index, [dx, dy]) in NEIGHBORS.into_iter().enumerate() {
            let Some(cell) = self.grid.signed_cell(
                i32::from(coordinate.cell.x) + dx,
                i32::from(coordinate.cell.y) + dy,
            ) else {
                continue;
            };
            let next = self.grid.resolve_layer(LayerCell {
                cell,
                layer: coordinate.layer,
            });
            if self.nodes.contains_key(&next)
                || (index >= 4 && !admitted[index - 4] && !admitted[(index - 3) % 4])
            {
                continue;
            }
            if self.units.values().any(|units| units.downhill_only)
                && self.grid.ground_height_at(coordinate.cell) < self.grid.ground_height_at(cell)
            {
                continue;
            }
            let blocked = self.grid.layer_blocked(next);
            let dozer = blocked
                && self
                    .units
                    .get(&next.layer)
                    .is_some_and(|units| units.dozer_obstacles.contains(&cell));
            if blocked && !dozer && !self.tunneling {
                continue;
            }
            admitted[index] = !dozer;
            let occupied = self.occupied(next);
            let valid = (!blocked || dozer)
                && self
                    .grid
                    .layer_movement_bounds(next, self.radius, self.centered)
                && !occupied.2;
            if !valid && !self.tunneling {
                continue;
            }
            if valid && !(next.layer == 1 && self.grid.pinched.contains(&cell)) {
                self.tunneling = false;
            }
            let mut cost = self.nodes[&parent].cost
                + step_cost(previous, coordinate.cell, self.coordinate(next).cell)
                + self.grid.ground_step_penalty(coordinate.cell, next);
            if occupied.0 {
                cost += 42;
            }
            if occupied.1
                && cell.x.abs_diff(self.start.cell.x) < 10
                && cell.y.abs_diff(self.start.cell.y) < 10
            {
                cost += 42;
            }
            if let Some((_, penalty)) = self.attack_cost
                && self.goal_occupied(next, self.radius, self.centered)
            {
                cost += penalty;
            }
            if self.tunneling && self.grid.layer_blocked(next) {
                cost += 100;
            }
            if next.layer == 1
                && !super::hierarchy::passable(
                    self.units.get(&1).and_then(|units| units.corridor.as_ref()),
                    cell,
                )
            {
                cost += 1000;
            }
            if self.grid.layer_obstacle(next) {
                cost += 1000;
            }
            let remaining = if self.tunneling || !self.use_heuristic {
                0
            } else {
                self.attack_cost.map_or_else(
                    || heuristic(self.coordinate(next).cell, self.coordinate(self.goal).cell),
                    |(distance, _)| {
                        super::search::attack::remaining(
                            cell,
                            self.coordinate(self.goal).cell,
                            distance,
                        )
                    },
                )
            };
            if !self.insert(next, Some(parent), cost, remaining) {
                return created;
            }
            created += u32::from(Some(next) != self.reserved && !self.has_info(next));
        }
        created
    }

    fn result(&self, end: Option<LayerCell>, terminal_removed: bool) -> SearchResult {
        SearchResult {
            retained_infos: Vec::new(),
            info_cleanup: self.info_cleanup(),
            route: end.map(|node| self.route(node)),
            cells: self.listed - usize::from(terminal_removed),
        }
    }

    fn info_cleanup(&self) -> Vec<(LayerCell, super::CellInfo)> {
        let mut cleanup: Vec<_> = self
            .nodes
            .iter()
            .map(|(&node, record)| {
                (
                    node,
                    super::CellInfo {
                        coordinate: record.coordinate,
                        cost: record.cost,
                        total: record.total,
                    },
                )
            })
            .collect();
        if let Some(node) = self.reserved
            && !self.nodes.contains_key(&node)
        {
            let info = self
                .units
                .get(&node.layer)
                .and_then(|units| units.info_record(node.cell))
                .copied()
                .unwrap_or_else(|| super::CellInfo::new(node));
            cleanup.push((node, info));
        }
        cleanup
    }

    fn route(&self, mut node: LayerCell) -> Route {
        let mut raw = vec![self.nodes[&node].coordinate];
        while let Some(parent) = self.nodes[&node].parent {
            raw.push(self.nodes[&parent].coordinate);
            node = parent;
        }
        raw.reverse();
        raw[0] = node;
        self.optimize(raw, false)
    }

    fn optimize(&self, raw: Vec<LayerCell>, transient: bool) -> Route {
        let raw_flags: Vec<_> = raw
            .iter()
            .map(|&node| {
                self.grid.path_cell_flags(
                    node,
                    self.occupied(node).0
                        && !self
                            .units
                            .get(&node.layer)
                            .is_some_and(|units| units.can_path_through_units),
                )
            })
            .collect();
        let path = self
            .grid
            .preview_path(&raw, &raw_flags, self.centered, |from, to| {
                self.grid
                    .layer_line_passable(from, to, self.radius, self.centered, |node| {
                        self.units.get(&node.layer).is_some_and(|units| {
                            units.fixed_allies.contains(&node.cell)
                                || units.fixed_enemies.contains(&node.cell)
                                || (transient
                                    && (units.transient_allies.contains(&node.cell)
                                        || units.transient_enemies.contains(&node.cell)))
                        })
                    })
            });
        Route {
            optimized: path.optimized_cells(),
            blocked_by_ally: path.blocked_by_ally,
            raw,
            raw_flags,
            optimization_blocked: transient,
            exact_endpoint: None,
            quick_start: None,
        }
    }

    fn transient_occupied(&self, node: LayerCell) -> bool {
        let above = self.radius + i32::from(self.centered);
        (i32::from(node.cell.x) - self.radius..i32::from(node.cell.x) + above).any(|x| {
            (i32::from(node.cell.y) - self.radius..i32::from(node.cell.y) + above).any(|y| {
                self.grid.signed_cell(x, y).is_some_and(|cell| {
                    let resolved = self.grid.resolve_layer(LayerCell {
                        cell,
                        layer: node.layer,
                    });
                    self.units.get(&resolved.layer).is_some_and(|units| {
                        units.transient_allies.contains(&cell)
                            || units.transient_enemies.contains(&cell)
                    })
                })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{BridgeGeometry, TerrainMap};

    #[test]
    fn attack_and_patch_routes_retain_the_live_bridge_deck() {
        let mut terrain = TerrainMap::classify(40, 20, [], []).unwrap();
        let layer = terrain
            .add_bridge(
                BridgeGeometry {
                    from: [80.0, 100.0, 20.0],
                    to: [320.0, 100.0, 20.0],
                    corners: [
                        [80.0, 60.0, 20.0],
                        [80.0, 140.0, 20.0],
                        [320.0, 60.0, 20.0],
                        [320.0, 140.0, 20.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        let grid = Grid::new(40, 20, terrain.blocked_ground_cells())
            .unwrap()
            .with_bridge_layers(&terrain);
        let start = LayerCell {
            cell: Cell { x: 12, y: 10 },
            layer,
        };
        let goal = LayerCell {
            cell: Cell { x: 28, y: 10 },
            layer,
        };
        let empty = BTreeMap::new();
        let unavailable = BTreeMap::new();
        let cells = BTreeSet::new();
        let query = LayerQuery {
            start,
            goal,
            radius: 0,
            centered: true,
            closest_cost: None,
        };
        let attack = crate::grid::search::attack::Attack {
            from: [125.0, 105.0, 20.0],
            target: [285.0, 105.0],
            range_targets: None,
            radius: 0,
            centered: true,
            weapon_range: [0.0, 40.0],
            geometry_radius: 0.0,
            distance: 40.0,
            vehicle: false,
            unavailable: &cells,
        };
        let route = grid
            .attack_search_layers_counted(&attack, &query, &empty, &unavailable, |point| {
                point.layer == layer
            })
            .route
            .unwrap();
        assert!(route.raw.iter().all(|node| node.layer == layer));
        let end = *route.raw.last().unwrap();
        assert!(attack.in_range(crate::grid::search::cell_position(end.cell, true)));
        let raw: Vec<_> = (12..=28)
            .map(|x| LayerCell {
                cell: Cell { x, y: 10 },
                layer,
            })
            .collect();
        let patch = grid
            .patch_layers_counted(
                &patch::LayerPatch {
                    start: LayerCell {
                        cell: Cell { x: 14, y: 11 },
                        layer,
                    },
                    indexed_start: LayerCell {
                        cell: Cell { x: 14, y: 11 },
                        layer,
                    },
                    position: [145.0, 115.0],
                    radius: 0,
                    centered: true,
                    raw: &raw,
                    blocked: true,
                },
                &empty,
            )
            .route
            .unwrap();
        assert!(patch.raw.iter().all(|node| node.layer == layer));
        assert!(patch.raw.last() == Some(&goal));
        let group = grid
            .group_path_counted(start, goal, &empty)
            .route
            .unwrap()
            .optimized;
        assert!(group.iter().any(|node| node.layer == layer));
        assert!(grid.path_cost_counted(start, goal, &empty).cost.is_some());
    }

    #[test]
    fn bridge_connectors_cross_water_and_destruction_removes_the_route() {
        let water = (0..20).flat_map(|y| (15..25).map(move |x| Cell { x, y }));
        let mut terrain = TerrainMap::classify(40, 20, [], water).unwrap();
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
        let grid = Grid::new(40, 20, terrain.blocked_ground_cells())
            .unwrap()
            .with_bridge_layers(&terrain);
        let start = LayerCell {
            cell: Cell { x: 10, y: 10 },
            layer: 1,
        };
        let goal = LayerCell {
            cell: Cell { x: 30, y: 10 },
            layer: 1,
        };
        let empty = BTreeMap::new();
        let unavailable = BTreeMap::new();
        let route = grid
            .path_for_layers_counted(
                &LayerQuery {
                    start,
                    goal,
                    radius: 0,
                    centered: true,
                    closest_cost: None,
                },
                &empty,
                &unavailable,
            )
            .route
            .unwrap();
        assert!(route.raw.iter().any(|node| node.layer == layer));
        assert!(route.raw.first() == Some(&start));
        assert!(route.raw.last() == Some(&goal));
        let transitions: Vec<_> = route
            .raw
            .windows(2)
            .filter(|pair| pair[0].layer != pair[1].layer)
            .collect();
        assert_eq!(transitions.len(), 2);
        for pair in transitions {
            if pair[0].cell == pair[1].cell {
                assert!(grid.connection(pair[0]) == Some(pair[1]));
            } else {
                // Native neighbor lookup falls back to ground outside the deck.
                assert_eq!(pair[0].layer, layer);
                assert_eq!(pair[1].layer, 1);
                assert!(pair[0].cell.x.abs_diff(pair[1].cell.x) <= 1);
                assert!(pair[0].cell.y.abs_diff(pair[1].cell.y) <= 1);
                assert!(
                    grid.resolve_layer(LayerCell {
                        cell: pair[1].cell,
                        layer: pair[0].layer,
                    }) == pair[1]
                );
                assert!(!grid.layer_blocked(pair[1]));
            }
        }
        assert!(route.optimized.iter().any(|node| node.layer == layer));
        terrain.set_bridge_destroyed(layer, true).unwrap();
        let destroyed = Grid::new(40, 20, terrain.blocked_ground_cells())
            .unwrap()
            .with_bridge_layers(&terrain);
        assert!(
            destroyed
                .path_for_layers_counted(
                    &LayerQuery {
                        start,
                        goal,
                        radius: 0,
                        centered: true,
                        closest_cost: None
                    },
                    &empty,
                    &unavailable
                )
                .route
                .is_none()
        );
    }
    #[test]
    fn layer_line_checks_fixed_footprint_but_only_center_static_terrain() {
        let side = Cell { x: 2, y: 1 };
        let grid = Grid::new(7, 7, [side]).unwrap();
        let start = LayerCell {
            cell: Cell { x: 1, y: 2 },
            layer: 1,
        };
        let end = LayerCell {
            cell: Cell { x: 4, y: 2 },
            layer: 1,
        };
        assert!(grid.layer_line_passable(start, end, 1, false, |_| false));
        assert!(!grid.layer_line_passable(start, end, 1, false, |node| node.cell == side));
        assert_eq!(
            grid.layer_line_passable(start, end, 1, false, |_| false),
            grid.line_passable(start.cell, end.cell, 1, false, |_| false)
        );
    }

    #[test]
    fn low_bridge_deck_blocks_actual_ground_route_before_and_after_destruction() {
        for deck in [5.0, 15.0] {
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
                let grid = Grid::new(40, 20, []).unwrap().with_bridge_layers(&terrain);
                let query = LayerQuery {
                    start: LayerCell {
                        cell: Cell { x: 5, y: 10 },
                        layer: 1,
                    },
                    goal: LayerCell {
                        cell: Cell { x: 19, y: 10 },
                        layer: 1,
                    },
                    radius: 0,
                    centered: true,
                    closest_cost: None,
                };
                let result =
                    grid.path_for_layers_counted(&query, &BTreeMap::new(), &BTreeMap::new());
                assert_eq!(
                    result.route.is_some(),
                    deck > 10.0,
                    "deck {deck}, destroyed {destroyed}"
                );
            }
        }
    }
    #[test]
    fn allocated_upper_padding_falls_back_but_destroyed_padding_stays_upper() {
        let mut terrain = crate::terrain::TerrainMap::classify(50, 50, [], []).unwrap();
        let layer = terrain
            .add_bridge(
                crate::terrain::BridgeGeometry {
                    from: [100.0, 200.0, 20.0],
                    to: [300.0, 200.0, 40.0],
                    corners: [
                        [100.0, 220.0, 20.0],
                        [100.0, 180.0, 20.0],
                        [300.0, 220.0, 40.0],
                        [300.0, 180.0, 40.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        let padding = LayerCell {
            cell: Cell { x: 8, y: 16 },
            layer,
        };
        let absent = LayerCell {
            cell: Cell { x: 0, y: 0 },
            layer,
        };
        let deck = LayerCell {
            cell: Cell { x: 15, y: 19 },
            layer,
        };
        let side = LayerCell {
            cell: Cell { x: 15, y: 22 },
            layer,
        };
        assert!(matches!(
            terrain.bridge_cell(layer, padding.cell).unwrap().kind,
            crate::terrain::CellType::Impassable
        ));
        assert!(terrain.bridge_cell(layer, absent.cell).is_none());
        assert!(matches!(
            terrain.bridge_cell(layer, side.cell).unwrap().kind,
            crate::terrain::CellType::BridgeImpassable
        ));
        let grid = Grid::new(50, 50, []).unwrap().with_bridge_layers(&terrain);
        assert_eq!(grid.resolve_layer(padding).layer, 1);
        assert_eq!(grid.resolve_layer(absent).layer, 1);
        assert_eq!(grid.resolve_layer(deck).layer, layer);
        assert_eq!(grid.resolve_layer(side).layer, layer);
        assert!(!grid.layer_blocked(padding));
        assert!(!grid.layer_blocked(deck));
        assert!(grid.layer_blocked(side));
        assert!(grid.layer_line_passable(padding, padding, 0, true, |_| false));
        assert!(!grid.layer_line_passable(side, side, 0, true, |_| false));
        terrain.set_bridge_destroyed(layer, true).unwrap();
        let destroyed = Grid::new(50, 50, []).unwrap().with_bridge_layers(&terrain);
        assert_eq!(destroyed.resolve_layer(padding).layer, layer);
        assert_eq!(destroyed.resolve_layer(absent).layer, 1);
        assert!(destroyed.layer_blocked(padding));
        assert!(destroyed.layer_blocked(deck));
        assert!(!destroyed.layer_line_passable(padding, padding, 0, true, |_| false));
        terrain.set_bridge_destroyed(layer, false).unwrap();
        let repaired = Grid::new(50, 50, []).unwrap().with_bridge_layers(&terrain);
        assert_eq!(repaired.resolve_layer(padding).layer, 1);
        assert!(!repaired.layer_blocked(deck));
    }
}
