//! Ground-cell claims from `AIPathfind.cpp` `updateGoal`, `updatePos`,
//! `removeGoal`, `setGoalUnit` and `setPosUnit` at the pinned source revision.
//! One position/goal owner per cell is intentional; fixed occupants retain their
//! position claim when another object crosses their cell.

mod info;
mod snap;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::movement::grid_coordinate;
use super::{CELL_SIZE, Cell, GroupKind, MovementSurface, Order, Skirmish, startup};
use crate::grid::{CellInfo, LayerCell, RouteUnits};

pub(super) struct Occupancy {
    infos: Arc<BTreeMap<LayerCell, CellInfo>>,
    goals: BTreeMap<LayerCell, usize>,
    positions: BTreeMap<LayerCell, usize>,
    aircraft_goals: BTreeMap<Cell, usize>,
    goal_cells: Vec<Option<LayerCell>>,
    position_cells: Vec<Option<LayerCell>>,
}

impl Occupancy {
    pub(super) fn new() -> Self {
        Self {
            infos: Arc::new(BTreeMap::new()),
            goals: BTreeMap::new(),
            positions: BTreeMap::new(),
            aircraft_goals: BTreeMap::new(),
            goal_cells: Vec::new(),
            position_cells: Vec::new(),
        }
    }

    pub(super) fn goal_cell(&self, id: usize) -> Option<Cell> {
        self.goal_cells
            .get(id)
            .copied()
            .flatten()
            .map(|node| node.cell)
    }

    pub(super) fn position_owner_on(&self, node: LayerCell) -> Option<usize> {
        self.positions.get(&node).copied()
    }

    pub(super) fn landing_claimed(&self, cell: Cell) -> bool {
        self.goals.contains_key(&LayerCell { cell, layer: 1 })
    }

    pub(super) fn clear(&mut self) {
        Arc::make_mut(&mut self.infos).clear();
        self.goals.clear();
        self.positions.clear();
        self.aircraft_goals.clear();
        self.goal_cells.clear();
        self.position_cells.clear();
    }

    fn claim_info(&mut self, node: LayerCell) -> bool {
        if self.infos.contains_key(&node) {
            return true;
        }
        if self.infos.len() == crate::grid::CELL_INFO_CAPACITY {
            eprintln!("Original pathfinder INFO pool exhausted: 30000 retained cells");
            return false;
        }
        Arc::make_mut(&mut self.infos).insert(node, CellInfo::new(node));
        true
    }

    fn release_info(&mut self, node: LayerCell, obstacle: bool) {
        if !obstacle
            && !self.goals.contains_key(&node)
            && !self.positions.contains_key(&node)
            && (node.layer != 1 || !self.aircraft_goals.contains_key(&node.cell))
        {
            Arc::make_mut(&mut self.infos).remove(&node);
        }
    }
}

impl Skirmish {
    pub(super) fn refresh_occupancy(&mut self) {
        self.occupancy.goal_cells.resize(self.entities.len(), None);
        self.occupancy
            .position_cells
            .resize(self.entities.len(), None);
        for id in 0..self.entities.len() {
            self.update_position_claim(id);
            if self.entities[id].alive()
                && matches!(self.entities[id].ai.order, Order::Idle)
                && self.entities[id].ai.blocking.temporary_goal().is_none()
            {
                self.reserve_goal(id, self.entities[id].position);
            }
        }
    }

    fn ground_mobile(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        actor.alive()
            && !actor.containment.nested_removed
            && !actor.disabled_held()
            && !actor.masked()
            && unit.speed > 0.0
            && (matches!(unit.movement_surface, MovementSurface::Ground) || actor.landed())
    }

    pub(super) fn movement_cell(&self, id: usize, point: [f32; 2]) -> Option<Cell> {
        let unit = &self.catalog[self.entities[id].type_index];
        let (_, centered) = startup::radius_and_center(&unit.footprint(point, 0.0));
        let offset = if centered { 0.0 } else { 0.5 };
        Some(Cell {
            x: grid_coordinate(offset + point[0] / CELL_SIZE, self.grid.width())?,
            y: grid_coordinate(offset + point[1] / CELL_SIZE, self.grid.height())?,
        })
    }

    pub(super) fn body_cells(&self, id: usize, center: Cell) -> Vec<Cell> {
        self.body_cells_for(id, center, false)
    }

    /// Aircraft branch of `Pathfinder::checkDestination`; every footprint cell
    /// must exist, but only another aircraft's destination prevents placement.
    pub(in crate::skirmish) fn aircraft_destination_unclaimed(
        &self,
        id: usize,
        center: Cell,
    ) -> bool {
        let unit = &self.catalog[self.entities[id].type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint([0.0, 0.0], 0.0));
        let above = radius + i32::from(centered);
        for x in i32::from(center.x) - radius..i32::from(center.x) + above {
            for y in i32::from(center.y) - radius..i32::from(center.y) + above {
                let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                    return false;
                };
                let cell = Cell { x, y };
                if !self.grid.contains(cell)
                    || self
                        .occupancy
                        .aircraft_goals
                        .get(&cell)
                        .is_some_and(|owner| *owner != id)
                {
                    return false;
                }
            }
        }
        true
    }

    fn body_cells_for(&self, id: usize, center: Cell, remove_goal: bool) -> Vec<Cell> {
        let unit = &self.catalog[self.entities[id].type_index];
        let (mut radius, centered) = startup::radius_and_center(&unit.footprint([0.0, 0.0], 0.0));
        if remove_goal && radius == 0 {
            radius = 1;
        }
        let above = radius + i32::from(centered);
        let mut result = Vec::new();
        for x in i32::from(center.x) - radius..i32::from(center.x) + above {
            for y in i32::from(center.y) - radius..i32::from(center.y) + above {
                if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) {
                    let cell = Cell { x, y };
                    if self.grid.contains(cell) {
                        result.push(cell);
                    }
                }
            }
        }
        result
    }

    fn claim_nodes(
        &self,
        id: usize,
        center: LayerCell,
        ground: bool,
        remove_goal: bool,
    ) -> Vec<LayerCell> {
        let mut nodes = Vec::new();
        for cell in self.body_cells_for(id, center.cell, remove_goal) {
            nodes.push(self.navigation.resolve_layer(LayerCell {
                cell,
                layer: center.layer,
            }));
            if ground && center.layer != 1 {
                nodes.push(LayerCell { cell, layer: 1 });
            }
        }
        nodes
    }

    pub(super) fn remove_goal(&mut self, id: usize) {
        let Some(center) = self.occupancy.goal_cells.get_mut(id).and_then(Option::take) else {
            return;
        };
        for node in self.claim_nodes(id, center, true, true) {
            if self.occupancy.goals.get(&node) == Some(&id) {
                self.occupancy.goals.remove(&node);
            }
            if node.layer == 1 && self.occupancy.aircraft_goals.get(&node.cell) == Some(&id) {
                self.occupancy.aircraft_goals.remove(&node.cell);
            }
            self.occupancy
                .release_info(node, self.navigation.layer_obstacle(node));
        }
    }

    pub(in crate::skirmish) fn remove_position(&mut self, id: usize) {
        let Some(center) = self
            .occupancy
            .position_cells
            .get_mut(id)
            .and_then(Option::take)
        else {
            return;
        };
        for node in self.claim_nodes(id, center, true, false) {
            if self.occupancy.positions.get(&node) == Some(&id) {
                self.occupancy.positions.remove(&node);
                self.occupancy
                    .release_info(node, self.navigation.layer_obstacle(node));
            }
        }
    }

    fn aircraft_goal(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        actor.alive()
            && !actor.containment.nested_removed
            && !actor.masked()
            && !matches!(unit.ai.update, super::UnitAiUpdate::Absent)
            && actor.active_locomotor(unit).ground.is_none()
            && actor.active_locomotor(unit).flight.is_some()
    }

    fn reserve_aircraft_goal(&mut self, id: usize, point: [f32; 2]) {
        self.remove_goal(id);
        if !self.aircraft_goal(id) {
            return;
        }
        let Some(cell) = self.movement_cell(id, point) else {
            return;
        };
        self.occupancy.goal_cells.resize(self.entities.len(), None);
        self.occupancy.goal_cells[id] = Some(LayerCell { cell, layer: 1 });
        for cell in self.body_cells(id, cell) {
            if self.occupancy.claim_info(LayerCell { cell, layer: 1 }) {
                self.occupancy.aircraft_goals.insert(cell, id);
            }
        }
    }

    pub(super) fn reserve_goal(&mut self, id: usize, point: [f32; 2]) {
        let layer = self.entities[id]
            .ai
            .path
            .optimized
            .back()
            .map_or(self.entities[id].layer, |node| node.layer);
        self.reserve_goal_on(id, point, layer);
    }

    pub(super) fn reserve_goal_on(&mut self, id: usize, point: [f32; 2], layer: u8) {
        if !self.ground_mobile(id) {
            self.reserve_aircraft_goal(id, point);
            return;
        }
        let Some(cell) = self.movement_cell(id, point) else {
            return;
        };
        let node = LayerCell { cell, layer };
        self.occupancy.goal_cells.resize(self.entities.len(), None);
        if self.occupancy.goal_cells[id] == Some(node) {
            return;
        }
        self.remove_goal(id);
        self.occupancy.goal_cells[id] = Some(node);
        let ground = self.movement_terrain.bridge_end_interacts(
            layer,
            self.object_position(id),
            self.catalog[self.entities[id].type_index].geometry.minor,
        );
        for node in self.claim_nodes(id, node, ground, false) {
            if self.occupancy.claim_info(node) {
                self.occupancy.goals.insert(node, id);
            }
        }
    }

    pub(super) fn update_position_claim(&mut self, id: usize) {
        self.notify_terrain_movement(id);
        self.occupancy
            .position_cells
            .resize(self.entities.len(), None);
        if !self.ground_mobile(id) {
            if !self.entities[id].alive()
                || self.entities[id].masked()
                || self.entities[id].containment.nested_removed
                || self.entities[id].disabled_held()
            {
                self.remove_goal(id);
            }
            self.remove_position(id);
            return;
        }
        let Some(cell) = self.movement_cell(id, self.entities[id].position) else {
            return;
        };
        let node = LayerCell {
            cell,
            layer: self.entities[id].layer,
        };
        if self.occupancy.position_cells[id] == Some(node) {
            return;
        }
        self.remove_position(id);
        self.occupancy.position_cells[id] = Some(node);
        let ground = self.movement_terrain.bridge_end_interacts(
            node.layer,
            self.object_position(id),
            self.catalog[self.entities[id].type_index].geometry.minor,
        );
        for claimed in self.claim_nodes(id, node, ground, false) {
            // PathfindCell::setPosUnit never overwrites an already fixed owner.
            let existing = self.occupancy.positions.get(&claimed);
            if existing.is_some() && existing == self.occupancy.goals.get(&claimed) {
                continue;
            }
            if self.occupancy.claim_info(claimed) {
                self.occupancy.positions.insert(claimed, id);
            }
        }
    }

    /// Selected `Object::canCrushOrSquish(TEST_CRUSH_OR_SQUISH)` branch.
    pub(super) fn can_crush(&self, id: usize, other: usize) -> bool {
        self.can_squish_only(id, other) || self.can_crush_only(id, other)
    }

    pub(in crate::skirmish) fn can_squish_only(&self, id: usize, other: usize) -> bool {
        !self.entities[id].unmanned()
            && self.object_relationship(id, other) != 2
            && self.catalog[self.entities[id].type_index]
                .crush
                .crusher_level
                > 0
            && self.catalog[self.entities[other].type_index]
                .crush
                .squish_module
                .is_some()
    }

    pub(in crate::skirmish) fn can_crush_only(&self, id: usize, other: usize) -> bool {
        let actor = &self.entities[id];
        let victim = &self.entities[other];
        let level = self.catalog[actor.type_index].crush.crusher_level;
        !actor.unmanned()
            && self.object_relationship(id, other) != 2
            && level != 0
            && level > self.catalog[victim.type_index].crush.crushable_level
    }

    /// `checkForMovement` checks claim identity, not velocity or the UI order.
    /// A position with the same goal owner is fixed even during an active task.
    pub(super) fn route_units(&self, id: usize) -> RouteUnits {
        self.route_units_on(id, self.entities[id].layer)
    }

    pub(super) fn route_units_on(&self, id: usize, layer: u8) -> RouteUnits {
        let actor = &self.entities[id];
        let infantry = matches!(
            self.catalog[actor.type_index].group_kind,
            GroupKind::Infantry
        );
        let mut result = self.cell_info_context(layer);
        result.downhill_only = actor
            .active_locomotor(&self.catalog[actor.type_index])
            .ground
            .as_ref()
            .is_some_and(|rules| rules.downhill_only);
        if self.catalog[actor.type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "DOZER")
        {
            result.dozer_obstacles = self
                .navigation_obstacles
                .obstacle_owners()
                .filter_map(|(node, owner)| {
                    let friendly = match owner.id {
                        super::movement::obstacles::OwnerId::Actor(other) => {
                            self.object_relationship(id, other) != 0
                        }
                        super::movement::obstacles::OwnerId::Static(_)
                        | super::movement::obstacles::OwnerId::Neutral(_) => {
                            !self.enemies(actor.owner, owner.player)
                        }
                        super::movement::obstacles::OwnerId::Fence(_) => false,
                    };
                    (node.layer == layer && friendly).then_some(node.cell)
                })
                .collect();
        }
        result.can_path_through_units = actor.ai.blocking.path_through_units;
        for (&node, &other) in &self.occupancy.positions {
            if node.layer != layer {
                continue;
            }
            let cell = node.cell;
            if other == id
                || Some(other) == actor.ai.ignored_producer
                || !self.entities[other].alive()
            {
                continue;
            }
            let unit = &self.entities[other];
            let ally = self.allied(actor.owner, unit.owner);
            let transient = self.occupancy.goals.get(&node) != Some(&other);
            if transient && ally {
                result.moving_allies.insert(cell);
            }
            // INFANTRY_MOVES_THROUGH_INFANTRY is enabled in AIPathfind.h.
            if infantry
                && matches!(
                    self.catalog[unit.type_index].group_kind,
                    GroupKind::Infantry
                )
            {
                continue;
            }
            if transient && ally {
                result.transient_allies.insert(cell);
            } else if transient && !self.can_crush(id, other) {
                result.transient_enemies.insert(cell);
            } else if ally {
                result.fixed_allies.insert(cell);
            } else if !transient && !self.can_crush(id, other) {
                result.fixed_enemies.insert(cell);
            }
        }
        result
    }

    /// Fresh retained INFO for source hierarchy/path-cost queries, which do not
    /// inspect actor-relative collision classes or unavailable destinations.
    pub(super) fn layer_path_info(&self) -> BTreeMap<u8, RouteUnits> {
        std::iter::once(1)
            .chain(self.movement_terrain.path_layers())
            .map(|layer| (layer, self.path_info_on(layer)))
            .collect()
    }

    fn path_info_on(&self, layer: u8) -> RouteUnits {
        let mut result = RouteUnits::new();
        result.info_available = crate::grid::CELL_INFO_CAPACITY - self.occupancy.infos.len();
        // Searches share the current authoritative INFO; no per-layer copy or cache.
        result.shared_info = Some((layer, Arc::clone(&self.occupancy.infos)));
        result
    }

    fn cell_info_context(&self, layer: u8) -> RouteUnits {
        let mut result = self.path_info_on(layer);
        result.width_blockers = self
            .occupancy
            .positions
            .iter()
            .filter_map(|(&node, &owner)| {
                (node.layer == layer
                    && self.occupancy.goals.get(&node) == Some(&owner)
                    && self.entities[owner].alive()
                    && self.catalog[self.entities[owner].type_index]
                        .crush
                        .crushable_level
                        > 0)
                .then_some(node.cell)
            })
            .collect();
        result.goal_flags = self
            .occupancy
            .goals
            .iter()
            .filter_map(|(&cell, &owner)| {
                (cell.layer == layer && self.occupancy.positions.get(&cell) != Some(&owner))
                    .then_some(cell.cell)
            })
            .collect();
        result.idle_positions = self
            .occupancy
            .positions
            .iter()
            .filter_map(|(&cell, &owner)| {
                (cell.layer == layer && matches!(self.entities[owner].ai.order, Order::Idle))
                    .then_some(cell.cell)
            })
            .collect();
        result
    }

    /// The fixed-unit branch of native checkForMovement for line callbacks.
    /// Query the current cell claims rather than materializing every map cell's
    /// actor-relative flags for each retained-route sample.
    pub(super) fn fixed_unit_blocks(&self, id: usize, cell: Cell) -> bool {
        self.fixed_unit_blocks_on(
            id,
            LayerCell {
                cell,
                layer: self.entities[id].layer,
            },
        )
    }

    pub(super) fn fixed_unit_blocks_on(&self, id: usize, cell: LayerCell) -> bool {
        let cell = self.navigation.resolve_layer(cell);
        let Some(&other) = self.occupancy.positions.get(&cell) else {
            return false;
        };
        let actor = &self.entities[id];
        let target = &self.entities[other];
        if other == id
            || Some(other) == actor.ai.ignored_producer
            || !target.alive()
            || self.occupancy.goals.get(&cell) != Some(&other)
        {
            return false;
        }
        let infantry = |index: usize| matches!(self.catalog[index].group_kind, GroupKind::Infantry);
        !(infantry(actor.type_index) && infantry(target.type_index))
            && (self.allied(actor.owner, target.owner) || !self.can_crush(id, other))
    }

    pub(super) fn destination_unclaimed_on(&self, id: usize, node: LayerCell) -> bool {
        let actor = &self.entities[id];
        self.body_cells(id, node.cell).into_iter().all(|cell| {
            let cell = self.navigation.resolve_layer(LayerCell {
                cell,
                layer: node.layer,
            });
            let Some(&owner) = self.occupancy.goals.get(&cell) else {
                return true;
            };
            owner == id
                || Some(owner) == actor.ai.ignored_producer
                || !self.entities[owner].alive()
                || (!self.allied(actor.owner, self.entities[owner].owner)
                    && (self.occupancy.positions.get(&cell) != Some(&owner)
                        || self.can_crush(id, owner)))
        })
    }

    pub(super) fn unavailable_goals(&self, id: usize) -> BTreeSet<Cell> {
        self.unavailable_goals_on(id, self.entities[id].layer)
    }

    pub(super) fn unavailable_goals_on(&self, id: usize, layer: u8) -> BTreeSet<Cell> {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let above = radius + i32::from(centered);
        let mut result = Vec::new();
        for (&node, &owner) in &self.occupancy.goals {
            if node.layer != layer {
                continue;
            }
            let cell = node.cell;
            if owner == id
                || Some(owner) == actor.ai.ignored_producer
                || !self.entities[owner].alive()
                || (!self.allied(actor.owner, self.entities[owner].owner)
                    && (self.occupancy.positions.get(&node) != Some(&owner)
                        || self.can_crush(id, owner)))
            {
                continue;
            }
            for x in i32::from(cell.x) - above + 1..=i32::from(cell.x) + radius {
                for y in i32::from(cell.y) - above + 1..=i32::from(cell.y) + radius {
                    if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) {
                        let cell = Cell { x, y };
                        if self.grid.contains(cell) {
                            result.push(cell);
                        }
                    }
                }
            }
        }
        result.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Kind, tests::game};
    use super::{Arc, LayerCell, MovementSurface};

    #[test]
    fn global_infos_are_bounded_shared_by_claims_and_distinct_between_layers() {
        let mut occupancy = super::Occupancy::new();
        for y in 0..150 {
            for x in 0..200 {
                let node = crate::grid::LayerCell {
                    cell: super::Cell { x, y },
                    layer: 1,
                };
                assert!(occupancy.claim_info(node));
                occupancy.positions.insert(node, 0);
            }
        }
        let ground = crate::grid::LayerCell {
            cell: super::Cell { x: 0, y: 0 },
            layer: 1,
        };
        let upper = crate::grid::LayerCell {
            cell: ground.cell,
            layer: 2,
        };
        assert!(occupancy.claim_info(ground));
        assert!(!occupancy.claim_info(upper));
        occupancy.goals.insert(ground, 1);
        occupancy.positions.remove(&ground);
        occupancy.release_info(ground, false);
        assert!(!occupancy.claim_info(upper));
        occupancy.goals.remove(&ground);
        occupancy.release_info(ground, false);
        assert!(occupancy.claim_info(upper));
        assert_eq!(occupancy.infos.len(), crate::grid::CELL_INFO_CAPACITY);
    }

    #[test]
    fn obstacle_info_retains_its_original_coordinates_until_actual_clear_cell_release() {
        let mut occupancy = super::Occupancy::new();
        let node = LayerCell {
            cell: super::Cell { x: 4, y: 5 },
            layer: 1,
        };
        let shifted = LayerCell {
            cell: super::Cell { x: 3, y: 4 },
            layer: 1,
        };
        assert!(occupancy.claim_info(node));
        Arc::make_mut(&mut occupancy.infos).insert(node, crate::grid::CellInfo::new(shifted));
        occupancy.positions.insert(node, 7);
        occupancy.positions.remove(&node);
        occupancy.release_info(node, true);
        assert!(occupancy.claim_info(node));
        assert!(
            occupancy
                .infos
                .get(&node)
                .is_some_and(|info| info.coordinate == shifted)
        );
        // removeObstacle at the pinned allocation guard does not release INFO.
        // Only a later releaseInfo on the now-clear cell frees the record.
        assert_eq!(occupancy.infos.len(), 1);
        occupancy.release_info(node, false);
        assert!(occupancy.infos.is_empty());
    }

    #[test]
    fn bridge_end_claims_both_layers_and_removal_releases_both() {
        let mut game = game();
        for id in 0..game.entities.len() {
            game.destroy_maintenance_object(id);
        }
        game.process_update_destruction();
        assert!(game.entities.iter().all(|actor| !actor.exists()));
        assert!(game.occupancy.positions.is_empty());
        assert!(game.occupancy.goals.is_empty());
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
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let actor = game.spawn(tank, 0, [125.0, 105.0], true);
        game.entities[actor].layer = layer;
        game.update_position_claim(actor);
        game.reserve_goal_on(actor, [125.0, 105.0], layer);
        let cell = game.movement_cell(actor, [125.0, 105.0]).unwrap();
        for node in [LayerCell { cell, layer: 1 }, LayerCell { cell, layer }] {
            assert_eq!(game.occupancy.position_owner_on(node), Some(actor));
            assert_eq!(game.occupancy.goals.get(&node), Some(&actor));
        }
        game.remove_goal(actor);
        game.remove_position(actor);
        assert!(game.occupancy.goals.is_empty());
        assert!(game.occupancy.positions.is_empty());
        assert!(game.occupancy.infos.is_empty());
        // A retained upper layer beyond its allocated rectangle uses getCell's
        // native ground fallback for every position and goal footprint cell.
        game.entities[actor].position = [25.0, 105.0];
        game.update_position_claim(actor);
        game.reserve_goal_on(actor, [25.0, 105.0], layer);
        assert!(game.occupancy.infos.keys().all(|node| node.layer == 1));
        assert!(
            game.occupancy.position_owner_on(LayerCell {
                cell: game.movement_cell(actor, [25.0, 105.0]).unwrap(),
                layer: 1
            }) == Some(actor)
        );
    }

    #[test]
    fn aircraft_goal_info_survives_airborne_position_update_and_ground_owner_removal() {
        let mut game = game();
        for id in 0..game.entities.len() {
            game.destroy_maintenance_object(id);
        }
        game.process_update_destruction();
        assert!(game.entities.iter().all(|actor| !actor.exists()));
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let ground = game.spawn(tank, 0, [255.0, 155.0], true);
        game.catalog[tank].locomotor.ground = None;
        game.catalog[tank].locomotor.flight = Some(crate::skirmish::flight::tests::rules(
            crate::skirmish::flight::Appearance::Hover,
        ));
        game.catalog[tank].movement_surface = MovementSurface::Air;
        let aircraft = game.spawn(tank, 0, [105.0, 105.0], true);
        game.reserve_goal(aircraft, [255.0, 155.0]);
        let cell = game.movement_cell(aircraft, [255.0, 155.0]).unwrap();
        game.update_position_claim(aircraft);
        assert_eq!(game.occupancy.aircraft_goals.get(&cell), Some(&aircraft));
        game.remove_goal(ground);
        game.remove_position(ground);
        assert!(
            game.occupancy
                .infos
                .contains_key(&LayerCell { cell, layer: 1 })
        );
        assert!(game.destination_unclaimed_on(
            aircraft,
            crate::grid::LayerCell {
                cell,
                layer: game.entities[aircraft].layer
            }
        ));
        game.remove_goal(aircraft);
        assert!(game.occupancy.aircraft_goals.is_empty());
        assert!(game.occupancy.infos.is_empty());
    }

    #[test]
    fn bulk_context_preserves_overlapping_claims_and_direct_destination_admission() {
        for width in [5.0, 10.0, 16.0] {
            let mut game = game();
            let tank = game.type_for(0, Kind::Tank).unwrap();
            game.catalog[tank].geometry.major = width;
            game.catalog[tank].geometry.minor = width;
            let ids: Vec<_> = (0..12_u16)
                .map(|index| {
                    game.spawn(
                        tank,
                        usize::from(index % 3 == 0),
                        [
                            5.0 + f32::from(index % 4) * 12.0,
                            5.0 + f32::from(index / 4) * 12.0,
                        ],
                        true,
                    )
                })
                .collect();
            for round in 0..4 {
                for (index, &id) in ids.iter().enumerate() {
                    if index % 3 == round {
                        game.reserve_goal(id, [85.0, 35.0]);
                    }
                }
                game.entities[ids[1]].health = if round == 1 { 0.0 } else { 100.0 };
                game.catalog[tank].crush.crusher_level = u8::try_from(round).unwrap();
                game.slots[0].team = (round == 3).then_some(1);
                game.slots[1].team = (round == 3).then_some(1);
                for &id in &ids {
                    game.entities[id].ai.ignored_producer = (round == 2).then_some(ids[0]);
                    assert_context(&game, id);
                }
            }
        }
    }

    fn assert_context(game: &super::Skirmish, id: usize) {
        let flags = game.route_units(id);
        let unavailable = game.unavailable_goals(id);
        for x in 0..20 {
            for y in 0..20 {
                let cell = super::Cell { x, y };
                let position = game
                    .occupancy
                    .positions
                    .get(&crate::grid::LayerCell { cell, layer: 1 });
                let goal = game
                    .occupancy
                    .goals
                    .get(&crate::grid::LayerCell { cell, layer: 1 });
                assert_eq!(flags.has_info(cell), position.is_some() || goal.is_some());
                assert_eq!(
                    flags.goal_flags.contains(&cell),
                    goal.is_some() && goal != position
                );
                assert_eq!(
                    flags.idle_positions.contains(&cell),
                    position.is_some_and(|&owner| {
                        matches!(game.entities[owner].ai.order, super::Order::Idle)
                    })
                );
                assert_eq!(
                    unavailable.contains(&cell),
                    !game.destination_unclaimed_on(
                        id,
                        crate::grid::LayerCell {
                            cell,
                            layer: game.entities[id].layer
                        }
                    )
                );
            }
        }
    }

    #[test]
    fn moving_and_retiring_claims_match_full_owner_removal_with_overlaps() {
        let mut game = game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        game.catalog[tank].geometry.major = 16.0;
        game.catalog[tank].geometry.minor = 16.0;
        let ids: Vec<_> = (0..64_u16)
            .map(|index| {
                game.spawn(
                    tank,
                    0,
                    [
                        100.0 + f32::from(index % 8) * 8.0,
                        100.0 + f32::from(index / 8) * 8.0,
                    ],
                    true,
                )
            })
            .collect();
        game.refresh_occupancy();
        for id in ids.into_iter().rev() {
            let mut goals = game.occupancy.goals.clone();
            goals.retain(|_, owner| *owner != id);
            game.remove_goal(id);
            assert!(game.occupancy.goals == goals);
            assert!(game.occupancy.goal_cell(id).is_none());
            let mut positions = game.occupancy.positions.clone();
            positions.retain(|_, owner| *owner != id);
            game.entities[id].health = 0.0;
            game.update_position_claim(id);
            assert!(game.occupancy.positions == positions);
            // A repeated retired-object update must preserve other owners.
            game.update_position_claim(id);
            assert!(game.occupancy.positions == positions);
            assert!(game.occupancy.goals == goals);
        }
    }

    #[test]
    fn live_fixed_cell_queries_match_materialized_actor_flags() {
        let mut game = game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let infantry = game.type_for(0, Kind::Infantry).unwrap();
        let ids: Vec<_> = (0..24_u16)
            .map(|index| {
                let kind = if index % 2 == 0 { tank } else { infantry };
                game.spawn(
                    kind,
                    usize::from(index % 3 == 0),
                    [
                        100.0 + f32::from(index % 8) * 15.0,
                        100.0 + f32::from(index / 8) * 15.0,
                    ],
                    true,
                )
            })
            .collect();
        for round in 0..4 {
            for (index, &id) in ids.iter().enumerate() {
                if index % 3 == round {
                    game.reserve_goal(id, [450.0, game.entities[id].position[1]]);
                }
            }
            game.catalog[tank].crush.crusher_level = u8::try_from(round).unwrap();
            game.catalog[infantry].crush.squish_module = (round > 1).then_some(0);
            game.slots[0].team = (round == 3).then_some(1);
            game.slots[1].team = (round == 3).then_some(1);
            for &id in &ids {
                game.entities[id].ai.ignored_producer = (round == 2).then_some(ids[0]);
                let flags = game.route_units(id);
                for x in 8..26 {
                    for y in 8..18 {
                        let cell = super::Cell { x, y };
                        assert_eq!(
                            game.fixed_unit_blocks(id, cell),
                            flags.fixed_allies.contains(&cell)
                                || flags.fixed_enemies.contains(&cell)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn another_players_allied_goal_is_not_crushable_or_available() {
        let mut game = game();
        for id in 0..game.entities.len() {
            game.destroy_maintenance_object(id);
        }
        game.process_update_destruction();
        assert!(game.entities.iter().all(|actor| !actor.exists()));
        game.slots[0].team = Some(1);
        game.slots[1].team = Some(1);
        let tank = game.type_for(0, Kind::Tank).unwrap();
        game.catalog[tank].crush.crusher_level = 9;
        game.catalog[tank].crush.crushable_level = 0;
        let mover = game.spawn(tank, 0, [55.0, 85.0], true);
        let ally = game.spawn(tank, 1, [155.0, 85.0], true);
        game.refresh_occupancy();
        let cell = game
            .movement_cell(ally, game.entities[ally].position)
            .unwrap();
        assert!(!game.can_crush(mover, ally));
        assert!(!game.destination_unclaimed_on(
            mover,
            crate::grid::LayerCell {
                cell,
                layer: game.entities[mover].layer
            }
        ));
        assert!(game.unavailable_goals(mover).contains(&cell));
        assert!(game.route_units(mover).fixed_allies.contains(&cell));
        assert!(game.route_units(mover).fixed_enemies.is_empty());
    }

    #[test]
    fn fixed_infantry_can_be_crossed_but_keeps_its_final_goal_and_moving_cost() {
        let mut game = game();
        for id in 0..game.entities.len() {
            game.destroy_maintenance_object(id);
        }
        game.process_update_destruction();
        assert!(game.entities.iter().all(|actor| !actor.exists()));
        game.rebuild_navigation();
        let infantry = game.type_for(0, Kind::Infantry).unwrap();
        let mover = game.spawn(infantry, 0, [55.0, 85.0], true);
        let ally = game.spawn(infantry, 0, [155.0, 85.0], true);
        game.refresh_occupancy();
        let cell = game
            .movement_cell(ally, game.entities[ally].position)
            .unwrap();
        assert!(game.route_units(mover).fixed_allies.is_empty());
        assert!(!game.destination_unclaimed_on(
            mover,
            crate::grid::LayerCell {
                cell,
                layer: game.entities[mover].layer
            }
        ));
        game.reserve_goal(ally, [255.0, 85.0]);
        assert!(game.route_units(mover).moving_allies.contains(&cell));
        assert!(game.destination_unclaimed_on(
            mover,
            crate::grid::LayerCell {
                cell,
                layer: game.entities[mover].layer
            }
        ));
    }

    #[test]
    fn source_crush_levels_and_squish_module_control_fixed_enemy_goal_admission() {
        let mut game = game();
        for id in 0..game.entities.len() {
            game.destroy_maintenance_object(id);
        }
        game.process_update_destruction();
        assert!(game.entities.iter().all(|actor| !actor.exists()));
        game.rebuild_navigation();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let infantry = game.type_for(0, Kind::Infantry).unwrap();
        let mover = game.spawn(tank, 0, [55.0, 85.0], true);
        let victim = game.spawn(infantry, 1, [155.0, 85.0], true);
        game.refresh_occupancy();
        let cell = game
            .movement_cell(victim, game.entities[victim].position)
            .unwrap();
        assert!(!game.destination_unclaimed_on(
            mover,
            crate::grid::LayerCell {
                cell,
                layer: game.entities[mover].layer
            }
        ));
        assert!(game.route_units(mover).fixed_enemies.contains(&cell));
        assert!(game.route_units(mover).fixed_allies.is_empty());
        game.entities[mover].ai.ignored_producer = Some(victim);
        assert!(game.route_units(mover).fixed_enemies.is_empty());
        assert!(game.destination_unclaimed_on(
            mover,
            crate::grid::LayerCell {
                cell,
                layer: game.entities[mover].layer
            }
        ));
        game.entities[mover].ai.ignored_producer = None;
        game.catalog[tank].crush.crusher_level = 1;
        game.catalog[infantry].crush.crushable_level = 1;
        assert!(!game.can_crush(mover, victim));
        game.catalog[tank].crush.crusher_level = 2;
        assert!(game.can_crush(mover, victim));
        assert!(game.destination_unclaimed_on(
            mover,
            crate::grid::LayerCell {
                cell,
                layer: game.entities[mover].layer
            }
        ));
        assert!(game.route_units(mover).fixed_enemies.is_empty());
        game.catalog[infantry].crush.crushable_level = 255;
        game.catalog[infantry].crush.squish_module = Some(0);
        assert!(game.can_crush(mover, victim));
        game.catalog[tank].crush.crusher_level = 0;
        assert!(!game.can_crush(mover, victim));
        game.catalog[tank].crush.crusher_level = 2;
        game.entities[victim].owner = 0;
        assert!(!game.can_crush(mover, victim));
        assert!(!game.destination_unclaimed_on(
            mover,
            crate::grid::LayerCell {
                cell,
                layer: game.entities[mover].layer
            }
        ));
    }
}

#[cfg(test)]
#[path = "occupancy/info_context_tests.rs"]
mod info_context_tests;
