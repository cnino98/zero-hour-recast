//! Ground, single-layer branches of the pinned `SuperHackers` `AIPathfind.cpp`:
//! `examineCellsCallback`, `examineNeighboringCells`, `PathfindCell::costSoFar`,
//! `iterateCellsAlongLine` and `Path::optimize`. See docs/PATHFINDING.md for the
//! source profile and the terrain/occupancy systems not yet represented here.
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeSet, BinaryHeap};

use super::{Cell, Grid, Route, RouteUnits, SearchResult};

pub(super) mod attack;
mod flags;
mod nodes;

pub(super) const NEIGHBORS: [[i32; 2]; 8] = [
    [1, 0],
    [0, 1],
    [-1, 0],
    [0, -1],
    [1, 1],
    [-1, 1],
    [-1, -1],
    [1, -1],
];

struct Node {
    coordinate: Cell,
    parent: Option<Cell>,
    cost: u32,
    total: u32,
    insertion: u64,
}

struct Search<'a> {
    grid: &'a Grid,
    units: &'a RouteUnits,
    flags: Vec<u8>,
    start: Cell,
    goal: Cell,
    radius: i32,
    centered: bool,
    nodes: nodes::Pages,
    open: BinaryHeap<Reverse<(u32, u64, Cell)>>,
    sequence: u64,
    tunneling: bool,
    preallocated: Option<Cell>,
    attack_cost: Option<(u32, u32)>,
    listed: usize,
    allocated: usize,
}

pub(super) fn find_counted(
    grid: &Grid,
    start: Cell,
    goal: Cell,
    radius: i32,
    centered: bool,
    tunneling: bool,
    units: &RouteUnits,
) -> SearchResult {
    let mut search = Search::new(grid, units, start, goal, radius, centered, tunneling);
    if !search.reserve_info(goal)
        || !search.insert(
            start,
            None,
            0,
            heuristic(search.coordinate(start), search.coordinate(goal)),
        )
    {
        return search.result(None);
    }
    while let Some(Reverse((_, insertion, current))) = search.open.pop() {
        if search.node(current).insertion != insertion {
            continue;
        }
        if current == goal {
            search.listed -= 1;
            return search.result(Some(search.path(current)));
        }
        if !search.tunneling && !units.downhill_only {
            search.expand_line(current);
        }
        search.expand_neighbors(current);
    }
    search.result(None)
}

pub(super) fn closest_counted(
    grid: &Grid,
    start: Cell,
    goal: Cell,
    radius: i32,
    centered: bool,
    unavailable: &BTreeSet<Cell>,
    units: &RouteUnits,
) -> SearchResult {
    closest_scored(
        grid,
        start,
        goal,
        radius,
        centered,
        (unavailable, 0.2),
        units,
    )
}

pub(super) fn recovery_counted(
    grid: &Grid,
    start: Cell,
    goal: Cell,
    radius: i32,
    centered: bool,
    unavailable: &BTreeSet<Cell>,
    units: &RouteUnits,
) -> SearchResult {
    closest_scored(
        grid,
        start,
        goal,
        radius,
        centered,
        (unavailable, 0.0),
        units,
    )
}

fn closest_scored(
    grid: &Grid,
    start: Cell,
    goal: Cell,
    radius: i32,
    centered: bool,
    cost: (&BTreeSet<Cell>, f32),
    units: &RouteUnits,
) -> SearchResult {
    let (unavailable, cost_factor) = cost;
    let mut search = Search::new(
        grid,
        units,
        start,
        goal,
        radius,
        centered,
        grid.is_obstacle(start) || grid.is_blocked(start),
    );
    search.tunneling |= !search.within_footprint_bounds(start) || search.occupied(start).enemy;
    let started_stuck = search.tunneling;
    if !search.reserve_info(goal)
        || !search.insert(
            start,
            None,
            0,
            heuristic(search.coordinate(start), search.coordinate(goal)),
        )
    {
        return search.result(None);
    }
    let mut closest = None;
    let mut closest_score = f32::MAX;
    let mut closest_screen = f32::MAX;
    let mut found_goal = false;
    let mut neighbor_count = 0;
    while let Some(Reverse((_, insertion, current))) = search.open.pop() {
        if search.node(current).insertion != insertion {
            continue;
        }
        let coordinate = search.coordinate(current);
        let clear = grid.destination_clear(coordinate, radius, centered)
            && !unavailable.contains(&coordinate)
            && !search.occupied(coordinate).enemy;
        if current == goal {
            if closest.is_some() && !clear {
                found_goal = true;
                search.listed -= 1;
                continue;
            }
            search.listed -= 1;
            return search.result(Some(search.path(current)));
        }
        let dx = f32::from(search.coordinate(goal).x.abs_diff(coordinate.x));
        let dy = f32::from(search.coordinate(goal).y.abs_diff(coordinate.y));
        let x_squared = dx * dx;
        let y_squared = dy * dy;
        let distance = x_squared + y_squared;
        if !search.tunneling && clear && (!started_stuck || !grid.is_blocked(current)) {
            closest_screen = closest_screen.min(distance);
            let cost = source_real(search.node(current).cost);
            // AIMoveAndTighten uses .2; AIUpdate::computePath recovery uses 0.
            // Conversion is 1 / COST_ORTHOGONAL, squared before this expression.
            let scaled = cost * (0.1_f32 * 0.1_f32);
            let penalty = (cost * scaled) * cost_factor;
            let score = distance + penalty;
            if score < closest_score {
                closest = Some(current);
                closest_score = score;
            }
        }
        if distance > closest_screen * 4.0
            && (units.corridor.is_none() || neighbor_count > 2000 || closest_screen < 1000.0)
        {
            continue;
        }
        if !found_goal {
            if !search.tunneling && !units.downhill_only {
                search.expand_line(current);
            }
            neighbor_count += search.expand_neighbors_toward(current, Some(goal));
        }
    }
    search.result(closest.map(|cell| search.path(cell)))
}

pub(super) fn source_real(value: u32) -> f32 {
    let high = u16::try_from(value >> 16).expect("Upper word fits u16");
    let low = u16::try_from(value & 0xffff).expect("Lower word fits u16");
    f32::from(high) * 65536.0 + f32::from(low)
}

/// `getMoveAwayFromPath` supplies no goal cell: neighbor costs have no
/// heuristic and the ordinary direct-to-goal line expansion is not called.
pub(super) fn away_counted(
    grid: &Grid,
    start: Cell,
    indexed_start: Cell,
    radius: i32,
    centered: bool,
    units: &RouteUnits,
    mut destination: impl FnMut(Cell) -> bool,
) -> SearchResult {
    let mut search = Search::new(
        grid,
        units,
        start,
        start,
        radius,
        centered,
        grid.is_obstacle(start) || grid.is_blocked(start),
    );
    search.tunneling |=
        !search.within_footprint_bounds(indexed_start) || search.occupied(indexed_start).enemy;
    search.start = indexed_start;
    if !search.insert(start, None, 0, 0) {
        return search.result(None);
    }
    let start_slot = search.index(start);
    if !units.has_info(start) {
        search
            .nodes
            .get_mut(start_slot)
            .expect("inserted start cell")
            .coordinate = indexed_start;
    }
    while let Some(Reverse((_, insertion, current))) = search.open.pop() {
        if search.node(current).insertion != insertion {
            continue;
        }
        if destination(search.node(current).coordinate) {
            search.listed -= 1;
            return search.result(Some(search.path(current)));
        }
        search.expand_neighbors_toward(current, None);
    }
    search.result(None)
}

/// The source expands with no goal heuristic and counts newly allocated cells,
/// then continues draining the open list after the 2000-cell expansion budget.
pub(super) fn patch_counted(
    grid: &Grid,
    units: &RouteUnits,
    query: &super::patch::Patch<'_>,
) -> SearchResult {
    if query.raw.len() < 2 || !grid.contains(query.start) || !grid.contains(query.indexed_start) {
        return SearchResult::empty();
    }
    let mut search = Search::new(
        grid,
        units,
        query.indexed_start,
        query.indexed_start,
        query.radius,
        query.centered,
        false,
    );
    let Some((first, candidate)) = search.patch_tail(query) else {
        return search.result(None);
    };
    if !search.reserve_info(candidate) || !search.insert(query.start, None, 0, 0) {
        return search.result(None);
    }
    let start_slot = search.index(query.start);
    if !units.has_info(query.start) {
        search
            .nodes
            .get_mut(start_slot)
            .expect("inserted path start")
            .coordinate = query.indexed_start;
    }
    let mut allocations = 0;
    while let Some(Reverse((_, insertion, current))) = search.open.pop() {
        if search.node(current).insertion != insertion {
            continue;
        }
        let coordinate = search.node(current).coordinate;
        if let Some(match_index) = (first..query.raw.len())
            .rev()
            .find(|&index| query.raw[index] == coordinate)
        {
            let mut raw: Vec<_> = search
                .path(current)
                .raw
                .into_iter()
                .map(|node| node.cell)
                .collect();
            raw.extend_from_slice(&query.raw[match_index + 1..]);
            search.listed -= 1;
            return search.result(Some(search.optimize(raw, query.blocked)));
        }
        if allocations < 2000 {
            allocations += search.expand_neighbors_toward(current, None);
        }
    }
    search.result(None)
}

impl<'a> Search<'a> {
    fn new(
        grid: &'a Grid,
        units: &'a RouteUnits,
        start: Cell,
        goal: Cell,
        radius: i32,
        centered: bool,
        tunneling: bool,
    ) -> Self {
        Self {
            grid,
            units,
            flags: flags::build(grid, units),
            start,
            goal,
            radius,
            centered,
            tunneling,
            preallocated: None,
            attack_cost: None,
            listed: 0,
            allocated: 0,
            nodes: nodes::Pages::new(usize::from(grid.width) * usize::from(grid.height)),
            open: BinaryHeap::new(),
            sequence: 0,
        }
    }
    fn result(&self, route: Option<Route>) -> SearchResult {
        SearchResult {
            retained_infos: Vec::new(),
            info_cleanup: self.info_cleanup(),
            route,
            cells: self.listed,
        }
    }

    fn info_cleanup(&self) -> Vec<(super::LayerCell, super::CellInfo)> {
        let mut cleanup: Vec<_> = self
            .nodes
            .iter()
            .map(|(index, node)| {
                let cell = Cell {
                    x: u16::try_from(index % usize::from(self.grid.width)).expect("cell"),
                    y: u16::try_from(index / usize::from(self.grid.width)).expect("cell"),
                };
                (
                    super::LayerCell { cell, layer: 1 },
                    super::CellInfo {
                        coordinate: super::LayerCell {
                            cell: node.coordinate,
                            layer: 1,
                        },
                        cost: node.cost,
                        total: node.total,
                    },
                )
            })
            .collect();
        if let Some(cell) = self.preallocated
            && self.nodes.get(self.index(cell)).is_none()
        {
            let info = self
                .units
                .info_record(cell)
                .copied()
                .unwrap_or_else(|| super::CellInfo::new(super::LayerCell { cell, layer: 1 }));
            cleanup.push((super::LayerCell { cell, layer: 1 }, info));
        }
        cleanup
    }

    fn index(&self, cell: Cell) -> usize {
        usize::from(cell.y) * usize::from(self.grid.width) + usize::from(cell.x)
    }

    fn has(&self, cell: Cell, flag: u8) -> bool {
        self.flags[self.index(cell)] & flag != 0
    }

    fn node(&self, cell: Cell) -> &Node {
        self.nodes
            .get(self.index(cell))
            .expect("Search node was inserted")
    }

    fn coordinate(&self, cell: Cell) -> Cell {
        self.nodes
            .get(self.index(cell))
            .map(|node| node.coordinate)
            .or_else(|| {
                self.units
                    .info_record(cell)
                    .map(|info| info.coordinate.cell)
            })
            .unwrap_or(cell)
    }

    fn stored_cost(&self, cell: Cell) -> u32 {
        self.nodes
            .get(self.index(cell))
            .map(|node| node.cost)
            .or_else(|| self.units.info_record(cell).map(|info| info.cost))
            .expect("native line predecessor has allocated INFO")
    }

    fn reserve_info(&mut self, cell: Cell) -> bool {
        if self.nodes.get(self.index(cell)).is_none() && !self.has(cell, flags::INFO) {
            if self.allocated == self.units.info_available {
                return false;
            }
            self.allocated += 1;
        }
        self.preallocated = Some(cell);
        true
    }

    fn insert(&mut self, cell: Cell, parent: Option<Cell>, cost: u32, remaining: u32) -> bool {
        self.sequence += 1;
        let index = self.index(cell);
        if self.nodes.get(index).is_none() {
            if self.preallocated != Some(cell) && !self.has(cell, flags::INFO) {
                if self.allocated == self.units.info_available {
                    return false;
                }
                self.allocated += 1;
            }
            self.listed += 1;
        }
        self.nodes.insert(
            index,
            Node {
                coordinate: self.coordinate(cell),
                parent,
                cost,
                total: cost + remaining,
                insertion: self.sequence,
            },
        );
        // Fixed-profile sorted insertion places a new equal-cost cell after
        // existing equal-cost cells. Allocation addresses never break ties.
        self.open
            .push(Reverse((cost + remaining, self.sequence, cell)));
        true
    }

    fn patch_tail(&self, query: &super::patch::Patch<'_>) -> Option<(usize, Cell)> {
        let mut first = query.raw.len();
        let mut candidate = *query.raw.last()?;
        let mut nearest = squared_distance(query.position, cell_position(candidate, self.centered));
        // The first path node is excluded, even when the entire old tail is clear.
        for index in (1..query.raw.len()).rev() {
            let cell = query.raw[index];
            let occupied = self.occupied(cell);
            let transient = query.blocked
                && cell.x.abs_diff(query.indexed_start.x) <= 2
                && cell.y.abs_diff(query.indexed_start.y) <= 2;
            if !self.within_footprint_bounds(cell)
                || occupied.fixed
                || occupied.enemy
                || (transient && self.transient_occupied(cell))
            {
                break;
            }
            first = index;
            let distance = squared_distance(query.position, cell_position(cell, self.centered));
            if distance < nearest {
                candidate = cell;
                nearest = distance;
            }
        }
        (first < query.raw.len()).then_some((first, candidate))
    }

    fn within_footprint_bounds(&self, cell: Cell) -> bool {
        let above = self.radius + i32::from(self.centered);
        i32::from(cell.x) >= self.radius
            && i32::from(cell.y) >= self.radius
            && i32::from(cell.x) + above <= i32::from(self.grid.width)
            && i32::from(cell.y) + above <= i32::from(self.grid.height)
    }

    fn expand_line(&mut self, start: Cell) {
        let mut cells = line_cells_iter(
            self.grid,
            self.coordinate(start),
            self.coordinate(self.goal),
        );
        let Some(mut previous) = cells.next() else {
            return;
        };
        for to in cells {
            let from = previous;
            previous = to;
            let occupied = self.occupied(to);
            if !super::hierarchy::passable(self.units.corridor.as_ref(), to)
                || self.grid.cliffs.contains(&to)
                || self.grid.pinched.contains(&to)
                || self.has(to, flags::BLOCKED)
                || !self.within_footprint_bounds(to)
                || occupied.enemy
                || occupied.fixed
            {
                break;
            }
            // examineCellsCallback deliberately charges five per traversed
            // orthogonal substep, rather than costSoFar's ordinary 10/14.
            let cost = self.stored_cost(from) + 5;
            if self
                .nodes
                .get(self.index(to))
                .is_some_and(|node| node.cost <= cost)
            {
                continue;
            }
            if !self.insert(
                to,
                Some(from),
                cost,
                heuristic(self.coordinate(to), self.coordinate(self.goal)),
            ) {
                break;
            }
        }
    }

    fn expand_neighbors(&mut self, parent: Cell) {
        self.expand_neighbors_toward(parent, Some(self.goal));
    }

    fn expand_neighbors_toward(&mut self, parent: Cell, goal: Option<Cell>) -> u32 {
        let parent_coordinate = self.node(parent).coordinate;
        let previous = self
            .node(parent)
            .parent
            .map(|cell| self.node(cell).coordinate);
        let mut created = 0;
        let mut neighbor_flags = [false; 8];
        for (index, [dx, dy]) in NEIGHBORS.into_iter().enumerate() {
            let Some(cell) = self.grid.signed_cell(
                i32::from(parent_coordinate.x) + dx,
                i32::from(parent_coordinate.y) + dy,
            ) else {
                continue;
            };
            // Native ordinary expansion does not reopen visited cells. The
            // goal-directed line callback above can reopen a cheaper route.
            if self.nodes.get(self.index(cell)).is_some() {
                continue;
            }
            if index >= 4 && !neighbor_flags[index - 4] && !neighbor_flags[(index - 3) % 4] {
                continue;
            }
            if self.units.downhill_only
                && self.grid.ground_height_at(parent_coordinate) < self.grid.ground_height_at(cell)
            {
                continue;
            }
            let blocked = self.has(cell, flags::BLOCKED);
            let dozer = blocked && self.units.dozer_obstacles.contains(&cell);
            if blocked && !dozer && !self.tunneling {
                continue;
            }
            neighbor_flags[index] = !dozer;
            let occupied = self.occupied(cell);
            let movement_valid =
                (!blocked || dozer) && self.within_footprint_bounds(cell) && !occupied.enemy;
            if !movement_valid && !self.tunneling {
                continue;
            }
            if movement_valid && !self.grid.pinched.contains(&cell) {
                self.tunneling = false;
            }
            let mut cost = self.node(parent).cost
                + step_cost(previous, parent_coordinate, self.coordinate(cell))
                + self
                    .grid
                    .ground_step_penalty(parent_coordinate, super::LayerCell { cell, layer: 1 });
            // Zero Hour adds one 3*COST_DIAGONAL penalty regardless of how
            // many distinct fixed allies occupy the actor's whole footprint.
            if occupied.fixed {
                cost += 42;
            }
            if occupied.moving
                && cell.x.abs_diff(self.start.x) < 10
                && cell.y.abs_diff(self.start.y) < 10
            {
                cost += 42;
            }
            if let Some((_, penalty)) = self.attack_cost
                && self.goal_occupied(cell, self.radius, self.centered)
            {
                cost += penalty;
            }
            if !super::hierarchy::passable(self.units.corridor.as_ref(), cell) {
                cost += 1000;
            }
            if self.has(cell, flags::OBSTACLE) {
                cost += 1000;
            }
            if self.tunneling && blocked {
                cost += 100;
            }
            let remaining = if self.tunneling {
                0
            } else {
                goal.map_or(0, |goal| {
                    self.attack_cost.map_or_else(
                        || heuristic(self.coordinate(cell), self.coordinate(goal)),
                        |(distance, _)| attack::remaining(cell, self.coordinate(goal), distance),
                    )
                })
            };
            if !self.insert(cell, Some(parent), cost, remaining) {
                return created;
            }
            created += u32::from(self.preallocated != Some(cell) && !self.has(cell, flags::INFO));
        }
        created
    }

    fn occupied(&self, center: Cell) -> Occupied {
        self.occupied_at(center, self.radius, self.centered)
    }

    fn occupied_at(&self, center: Cell, radius: i32, centered: bool) -> Occupied {
        let above = radius + i32::from(centered);
        let mut occupied = Occupied {
            fixed: false,
            moving: false,
            enemy: false,
        };
        for x in i32::from(center.x) - radius..i32::from(center.x) + above {
            for y in i32::from(center.y) - radius..i32::from(center.y) + above {
                if let Some(cell) = self.grid.signed_cell(x, y) {
                    let flags = self.flags[self.index(cell)];
                    occupied.fixed |= flags & flags::FIXED != 0;
                    occupied.moving |= flags & flags::MOVING != 0;
                    occupied.enemy |= flags & flags::ENEMY != 0;
                }
            }
        }
        occupied
    }

    fn goal_occupied(&self, center: Cell, radius: i32, centered: bool) -> bool {
        let above = radius + i32::from(centered);
        (i32::from(center.x) - radius..i32::from(center.x) + above).any(|x| {
            (i32::from(center.y) - radius..i32::from(center.y) + above).any(|y| {
                self.grid
                    .signed_cell(x, y)
                    .is_some_and(|cell| self.has(cell, flags::GOAL))
            })
        })
    }

    fn transient_occupied(&self, center: Cell) -> bool {
        let above = self.radius + i32::from(self.centered);
        (i32::from(center.x) - self.radius..i32::from(center.x) + above).any(|x| {
            (i32::from(center.y) - self.radius..i32::from(center.y) + above).any(|y| {
                self.grid
                    .signed_cell(x, y)
                    .is_some_and(|cell| self.has(cell, flags::TRANSIENT))
            })
        })
    }

    fn path(&self, mut cell: Cell) -> Route {
        let mut raw = vec![self.node(cell).coordinate];
        while let Some(parent) = self.node(cell).parent {
            raw.push(self.node(parent).coordinate);
            cell = parent;
        }
        raw.reverse();
        // prependCells prepends the actual object's position, not the start
        // info's shifted index. Route retains its cell; Path retains exact XY.
        raw[0] = cell;
        self.optimize(raw, false)
    }

    fn optimize(&self, raw: Vec<Cell>, transient: bool) -> Route {
        let raw: Vec<_> = raw
            .into_iter()
            .map(|cell| super::LayerCell { cell, layer: 1 })
            .collect();
        let raw_flags: Vec<_> = raw
            .iter()
            .map(|&node| {
                self.grid.path_cell_flags(
                    node,
                    !self.units.can_path_through_units && self.occupied(node.cell).fixed,
                )
            })
            .collect();
        let path = self
            .grid
            .preview_path(&raw, &raw_flags, self.centered, |from, to| {
                self.shortcut(from.cell, to.cell, transient)
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

    fn shortcut(&self, from: Cell, to: Cell, transient: bool) -> bool {
        line_passable(self.grid, from, to, self.radius, self.centered, |cell| {
            self.units.fixed_allies.contains(&cell) || self.units.fixed_enemies.contains(&cell)
        }) && (!transient
            || line_cells(self.grid, from, to)
                .into_iter()
                .all(|cell| !self.transient_occupied(cell)))
    }
}

pub(super) fn cell_position(cell: Cell, centered: bool) -> [f32; 2] {
    let offset = if centered { 0.5 } else { 0.05 };
    [
        (f32::from(cell.x) + offset) * 10.0,
        (f32::from(cell.y) + offset) * 10.0,
    ]
}

pub(super) fn squared_distance(first: [f32; 2], second: [f32; 2]) -> f32 {
    let x = first[0] - second[0];
    let y = first[1] - second[1];
    let x_squared = x * x;
    let y_squared = y * y;
    x_squared + y_squared
}

struct Occupied {
    fixed: bool,
    moving: bool,
    enemy: bool,
}

pub(super) fn line_passable(
    grid: &Grid,
    start: Cell,
    goal: Cell,
    radius: i32,
    centered: bool,
    mut fixed_unit: impl FnMut(Cell) -> bool,
) -> bool {
    let above = radius + i32::from(centered);
    line_cells(grid, start, goal).into_iter().all(|cell| {
        if grid.is_blocked(cell)
            || i32::from(cell.x) < radius
            || i32::from(cell.y) < radius
            || i32::from(cell.x) + above > i32::from(grid.width)
            || i32::from(cell.y) + above > i32::from(grid.height)
        {
            return false;
        }
        (i32::from(cell.x) - radius..i32::from(cell.x) + above).all(|x| {
            (i32::from(cell.y) - radius..i32::from(cell.y) + above).all(|y| {
                grid.signed_cell(x, y)
                    .is_some_and(|occupied| !fixed_unit(occupied))
            })
        })
    })
}

pub(super) fn heuristic(from: Cell, to: Cell) -> u32 {
    let dx = u32::from(from.x.abs_diff(to.x));
    let dy = u32::from(from.y.abs_diff(to.y));
    10 * dx.max(dy) + 5 * dx.min(dy)
}

pub(super) fn step_cost(previous: Option<Cell>, parent: Cell, cell: Cell) -> u32 {
    let direction = [
        i32::from(parent.x) - i32::from(cell.x),
        i32::from(parent.y) - i32::from(cell.y),
    ];
    let base = if direction[0] == 0 || direction[1] == 0 {
        10
    } else {
        14
    };
    let Some(previous) = previous else {
        return base;
    };
    let before = [
        i32::from(previous.x) - i32::from(parent.x),
        i32::from(previous.y) - i32::from(parent.y),
    ];
    if before == direction {
        return base;
    }
    let dot = before[0] * direction[0] + before[1] * direction[1];
    base + match dot.cmp(&0) {
        Ordering::Greater => 4,
        Ordering::Equal => 8,
        Ordering::Less => 16,
    }
}

/// The source visits both orthogonal substeps when crossing a cell corner.
/// Its final minor-axis callback is retained, including the zero-length case.
// AIPathfind.cpp::iterateCellsAlongLine stops as soon as its callback rejects
// a cell. Keep its minor-before-major orthogonal substeps, including ties.
pub(super) fn line_cells_iter(
    grid: &Grid,
    start: Cell,
    end: Cell,
) -> impl Iterator<Item = Cell> + '_ {
    let dx = i32::from(start.x.abs_diff(end.x));
    let dy = i32::from(start.y.abs_diff(end.y));
    let sx = if end.x >= start.x { 1 } else { -1 };
    let sy = if end.y >= start.y { 1 } else { -1 };
    let (minor, major, denominator, addition) = if dx >= dy {
        ([0, sy], [sx, 0], dx, dy)
    } else {
        ([sx, 0], [0, sy], dy, dx)
    };
    let mut numerator = denominator / 2;
    let mut point = [i32::from(start.x), i32::from(start.y)];
    let mut remaining = denominator + 1;
    let mut minor_pending = false;
    std::iter::from_fn(move || {
        if remaining == 0 {
            return None;
        }
        let Some(cell) = grid.signed_cell(point[0], point[1]) else {
            remaining = 0;
            return None;
        };
        if minor_pending {
            minor_pending = false;
            point[0] += major[0];
            point[1] += major[1];
            remaining -= 1;
        } else {
            numerator += addition;
            if numerator >= denominator {
                numerator -= denominator;
                point[0] += minor[0];
                point[1] += minor[1];
                minor_pending = true;
            } else {
                point[0] += major[0];
                point[1] += major[1];
                remaining -= 1;
            }
        }
        Some(cell)
    })
}

pub(super) fn line_cells(grid: &Grid, start: Cell, end: Cell) -> Vec<Cell> {
    line_cells_iter(grid, start, end).collect()
}

#[cfg(test)]
mod tests {
    use super::NEIGHBORS;
    use super::{Cell, Grid, RouteUnits, Search};
    use std::sync::Arc;

    #[test]
    fn streamed_line_preserves_all_octants_ties_degeneracy_and_map_edges() {
        let grid = Grid::new(7, 7, []).unwrap();
        // Native numerator starts at major/2; equality takes the minor substep first.
        for (end, expected) in [
            ([5, 4], vec![[3, 3], [3, 4], [4, 4], [5, 4], [5, 5]]),
            ([4, 5], vec![[3, 3], [4, 3], [4, 4], [4, 5], [5, 5]]),
            ([2, 5], vec![[3, 3], [2, 3], [2, 4], [2, 5], [1, 5]]),
            ([1, 4], vec![[3, 3], [3, 4], [2, 4], [1, 4], [1, 5]]),
            ([1, 2], vec![[3, 3], [3, 2], [2, 2], [1, 2], [1, 1]]),
            ([2, 1], vec![[3, 3], [2, 3], [2, 2], [2, 1], [1, 1]]),
            ([4, 1], vec![[3, 3], [4, 3], [4, 2], [4, 1], [5, 1]]),
            ([5, 2], vec![[3, 3], [3, 2], [4, 2], [5, 2], [5, 1]]),
        ] {
            let actual: Vec<_> = super::line_cells_iter(
                &grid,
                Cell { x: 3, y: 3 },
                Cell {
                    x: end[0],
                    y: end[1],
                },
            )
            .map(|cell| [cell.x, cell.y])
            .collect();
            assert_eq!(actual, expected);
        }
        for (start, end, expected) in [
            (
                [1, 1],
                [3, 3],
                vec![[1, 1], [1, 2], [2, 2], [2, 3], [3, 3], [3, 4]],
            ),
            ([2, 2], [2, 2], vec![[2, 2], [2, 3]]),
            ([6, 6], [6, 6], vec![[6, 6]]),
            ([1, 1], [3, 1], vec![[1, 1], [2, 1], [3, 1]]),
            ([1, 1], [1, 3], vec![[1, 1], [1, 2], [1, 3]]),
            ([5, 5], [6, 6], vec![[5, 5], [5, 6], [6, 6]]),
        ] {
            let actual: Vec<_> = super::line_cells_iter(
                &grid,
                Cell {
                    x: start[0],
                    y: start[1],
                },
                Cell {
                    x: end[0],
                    y: end[1],
                },
            )
            .map(|cell| [cell.x, cell.y])
            .collect();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn streamed_line_can_stop_at_the_first_blocked_substep_without_consuming_its_tail() {
        let grid = Grid::new(1000, 1000, []).unwrap();
        let mut cells = super::line_cells_iter(&grid, Cell { x: 1, y: 1 }, Cell { x: 998, y: 998 });
        assert_eq!(cells.next().map(|cell| [cell.x, cell.y]), Some([1, 1]));
        let blocked = Cell { x: 1, y: 2 };
        assert!(
            cells
                .try_for_each(|cell| if cell == blocked { Err(()) } else { Ok(()) })
                .is_err()
        );
        assert_eq!(cells.next().map(|cell| [cell.x, cell.y]), Some([2, 2]));
    }

    #[test]
    fn direct_line_stops_at_fixed_allies_but_keeps_the_native_transient_exception() {
        let grid = Grid::new(25, 20, []).unwrap();
        let start = Cell { x: 3, y: 8 };
        let goal = Cell { x: 20, y: 8 };
        let blocker = Cell { x: 10, y: 8 };
        let mut units = RouteUnits::new();
        units.fixed_allies.insert(blocker);
        let mut search = Search::new(&grid, &units, start, goal, 0, true, false);
        search.insert(start, None, 0, 0);
        search.expand_line(start);
        assert!(
            search
                .nodes
                .get(search.index(Cell { x: 9, y: 8 }))
                .is_some()
        );
        assert!(search.nodes.get(search.index(blocker)).is_none());
        assert!(search.nodes.get(search.index(goal)).is_none());
        units.fixed_allies.clear();
        units.moving_allies.insert(blocker);
        let mut search = Search::new(&grid, &units, start, goal, 0, true, false);
        search.insert(start, None, 0, 0);
        search.expand_line(start);
        assert!(search.nodes.get(search.index(goal)).is_some());
    }

    #[test]
    fn fixed_footprint_is_charged_once_and_moving_penalty_ends_at_ten_cells() {
        let grid = Grid::new(30, 20, []).unwrap();
        let mut units = RouteUnits::new();
        units
            .fixed_allies
            .extend([Cell { x: 10, y: 9 }, Cell { x: 11, y: 9 }]);
        let start = Cell { x: 9, y: 10 };
        let next = Cell { x: 10, y: 10 };
        let mut search = Search::new(&grid, &units, start, Cell { x: 25, y: 10 }, 1, true, false);
        search.insert(start, None, 0, 0);
        search.expand_neighbors(start);
        assert_eq!(search.node(next).cost, 10 + 42);

        for (distance, expected) in [(9, 52), (10, 10)] {
            let start = Cell { x: 2, y: 2 };
            let parent = Cell {
                x: start.x + distance - 1,
                y: 2,
            };
            let next = Cell {
                x: parent.x + 1,
                y: 2,
            };
            let mut units = RouteUnits::new();
            units.moving_allies.insert(next);
            let mut search =
                Search::new(&grid, &units, start, Cell { x: 25, y: 2 }, 0, true, false);
            search.insert(parent, None, 0, 0);
            search.expand_neighbors(parent);
            assert_eq!(search.node(next).cost, expected);
        }
    }
    #[test]
    fn downhill_route_cannot_cross_an_uphill_ridge_despite_lower_final_goal() {
        let mut grid = Grid::new(6, 5, []).unwrap();
        for y in 0..5_usize {
            Arc::make_mut(&mut grid.ground_heights)[y * 6 + 1] = 3.0;
            Arc::make_mut(&mut grid.ground_heights)[y * 6 + 2] = 5.0;
        }
        let mut units = RouteUnits::new();
        let start = Cell { x: 1, y: 2 };
        let goal = Cell { x: 4, y: 2 };
        assert!(
            grid.path_for_counted(1, start, goal, 0, true, &units)
                .route
                .is_some()
        );
        units.downhill_only = true;
        assert!(
            grid.path_for_counted(1, start, goal, 0, true, &units)
                .route
                .is_none()
        );
    }

    #[test]
    fn native_cliff_height_threshold_and_pinched_costs_remain_distinct() {
        let start = Cell { x: 2, y: 2 };
        let east = Cell { x: 3, y: 2 };
        for (height, pinched, expected) in [(9.99, false, 108), (10.0, false, 10), (0.0, true, 34)]
        {
            let mut grid = Grid::new(7, 7, []).unwrap();
            Arc::make_mut(&mut grid.cliffs).insert(east);
            Arc::make_mut(&mut grid.ground_heights)[2 * 7 + 3] = height;
            if pinched {
                Arc::make_mut(&mut grid.pinched).insert(east);
            }
            let units = RouteUnits::new();
            let mut search = Search::new(&grid, &units, start, Cell { x: 5, y: 5 }, 0, true, false);
            search.insert(start, None, 0, 0);
            search.expand_neighbors(start);
            assert_eq!(search.node(east).cost, expected);
        }
    }

    #[test]
    fn friendly_dozer_obstacles_charge_native_cost_without_opening_diagonal_sides() {
        let start = Cell { x: 2, y: 2 };
        let east = Cell { x: 3, y: 2 };
        let north = Cell { x: 2, y: 3 };
        let diagonal = Cell { x: 3, y: 3 };
        let grid = Grid::new(7, 7, []).unwrap().with_obstacles([east, north]);
        let mut units = RouteUnits::new();
        units.dozer_obstacles.extend([east, north]);
        let mut search = Search::new(&grid, &units, start, Cell { x: 5, y: 5 }, 0, true, false);
        search.insert(start, None, 0, 0);
        search.expand_neighbors(start);
        assert_eq!(search.node(east).cost, 1010);
        assert_eq!(search.node(north).cost, 1010);
        assert!(search.nodes.get(search.index(diagonal)).is_none());
    }

    #[test]
    fn pinned_native_neighbor_expansions_match_costs_and_admission() {
        let fixture = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/reference/path-expansion-reference.txt"
        ))
        .unwrap();
        let mut lines = fixture.lines();
        let mut cases = 0;
        while let Some(mode) = lines.next() {
            let mode: u8 = mode.parse().unwrap();
            let east = Cell { x: 3, y: 2 };
            let north = Cell { x: 2, y: 3 };
            let start = Cell { x: 2, y: 2 };
            let mut grid = Grid::new(7, 7, []).unwrap();
            if mode & 8 != 0 {
                grid = grid.with_obstacles([east, north]);
            } else {
                Arc::make_mut(&mut grid.cliffs).insert(east);
            }
            if mode & 16 != 0 {
                Arc::make_mut(&mut grid.pinched).insert(east);
            }
            Arc::make_mut(&mut grid.ground_heights)[2 * 7 + 3] =
                if mode & 2 != 0 { 10.0 } else { 9.5 };
            let mut units = RouteUnits::new();
            units.downhill_only = mode & 2 != 0;
            if mode & 1 != 0 && mode & 8 != 0 {
                units.dozer_obstacles.extend([east, north]);
            }
            let mut search = Search::new(
                &grid,
                &units,
                start,
                Cell { x: 5, y: 5 },
                0,
                true,
                mode & 4 != 0,
            );
            search.insert(start, None, 0, 0);
            search.expand_neighbors_toward(start, None);
            let actual: Vec<i64> = NEIGHBORS
                .iter()
                .map(|&[dx, dy]| {
                    let cell = grid.signed_cell(2 + dx, 2 + dy).unwrap();
                    search
                        .nodes
                        .get(search.index(cell))
                        .map_or(-1, |node| i64::from(node.cost))
                })
                .collect();
            let expected: Vec<i64> = lines
                .next()
                .unwrap()
                .split_whitespace()
                .map(|word| word.parse().unwrap())
                .collect();
            assert_eq!(actual, expected, "native expansion mode {mode}");
            cases += 1;
        }
        assert_eq!(cases, 32);
    }

    #[test]
    fn ignored_producer_remains_obstacle_for_native_neighbor_cost() {
        let start = Cell { x: 1, y: 1 };
        let goal = Cell { x: 2, y: 1 };
        let obstacle = super::super::LayerCell {
            cell: goal,
            layer: 1,
        };
        let grid = Grid::new(5, 5, []).unwrap().with_obstacles([goal]);
        let ignored = grid.without_layer_obstacles([obstacle]);
        assert!(grid.layer_blocked(obstacle));
        assert!(!ignored.layer_blocked(obstacle));
        assert!(ignored.layer_obstacle(obstacle));
        let units = RouteUnits::new();
        let mut search = Search::new(&ignored, &units, start, goal, 0, true, false);
        assert!(search.insert(start, None, 0, 0));
        search.expand_neighbors(start);
        assert_eq!(search.node(goal).cost, 1010);
        assert!(ignored.layer_destination_clear(obstacle, 0, true));
    }
    #[test]
    fn native_corridor_stops_line_but_charges_neighbor_instead_of_rejecting_it() {
        let grid = Grid::new(25, 10, []).unwrap();
        let mut units = RouteUnits::new();
        units.corridor = Some(std::collections::BTreeSet::from([Cell { x: 0, y: 0 }]));
        let start = Cell { x: 9, y: 5 };
        let next = Cell { x: 10, y: 5 };
        let mut search = Search::new(&grid, &units, start, Cell { x: 22, y: 5 }, 0, true, false);
        search.insert(start, None, 0, 0);
        search.expand_line(start);
        assert!(search.nodes.get(search.index(next)).is_none());
        search.expand_neighbors(start);
        assert_eq!(search.node(next).cost, 1010);
    }

    #[test]
    fn retained_info_coordinates_drive_native_step_cost_and_line_destination() {
        let grid = Grid::new(25, 8, []).unwrap();
        let start = Cell { x: 3, y: 2 };
        let goal = Cell { x: 20, y: 2 };
        let east = Cell { x: 4, y: 2 };
        let mut units = RouteUnits::new();
        for (physical, indexed) in [(east, Cell { x: 4, y: 3 }), (goal, Cell { x: 20, y: 4 })] {
            units.info_cells.insert(physical);
            units.info_records.insert(
                physical,
                super::super::CellInfo {
                    coordinate: super::super::LayerCell {
                        cell: indexed,
                        layer: 1,
                    },
                    cost: 700,
                    total: 999,
                },
            );
        }
        let mut search = Search::new(&grid, &units, start, goal, 0, true, false);
        search.insert(start, None, 0, 0);
        search.expand_neighbors(start);
        assert_eq!((search.node(east).cost, search.node(east).total), (14, 179));
        let mut search = Search::new(&grid, &units, start, goal, 0, true, false);
        search.insert(start, None, 0, 0);
        search.expand_line(start);
        assert!(
            search
                .nodes
                .get(search.index(Cell { x: 20, y: 4 }))
                .is_some()
        );
        assert!(search.nodes.get(search.index(goal)).is_none());
    }
}
