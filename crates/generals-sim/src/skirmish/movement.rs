use std::collections::{BTreeSet, VecDeque};
use std::f32::consts::TAU;

mod attack;
mod hierarchy;
pub(super) mod obstacles;
mod position;

use super::{
    CELL_SIZE, Cell, Footprint, MovementSurface, Order, STEP_SECONDS, Skirmish, locomotor, startup,
};

impl Skirmish {
    pub(super) fn travel_to(&mut self, id: usize, goal: [f32; 2], stop_range: f32) -> bool {
        if self.entities[id].disabled_held() {
            return false;
        }
        if self.intermediate_waypoint(id) {
            // AIInternalMoveToState removes the goal when FollowPath disables
            // destination adjustment for an intermediate segment.
            self.remove_goal(id);
        }
        if self.entities[id]
            .active_locomotor(&self.catalog[self.entities[id].type_index])
            .ground
            .is_some()
        {
            return self.travel_ground(id, goal, stop_range, None);
        }
        if self.entities[id].flight.is_some()
            && self.entities[id]
                .active_locomotor(&self.catalog[self.entities[id].type_index])
                .flight
                .is_some()
        {
            return self.travel_flight(id, goal, stop_range);
        }
        if distance_squared(self.entities[id].position, goal) <= stop_range * stop_range {
            self.entities[id].ai.path.clear();
            self.entities[id].ai.path_endpoint = None;
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(crate::skirmish::ModelConditions::MOVING, false);
            return true;
        }
        let actor = &self.entities[id];
        let reusable = actor.ai.path_goal.is_some_and(|cell| {
            (stop_range < CELL_SIZE * 0.5 && actor.ai.path_endpoint == Some(goal))
                || distance_squared(self.path_position(id, cell), goal)
                    <= stop_range.max(0.1).powi(2)
        }) && !actor.ai.path.optimized.is_empty();
        if !reusable {
            let Some(mut path) = self.approach_path(id, actor.position, goal, stop_range) else {
                self.set_order(id, Order::Idle);
                return false;
            };
            let destination = *path
                .optimized
                .back()
                .expect("A successful route includes its destination");
            path.optimized.pop_front();
            if path.optimized.is_empty() {
                path.optimized.push_back(destination);
            }
            self.entities[id].ai.path = path;
            self.entities[id].ai.path_goal = Some(destination.cell);
            self.entities[id].ai.path_endpoint = Some(
                if stop_range < CELL_SIZE * 0.5
                    || matches!(
                        self.catalog[self.entities[id].type_index].movement_surface,
                        MovementSurface::Air
                    )
                {
                    goal
                } else {
                    self.path_position(id, destination.cell)
                },
            );
            if !self.intermediate_waypoint(id)
                && let Some(endpoint) = self.entities[id].ai.path_endpoint
            {
                self.reserve_goal(id, endpoint);
            }
        }
        self.advance_path(id);
        let arrived =
            distance_squared(self.entities[id].position, goal) <= stop_range.max(0.1).powi(2);
        if arrived {
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(crate::skirmish::ModelConditions::MOVING, false);
        }
        arrived
    }

    /// Docking retains its order and world-space request. Native ground routes
    /// use their state-dependent grid/quick endpoint; the aircraft adapter keeps
    /// the precise bone coordinate.
    pub(super) fn travel_to_exact(&mut self, id: usize, goal: [f32; 2]) -> bool {
        self.travel_to(id, goal, 0.1)
    }

    pub(super) fn construction_approach(&mut self, id: usize, building: usize) -> Option<[f32; 2]> {
        let actor = self.entities.get(id)?;
        let target = self.entities.get(building)?;
        let working = biased_approach(
            actor.position,
            target.position,
            self.catalog[target.type_index].geometry.major,
        );
        let ignore = matches!(
            self.catalog[actor.type_index].movement_surface,
            MovementSurface::Air
        )
        .then_some(building);
        // DozerAIUpdate::findGoodBuildOrRepairPosition uses the same source
        // finder as docks, with a ground-only maximum height difference of 10.
        // The current XY map does not yet represent that vertical filter.
        Some(self.find_approach_position(id, working, ignore, None))
    }

    /// `DozerAIUpdate::findGoodBuildOrRepairPosition` uses a full XYZ bias and
    /// rejects ground candidates more than ten units from that biased height.
    pub(in crate::skirmish) fn dozer_dock_position(
        &mut self,
        id: usize,
        target: usize,
    ) -> [f32; 3] {
        let origin = self.object_position(id);
        let center = self.object_position(target);
        let offset = std::array::from_fn::<_, 3, _>(|axis| origin[axis] - center[axis]);
        let squared = offset.map(|value| value * value);
        let length = (squared[0] + squared[1] + squared[2]).sqrt();
        let radius = self.catalog[self.entities[target].type_index]
            .geometry
            .major
            / 2.0;
        let inverse = if length == 0.0 { 0.0 } else { 1.0 / length };
        let normalized = offset.map(|value| value * inverse);
        let offset = normalized.map(|value| value * radius);
        let working = std::array::from_fn(|axis| center[axis] + offset[axis]);
        let point = [working[0], working[1]];
        let actor = &self.entities[id];
        let airborne = actor.is_using_airborne_locomotor(&self.catalog[actor.type_index]);
        let mut selected = None;
        self.find_position_around(point, |game, point, angle| {
            let height = game.ground_height(point);
            let surface = game.movement_terrain.physics_surfaces(point, height);
            if surface.is_none_or(|flags| flags & (2 | 4) != 0)
                || (!airborne && (height - working[2]).abs() > 10.0)
                || !game.clear_approach_geometry(id, point, angle, airborne.then_some(target), None)
                || game
                    .movement_path(id, game.entities[id].position, point)
                    .is_none()
            {
                return false;
            }
            selected = Some([point[0], point[1], height]);
            true
        });
        selected.unwrap_or(working)
    }

    pub(in crate::skirmish) fn find_native_dock_position(
        &mut self,
        id: usize,
        target: usize,
        working: [f32; 3],
    ) -> [f32; 3] {
        let actor = &self.entities[id];
        let airborne = actor.is_using_airborne_locomotor(&self.catalog[actor.type_index]);
        let mut selected = None;
        self.find_position_around([working[0], working[1]], |game, point, angle| {
            let height = game.ground_height(point);
            if game
                .movement_terrain
                .physics_surfaces(point, height)
                .is_none_or(|flags| flags & (2 | 4) != 0)
                || !game.clear_approach_geometry(id, point, angle, airborne.then_some(target), None)
                || game
                    .movement_path(id, game.entities[id].position, point)
                    .is_none()
            {
                return false;
            }
            selected = Some([point[0], point[1], height]);
            true
        });
        selected.unwrap_or(working)
    }

    pub(in crate::skirmish) fn supply_regroup_position(
        &mut self,
        target: usize,
    ) -> Option<[f32; 3]> {
        let working = self.entities[target].position;
        let mut selected = None;
        self.find_position_around(working, |game, point, angle| {
            let height = game.ground_height(point);
            let probe = Footprint {
                position: point,
                angle,
                major: 5.0,
                minor: 5.0,
                shape: crate::collision_geometry::Shape::Sphere,
            };
            if game
                .movement_terrain
                .physics_surfaces(point, height)
                .is_none_or(|flags| flags & (2 | 4) != 0)
                || game
                    .layout
                    .obstacles
                    .iter()
                    .any(|obstacle| footprints_overlap(&probe, obstacle))
                || game.entities.iter().enumerate().any(|(id, actor)| {
                    actor.exists() && footprints_overlap(&probe, &game.object_footprint(id))
                })
            {
                return false;
            }
            selected = Some([point[0], point[1], height]);
            true
        });
        selected
    }

    pub(in crate::skirmish) fn dozer_action_position(&self, id: usize, target: usize) -> [f32; 3] {
        let center = self.object_position(target);
        if self.cell_at([center[0], center[1]]).is_none() {
            return center;
        }
        let origin = self.object_position(id);
        let angle = (origin[1] - center[1]).atan2(origin[0] - center[0]);
        let radius = self.catalog[self.entities[target].type_index]
            .geometry_bounds
            .sphere_radius;
        // minRadius == maxRadius: the native first ring has exactly one sample.
        let displacement = [radius * angle.cos(), radius * angle.sin()];
        let point = [center[0] + displacement[0], center[1] + displacement[1]];
        let height = self.ground_height(point);
        let surface = self.movement_terrain.physics_surfaces(point, height);
        let probe = Footprint {
            position: point,
            angle,
            major: 5.0,
            minor: 5.0,
            shape: crate::collision_geometry::Shape::Sphere,
        };
        if surface.is_some_and(|flags| flags & (2 | 4) == 0)
            && self.clear_approach_geometry(id, point, angle, None, None)
            && !footprints_overlap(&probe, &self.object_footprint(id))
        {
            [point[0], point[1], height]
        } else {
            center
        }
    }

    fn find_approach_position(
        &mut self,
        id: usize,
        working: [f32; 2],
        ignore_entity: Option<usize>,
        ignore_supply: Option<usize>,
    ) -> [f32; 2] {
        self.find_position_around(working, |game, point, angle| {
            game.clear_approach_geometry(id, point, angle, ignore_entity, ignore_supply)
                && game
                    .movement_path(id, game.entities[id].position, point)
                    .is_some()
        })
    }

    pub(super) fn find_position_around(
        &mut self,
        working: [f32; 2],
        mut accept: impl FnMut(&mut Self, [f32; 2], f32) -> bool,
    ) -> [f32; 2] {
        if self.cell_at(working).is_none() {
            return working;
        }
        let start_angle = startup::random_angle(&mut self.random);
        for ring in 0..=20_u16 {
            let distance = f32::from(ring) * 5.0;
            let spacing = if ring == 0 {
                TAU
            } else {
                (5.0 / (distance + 1.0)) * (TAU / 6.0)
            };
            let samples = ((TAU / spacing) / 2.0).ceil();
            let mut sample = 0_u16;
            loop {
                if f32::from(sample) >= samples {
                    break;
                }
                let offset = spacing * f32::from(sample);
                for angle in [start_angle + offset, start_angle - offset]
                    .into_iter()
                    .take(if sample == 0 { 1 } else { 2 })
                {
                    let (sine, cosine) = angle.sin_cos();
                    let displacement = [distance * cosine, distance * sine];
                    let point = [working[0] + displacement[0], working[1] + displacement[1]];
                    if accept(self, point, angle) {
                        return point;
                    }
                }
                sample += 1;
            }
        }
        // Native DockUpdate returns its original working coordinate when the
        // bounded finder fails; it does not teleport to a nearest grid cell.
        working
    }

    pub(super) fn clear_approach_geometry(
        &self,
        id: usize,
        point: [f32; 2],
        angle: f32,
        ignore_entity: Option<usize>,
        ignore_supply: Option<usize>,
    ) -> bool {
        if self
            .cell_at(point)
            .is_none_or(|cell| self.grid.is_blocked(cell))
        {
            return false;
        }
        let probe = Footprint {
            position: point,
            angle,
            major: 5.0,
            minor: 5.0,
            shape: crate::collision_geometry::Shape::Sphere,
        };
        let warehouse_object = ignore_supply.map(|index| self.supplies[index].object);
        if self
            .layout
            .obstacles
            .iter()
            .any(|other| footprints_overlap(&probe, other))
        {
            return false;
        }
        if self.entities.iter().enumerate().any(|(other_id, other)| {
            if other_id == id
                || Some(other_id) == ignore_entity
                || Some(other_id) == warehouse_object
                || !other.alive()
            {
                return false;
            }
            let footprint = self.catalog[other.type_index].footprint(other.position, other.facing);
            footprints_overlap(&probe, &footprint)
        }) {
            return false;
        }
        true
    }

    pub(super) fn advance_path(&mut self, id: usize) {
        if self.entities[id]
            .active_locomotor(&self.catalog[self.entities[id].type_index])
            .ground
            .is_some()
        {
            self.advance_ground(id);
            return;
        }
        if self.entities[id].flight.is_some() {
            let actor = &self.entities[id];
            if let Some(goal) = actor.ai.path_endpoint
                && self.travel_flight(id, goal, 0.1)
            {
                self.entities[id].ai.path.clear();
            }
            return;
        }
        let terrain = super::physical_ocl::transform::TerrainContext {
            terrain: self.projectile_terrain.as_ref(),
            layers: &self.movement_terrain,
            height: self.object_transform(id).0[2],
        };
        let unit = &self.catalog[self.entities[id].type_index];
        let mut distance_left = locomotor::speed(&self.entities[id], unit) * STEP_SECONDS;
        let mut turn_left = locomotor::turn_rate(&self.entities[id], unit);
        let (_, centered) = startup::radius_and_center(&unit.footprint([0.0, 0.0], 0.0));
        let actor = &mut self.entities[id];
        // AIInternalMoveToState::update sets MOVING on ordinary ground even while
        // turning. Its blocked-frame/cliff branches require native path physics.
        actor.drawable.model_conditions.set_flag(
            crate::skirmish::ModelConditions::MOVING,
            !actor.ai.path.optimized.is_empty(),
        );
        while let Some(&cell) = actor.ai.path.optimized.front() {
            if distance_left <= 0.0 {
                break;
            }
            let goal = if actor.ai.path.optimized.len() == 1 {
                actor
                    .ai
                    .path_endpoint
                    .unwrap_or_else(|| match actor.ai.order {
                        Order::Exit(goal) => [goal[0], goal[1]],
                        _ => native_cell_position(cell.cell, centered),
                    })
            } else {
                native_cell_position(cell.cell, centered)
            };
            let delta = [goal[0] - actor.position[0], goal[1] - actor.position[1]];
            let distance = delta[0].hypot(delta[1]);
            if distance <= f32::EPSILON {
                actor.ai.path.optimized.pop_front();
                continue;
            }
            let (finished, angle) = locomotor::turn_towards(actor, unit, goal, turn_left, &terrain);
            if !finished {
                break;
            }
            turn_left = (turn_left - angle).max(0.0);
            // A noncentral native turn pivot changes the center position.
            let delta = [goal[0] - actor.position[0], goal[1] - actor.position[1]];
            let distance = delta[0].hypot(delta[1]);
            if distance <= f32::EPSILON {
                actor.ai.path.optimized.pop_front();
                continue;
            }
            let travel = distance.min(distance_left);
            actor.position[0] = (delta[0] / distance).mul_add(travel, actor.position[0]);
            actor.position[1] = (delta[1] / distance).mul_add(travel, actor.position[1]);
            distance_left -= travel;
            if travel >= distance {
                actor.position = goal;
                actor.ai.path.optimized.pop_front();
            }
        }
    }

    pub(super) fn cell_at(&self, point: [f32; 2]) -> Option<Cell> {
        Some(Cell {
            x: axis_cell(point[0], self.grid.width())?,
            y: axis_cell(point[1], self.grid.height())?,
        })
    }

    fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.grid.height()).flat_map(|y| (0..self.grid.width()).map(move |x| Cell { x, y }))
    }

    pub(super) fn movement_path(
        &mut self,
        id: usize,
        from: [f32; 2],
        target: [f32; 2],
    ) -> Option<crate::grid::Route> {
        self.movement_path_counted(id, from, target).route
    }

    pub(super) fn movement_path_counted(
        &mut self,
        id: usize,
        from: [f32; 2],
        target: [f32; 2],
    ) -> crate::grid::SearchResult {
        self.movement_path_world(id, from, self.movement_goal_position(id, target))
    }

    pub(super) fn movement_path_world(
        &mut self,
        id: usize,
        from: [f32; 2],
        target: [f32; 3],
    ) -> crate::grid::SearchResult {
        let unit = &self.catalog[self.entities[id].type_index];
        if matches!(unit.movement_surface, MovementSurface::Air)
            && self.entities[id].active_locomotor(unit).ground.is_none()
        {
            let result = self.search_movement_path(id, from, target, None);
            return self.finish_path_search(result);
        }
        let mut from_world = self.object_position(id);
        from_world[0] = from[0];
        from_world[1] = from[1];
        let query = self.hierarchy_query(id, from_world, target);
        let hierarchy = self.hierarchical_path(&query);
        let mut result = self.search_movement_path(id, from, target, hierarchy.blocks.as_ref());
        result.cells += hierarchy.search.cells;
        self.finish_path_search(result)
    }

    fn search_movement_path(
        &self,
        id: usize,
        from: [f32; 2],
        target: [f32; 3],
        corridor: Option<&BTreeSet<Cell>>,
    ) -> crate::grid::SearchResult {
        let unit = &self.catalog[self.entities[id].type_index];
        if matches!(unit.movement_surface, MovementSurface::Air)
            && self.entities[id].active_locomotor(unit).ground.is_none()
        {
            // Native quick paths are installed by path_service::air before this
            // grid fallback; they retain world coordinates and tall-building detours.
            return crate::grid::SearchResult {
                info_cleanup: Vec::new(),
                retained_infos: Vec::new(),
                route: self.cell_at([target[0], target[1]]).map(|goal| {
                    crate::grid::Route::direct(self.path_cell([from[0], from[1], target[2]]), goal)
                }),
                cells: 0,
            };
        }
        let mut source = self.object_position(id);
        source[0] = from[0];
        source[1] = from[1];
        let Some(query) = self.path_query(id, source, target, None) else {
            return crate::grid::SearchResult::empty();
        };
        if !self.intermediate_waypoint(id) && !self.destination_unclaimed_on(id, query.goal) {
            return crate::grid::SearchResult::empty();
        }
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        if navigation.has_bridge_layers() {
            let (mut units, unavailable) = self.layer_route_context(id);
            for context in units.values_mut() {
                context.corridor = corridor.cloned();
            }
            return navigation.path_for_layers_counted(&query, &units, &unavailable);
        }
        let mut units = self.route_units(id);
        units.corridor = corridor.cloned();
        navigation.path_for_counted(
            self.entities[id].locomotor_surfaces(unit),
            query.start.cell,
            query.goal.cell,
            query.radius,
            query.centered,
            &units,
        )
    }

    pub(super) fn intermediate_waypoint(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        actor.ai.blocking.temporary_goal().is_none()
            && (matches!(actor.ai.order, Order::FollowPath)
                || self.jet_follow_path_index(id).is_some())
            && actor.ai.waypoints.index + 1 < actor.ai.waypoints.points.len()
    }

    pub(super) fn destination_layer(&self, point: [f32; 2]) -> u8 {
        let height = self.ground_height(point);
        self.movement_terrain
            .get_layer_for_destination([point[0], point[1], height], height)
    }

    pub(super) fn move_destination_adjusts(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        let state_adjusts = actor.ai.blocking.temporary_adjusts().unwrap_or_else(|| {
            if actor.ai.docking.is_some()
                || matches!(
                    actor.ai.command.main_state,
                    super::ai_goals::MainState::Chinook
                        | super::ai_goals::MainState::Jet
                        | super::ai_goals::MainState::Repulsors
                        | super::ai_goals::MainState::WanderInPlace
                        | super::ai_goals::MainState::Wander
                        | super::ai_goals::MainState::Panic
                )
            {
                actor.ai.command.move_to.adjusts
            } else {
                !matches!(
                    actor.ai.order,
                    Order::Move(_) | Order::FollowPath | Order::Exit(_)
                ) || actor.ai.command.move_to.adjusts
            }
        });
        state_adjusts
            && !actor.parachute_rider()
            && !actor.landed()
            && !self.intermediate_waypoint(id)
            && !self.approaches_ignored_object(id)
    }

    pub(super) fn movement_goal_position(&self, id: usize, point: [f32; 2]) -> [f32; 3] {
        let actor = &self.entities[id];
        if let Some(goal) = actor.ai.blocking.temporary_world_goal() {
            return goal;
        }
        let goal = if actor.ai.docking.is_some()
            || matches!(
                actor.ai.order,
                Order::Move(_) | Order::FollowPath | Order::Exit(_)
            )
            || matches!(
                actor.ai.command.main_state,
                super::ai_goals::MainState::Chinook
                    | super::ai_goals::MainState::Jet
                    | super::ai_goals::MainState::Repulsors
                    | super::ai_goals::MainState::WanderInPlace
                    | super::ai_goals::MainState::Wander
                    | super::ai_goals::MainState::Panic
            ) {
            Some(actor.ai.command.move_to.position)
        } else if matches!(actor.ai.order, Order::Guard(_)) {
            let height = if matches!(actor.ai.guard.phase, super::ai::GuardPhase::Return) {
                actor.ai.guard.return_position[2]
            } else {
                actor.ai.guard.center[2]
            };
            Some([point[0], point[1], height])
        } else if matches!(actor.ai.order, Order::FollowPath) {
            actor
                .ai
                .waypoints
                .points
                .get(actor.ai.waypoints.index)
                .copied()
        } else {
            let goal = actor.ai_goal_position();
            ([goal[0], goal[1]].partial_cmp(&point) == Some(std::cmp::Ordering::Equal))
                .then_some(goal)
        };
        goal.unwrap_or_else(|| [point[0], point[1], self.ground_height(point)])
    }

    pub(super) fn movement_goal_layer(&self, id: usize, point: [f32; 2]) -> u8 {
        let goal = self.movement_goal_position(id, point);
        self.movement_terrain
            .get_layer_for_destination(goal, self.ground_height(point))
    }

    pub(super) fn movement_layer_height(&self, layer: u8, point: [f32; 2]) -> f32 {
        // W3DTerrainLogic::getLayerHeight retains a broken bridge's surface.
        // Connectivity and layer-height queries are distinct native operations.
        let ground = self.ground_height(point);
        if let Some(geometry) = self.movement_terrain.bridge_geometry(layer) {
            return if geometry.contains(point) {
                ground.max(geometry.height(point))
            } else {
                ground
            };
        }
        self.movement_terrain
            .bridge_height(layer, point)
            .unwrap_or(ground)
    }

    pub(super) fn layer_route_context(
        &self,
        id: usize,
    ) -> (
        std::collections::BTreeMap<u8, crate::grid::RouteUnits>,
        std::collections::BTreeMap<u8, BTreeSet<Cell>>,
    ) {
        let layers = std::iter::once(1).chain(self.movement_terrain.path_layers());
        let mut units = std::collections::BTreeMap::new();
        let mut unavailable = std::collections::BTreeMap::new();
        for layer in layers {
            units.insert(layer, self.route_units_on(id, layer));
            unavailable.insert(layer, self.unavailable_goals_on(id, layer));
        }
        (units, unavailable)
    }

    /// Native terrain-zone preview; it does not promise a completed movement route.
    /// Shared by the placement icon and the committed building command.
    ///
    /// # Panics
    /// Panics if the builder's stored position violates the finite-coordinate invariant.
    #[must_use]
    pub fn build_site_reachable(&self, id: usize, target: [f32; 2]) -> bool {
        let Some(actor) = self.entities.get(id) else {
            return false;
        };
        let unit = &self.catalog[actor.type_index];
        // BuildAssistant skips CLEAR_PATH for immobile builders; mobile
        // builders without an AI interface cannot supply isQuickPathAvailable.
        if unit.kind_of.iter().any(|kind| kind == "IMMOBILE") {
            return true;
        }
        if matches!(unit.ai.update, super::UnitAiUpdate::Absent) {
            return false;
        }
        if !target.into_iter().all(f32::is_finite) {
            return false;
        }
        let from = self.object_position(id);
        let to = [target[0], target[1], self.ground_height(target)];
        let clipped = |point: [f32; 3]| {
            self.cell_at([
                point[0].clamp(0.0, f32::from(self.grid.width() - 1) * CELL_SIZE),
                point[1].clamp(0.0, f32::from(self.grid.height() - 1) * CELL_SIZE),
            ])
            .expect("native clipped cell")
        };
        let node = |point: [f32; 3]| crate::grid::LayerCell {
            cell: clipped(point),
            layer: self
                .movement_terrain
                .get_layer_for_destination(point, self.ground_height([point[0], point[1]])),
        };
        self.navigation
            .quick_ui_path_exists(node(from), node(to), actor.locomotor_surfaces(unit))
    }

    pub(super) fn path_position(&self, id: usize, cell: Cell) -> [f32; 2] {
        let unit = &self.catalog[self.entities[id].type_index];
        let (_, centered) = startup::radius_and_center(&unit.footprint([0.0, 0.0], 0.0));
        native_cell_position(cell, centered)
    }

    pub(super) fn approach_path(
        &mut self,
        id: usize,
        from: [f32; 2],
        target: [f32; 2],
        range: f32,
    ) -> Option<crate::grid::Route> {
        self.approach_path_counted(id, from, target, range).route
    }

    pub(super) fn approach_path_counted(
        &mut self,
        id: usize,
        from: [f32; 2],
        target: [f32; 2],
        range: f32,
    ) -> crate::grid::SearchResult {
        self.approach_path_world(id, from, self.movement_goal_position(id, target), range)
    }

    pub(super) fn approach_path_world(
        &mut self,
        id: usize,
        from: [f32; 2],
        world: [f32; 3],
        range: f32,
    ) -> crate::grid::SearchResult {
        let target = [world[0], world[1]];
        let Some(start) = self.cell_at(from) else {
            return crate::grid::SearchResult::empty();
        };
        if range < CELL_SIZE * 0.5
            || matches!(
                self.catalog[self.entities[id].type_index].movement_surface,
                MovementSurface::Air
            )
        {
            return self.movement_path_world(id, from, world);
        }
        let Some(closest) = self
            .cells()
            .filter(|&cell| {
                !self.navigation_for(id).is_blocked(cell)
                    && distance_squared(self.path_position(id, cell), target) <= range * range
            })
            .min_by(|&a, &b| {
                distance_squared(self.path_position(id, a), from)
                    .total_cmp(&distance_squared(self.path_position(id, b), from))
            })
        else {
            return crate::grid::SearchResult::empty();
        };
        let mut result = self.movement_path_world(id, from, {
            let point = self.path_position(id, closest);
            [point[0], point[1], world[2]]
        });
        if result.route.is_some() {
            return result;
        }
        // The nearest geometric approach can be isolated by cliffs. Search the
        // reachable component once instead of retrying every candidate path.
        let mut frontier = VecDeque::from([start]);
        let mut visited = BTreeSet::from([start]);
        while let Some(cell) = frontier.pop_front() {
            if distance_squared(self.path_position(id, cell), target) <= range * range {
                let next = self.movement_path_world(id, from, {
                    let point = self.path_position(id, cell);
                    [point[0], point[1], world[2]]
                });
                result.cells += next.cells;
                result.route = next.route;
                if result.route.is_some() {
                    return result;
                }
            }
            let adjacent = [
                cell.y.checked_sub(1).map(|y| Cell { x: cell.x, y }),
                cell.x.checked_add(1).map(|x| Cell { x, y: cell.y }),
                cell.y.checked_add(1).map(|y| Cell { x: cell.x, y }),
                cell.x.checked_sub(1).map(|x| Cell { x, y: cell.y }),
            ];
            for neighbor in adjacent.into_iter().flatten() {
                if !self.navigation_for(id).is_blocked(neighbor) && visited.insert(neighbor) {
                    frontier.push_back(neighbor);
                }
            }
        }
        result
    }

    /// Finds nearby unobstructed terrain for a catalog building's source footprint.
    /// This does not reserve or pay for the location.
    #[must_use]
    pub fn placement_near(&self, type_index: usize, point: [f32; 2]) -> Option<[f32; 2]> {
        self.catalog
            .get(type_index)
            .filter(|unit| unit.kind.is_building())?;
        let mut cells: Vec<_> = self.cells().collect();
        cells.sort_by(|&a, &b| {
            distance_squared(cell_center(a), point)
                .total_cmp(&distance_squared(cell_center(b), point))
        });
        cells
            .into_iter()
            .map(cell_center)
            .find(|&position| self.can_place(type_index, position))
    }

    /// Checks a building's source footprint against terrain, objects and supply restrictions.
    #[must_use]
    pub fn can_place(&self, type_index: usize, position: [f32; 2]) -> bool {
        self.catalog.get(type_index).is_some_and(|unit| {
            self.can_place_at_angle(type_index, position, unit.placement_angle, false)
        })
    }

    /// Checks the same rotated footprint used by a human construction command.
    #[must_use]
    pub fn can_place_facing(&self, type_index: usize, position: [f32; 2], angle: f32) -> bool {
        self.can_place_at_angle(type_index, position, angle, false)
    }

    /// Checks a human placement preview with the actual builder's relationships.
    /// Uses the same object, terrain and reachability checks as paid placement.
    #[must_use]
    pub fn can_build_facing(
        &self,
        builder: usize,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
    ) -> bool {
        self.entities.get(builder).is_some_and(super::Entity::alive)
            && self.can_place_for_builder(type_index, position, angle, false, Some(builder))
            && self.build_site_reachable(builder, position)
    }

    pub(super) fn can_place_at_angle(
        &self,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
        computer: bool,
    ) -> bool {
        self.can_place_for_builder(type_index, position, angle, computer, None)
    }

    pub(super) fn can_place_for_builder(
        &self,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
        computer: bool,
        builder: Option<usize>,
    ) -> bool {
        if !self.placement_objects_clear_for_builder(type_index, position, angle, computer, builder)
        {
            return false;
        }
        let unit = &self.catalog[type_index];
        let footprint = unit.footprint(position, angle);
        let [minimum, maximum] = footprint_bounds(&footprint);
        let Some(low) = self.cell_at(minimum) else {
            return false;
        };
        let Some(high) = self.cell_at(maximum) else {
            return false;
        };
        for y in low.y..=high.y {
            for x in low.x..=high.x {
                let cell = Cell { x, y };
                // The same rotated footprint determines cells blocked after placement.
                if footprint.contains(cell_center(cell)) && self.grid.is_blocked(cell) {
                    return false;
                }
            }
        }
        true
    }

    pub(super) fn placement_objects_clear(
        &self,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
        computer: bool,
    ) -> bool {
        self.placement_objects_clear_for_builder(type_index, position, angle, computer, None)
    }

    fn placement_objects_clear_for_builder(
        &self,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
        computer: bool,
        builder: Option<usize>,
    ) -> bool {
        let Some(unit) = self.catalog.get(type_index) else {
            return false;
        };
        // isLocationLegalToBuild always bounds the requested center, even without
        // TERRAIN_RESTRICTIONS. The supply-border test is also unconditional.
        if !unit.kind.is_building() || !angle.is_finite() || self.cell_at(position).is_none() {
            return false;
        }
        let footprint = unit.footprint(position, angle);
        if (unit.avoid_supply() && self.too_close_to_supply(&footprint))
            || !self.exit_areas_clear(type_index, position, angle, computer)
        {
            return false;
        }
        let world = [position[0], position[1], self.ground_height(position)];
        if builder.is_some_and(|id| {
            !matches!(
                self.visibility_cell(self.entities[id].owner, position),
                super::visibility::Status::Clear
            )
        }) {
            return false;
        }
        if self
            .construction_collisions(&unit.geometry, world, angle)
            .into_iter()
            .any(|id| {
                let other = &self.catalog[self.entities[id].type_index];
                if self.removable_for_construction(id)
                    || other
                        .kind_of
                        .iter()
                        .any(|kind| matches!(kind.as_str(), "MINE" | "INERT"))
                {
                    return false;
                }
                let relationship =
                    builder.map_or(1, |builder| self.object_relationship(builder, id));
                builder.is_some_and(|builder| {
                    matches!(
                        self.visibility_status(self.entities[builder].owner, id),
                        super::visibility::Status::Fogged | super::visibility::Status::Shrouded
                    )
                }) || (relationship == 2 && self.object_busy(id))
                    || other.kind_of.iter().any(|kind| kind == "IMMOBILE")
                    || self.object_disabled(id)
                    || relationship == 0
            })
            || self
                .layout
                .obstacles
                .iter()
                .any(|other| footprints_overlap(&footprint, other))
        {
            return false;
        }
        true
    }

    pub(super) fn too_close_to_supply(&self, footprint: &Footprint) -> bool {
        let border = self.rules.supply_build_border;
        if border <= 0.0 {
            return false;
        }
        // BuildAssistant::isLocationLegalToBuild, GeneralsMD/.../BuildAssistant.cpp:
        // only CANNOT_BUILD_NEAR_SUPPLIES uses the closest supply's expanded geometry.
        // getClosestObject's FROM_BOUNDINGSPHERE_2D subtracts the depot circle radius.
        let doubled_border = border * 2.0;
        let range = bounding_radius(footprint) + doubled_border;
        let closest = self
            .supplies
            .iter()
            .filter(|supply| self.entities[supply.object].exists())
            .filter_map(|supply| {
                let separation =
                    (distance_squared(footprint.position, self.entities[supply.object].position)
                        .sqrt()
                        - bounding_radius(&self.object_footprint(supply.object)))
                    .max(0.0);
                (separation < range).then_some((supply, separation))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        closest.is_some_and(|(supply, _)| {
            let mut expanded = self.object_footprint(supply.object);
            expanded.major += border;
            expanded.minor += border;
            footprints_overlap(footprint, &expanded)
        })
    }

    pub(super) fn rebuild_navigation(&mut self) {
        let blocked = self.registered_obstacle_cells();
        let modifications = self.navigation_obstacles.take_zone_modifications();
        let cleared: Vec<_> = modifications
            .iter()
            .flat_map(|event| event.cells.iter().chain(&event.post_cells))
            .filter_map(|change| {
                matches!(change.kind, crate::terrain::CellType::Clear).then_some(change.cell)
            })
            .collect();
        if !cleared.is_empty() {
            self.grid.clear_ground_barriers(cleared);
        }
        self.cliff_navigation = self
            .catalog
            .iter()
            .any(|unit| {
                (super::locomotor::set::NORMAL..=super::locomotor::set::SLUGGISH)
                    .any(|role| unit.locomotor_set_surfaces(role) & 4 != 0)
            })
            .then(|| {
                self.grid
                    .without_surface_barriers(self.movement_terrain.cliff_cells())
                    .with_navigation_obstacles(
                        blocked.iter().copied(),
                        &self.movement_terrain,
                        self.navigation_obstacles.layer_cells(),
                        self.navigation_obstacles.fence_cells(),
                    )
            });
        let navigation = self.grid.clone().with_navigation_obstacles(
            blocked,
            &self.movement_terrain,
            self.navigation_obstacles.layer_cells(),
            self.navigation_obstacles.fence_cells(),
        );
        let previous = std::mem::replace(&mut self.navigation, navigation);
        if self.tick > 0 {
            if self
                .navigation
                .inherit_source_zones(&previous, &modifications)
            {
                self.path_service.mark_zones_dirty(self.tick);
            }
            if let Some(grid) = &mut self.cliff_navigation {
                grid.inherit_source_zones(&previous, &modifications);
            }
        } else {
            if let Some(grid) = &mut self.cliff_navigation {
                grid.recalculate_source_zones();
            }
            self.navigation.recalculate_source_zones();
        }
        self.rebuild_rubble();
    }
}

fn biased_approach(from: [f32; 2], center: [f32; 2], major_radius: f32) -> [f32; 2] {
    let delta = [from[0] - center[0], from[1] - center[1]];
    let length = delta[0].hypot(delta[1]);
    if length <= 0.0 {
        return center;
    }
    let distance = major_radius / 2.0;
    let offset = [delta[0] / length * distance, delta[1] / length * distance];
    [center[0] + offset[0], center[1] + offset[1]]
}

// PartitionManager.cpp rectToFourPoints, testRotatedPointsAgainstRect and
// xy_collideTest_Rect_Rect/Rect_Circle/Circle_Circle at the pinned source revision.
// These are its XY overlap rules; full partition filters/exit bibs remain unported.
fn footprint_corners(shape: &Footprint) -> [[f32; 2]; 4] {
    let (sine, cosine) = shape.angle.sin_cos();
    let minor = if matches!(shape.shape, crate::collision_geometry::Shape::Box) {
        shape.minor
    } else {
        shape.major
    };
    let major_cosine = shape.major * cosine;
    let minor_cosine = minor * cosine;
    let major_sine = shape.major * sine;
    let minor_sine = minor * sine;
    let [x, y] = shape.position;
    [
        [x - major_cosine - minor_sine, y + minor_cosine - major_sine],
        [x + major_cosine - minor_sine, y + minor_cosine + major_sine],
        [x - major_cosine + minor_sine, y - minor_cosine - major_sine],
        [x + major_cosine + minor_sine, y - minor_cosine + major_sine],
    ]
}

fn rectangle_contains(shape: &Footprint, point: [f32; 2]) -> bool {
    let (sine, cosine) = shape.angle.sin_cos();
    let minor = if matches!(shape.shape, crate::collision_geometry::Shape::Box) {
        shape.minor
    } else {
        shape.major
    };
    let x = point[0] - shape.position[0];
    let y = point[1] - shape.position[1];
    let products = [x * cosine, y * sine, y * cosine, x * sine];
    let local = [products[0] + products[1], products[2] - products[3]];
    local[0].abs() <= shape.major && local[1].abs() <= minor
}

pub(super) fn footprints_overlap(a: &Footprint, b: &Footprint) -> bool {
    use crate::collision_geometry::Shape;

    if !matches!(a.shape, Shape::Box) && !matches!(b.shape, Shape::Box) {
        return distance_squared(a.position, b.position) <= (a.major + b.major).powi(2);
    }
    if !matches!(a.shape, Shape::Sphere) && !matches!(b.shape, Shape::Sphere) {
        // geomCollidesWithGeom applies this preflight to boxes and cylinders.
        // Sphere pairs keep the direct XY tests at the represented ground plane.
        let minimum = a.major.min(a.minor).min(b.major).min(b.minor);
        if minimum * minimum > distance_squared(a.position, b.position) {
            return true;
        }
    }
    footprint_corners(a)
        .into_iter()
        .any(|point| rectangle_contains(b, point))
        || footprint_corners(b)
            .into_iter()
            .any(|point| rectangle_contains(a, point))
}

fn footprint_bounds(shape: &Footprint) -> [[f32; 2]; 2] {
    if !matches!(shape.shape, crate::collision_geometry::Shape::Box) {
        return [
            [
                shape.position[0] - shape.major,
                shape.position[1] - shape.major,
            ],
            [
                shape.position[0] + shape.major,
                shape.position[1] + shape.major,
            ],
        ];
    }
    let mut bounds = [shape.position, shape.position];
    for point in footprint_corners(shape) {
        for (axis, coordinate) in point.into_iter().enumerate() {
            bounds[0][axis] = bounds[0][axis].min(coordinate);
            bounds[1][axis] = bounds[1][axis].max(coordinate);
        }
    }
    bounds
}

pub(super) fn bounding_radius(shape: &Footprint) -> f32 {
    if matches!(shape.shape, crate::collision_geometry::Shape::Box) {
        let squared = [shape.major * shape.major, shape.minor * shape.minor];
        (squared[0] + squared[1]).sqrt()
    } else {
        shape.major
    }
}

pub(super) fn cell_center(cell: Cell) -> [f32; 2] {
    [
        (f32::from(cell.x) + 0.5) * CELL_SIZE,
        (f32::from(cell.y) + 0.5) * CELL_SIZE,
    ]
}

pub(super) fn native_cell_position(cell: Cell, centered: bool) -> [f32; 2] {
    // AIPathfind::adjustCoordToCell uses .05 (half a world unit), not zero,
    // for even-diameter footprints that straddle pathfinding cell boundaries.
    let offset = if centered { 0.5 } else { 0.05 };
    [
        (f32::from(cell.x) + offset) * CELL_SIZE,
        (f32::from(cell.y) + offset) * CELL_SIZE,
    ]
}

pub(super) fn distance_squared(a: [f32; 2], b: [f32; 2]) -> f32 {
    let delta_x = a[0] - b[0];
    let delta_y = a[1] - b[1];
    let x_squared = delta_x * delta_x;
    let y_squared = delta_y * delta_y;
    x_squared + y_squared
}

fn axis_cell(value: f32, limit: u16) -> Option<u16> {
    grid_coordinate(value / CELL_SIZE, limit)
}

pub(super) fn grid_coordinate(value: f32, limit: u16) -> Option<u16> {
    if !value.is_finite() || value < 0.0 || value >= f32::from(limit) {
        return None;
    }
    // Binary integer search avoids unchecked float-to-grid-index conversions.
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

#[cfg(test)]
mod build_preview_tests {
    use crate::skirmish::{Kind, tests::game};

    #[test]
    fn build_assistant_skips_immobile_clear_path_but_rejects_mobile_objects_without_ai() {
        let mut game = game();
        let builder = game.type_for(0, Kind::Builder).unwrap();
        let actor = game.spawn(builder, 0, [355.0, 205.0], true);
        assert!(game.build_site_reachable(actor, [455.0, 205.0]));
        game.catalog[builder].ai.update = crate::skirmish::UnitAiUpdate::Absent;
        assert!(!game.build_site_reachable(actor, [455.0, 205.0]));
        game.catalog[builder].kind_of.push("IMMOBILE".into());
        assert!(game.build_site_reachable(actor, [455.0, 205.0]));
    }

    #[test]
    fn construction_ui_clips_world_queries_but_does_not_make_outside_build_sites_legal() {
        let mut game = game();
        let builder = game.type_for(0, Kind::Builder).unwrap();
        let actor = game.spawn(builder, 0, [355.0, 205.0], true);
        assert!(game.build_site_reachable(actor, [-100.0, 205.0]));
        assert!(game.build_site_reachable(actor, [100_000.0, 205.0]));
        let building = game.type_for(0, Kind::Barracks).unwrap();
        assert!(!game.can_place(building, [-100.0, 205.0]));
        assert!(!game.can_place(building, [100_000.0, 205.0]));
        assert!(!game.build_site_reachable(actor, [f32::NAN, 205.0]));
    }
}
