//! `AIPathfind.cpp` `findGroundPath`/`groundCellsCallback`,
//! `clearCellForDiameter` and `optimizeGroundPath`, fixed CRC/path profile.
//! The shared ground/bridge graph retains native width penalties and connectors.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use super::search::{NEIGHBORS, heuristic, line_cells, step_cost};
use super::{Grid, LayerCell, Route, RouteUnits, SearchResult};
#[cfg(test)]
mod tests;

const DIAMETER: i32 = 6;

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
    nodes: BTreeMap<LayerCell, Node>,
    open: BinaryHeap<Reverse<(u32, u64, LayerCell)>>,
    sequence: u64,
    units: &'a BTreeMap<u8, &'a RouteUnits>,
    available: usize,
    allocated: usize,
}

pub(super) fn find_layers(
    grid: &Grid,
    start: LayerCell,
    goal: LayerCell,
    units: &BTreeMap<u8, &RouteUnits>,
) -> SearchResult {
    if grid.layer_blocked(start) || !grid.contains(goal.cell) {
        return SearchResult::empty();
    }
    let Some(goal) = adjusted_goal(grid, goal, units) else {
        return SearchResult::empty();
    };
    let mut search = Search::new(grid, goal, units);
    if search.allocated > search.available {
        return SearchResult::empty();
    }
    if !search.insert(start, None, 0) {
        return search.result(None, false);
    }
    while let Some(Reverse((total, sequence, node))) = search.open.pop() {
        if search.node(node).insertion != sequence {
            continue;
        }
        if node == goal {
            return search.result(Some(node), true);
        }
        search.connect(node, total);
        search.expand_line(node);
        search.expand_neighbors(node);
    }
    search.result(None, false)
}

pub struct CostResult {
    pub cost: Option<u32>,
    pub search: SearchResult,
}

pub(super) fn cost_layers(
    grid: &Grid,
    start: LayerCell,
    goal: LayerCell,
    units: &BTreeMap<u8, &RouteUnits>,
) -> CostResult {
    let mut search = Search::new(grid, goal, units);
    if search.allocated > search.available {
        return CostResult {
            cost: None,
            search: SearchResult::empty(),
        };
    }
    if !search.insert(start, None, 0) || grid.layer_blocked(start) {
        return CostResult {
            cost: None,
            search: search.result(None, false),
        };
    }
    let mut examined = 0;
    while let Some(Reverse((total, insertion, parent))) = search.open.pop() {
        if search.node(parent).insertion != insertion {
            continue;
        }
        if parent == goal {
            return CostResult {
                cost: Some(search.node(parent).cost),
                search: search.result(None, true),
            };
        }
        if examined > 500 {
            continue;
        }
        search.connect(parent, total);
        let coordinate = search.node(parent).coordinate;
        let mut neighbors = [false; 8];
        for (index, [dx, dy]) in NEIGHBORS.into_iter().enumerate() {
            let Some(cell) = grid.signed_cell(
                i32::from(coordinate.cell.x) + dx,
                i32::from(coordinate.cell.y) + dy,
            ) else {
                continue;
            };
            let node = grid.resolve_layer(LayerCell {
                cell,
                layer: coordinate.layer,
            });
            if (index >= 4 && !neighbors[index - 4] && !neighbors[(index - 3) % 4])
                || grid.layer_blocked(node)
            {
                continue;
            }
            neighbors[index] = true;
            // checkPathCost counts every successful allocateInfo, including an
            // existing INFO whose cheaper path subsequently suppresses insertion.
            let has_info = search.nodes.contains_key(&node)
                || node == search.goal
                || units
                    .get(&node.layer)
                    .is_some_and(|units| units.has_info(node.cell));
            if !has_info && search.allocated == search.available {
                return CostResult {
                    cost: Some(examined),
                    search: search.result(None, false),
                };
            }
            examined += 1;
            let to = search.coordinate(node);
            let cost = search.node(parent).cost
                + step_cost(
                    search
                        .node(parent)
                        .parent
                        .map(|node| search.node(node).coordinate.cell),
                    coordinate.cell,
                    to.cell,
                )
                + if to.layer == 1 && grid.pinched.contains(&node.cell) {
                    14
                } else {
                    0
                };
            if search.nodes.get(&node).is_none_or(|node| node.cost > cost) {
                search.insert(node, Some(parent), cost);
            }
        }
    }
    CostResult {
        cost: None,
        search: search.result(None, false),
    }
}

fn adjusted_goal(
    grid: &Grid,
    goal: LayerCell,
    units: &BTreeMap<u8, &RouteUnits>,
) -> Option<LayerCell> {
    if clear_diameter(grid, goal, DIAMETER, units) == DIAMETER {
        return Some(goal);
    }
    for offset in 1..8 {
        let mut point = [i32::from(goal.cell.x), i32::from(goal.cell.y)];
        for [dx, dy] in [
            [1, 0],
            [0, 1],
            [-1, 0],
            [-1, 0],
            [0, -1],
            [0, -1],
            [1, 0],
            [1, 0],
        ] {
            point[0] += dx * offset;
            point[1] += dy * offset;
            if let Some(cell) = grid.signed_cell(point[0], point[1]) {
                let node = grid.resolve_layer(LayerCell {
                    cell,
                    layer: goal.layer,
                });
                if clear_diameter(grid, node, DIAMETER, units) == DIAMETER {
                    return Some(node);
                }
            }
        }
    }
    None
}

fn clear_diameter(
    grid: &Grid,
    cell: LayerCell,
    diameter: i32,
    units: &BTreeMap<u8, &RouteUnits>,
) -> i32 {
    let layer = cell.layer;
    let radius = diameter / 2;
    let above = radius.max(1);
    let low = [
        i32::from(cell.cell.x) - radius,
        i32::from(cell.cell.y) - radius,
    ];
    let high = [
        i32::from(cell.cell.x) + above,
        i32::from(cell.cell.y) + above,
    ];
    let mut clear = true;
    for x in low[0]..high[0] {
        for y in low[1]..high[1] {
            if radius > 1 && (x == low[0] || x == high[0] - 1) && (y == low[1] || y == high[1] - 1)
            {
                continue;
            }
            let Some(cell) = grid.signed_cell(x, y) else {
                return 0;
            };
            let node = grid.resolve_layer(LayerCell { cell, layer });
            if grid.layer_blocked(node)
                || (diameter >= 2
                    && units
                        .get(&node.layer)
                        .is_some_and(|units| units.width_blockers.contains(&cell)))
            {
                clear = false;
                break;
            }
        }
    }
    if clear {
        (radius * 2).max(1)
    } else if diameter < 2 {
        0
    } else {
        clear_diameter(grid, cell, diameter - 2, units)
    }
}

impl<'a> Search<'a> {
    fn new(grid: &'a Grid, goal: LayerCell, units: &'a BTreeMap<u8, &'a RouteUnits>) -> Self {
        let available = units
            .values()
            .map(|units| units.info_available)
            .min()
            .unwrap_or(super::CELL_INFO_CAPACITY);
        let allocated = usize::from(
            !units
                .get(&goal.layer)
                .is_some_and(|units| units.has_info(goal.cell)),
        );
        Search {
            grid,
            goal,
            units,
            available,
            allocated,
            nodes: BTreeMap::new(),
            open: BinaryHeap::new(),
            sequence: 0,
        }
    }

    fn coordinate(&self, node: LayerCell) -> LayerCell {
        LayerCell {
            cell: self
                .units
                .get(&node.layer)
                .and_then(|units| units.info_record(node.cell))
                .map_or(node.cell, |info| info.coordinate.cell),
            layer: node.layer,
        }
    }

    fn result(&self, end: Option<LayerCell>, terminal_removed: bool) -> SearchResult {
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
        if !self.nodes.contains_key(&self.goal) {
            cleanup.push((
                self.goal,
                self.units
                    .get(&self.goal.layer)
                    .and_then(|units| units.info_record(self.goal.cell))
                    .copied()
                    .unwrap_or_else(|| super::CellInfo::new(self.goal)),
            ));
        }
        let route = end.map(|node| {
            let raw = self.raw_path(node);
            let raw_flags: Vec<_> = raw
                .iter()
                .map(|&cell| self.grid.path_cell_flags(cell, false))
                .collect();
            let position = |node: LayerCell| {
                let xy = super::search::cell_position(node.cell, false);
                [xy[0], xy[1], 0.0]
            };
            let path = self.grid.construct_group_path(
                &raw,
                &raw_flags,
                position(raw[0]),
                self.units,
                position,
            );
            Route {
                optimized: path.optimized_cells(),
                raw,
                raw_flags,
                optimization_blocked: false,
                blocked_by_ally: false,
                exact_endpoint: None,
                quick_start: None,
            }
        });
        SearchResult {
            retained_infos: Vec::new(),
            route,
            cells: self.nodes.len() - usize::from(terminal_removed),
            info_cleanup: cleanup,
        }
    }

    fn raw_path(&self, mut node: LayerCell) -> Vec<LayerCell> {
        let mut raw = vec![self.node(node).coordinate];
        while let Some(parent) = self.node(node).parent {
            raw.push(self.node(parent).coordinate);
            node = parent;
        }
        raw.reverse();
        raw
    }

    fn node(&self, cell: LayerCell) -> &Node {
        self.nodes.get(&cell).expect("Inserted group path cell")
    }

    fn insert(&mut self, cell: LayerCell, parent: Option<LayerCell>, cost: u32) -> bool {
        self.insert_total(
            cell,
            parent,
            cost,
            cost + heuristic(self.coordinate(cell).cell, self.coordinate(self.goal).cell),
        )
    }

    fn insert_total(
        &mut self,
        cell: LayerCell,
        parent: Option<LayerCell>,
        cost: u32,
        total: u32,
    ) -> bool {
        if !self.nodes.contains_key(&cell)
            && cell != self.goal
            && !self
                .units
                .get(&cell.layer)
                .is_some_and(|units| units.has_info(cell.cell))
        {
            if self.allocated == self.available {
                return false;
            }
            self.allocated += 1;
        }
        self.sequence += 1;
        self.nodes.insert(
            cell,
            Node {
                coordinate: self.coordinate(cell),
                parent,
                cost,
                total,
                insertion: self.sequence,
            },
        );
        self.open.push(Reverse((total, self.sequence, cell)));
        true
    }

    fn connect(&mut self, node: LayerCell, total: u32) {
        if let Some(next) = self.grid.connection(node)
            && !self.nodes.contains_key(&next)
        {
            let cost = self.node(node).cost;
            self.insert_total(next, Some(node), cost, total);
        }
    }

    fn expand_line(&mut self, start: LayerCell) {
        let coordinate = self.node(start).coordinate;
        for pair in
            line_cells(self.grid, coordinate.cell, self.coordinate(self.goal).cell).windows(2)
        {
            let [from, to] = [pair[0], pair[1]].map(|cell| {
                self.grid.resolve_layer(LayerCell {
                    cell,
                    layer: coordinate.layer,
                })
            });
            if self.nodes.contains_key(&to)
                || clear_diameter(self.grid, to, DIAMETER, self.units) != DIAMETER
            {
                break;
            }
            if !self.insert(to, Some(from), self.node(from).cost + 5) {
                break;
            }
        }
    }

    fn expand_neighbors(&mut self, parent: LayerCell) {
        let coordinate = self.node(parent).coordinate;
        let mut neighbors = [false; 8];
        for (index, [dx, dy]) in NEIGHBORS.into_iter().enumerate() {
            let Some(cell) = self.grid.signed_cell(
                i32::from(coordinate.cell.x) + dx,
                i32::from(coordinate.cell.y) + dy,
            ) else {
                continue;
            };
            let cell = self.grid.resolve_layer(LayerCell {
                cell,
                layer: coordinate.layer,
            });
            if cell.layer == 1
                && !super::hierarchy::group_passable(
                    self.grid,
                    self.units.get(&1).and_then(|units| units.corridor.as_ref()),
                    cell.cell,
                )
            {
                continue;
            }
            let diameter = if cell == self.goal {
                0
            } else {
                if (index >= 4 && !neighbors[index - 4] && !neighbors[(index - 3) % 4])
                    || self.grid.layer_blocked(cell)
                {
                    continue;
                }
                neighbors[index] = true;
                clear_diameter(self.grid, cell, DIAMETER, self.units)
            };
            let penalty =
                u32::try_from(6 * (DIAMETER - diameter)).expect("Clear diameter is bounded");
            let cost = self.node(parent).cost
                + step_cost(
                    self.node(parent)
                        .parent
                        .map(|node| self.node(node).coordinate.cell),
                    coordinate.cell,
                    self.coordinate(cell).cell,
                )
                + penalty
                + if cell.layer == 1 && self.grid.pinched.contains(&cell.cell) {
                    14
                } else {
                    0
                };
            if self.nodes.get(&cell).is_none_or(|node| node.cost > cost) {
                self.insert(cell, Some(parent), cost);
            }
        }
    }
}

impl Grid {
    pub(crate) fn construct_group_path(
        &self,
        raw: &[LayerCell],
        flags: &[super::construction::CellFlags],
        from: [f32; 3],
        units: &BTreeMap<u8, &RouteUnits>,
        mut position: impl FnMut(LayerCell) -> [f32; 3],
    ) -> super::construction::BuiltPath {
        use num_traits::ToPrimitive;
        assert_eq!(raw.len(), flags.len());
        let cells: Vec<_> = raw
            .iter()
            .zip(flags)
            .map(|(&node, flags)| super::construction::CellNode {
                cell: [node.cell.x, node.cell.y],
                position: position(node),
                layer: node.layer,
                cliff: flags.cliff,
                pinched: flags.pinched,
                blocked_by_ally: flags.blocked_by_ally,
            })
            .collect();
        let mut path = super::construction::prepend_group_cells(&cells, from)
            .expect("successful group search retains raw cells");
        path.optimize_ground(|layer, from, to| {
            let cell = |point: [f32; 3]| -> Option<super::Cell> {
                let cell = super::Cell {
                    x: (point[0] / 10.0).to_u16()?,
                    y: (point[1] / 10.0).to_u16()?,
                };
                self.contains(cell).then_some(cell)
            };
            let (Some(start), Some(end)) = (cell(from), cell(to)) else {
                return false;
            };
            let mut previous: Option<LayerCell> = None;
            line_cells(self, start, end).into_iter().all(|cell| {
                let node = self.resolve_layer(LayerCell { cell, layer });
                let passable = clear_diameter(self, node, DIAMETER, units) == DIAMETER
                    || previous.is_some_and(|prior| node.layer != 1 && prior.layer == node.layer);
                previous = Some(node);
                passable
            })
        });
        path
    }
}
