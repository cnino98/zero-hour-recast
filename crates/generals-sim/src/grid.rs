use std::collections::{BTreeSet, VecDeque};

use std::{fmt, sync::Arc};

const MAX_GRID_CELLS: usize = 1_048_576;
pub const CELL_INFO_CAPACITY: usize = 30_000;

mod cache;
pub mod construction;
mod group;
mod hierarchy;
pub use hierarchy::{HierarchyQuery, HierarchyResult};
mod layers;
pub use layers::LayerQuery;
pub use layers::patch::LayerPatch;
pub mod move_away;
pub mod patch;
mod search;
mod zones;
pub use search::attack::Attack;
pub use search::attack::AttackPoint;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod sharing_tests;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Cell {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct LayerCell {
    pub cell: Cell,
    pub layer: u8,
}

// Queries share immutable backing; each ignored-object view owns its admission mask.
#[derive(Clone)]
pub struct Grid {
    width: u16,
    height: u16,
    blocked: Arc<BTreeSet<Cell>>,
    cliffs: Arc<BTreeSet<Cell>>,
    ground_heights: Arc<Vec<f32>>,
    source_kinds: Arc<Vec<crate::terrain::CellType>>,
    source_zones: Option<Arc<hierarchy::zones::Zones>>,
    bridge_ends: Arc<std::collections::BTreeMap<u8, [Cell; 2]>>,
    pinched: Arc<BTreeSet<Cell>>,
    obstacles: Arc<BTreeSet<Cell>>,
    ignored_obstacles: BTreeSet<LayerCell>,
    fences: Arc<BTreeSet<LayerCell>>,
    layers: Arc<std::collections::BTreeMap<u8, layers::Layer>>,
    ground_connections: Arc<std::collections::BTreeMap<Cell, u8>>,
}

/// Retained fields of native `PathfindCellInfo`; search-list parent links are
/// temporary and are cleared by the fixed profile's releaseInfo.
#[derive(Clone, Copy)]
pub struct CellInfo {
    pub coordinate: LayerCell,
    pub cost: u32,
    pub total: u32,
}

impl CellInfo {
    pub(crate) const fn new(coordinate: LayerCell) -> Self {
        Self {
            coordinate,
            cost: 0,
            total: 0,
        }
    }
}

/// Ordered native object-footprint cell classification and zone update event.
pub struct ZoneModification {
    pub update_bounds: Option<[[i32; 2]; 2]>,
    pub cells: Vec<ZoneCellChange>,
    pub post_cells: Vec<ZoneCellChange>,
    pub pinched: Vec<(Cell, bool)>,
    pub force_dirty: bool,
}

pub struct ZoneCellChange {
    pub cell: Cell,
    pub kind: crate::terrain::CellType,
    pub fence: bool,
    pub reset_zone: bool,
}

/// Actor-relative mobile-cell flags from `Pathfinder::checkForMovement`.
/// Infantry/crush/ignored-object policy is resolved by the simulation; static
/// searches use an empty context and never mutate the underlying terrain grid.
pub struct RouteUnits {
    pub info_available: usize,
    // Authoritative per-query snapshot; standalone contexts retain their owned inputs.
    pub(crate) shared_info: Option<(u8, Arc<std::collections::BTreeMap<LayerCell, CellInfo>>)>,
    pub downhill_only: bool,
    pub corridor: Option<BTreeSet<Cell>>,
    pub dozer_obstacles: BTreeSet<Cell>,
    pub info_cells: BTreeSet<Cell>,
    pub info_records: std::collections::BTreeMap<Cell, CellInfo>,
    pub width_blockers: BTreeSet<Cell>,
    pub goal_flags: BTreeSet<Cell>,
    pub idle_positions: BTreeSet<Cell>,
    pub fixed_allies: BTreeSet<Cell>,
    pub can_path_through_units: bool,
    pub moving_allies: BTreeSet<Cell>,
    pub transient_allies: BTreeSet<Cell>,
    pub transient_enemies: BTreeSet<Cell>,
    pub fixed_enemies: BTreeSet<Cell>,
}

/// Native searched nodes remain available after smoothing for `moveAllies`.
pub struct Route {
    pub raw: Vec<LayerCell>,
    pub(crate) raw_flags: Vec<construction::CellFlags>,
    pub(crate) optimization_blocked: bool,
    pub optimized: VecDeque<LayerCell>,
    pub blocked_by_ally: bool,
    pub exact_endpoint: Option<[f32; 3]>,
    pub quick_start: Option<[f32; 3]>,
}

/// Source open/closed-list cleanup cost, including unsuccessful searches.
pub struct SearchResult {
    pub route: Option<Route>,
    pub cells: usize,
    pub info_cleanup: Vec<(LayerCell, CellInfo)>,
    pub retained_infos: Vec<(LayerCell, CellInfo)>,
}

impl SearchResult {
    pub(crate) const fn empty() -> Self {
        Self {
            route: None,
            cells: 0,
            info_cleanup: Vec::new(),
            retained_infos: Vec::new(),
        }
    }
}

impl Route {
    pub(crate) const fn new() -> Self {
        Self {
            raw: Vec::new(),
            raw_flags: Vec::new(),
            optimization_blocked: false,
            optimized: VecDeque::new(),
            blocked_by_ally: false,
            exact_endpoint: None,
            quick_start: None,
        }
    }

    pub(crate) fn direct(start: Cell, goal: Cell) -> Self {
        Self::direct_on(
            LayerCell {
                cell: start,
                layer: 1,
            },
            LayerCell {
                cell: goal,
                layer: 1,
            },
        )
    }

    pub(crate) fn direct_on(start: LayerCell, goal: LayerCell) -> Self {
        Self {
            raw: vec![start, goal],
            raw_flags: Vec::new(),
            optimization_blocked: false,
            optimized: VecDeque::from([start, goal]),
            blocked_by_ally: false,
            exact_endpoint: None,
            quick_start: None,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.raw.clear();
        self.raw_flags.clear();
        self.optimization_blocked = false;
        self.optimized.clear();
        self.blocked_by_ally = false;
        self.exact_endpoint = None;
        self.quick_start = None;
    }
}

impl RouteUnits {
    pub(crate) fn has_info(&self, cell: Cell) -> bool {
        self.shared_info.as_ref().map_or_else(
            || self.info_cells.contains(&cell),
            |(layer, infos)| {
                infos.contains_key(&LayerCell {
                    cell,
                    layer: *layer,
                })
            },
        )
    }

    pub(crate) fn info_record(&self, cell: Cell) -> Option<&CellInfo> {
        self.shared_info.as_ref().map_or_else(
            || self.info_records.get(&cell),
            |(layer, infos)| {
                infos.get(&LayerCell {
                    cell,
                    layer: *layer,
                })
            },
        )
    }

    pub(crate) fn retained_cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.info_cells
            .iter()
            .copied()
            .filter(|_| self.shared_info.is_none())
            .chain(self.shared_info.iter().flat_map(|(layer, infos)| {
                infos
                    .keys()
                    .filter_map(move |node| (node.layer == *layer).then_some(node.cell))
            }))
    }

    pub(crate) const fn new() -> Self {
        Self {
            info_available: CELL_INFO_CAPACITY,
            shared_info: None,
            downhill_only: false,
            corridor: None,
            dozer_obstacles: BTreeSet::new(),
            info_cells: BTreeSet::new(),
            info_records: std::collections::BTreeMap::new(),
            width_blockers: BTreeSet::new(),
            goal_flags: BTreeSet::new(),
            idle_positions: BTreeSet::new(),
            fixed_allies: BTreeSet::new(),
            can_path_through_units: false,
            moving_allies: BTreeSet::new(),
            transient_allies: BTreeSet::new(),
            transient_enemies: BTreeSet::new(),
            fixed_enemies: BTreeSet::new(),
        }
    }
}

impl Grid {
    /// Creates a static battlefield with at most 1,048,576 cells.
    ///
    /// # Errors
    /// Returns an error for empty/oversized grids or blocked cells outside the battlefield.
    pub fn new(
        width: u16,
        height: u16,
        blocked: impl IntoIterator<Item = Cell>,
    ) -> Result<Self, GridError> {
        if width == 0 || height == 0 || usize::from(width) * usize::from(height) > MAX_GRID_CELLS {
            return Err(GridError::InvalidGrid);
        }
        let mut grid = Self {
            width,
            height,
            blocked: Arc::new(blocked.into_iter().collect()),
            cliffs: Arc::new(BTreeSet::new()),
            ground_heights: Arc::new(vec![0.0; usize::from(width) * usize::from(height)]),
            source_kinds: Arc::new(vec![
                crate::terrain::CellType::Clear;
                usize::from(width) * usize::from(height)
            ]),
            source_zones: None,
            bridge_ends: Arc::new(std::collections::BTreeMap::new()),
            pinched: Arc::new(BTreeSet::new()),
            obstacles: Arc::new(BTreeSet::new()),
            ignored_obstacles: BTreeSet::new(),
            fences: Arc::new(BTreeSet::new()),
            layers: Arc::new(std::collections::BTreeMap::new()),
            ground_connections: Arc::new(std::collections::BTreeMap::new()),
        };
        if grid.blocked.iter().any(|&cell| !grid.contains(cell)) {
            return Err(GridError::OutsideBattlefield);
        }
        for &cell in grid.blocked.iter() {
            let index = grid.source_index(cell);
            Arc::make_mut(&mut grid.source_kinds)[index] = crate::terrain::CellType::Impassable;
        }
        grid.recalculate_source_zones();
        Ok(grid)
    }

    fn ground_height_at(&self, cell: Cell) -> f32 {
        self.ground_heights[usize::from(cell.y) * usize::from(self.width) + usize::from(cell.x)]
    }

    fn ground_step_penalty(&self, from: Cell, to: LayerCell) -> u32 {
        if to.layer != 1 {
            return 0;
        }
        if self.cliffs.contains(&to.cell) && !self.pinched.contains(&to.cell) {
            if (self.ground_height_at(from) - self.ground_height_at(to.cell)).abs() < 10.0 {
                98
            } else {
                0
            }
        } else if self.pinched.contains(&to.cell) {
            24
        } else {
            0
        }
    }

    pub(crate) const fn width(&self) -> u16 {
        self.width
    }

    pub(crate) const fn height(&self) -> u16 {
        self.height
    }

    pub(crate) const fn contains(&self, cell: Cell) -> bool {
        cell.x < self.width && cell.y < self.height
    }

    /// An out-of-bounds cell is also impassable.
    pub(crate) fn is_blocked(&self, cell: Cell) -> bool {
        !self.contains(cell)
            || (!self
                .ignored_obstacles
                .contains(&LayerCell { cell, layer: 1 })
                && (self.blocked.contains(&cell) || self.obstacles.contains(&cell)))
    }

    pub(crate) fn restore_ground_terrain(&mut self, terrain: &crate::terrain::TerrainMap) {
        self.blocked = Arc::new(terrain.blocked_ground_cells().collect());
        self.cliffs = Arc::new(terrain.cliff_cells().collect());
        self.pinched = Arc::new(BTreeSet::new());
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = Cell { x, y };
                let index = self.source_index(cell);
                Arc::make_mut(&mut self.source_kinds)[index] =
                    terrain.path_kind(cell).expect("matching terrain extent");
                if terrain.cell(cell).is_some_and(|data| data.pinched) {
                    Arc::make_mut(&mut self.pinched).insert(cell);
                }
            }
        }
        self.recalculate_source_zones();
    }

    pub(crate) fn clear_ground_barriers(&mut self, cells: impl IntoIterator<Item = Cell>) {
        for cell in cells {
            Arc::make_mut(&mut self.blocked).remove(&cell);
            Arc::make_mut(&mut self.cliffs).remove(&cell);
        }
    }

    fn set_obstacles(&mut self, obstacles: impl IntoIterator<Item = Cell>) {
        self.obstacles = Arc::new(obstacles.into_iter().collect());
        for &cell in self.obstacles.iter() {
            let index = self.source_index(cell);
            Arc::make_mut(&mut self.source_kinds)[index] = crate::terrain::CellType::Obstacle;
        }
    }

    // Retained eager constructor for pathfinding fixtures and batch equivalence.
    #[cfg(test)]
    pub(crate) fn with_obstacles(&self, obstacles: impl IntoIterator<Item = Cell>) -> Self {
        let mut grid = self.clone();
        grid.set_obstacles(obstacles);
        grid.recalculate_source_zones();
        grid
    }

    pub(crate) fn without_surface_barriers(&self, cells: impl IntoIterator<Item = Cell>) -> Self {
        let mut grid = self.clone();
        for cell in cells {
            Arc::make_mut(&mut grid.blocked).remove(&cell);
            Arc::make_mut(&mut grid.cliffs).insert(cell);
        }
        grid
    }

    /// `prependCells` protects the non-cliff node on either side of each cliff
    /// run. Retain those anchors before smoothing away the intervening cells.
    #[cfg(test)]
    pub(crate) fn cliff_boundary_cells<'a>(
        &'a self,
        raw: &'a [LayerCell],
    ) -> impl Iterator<Item = LayerCell> + 'a {
        raw.windows(2).filter_map(|pair| {
            match (
                pair[0].layer == 1 && self.cliffs.contains(&pair[0].cell),
                pair[1].layer == 1 && self.cliffs.contains(&pair[1].cell),
            ) {
                (true, false) => Some(pair[1]),
                (false, true) => Some(pair[0]),
                _ => None,
            }
        })
    }

    pub(crate) fn with_surface_barriers(&self, cells: impl IntoIterator<Item = Cell>) -> Self {
        let mut grid = self.clone();
        Arc::make_mut(&mut grid.blocked).extend(cells);
        grid
    }

    pub(crate) fn is_obstacle(&self, cell: Cell) -> bool {
        self.obstacles.contains(&cell)
    }

    pub(crate) fn group_path_counted(
        &self,
        start: LayerCell,
        goal: LayerCell,
        units: &std::collections::BTreeMap<u8, RouteUnits>,
    ) -> SearchResult {
        group::find_layers(
            self,
            start,
            goal,
            &units.iter().map(|(&layer, units)| (layer, units)).collect(),
        )
    }

    pub(crate) fn path_cost_counted(
        &self,
        start: LayerCell,
        goal: LayerCell,
        units: &std::collections::BTreeMap<u8, RouteUnits>,
    ) -> group::CostResult {
        group::cost_layers(
            self,
            start,
            goal,
            &units.iter().map(|(&layer, units)| (layer, units)).collect(),
        )
    }

    pub(crate) fn line_cells(&self, start: Cell, goal: Cell) -> Vec<Cell> {
        search::line_cells(self, start, goal)
    }

    /// Ground `isLinePassable` with transient units ignored and pinched cells
    /// allowed. The represented grid has no separate layer or pinched metadata.
    pub(crate) fn line_passable(
        &self,
        start: Cell,
        goal: Cell,
        radius: i32,
        centered: bool,
        fixed_unit: impl FnMut(Cell) -> bool,
    ) -> bool {
        search::line_passable(self, start, goal, radius, centered, fixed_unit)
    }

    pub(crate) fn closest_path_counted(
        &self,
        start: Cell,
        goal: Cell,
        radius: i32,
        centered: bool,
        unavailable: &BTreeSet<Cell>,
        units: &RouteUnits,
    ) -> SearchResult {
        if !self.contains(start) || !self.contains(goal) {
            return SearchResult::empty();
        }
        search::closest_counted(self, start, goal, radius, centered, unavailable, units)
    }

    pub(crate) fn recovery_path_counted(
        &self,
        start: Cell,
        goal: Cell,
        radius: i32,
        centered: bool,
        unavailable: &BTreeSet<Cell>,
        units: &RouteUnits,
    ) -> SearchResult {
        if !self.contains(start) || !self.contains(goal) {
            return SearchResult::empty();
        }
        search::recovery_counted(self, start, goal, radius, centered, unavailable, units)
    }

    #[cfg(test)]
    pub(crate) fn path(&self, start: Cell, goal: Cell) -> Option<VecDeque<Cell>> {
        if self.is_blocked(start) || self.is_blocked(goal) {
            return None;
        }
        search::find_counted(self, start, goal, 0, true, false, &RouteUnits::new())
            .route
            .map(|route| route.optimized.into_iter().map(|node| node.cell).collect())
    }

    #[cfg(test)]
    pub(crate) fn path_from_occupied(&self, start: Cell, goal: Cell) -> Option<VecDeque<Cell>> {
        if !self.contains(start) || self.is_blocked(goal) {
            return None;
        }
        if self.is_blocked(start) {
            search::find_counted(self, start, goal, 0, true, true, &RouteUnits::new())
                .route
                .map(|route| route.optimized.into_iter().map(|node| node.cell).collect())
        } else {
            self.path(start, goal)
        }
    }

    pub(crate) fn path_for_counted(
        &self,
        surfaces: u8,
        start: Cell,
        goal: Cell,
        radius: i32,
        centered: bool,
        units: &RouteUnits,
    ) -> SearchResult {
        if !self.quick_path_exists(surfaces, start, goal)
            || !self.destination_clear(goal, radius, centered)
        {
            return SearchResult::empty();
        }
        search::find_counted(
            self,
            start,
            goal,
            radius,
            centered,
            self.is_obstacle(start) || self.is_blocked(start),
            units,
        )
    }

    /// Static part of `checkDestination`, including the fixed profile's rejection
    /// of impassable cells across the full source radius/centering footprint.
    pub(crate) fn destination_clear(&self, cell: Cell, radius: i32, centered: bool) -> bool {
        let above = radius + i32::from(centered);
        (i32::from(cell.x) - radius..i32::from(cell.x) + above).all(|x| {
            (i32::from(cell.y) - radius..i32::from(cell.y) + above).all(|y| {
                self.signed_cell(x, y)
                    .is_some_and(|cell| !self.is_blocked(cell))
            })
        })
    }

    fn signed_cell(&self, x: i32, y: i32) -> Option<Cell> {
        let cell = Cell {
            x: u16::try_from(x).ok()?,
            y: u16::try_from(y).ok()?,
        };
        self.contains(cell).then_some(cell)
    }
}

#[derive(Debug)]
pub enum GridError {
    InvalidGrid,
    OutsideBattlefield,
}

impl fmt::Display for GridError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidGrid => "battlefield dimensions are empty or too large",
            Self::OutsideBattlefield => "cell is outside the battlefield",
        })
    }
}

impl std::error::Error for GridError {}
