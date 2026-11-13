//! Ground-layer `Pathfinder::classifyMap` and `validMovementTerrain`.
//!
//! Source: `SuperHackers` 2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed,
//! `Core/GameEngine/Source/GameLogic/AI/AIPathfind.cpp`.
//! Physics retains unexpanded inputs for `PhysicsBehavior::testStunnedUnitForDestruction`;
//! off-map bounds follow `Object::reactToTransformChange` and `Region3D::isInRegion`.

mod bridges;
pub(crate) mod walls;
pub use bridges::{BridgeCell, BridgeGeometry};

use crate::{Cell, GridError};
use num_traits::ToPrimitive;

#[derive(Clone, Copy)]
pub enum CellType {
    Clear,
    Water,
    Cliff,
    Rubble,
    Obstacle,
    BridgeImpassable,
    Impassable,
}

pub struct TerrainCell {
    pub kind: CellType,
    pub pinched: bool,
    pub height: f32,
}

/// Original integer polygon points retained for exact physics water queries.
pub struct WaterPolygon {
    points: Vec<[i32; 3]>,
}

impl WaterPolygon {
    /// Validate the source signed intersection arithmetic before map admission.
    /// # Errors
    /// Rejects empty polygons or subtraction/multiplication overflow.
    pub fn new(points: Vec<[i32; 3]>) -> Result<Self, &'static str> {
        if points.is_empty() {
            return Err("Empty water polygon.");
        }
        for (index, start) in points.iter().enumerate() {
            let end = points[(index + 1) % points.len()];
            if end[1] == start[1] {
                continue;
            }
            let horizontal = end[0]
                .checked_sub(start[0])
                .ok_or("Water polygon subtraction overflows.")?;
            let vertical = end[1]
                .checked_sub(start[1])
                .ok_or("Water polygon subtraction overflows.")?;
            // A crossing ray's signed offset lies between zero and this edge's
            // vertical delta, so this bounds every possible source product.
            horizontal
                .checked_mul(vertical)
                .ok_or("Water polygon intersection overflows.")?;
        }
        Ok(Self { points })
    }
}

/// `TerrainLogic::getWaterHandle`: rounded native XY, highest nonnegative
/// polygon height, with the original last-on-equal-height handle ordering.
/// # Errors
/// Rejects nonfinite or nonrepresentable query coordinates and polygon arithmetic overflow.
pub fn water_height(
    polygons: &[WaterPolygon],
    position: [f32; 2],
) -> Result<Option<f32>, &'static str> {
    let point = [
        (position[0] + 0.5)
            .floor()
            .to_i32()
            .ok_or("Invalid water query coordinate")?,
        (position[1] + 0.5)
            .floor()
            .to_i32()
            .ok_or("Invalid water query coordinate")?,
    ];
    let mut result = None;
    let mut highest = 0.0;
    for polygon in polygons {
        if polygon_contains(&polygon.points, point)? {
            let height = source_integer_real(polygon.points[0][2]);
            if height >= highest {
                highest = height;
                result = Some(height);
            }
        }
    }
    Ok(result)
}

/// `PolygonTrigger::pointInTrigger` with original signed integer intersections.
/// # Errors
/// Rejects source subtraction/multiplication overflow rather than wrapping it.
pub fn polygon_contains(points: &[[i32; 3]], point: [i32; 2]) -> Result<bool, &'static str> {
    let Some(first) = points.first() else {
        return Ok(false);
    };
    let (mut minimum, mut maximum) = (*first, *first);
    for vertex in points {
        for axis in 0..2 {
            minimum[axis] = minimum[axis].min(vertex[axis]);
            maximum[axis] = maximum[axis].max(vertex[axis]);
        }
    }
    if (0..2).any(|axis| point[axis] < minimum[axis] || point[axis] > maximum[axis]) {
        return Ok(false);
    }
    let mut result = false;
    for (index, start) in points.iter().enumerate() {
        let end = points[(index + 1) % points.len()];
        if start[1] == end[1]
            || (start[1] < point[1] && end[1] < point[1])
            || (start[1] >= point[1] && end[1] >= point[1])
            || (start[0] < point[0] && end[0] < point[0])
        {
            continue;
        }
        // The original multiplication is signed integer arithmetic before its
        // result is converted to Real. Reject its undefined overflow inputs.
        let vertical = end[1]
            .checked_sub(start[1])
            .ok_or("Water polygon subtraction overflows.")?;
        let horizontal = end[0]
            .checked_sub(start[0])
            .ok_or("Water polygon subtraction overflows.")?;
        let offset = point[1]
            .checked_sub(start[1])
            .ok_or("Water polygon subtraction overflows.")?;
        let product = horizontal
            .checked_mul(offset)
            .ok_or("Water polygon intersection overflows.")?;
        let intersection = source_integer_real(start[0])
            + source_integer_real(product) / source_integer_real(vertical);
        if intersection >= source_integer_real(point[0]) {
            result = !result;
        }
    }
    Ok(result)
}

fn source_integer_real(value: i32) -> f32 {
    let upper = i16::try_from(value >> 16).expect("signed upper word");
    let lower = u16::try_from(value & 0xffff).expect("unsigned lower word");
    let high = f32::from(upper) * 65_536.0;
    high + f32::from(lower)
}

pub struct TerrainMap {
    width: u16,
    height: u16,
    cells: Vec<TerrainCell>,
    source_cliffs: Vec<u8>,
    water_polygons: Vec<WaterPolygon>,
    rubble: std::collections::BTreeSet<Cell>,
    path_changes: std::collections::BTreeMap<usize, (CellType, bool)>,
    bridges: Vec<bridges::BridgeLayer>,
    walls: walls::Layer,
    bridge_connections: Vec<Option<u8>>,
    build_restrictions: Vec<bool>,
}

impl TerrainMap {
    #[must_use]
    pub fn bridge_layer_active(&self, layer: u8) -> bool {
        if layer == 15 {
            return self.walls.bounds.is_some();
        }
        layer
            .checked_sub(2)
            .and_then(|index| self.bridges.get(usize::from(index)))
            .is_some_and(|bridge| !bridge.destroyed)
    }

    #[must_use]
    pub fn bridge_endpoint_cells(&self, layer: u8) -> Option<[Cell; 2]> {
        if layer == 15 {
            return None;
        }
        self.bridge_geometry(layer)?.endpoint_cells()
    }

    pub fn set_ground_heights(&mut self, sample: impl Fn([f32; 2]) -> f32) {
        for y in 0..self.height {
            for x in 0..self.width {
                let index = usize::from(y) * usize::from(self.width) + usize::from(x);
                self.cells[index].height = sample([f32::from(x) * 10.0, f32::from(y) * 10.0]);
            }
        }
    }
    pub fn set_build_restrictions(&mut self, cells: impl IntoIterator<Item = Cell>) {
        self.build_restrictions.fill(false);
        for cell in cells {
            if let Some(index) = self.index(cell) {
                self.build_restrictions[index] = true;
            }
        }
    }

    #[must_use]
    pub fn build_restricted(&self, cell: Cell) -> bool {
        self.index(cell)
            .is_none_or(|index| self.build_restrictions[index])
    }

    /// Installs native bridge cells in first-free layer order.
    /// # Errors
    /// Rejects invalid geometry, off-map bridges, or exhaustion of the 14 upper layers.
    pub fn add_bridge(
        &mut self,
        geometry: BridgeGeometry,
        ground_height: impl Fn([f32; 2]) -> f32,
    ) -> Result<u8, &'static str> {
        if self.bridges.len() >= 14 {
            return Err("No free bridge pathfinding layer");
        }
        let layer =
            u8::try_from(self.bridges.len() + 2).map_err(|_| "No free bridge pathfinding layer")?;
        let bridge = bridges::BridgeLayer::new(geometry, self.width, self.height, layer)?;
        for (cell, data) in &bridge.cells {
            let index = self.index(*cell).ok_or("Bridge cell outside map extent")?;
            if data.connect_ground {
                self.bridge_connections[index] = Some(layer);
            } else if !matches!(data.kind, CellType::Impassable) {
                let x = f32::from(cell.x) * 10.0;
                let y = f32::from(cell.y) * 10.0;
                let center = [x + 5.0, y + 5.0];
                if ground_height(center) + 10.0 > bridge.geometry.height(center)
                    && !matches!(self.cells[index].kind, CellType::Obstacle)
                {
                    self.cells[index].kind = CellType::BridgeImpassable;
                }
            }
        }
        self.bridges.push(bridge);
        Ok(layer)
    }

    pub fn bridge_layers(&self) -> impl Iterator<Item = u8> + '_ {
        (2_u8..=15).take(self.bridges.len())
    }

    pub(crate) fn destroyed_bridge_connections(
        &self,
    ) -> impl Iterator<Item = (u8, impl Iterator<Item = Cell> + '_)> + '_ {
        self.bridge_layers()
            .zip(&self.bridges)
            .filter(|(_, bridge)| bridge.destroyed)
            .map(|(layer, bridge)| {
                let ground = bridge
                    .cells
                    .iter()
                    .filter_map(|(&cell, info)| info.connect_ground.then_some(cell));
                (layer, ground)
            })
    }

    pub(crate) fn path_layers(&self) -> impl Iterator<Item = u8> + '_ {
        self.bridge_layers()
            .chain((self.bridges.len() < 14 && self.walls.bounds.is_some()).then_some(15))
    }

    #[must_use]
    pub fn bridge_cell(&self, layer: u8, cell: Cell) -> Option<&BridgeCell> {
        if layer == 15 {
            return self.walls.cells.get(&cell);
        }
        self.bridges
            .get(usize::from(layer.checked_sub(2)?))?
            .cells
            .get(&cell)
    }

    #[must_use]
    pub fn ground_bridge(&self, cell: Cell) -> Option<u8> {
        self.bridge_connections[self.index(cell)?]
    }

    /// `TerrainLogic::getLayerForDestination` compares vertical distance,
    /// including destroyed bridge geometry; newest bridge wins equal heights.
    #[must_use]
    pub fn get_layer_for_destination(&self, position: [f32; 3], ground_height: f32) -> u8 {
        let mut layer = 1;
        let mut distance = (position[2] - ground_height).abs();
        if distance > self.walls.height / 2.0 && self.is_point_on_wall([position[0], position[1]]) {
            let delta = (position[2] - self.walls.height).abs();
            if delta < distance {
                layer = 15;
                distance = delta;
            }
        }
        for (candidate, bridge) in (2_u8..=15).zip(&self.bridges).rev() {
            let point = [position[0], position[1]];
            if bridge.geometry.contains(point) {
                let delta = (position[2] - bridge.geometry.height(point)).abs();
                if delta < distance {
                    layer = candidate;
                    distance = delta;
                }
            }
        }
        layer
    }

    /// Highest bridge no higher than the destination, with optional health filtering.
    #[must_use]
    pub fn get_highest_layer_for_destination(
        &self,
        position: [f32; 3],
        ground_height: f32,
        only_healthy: bool,
    ) -> u8 {
        let mut layer = 1;
        let mut distance = position[2] - ground_height;
        if distance > self.walls.height / 2.0 && self.is_point_on_wall([position[0], position[1]]) {
            let delta = position[2] - self.walls.height;
            if delta >= 0.0 && delta.abs() < distance.abs() {
                layer = 15;
                distance = delta;
            }
        }
        for (candidate, bridge) in (2_u8..=15).zip(&self.bridges).rev() {
            if only_healthy && bridge.destroyed {
                continue;
            }
            let point = [position[0], position[1]];
            if bridge.geometry.contains(point) {
                let delta = position[2] - bridge.geometry.height(point);
                if delta >= 0.0 && delta.abs() < distance.abs() {
                    layer = candidate;
                    distance = delta;
                }
            }
        }
        layer
    }

    pub(crate) fn bridge_minimum_height(&self, layer: u8) -> Option<f32> {
        if layer == 15 {
            return Some(self.walls.height);
        }
        let bridge = self.bridges.get(usize::from(layer.checked_sub(2)?))?;
        Some(
            bridge
                .geometry
                .corners
                .iter()
                .map(|point| point[2])
                .fold(f32::INFINITY, f32::min),
        )
    }

    /// `W3DTerrainLogic` layer height uses the selected bridge plane even when
    /// destroyed, and never lowers an object beneath the ground terrain.
    #[must_use]
    pub fn destination_layer_height(&self, layer: u8, position: [f32; 2], ground: f32) -> f32 {
        if layer == 15 {
            return self.walls.height;
        }
        layer
            .checked_sub(2)
            .and_then(|index| self.bridges.get(usize::from(index)))
            .map_or(ground, |bridge| {
                bridge.geometry.height(position).max(ground)
            })
    }

    /// W3D layer normals retain the ground normal on walls, and the bridge
    /// normal even when its plane lies beneath the returned ground height.
    pub(crate) fn layer_height_and_normal(
        &self,
        layer: u8,
        position: [f32; 2],
        ground: f32,
        ground_normal: [f32; 3],
    ) -> (f32, [f32; 3]) {
        if layer == 15 {
            return (self.walls.height, ground_normal);
        }
        layer
            .checked_sub(2)
            .and_then(|index| self.bridges.get(usize::from(index)))
            .map_or((ground, ground_normal), |bridge| {
                (
                    bridge.geometry.height(position).max(ground),
                    bridge.geometry.normal(),
                )
            })
    }

    #[must_use]
    pub fn bridge_height(&self, layer: u8, position: [f32; 2]) -> Option<f32> {
        if layer == 15 {
            return self.is_point_on_wall(position).then_some(self.walls.height);
        }
        let bridge = self.bridges.get(usize::from(layer.checked_sub(2)?))?;
        (!bridge.destroyed).then(|| bridge.geometry.height(position))
    }

    #[must_use]
    pub fn bridge_contains(&self, layer: u8, position: [f32; 2]) -> bool {
        layer
            .checked_sub(2)
            .and_then(|index| self.bridges.get(usize::from(index)))
            .is_some_and(|bridge| bridge.geometry.contains(position))
    }

    /// `TerrainLogic::objectInteractsWithBridgeLayer` includes an object's end
    /// apron overlap and the native ten-unit vertical proximity check.
    pub(crate) fn bridge_interacts(
        &self,
        layer: u8,
        position: [f32; 3],
        minor_radius: f32,
        consider_health: bool,
    ) -> bool {
        if layer == 15 {
            return self.is_point_on_wall([position[0], position[1]]);
        }
        let Some(bridge) = layer
            .checked_sub(2)
            .and_then(|index| self.bridges.get(usize::from(index)))
        else {
            return false;
        };
        if consider_health && bridge.destroyed {
            return false;
        }
        let point = [position[0], position[1]];
        let radius = minor_radius + 5.0;
        let bounds = [
            [point[0] - radius, point[1] - radius],
            [point[0] + radius, point[1] + radius],
        ];
        (bridge.geometry.contains(point) || bridge.geometry.end_intersects(bounds, false))
            && (position[2] - bridge.geometry.height(point)).abs() <= 10.0
    }

    pub(crate) fn bridge_end_interacts(
        &self,
        layer: u8,
        position: [f32; 3],
        minor_radius: f32,
    ) -> bool {
        let Some(bridge) = layer
            .checked_sub(2)
            .and_then(|index| self.bridges.get(usize::from(index)))
        else {
            return false;
        };
        let point = [position[0], position[1]];
        let radius = minor_radius + 5.0;
        let bounds = [
            [point[0] - radius, point[1] - radius],
            [point[0] + radius, point[1] + radius],
        ];
        bridge.geometry.end_intersects(bounds, false)
            && (position[2] - bridge.geometry.height(point)).abs() <= 10.0
    }

    pub(crate) fn reset_bridges(&mut self) {
        self.walls = walls::Layer::new();
        for index in 0..self.bridges.len() {
            self.set_bridge_destroyed(
                u8::try_from(index + 2).expect("bounded bridge layers"),
                false,
            )
            .expect("existing bridge layer");
        }
    }

    pub(crate) fn bridge_bounds(&self, layer: u8) -> Option<[Cell; 2]> {
        if layer == 15 {
            return self.walls.bounds;
        }
        let bridge = self.bridges.get(usize::from(layer.checked_sub(2)?))?;
        let minimum = Cell {
            x: bridge.cells.keys().map(|cell| cell.x).min()?,
            y: bridge.cells.keys().map(|cell| cell.y).min()?,
        };
        let maximum = Cell {
            x: bridge.cells.keys().map(|cell| cell.x).max()? + 1,
            y: bridge.cells.keys().map(|cell| cell.y).max()? + 1,
        };
        Some([minimum, maximum])
    }

    #[must_use]
    pub fn bridge_geometry(&self, layer: u8) -> Option<&BridgeGeometry> {
        Some(
            &self
                .bridges
                .get(usize::from(layer.checked_sub(2)?))?
                .geometry,
        )
    }

    /// Reclassifies a changed bridge and disconnects destroyed ground entries.
    /// # Errors
    /// Rejects layers without a bridge.
    pub fn set_bridge_destroyed(
        &mut self,
        layer: u8,
        destroyed: bool,
    ) -> Result<bool, &'static str> {
        let index = usize::from(layer.checked_sub(2).ok_or("Invalid bridge layer")?);
        let bridge = self.bridges.get_mut(index).ok_or("Invalid bridge layer")?;
        if !bridge.set_destroyed(destroyed) {
            return Ok(false);
        }
        for (cell, data) in &bridge.cells {
            if data.connect_ground {
                let cell_index =
                    usize::from(cell.y) * usize::from(self.width) + usize::from(cell.x);
                self.bridge_connections[cell_index] = (!destroyed).then_some(layer);
            }
        }
        Ok(true)
    }

    /// Unexpanded cliff flags and exact polygon water for physics, independent
    /// of routing obstacles, rubble and pathfinding corner-water classification.
    /// `Object`'s `OFF_MAP` flag uses strict `Region3D` interior boundaries.
    pub(crate) fn physics_surfaces(&self, position: [f32; 2], ground_height: f32) -> Option<u8> {
        if position[0] <= 0.0 || position[1] <= 0.0 {
            return None;
        }
        let x = coordinate(position[0] / 10.0, self.width)?;
        let y = coordinate(position[1] / 10.0, self.height)?;
        let mut surfaces =
            self.source_cliffs[usize::from(y) * usize::from(self.width) + usize::from(x)];
        let water_height = water_height(&self.water_polygons, position)
            .expect("validated water geometry and bounded map coordinate");
        if water_height.is_some_and(|height| ground_height < height) {
            surfaces |= 2;
        }
        Some(surfaces)
    }

    /// Retain original water polygons once when the map is admitted.
    pub fn set_water_polygons(&mut self, polygons: Vec<WaterPolygon>) {
        self.water_polygons = polygons;
    }

    pub(crate) fn locomotor_surfaces(&self, position: [f32; 2]) -> u8 {
        let cell = coordinate(position[0] / 10.0, self.width)
            .zip(coordinate(position[1] / 10.0, self.height))
            .map(|(x, y)| Cell { x, y });
        let Some(cell) = cell else {
            return 1 | 8;
        };
        if self.rubble.contains(&cell) {
            return 16 | 8;
        }
        match self.cell(cell).map(|cell| &cell.kind) {
            Some(CellType::Clear) | None => 1 | 8,
            Some(CellType::Water) => 2 | 8,
            Some(CellType::Cliff) => 4 | 8,
            Some(CellType::Rubble) => 16 | 8,
            Some(CellType::Obstacle | CellType::BridgeImpassable | CellType::Impassable) => 8,
        }
    }

    pub(crate) fn cliff_cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.height).flat_map(move |y| {
            (0..self.width).filter_map(move |x| {
                let cell = Cell { x, y };
                matches!(
                    self.cells[usize::from(y) * usize::from(self.width) + usize::from(x)].kind,
                    CellType::Cliff
                )
                .then_some(cell)
            })
        })
    }

    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    /// Classify the original ground map, then expand cliffs and mark their
    /// outer pinched band. Water overrides a stored cliff flag.
    ///
    /// # Errors
    /// Rejects empty/oversized dimensions and cells outside the map.
    pub fn classify(
        width: u16,
        height: u16,
        cliffs: impl IntoIterator<Item = Cell>,
        water: impl IntoIterator<Item = Cell>,
    ) -> Result<Self, GridError> {
        let count = usize::from(width) * usize::from(height);
        if width == 0 || height == 0 || count > 1_048_576 {
            return Err(GridError::InvalidGrid);
        }
        let mut terrain = Self {
            rubble: std::collections::BTreeSet::new(),
            path_changes: std::collections::BTreeMap::new(),
            bridges: Vec::new(),
            walls: walls::Layer::new(),
            bridge_connections: vec![None; count],
            build_restrictions: vec![false; count],
            width,
            height,
            source_cliffs: vec![0; count],
            water_polygons: Vec::new(),
            cells: (0..count)
                .map(|_| TerrainCell {
                    kind: CellType::Clear,
                    pinched: false,
                    height: 0.0,
                })
                .collect(),
        };
        for cell in cliffs {
            let index = terrain.index(cell).ok_or(GridError::OutsideBattlefield)?;
            terrain.cells[index].kind = CellType::Cliff;
            terrain.source_cliffs[index] |= 4;
        }
        for cell in water {
            let index = terrain.index(cell).ok_or(GridError::OutsideBattlefield)?;
            terrain.cells[index].kind = CellType::Water;
        }
        terrain.mark_cliff_border();
        for cell in &mut terrain.cells {
            if cell.pinched && matches!(cell.kind, CellType::Clear) {
                cell.kind = CellType::Cliff;
            }
        }
        terrain.mark_cliff_border();
        Ok(terrain)
    }

    #[must_use]
    pub fn cell(&self, cell: Cell) -> Option<&TerrainCell> {
        self.index(cell).map(|index| &self.cells[index])
    }

    pub(crate) fn accepts_obstacle(&self, cell: Cell) -> bool {
        !self.rubble.contains(&cell)
            && self
                .cell(cell)
                .is_some_and(|cell| matches!(cell.kind, CellType::Clear | CellType::Impassable))
    }

    /// Native terrain validity intentionally permits obstacles and generic
    /// impassable cells; collision/path search handles those separately.
    #[must_use]
    pub fn valid_movement(&self, position: [f32; 2], legal_surfaces: u8) -> bool {
        self.valid_movement_on(1, position, legal_surfaces)
    }

    /// `Pathfinder::getCell` resolves an existing upper cell before ground;
    /// absent upper cells fall back to ground, including ground rubble.
    pub(crate) fn valid_movement_on(
        &self,
        layer: u8,
        position: [f32; 2],
        legal_surfaces: u8,
    ) -> bool {
        let Some(x) = coordinate(position[0] / 10.0, self.width) else {
            return false;
        };
        let Some(y) = coordinate(position[1] / 10.0, self.height) else {
            return false;
        };
        let cell = Cell { x, y };
        self.bridge_cell(layer, cell)
            .filter(|upper| !matches!(upper.kind, CellType::Impassable))
            .map_or_else(
                || {
                    if self.rubble.contains(&cell) {
                        CellType::Rubble.valid_movement(legal_surfaces)
                    } else {
                        self.cell(cell)
                            .is_some_and(|cell| cell.kind.valid_movement(legal_surfaces))
                    }
                },
                |upper| upper.kind.valid_movement(legal_surfaces),
            )
    }

    /// Static ground routing rejects terrain without the GROUND surface. The
    /// pinched band is separate metadata, not another terrain class.
    pub fn blocked_ground_cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.height).flat_map(move |y| {
            (0..self.width).filter_map(move |x| {
                let cell = Cell { x, y };
                (!matches!(
                    self.cells[usize::from(y) * usize::from(self.width) + usize::from(x)].kind,
                    CellType::Clear
                ))
                .then_some(cell)
            })
        })
    }

    pub(crate) fn rubble_cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.rubble.iter().copied()
    }

    pub(crate) fn reset_path_classification(&mut self) {
        self.rubble.clear();
        for (index, (kind, pinched)) in std::mem::take(&mut self.path_changes) {
            self.cells[index].kind = kind;
            self.cells[index].pinched = pinched;
        }
    }

    pub(crate) fn add_path_rubble(&mut self, cell: Cell) {
        self.rubble.insert(cell);
    }

    pub(crate) fn remove_path_rubble(&mut self, cell: Cell) -> bool {
        let removed = self.rubble.remove(&cell);
        if removed {
            self.clear_path_obstacle(cell);
        }
        removed
    }

    pub(crate) fn clear_path_obstacle(&mut self, cell: Cell) {
        if let Some(index) = self.index(cell)
            && !matches!(self.cells[index].kind, CellType::Clear)
        {
            self.path_changes
                .entry(index)
                .or_insert((self.cells[index].kind, self.cells[index].pinched));
            self.cells[index].kind = CellType::Clear;
        }
    }

    pub(crate) fn set_path_pinched(&mut self, cell: Cell, pinched: bool) {
        if let Some(index) = self.index(cell)
            && self.cells[index].pinched != pinched
        {
            self.path_changes
                .entry(index)
                .or_insert((self.cells[index].kind, self.cells[index].pinched));
            self.cells[index].pinched = pinched;
        }
    }

    pub(crate) fn path_kind(&self, cell: Cell) -> Option<CellType> {
        let stored = self.cell(cell)?;
        Some(if self.rubble.contains(&cell) {
            CellType::Rubble
        } else {
            stored.kind
        })
    }

    #[cfg(test)]
    pub(crate) fn set_rubble(&mut self, cells: &std::collections::BTreeSet<Cell>) {
        self.rubble.clone_from(cells);
    }

    fn index(&self, cell: Cell) -> Option<usize> {
        (cell.x < self.width && cell.y < self.height)
            .then_some(usize::from(cell.y) * usize::from(self.width) + usize::from(cell.x))
    }

    fn mark_cliff_border(&mut self) {
        // All marks are collected before changing cell types, so this expands
        // exactly one cell rather than flooding in traversal order.
        for y in 0..self.height {
            for x in 0..self.width {
                let index = usize::from(y) * usize::from(self.width) + usize::from(x);
                if !matches!(self.cells[index].kind, CellType::Cliff) {
                    continue;
                }
                for row in y.saturating_sub(1)..=y.saturating_add(1).min(self.height - 1) {
                    for column in x.saturating_sub(1)..=x.saturating_add(1).min(self.width - 1) {
                        let index =
                            usize::from(row) * usize::from(self.width) + usize::from(column);
                        if matches!(self.cells[index].kind, CellType::Clear) {
                            self.cells[index].pinched = true;
                        }
                    }
                }
            }
        }
    }
}

impl CellType {
    const fn valid_movement(self, legal_surfaces: u8) -> bool {
        // LocomotorSet.h: GROUND=1, WATER=2, CLIFF=4, AIR=8, RUBBLE=16.
        let acceptable = match self {
            Self::Clear => 1 | 8,
            Self::Water => 2 | 8,
            Self::Cliff => 4 | 8,
            Self::Rubble => 16 | 8,
            Self::BridgeImpassable => 8,
            Self::Obstacle | Self::Impassable => return true,
        };
        legal_surfaces & acceptable != 0
    }
}

fn coordinate(value: f32, limit: u16) -> Option<u16> {
    if !value.is_finite() || value < 0.0 || value >= f32::from(limit) {
        return None;
    }
    let (mut low, mut high) = (0, limit);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if f32::from(middle) <= value {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(low)
}

impl TerrainMap {
    pub(crate) fn parachute_water_height(&self, position: [f32; 2], ground: f32) -> Option<f32> {
        water_height(&self.water_polygons, position)
            .expect("validated native water polygons and bounded coordinates")
            .filter(|height| ground < *height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge_geometry() -> BridgeGeometry {
        BridgeGeometry {
            from: [100.0, 200.0, 20.0],
            to: [300.0, 200.0, 40.0],
            corners: [
                [100.0, 220.0, 20.0],
                [100.0, 180.0, 20.0],
                [300.0, 220.0, 40.0],
                [300.0, 180.0, 40.0],
            ],
        }
    }

    #[test]
    fn bridge_deck_layer_preserves_end_connectors_side_strips_and_sloping_height() {
        let mut terrain = TerrainMap::classify(50, 50, [], []).unwrap();
        let layer = terrain.add_bridge(bridge_geometry(), |_| 0.0).unwrap();
        assert_eq!(layer, 2);
        let entry = Cell { x: 9, y: 19 };
        assert_eq!(terrain.ground_bridge(entry), Some(layer));
        assert!(terrain.bridge_cell(layer, entry).unwrap().connect_ground);
        assert!(matches!(
            terrain.bridge_cell(layer, entry).unwrap().kind,
            CellType::Clear
        ));
        let deck = Cell { x: 15, y: 19 };
        assert!(matches!(
            terrain.bridge_cell(layer, deck).unwrap().kind,
            CellType::Clear
        ));
        assert!(!terrain.bridge_cell(layer, deck).unwrap().connect_ground);
        assert!(matches!(
            terrain
                .bridge_cell(layer, Cell { x: 15, y: 22 })
                .unwrap()
                .kind,
            CellType::BridgeImpassable
        ));
        assert_eq!(
            terrain
                .bridge_height(layer, [200.0, 200.0])
                .unwrap()
                .to_bits(),
            // Native PlaneClass intersection rounds after dividing by the
            // 1000-unit ray height, then multiplies back; it is one ULP above 30.
            0x41f0_0001
        );
        assert!(terrain.bridge_cell(layer, Cell { x: 0, y: 0 }).is_none());
        assert!(terrain.bridge_contains(layer, [100.0, 180.0]));
        assert!(!terrain.bridge_contains(layer, [200.0, 179.9]));
        assert!(terrain.set_bridge_destroyed(layer, true).unwrap());
        assert!(terrain.ground_bridge(entry).is_none());
        assert!(matches!(
            terrain.bridge_cell(layer, deck).unwrap().kind,
            CellType::BridgeImpassable
        ));
        assert!(terrain.bridge_height(layer, [200.0, 200.0]).is_none());
        assert!(terrain.bridge_contains(layer, [200.0, 200.0]));
        assert!(!terrain.set_bridge_destroyed(layer, true).unwrap());
        assert!(terrain.set_bridge_destroyed(layer, false).unwrap());
        assert_eq!(terrain.ground_bridge(entry), Some(layer));
        assert!(matches!(
            terrain.bridge_cell(layer, deck).unwrap().kind,
            CellType::Clear
        ));
    }

    #[test]
    fn fourteenth_bridge_keeps_wall_slot_metadata_without_allocating_cells_or_lower_clearance() {
        let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
        for expected in 2..=14 {
            assert_eq!(
                terrain.add_bridge(bridge_geometry(), |_| 0.0).unwrap(),
                expected
            );
        }
        let geometry = BridgeGeometry {
            from: [400.0, 205.0, 0.25],
            to: [510.0, 205.0, 0.25],
            corners: [
                [400.0, 230.0, 0.25],
                [400.0, 180.0, 0.25],
                [510.0, 230.0, 0.25],
                [510.0, 180.0, 0.25],
            ],
        };
        assert_eq!(terrain.add_bridge(geometry, |_| 0.0).unwrap(), 15);
        let center = Cell { x: 45, y: 20 };
        assert!(terrain.bridge_geometry(15).is_some());
        assert!(terrain.bridge_contains(15, [455.0, 205.0]));
        // W3DTerrainLogic reserves layer 15 for walls, even when the fourteenth
        // bridge owns that numeric layer. With no wall cells it clips to ground.
        assert!(terrain.bridge_height(15, [455.0, 205.0]).is_none());
        assert!(terrain.bridge_cell(15, center).is_none());
        assert!(terrain.bridge_endpoint_cells(15).is_none());
        assert!(terrain.ground_bridge(center).is_none());
        assert!(matches!(
            terrain.cell(center).unwrap().kind,
            CellType::Clear
        ));
        let grid = crate::Grid::new(80, 60, terrain.blocked_ground_cells())
            .unwrap()
            .with_bridge_layers(&terrain);
        assert_eq!(
            grid.resolve_layer(crate::grid::LayerCell {
                cell: center,
                layer: 15
            })
            .layer,
            1
        );
        assert!(terrain.bridge_cell(14, Cell { x: 20, y: 20 }).is_some());
        assert!(terrain.add_bridge(bridge_geometry(), |_| 0.0).is_err());
    }

    #[test]
    fn bridge_ground_clearance_uses_strict_ten_unit_source_threshold() {
        let deck = Cell { x: 15, y: 19 };
        let mut boundary = TerrainMap::classify(50, 50, [], []).unwrap();
        boundary
            .add_bridge(bridge_geometry(), |point| {
                let rise = (point[0] - 100.0) * 0.1;
                10.0 + rise
            })
            .unwrap();
        assert!(matches!(boundary.cell(deck).unwrap().kind, CellType::Clear));
        let mut below = TerrainMap::classify(50, 50, [], []).unwrap();
        below.add_bridge(bridge_geometry(), |_| 30.0).unwrap();
        assert!(matches!(
            below.cell(deck).unwrap().kind,
            CellType::BridgeImpassable
        ));
        let entry = Cell { x: 9, y: 19 };
        assert!(matches!(below.cell(entry).unwrap().kind, CellType::Clear));
    }

    #[test]
    fn physics_keeps_original_cliff_and_water_flags_separate_from_routing() {
        let wet_cliff = Cell { x: 3, y: 3 };
        let dry_cliff = Cell { x: 5, y: 5 };
        let mut terrain = TerrainMap::classify(8, 8, [wet_cliff, dry_cliff], [wet_cliff]).unwrap();
        terrain.set_water_polygons(vec![
            WaterPolygon::new(vec![[30, 30, 10], [40, 30, 10], [40, 40, 10], [30, 40, 10]])
                .unwrap(),
        ]);
        assert_eq!(terrain.physics_surfaces([35.0, 35.0], 0.0), Some(2 | 4));
        assert!(matches!(
            terrain.cell(wet_cliff).unwrap().kind,
            CellType::Water
        ));
        assert_eq!(terrain.physics_surfaces([55.0, 55.0], 0.0), Some(4));
        assert!(matches!(
            terrain.cell(Cell { x: 4, y: 4 }).unwrap().kind,
            CellType::Cliff
        ));
        assert_eq!(terrain.physics_surfaces([45.0, 45.0], 0.0), Some(0));
        let rubble = Cell { x: 1, y: 1 };
        terrain.set_rubble(&std::collections::BTreeSet::from([rubble]));
        assert!(!terrain.valid_movement([15.0, 15.0], 1));
        assert_eq!(terrain.physics_surfaces([15.0, 15.0], 0.0), Some(0));
        let obstacle = terrain.index(Cell { x: 1, y: 2 }).unwrap();
        terrain.cells[obstacle].kind = CellType::Obstacle;
        assert_eq!(terrain.physics_surfaces([15.0, 25.0], 0.0), Some(0));
    }

    #[test]
    fn physics_water_uses_rounded_point_boundaries_and_strict_height() {
        let mut terrain = TerrainMap::classify(4, 4, [], [Cell { x: 1, y: 1 }]).unwrap();
        assert_eq!(terrain.physics_surfaces([15.0, 15.0], 0.0), Some(0));
        terrain.set_water_polygons(vec![
            WaterPolygon::new(vec![[10, 10, 10], [20, 10, 10], [20, 20, 10], [10, 20, 10]])
                .unwrap(),
        ]);
        assert_eq!(terrain.physics_surfaces([15.0, 15.0], 9.99), Some(2));
        assert_eq!(terrain.physics_surfaces([15.0, 15.0], 10.0), Some(0));
        assert_eq!(terrain.physics_surfaces([15.0, 15.0], 10.01), Some(0));
        assert_eq!(terrain.physics_surfaces([15.0, 10.49], 0.0), Some(0));
        assert_eq!(terrain.physics_surfaces([15.0, 10.5], 0.0), Some(2));
        assert_eq!(terrain.physics_surfaces([10.0, 15.0], 0.0), Some(0));
        assert_eq!(terrain.physics_surfaces([20.0, 15.0], 0.0), Some(2));
        assert_eq!(terrain.physics_surfaces([15.0, 20.49], 0.0), Some(2));
        assert_eq!(terrain.physics_surfaces([15.0, 20.5], 0.0), Some(0));
        terrain.water_polygons.push(
            WaterPolygon::new(vec![[10, 10, 20], [20, 10, 20], [20, 20, 20], [10, 20, 20]])
                .unwrap(),
        );
        assert_eq!(terrain.physics_surfaces([15.0, 15.0], 15.0), Some(2));
        assert_eq!(terrain.physics_surfaces([15.0, 15.0], 20.0), Some(0));
        terrain.set_water_polygons(vec![
            WaterPolygon::new(vec![[10, 10, -5], [20, 10, -5], [20, 20, -5], [10, 20, -5]])
                .unwrap(),
        ]);
        assert_eq!(terrain.physics_surfaces([15.0, 15.0], -10.0), Some(0));
    }

    #[test]
    fn physics_water_rejects_original_signed_intersection_overflow() {
        assert!(
            WaterPolygon::new(vec![[0, 0, 0], [100_000, 100_000, 0], [0, 100_000, 0],]).is_err()
        );
    }

    #[test]
    fn physics_off_map_uses_strict_original_extent_boundaries() {
        let terrain = TerrainMap::classify(2, 3, [], []).unwrap();
        assert_eq!(terrain.physics_surfaces([0.001, 0.001], 0.0), Some(0));
        assert_eq!(terrain.physics_surfaces([19.999, 29.999], 0.0), Some(0));
        for position in [
            [0.0, 5.0],
            [5.0, 0.0],
            [-0.001, 5.0],
            [5.0, -0.001],
            [20.0, 5.0],
            [5.0, 30.0],
            [f32::NAN, 5.0],
            [5.0, f32::INFINITY],
        ] {
            assert_eq!(terrain.physics_surfaces(position, 0.0), None);
        }
    }

    #[test]
    fn water_overrides_cliffs_before_exactly_one_cell_expands() {
        let water = Cell { x: 3, y: 2 };
        let terrain = TerrainMap::classify(7, 7, [Cell { x: 3, y: 3 }, water], [water]).unwrap();
        assert!(matches!(terrain.cell(water).unwrap().kind, CellType::Water));
        assert!(matches!(
            terrain.cell(Cell { x: 2, y: 2 }).unwrap().kind,
            CellType::Cliff
        ));
        let border = terrain.cell(Cell { x: 1, y: 1 }).unwrap();
        assert!(matches!(border.kind, CellType::Clear));
        assert!(border.pinched);
        assert!(!terrain.cell(Cell { x: 0, y: 0 }).unwrap().pinched);
        assert!(!terrain.valid_movement([30.0, 20.0], 1));
        assert!(terrain.valid_movement([30.0, 20.0], 2));
    }

    #[test]
    fn terrain_validity_keeps_obstacles_and_surface_classes_distinct() {
        for kind in [CellType::Obstacle, CellType::Impassable] {
            assert!(kind.valid_movement(0));
        }
        assert!(!CellType::BridgeImpassable.valid_movement(1));
        assert!(CellType::BridgeImpassable.valid_movement(8));
        assert!(!CellType::Rubble.valid_movement(1));
        assert!(CellType::Rubble.valid_movement(1 | 16));
        assert!(!CellType::Cliff.valid_movement(1 | 16));
        let terrain = TerrainMap::classify(2, 2, [], []).unwrap();
        assert!(terrain.valid_movement([19.999, 0.0], 1));
        assert!(!terrain.valid_movement([20.0, 0.0], 8));
        assert!(!terrain.valid_movement([-0.001, 0.0], 8));
        assert!(!terrain.valid_movement([f32::NAN, 0.0], 8));
    }
    #[test]
    fn locomotor_terrain_validity_uses_upper_cells_and_ground_fallback() {
        let mut terrain = TerrainMap::classify(50, 50, [], []).unwrap();
        let layer = terrain.add_bridge(bridge_geometry(), |_| 40.0).unwrap();
        let position = [155.0, 205.0];
        assert!(matches!(
            terrain.cell(Cell { x: 15, y: 20 }).unwrap().kind,
            CellType::BridgeImpassable
        ));
        assert!(matches!(
            terrain
                .bridge_cell(layer, Cell { x: 15, y: 20 })
                .unwrap()
                .kind,
            CellType::Clear
        ));
        assert!(!terrain.valid_movement(position, 1));
        assert!(terrain.valid_movement_on(layer, position, 1));
        assert!(terrain.valid_movement_on(layer, position, 8));
        assert!(!terrain.valid_movement_on(layer, position, 2));
        assert!(!terrain.valid_movement_on(layer, position, 0));
        // Allocated upper padding with CELL_IMPASSABLE resolves to ground.
        let padding = Cell { x: 8, y: 16 };
        assert!(matches!(
            terrain.bridge_cell(layer, padding).unwrap().kind,
            CellType::Impassable
        ));
        terrain.add_path_rubble(padding);
        assert!(!terrain.valid_movement_on(layer, [85.0, 165.0], 1));
        assert!(terrain.valid_movement_on(layer, [85.0, 165.0], 16));
        terrain.add_path_rubble(Cell { x: 40, y: 40 });
        assert!(terrain.bridge_cell(layer, Cell { x: 40, y: 40 }).is_none());
        assert!(!terrain.valid_movement_on(layer, [405.0, 405.0], 1));
        assert!(terrain.valid_movement_on(layer, [405.0, 405.0], 16));
        assert!(!terrain.valid_movement_on(layer, [-0.1, 205.0], 8));
        assert!(!terrain.valid_movement_on(layer, [500.0, 205.0], 8));
        terrain.set_bridge_destroyed(layer, true).unwrap();
        assert!(!terrain.valid_movement_on(layer, position, 1));
        assert!(terrain.valid_movement_on(layer, position, 8));
    }
}
