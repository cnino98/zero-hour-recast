//! `Pathfinder`'s 512-slot FIFO and whole-request 5000-cell frame budget.
use super::{CELL_SIZE, Order, Skirmish};
use crate::grid::{Cell, Route, SearchResult};

#[cfg(test)]
mod producer_tests;
#[cfg(test)]
mod tests;

mod air;
mod safe;

pub(super) enum Kind {
    Move,
    Attack(usize),
    Tighten,
    Recovery,
    Safe,
}

pub(super) struct Request {
    pub goal: [f32; 3],
    pub range: f32,
    pub kind: Kind,
}

pub(super) struct State {
    pub pending: Option<Request>,
    pub failed: bool,
    pub attack_path: bool,
    final_goal: bool,
    repulsors: [Option<usize>; 2],
    queue_at: Option<u32>,
    pub(super) last_goal: Option<[f32; 2]>,
    pub(super) last_endpoint: Option<[f32; 2]>,
}

impl State {
    pub const fn new() -> Self {
        Self {
            pending: None,
            failed: false,
            attack_path: false,
            final_goal: true,
            repulsors: [None; 2],
            queue_at: None,
            last_goal: None,
            last_endpoint: None,
        }
    }

    pub const fn cancel(&mut self) {
        self.pending = None;
        self.failed = false;
        self.attack_path = false;
        self.queue_at = None;
    }
}

pub(super) struct Service {
    entries: [Option<usize>; 512],
    head: usize,
    tail: usize,
    pub zone_recalculate_at: Option<u64>,
    pub cumulative_cells: usize,
}

impl Service {
    pub const fn new() -> Self {
        Self {
            entries: [None; 512],
            head: 0,
            tail: 0,
            zone_recalculate_at: None,
            cumulative_cells: 0,
        }
    }

    pub fn mark_zones_dirty(&mut self, frame: u64) {
        let deadline = if frame < 2 { 2 } else { frame + 300 };
        self.zone_recalculate_at = Some(
            self.zone_recalculate_at
                .map_or(deadline, |current| current.min(deadline)),
        );
    }

    fn queue(&mut self, id: usize) -> bool {
        let mut slot = self.head;
        while slot != self.tail {
            if self.entries[slot] == Some(id) {
                return true;
            }
            slot = (slot + 1) % self.entries.len();
        }
        let next = (self.tail + 1) % self.entries.len();
        if next == self.head {
            return false;
        }
        self.entries[self.tail] = Some(id);
        self.tail = next;
        true
    }
}

impl Skirmish {
    pub(super) fn request_ground_path(&mut self, id: usize, request: Request) {
        let ordinary = matches!(request.kind, Kind::Move | Kind::Recovery);
        let attack = matches!(request.kind, Kind::Attack(_));
        let final_goal = if ordinary {
            self.move_destination_adjusts(id)
        } else {
            !self.intermediate_waypoint(id) && !self.approaches_ignored_object(id)
        };
        if ordinary
            && self.entities[id].locomotor_surfaces(&self.catalog[self.entities[id].type_index]) & 8
                != 0
            && !self.attack_ground_movement(id)
        {
            self.compute_quick_air_path(id, request.goal);
            return;
        }
        let delay = if ordinary { 30 } else { 60 };
        let actor = &mut self.entities[id];
        actor.ai.motion.starting_move();
        actor.ai.path_request.final_goal = final_goal;
        actor.ai.path_request.attack_path = attack;
        actor.ai.path_request.pending = Some(request);
        if matches!(actor.ai.order, Order::Move(_)) {
            actor.ai.command.move_to.waiting = true;
        }
        actor.ai.path_request.failed = false;
        actor.ai.path_request.last_goal = None;
        actor.ai.path_request.last_endpoint = None;
        if self.tick >= 3 && actor.ai.blocking.path_frame > self.tick - 3 {
            if attack {
                actor.ai.motion.clear_goal();
            }
            if ordinary && actor.ai.ground_path.is_some() && actor.ai.blocking.stuck {
                actor.ai.blocking.ignore_until = self.tick + 60;
                actor.ai.blocking.frames = 0;
                actor.ai.blocking.blocked = false;
                actor.ai.blocking.stuck = false;
            }
            self.set_queue_for_path_time(id, delay);
        } else if !self.path_service.queue(id) {
            eprintln!("Original pathfinder queue exhausted: 511 pending objects");
        }
    }

    fn set_queue_for_path_time(&mut self, id: usize, delay: u32) {
        if delay >= 1
            && !self.entities[id].ai.in_update
            && let Some(registration) = self.entities[id].ai.registration
            && self.update_remaining_sleep(id, registration) > delay
        {
            self.wake_update(id, registration, delay);
        }
        let frame = if delay == 0 {
            0
        } else {
            self.update_frame().wrapping_add(delay)
        };
        self.entities[id].ai.path_request.queue_at = (frame != 0).then_some(frame);
    }

    pub(in crate::skirmish) fn ai_path_sleep_frames(&self, id: usize) -> u32 {
        self.entities[id].ai.path_request.queue_at.map_or(
            super::module_updates::FOREVER,
            |deadline| {
                if deadline > self.update_frame() {
                    deadline - self.update_frame()
                } else {
                    1
                }
            },
        )
    }

    pub(super) fn wake_path_request(&mut self, id: usize) {
        if self.entities[id]
            .ai
            .path_request
            .queue_at
            .is_some_and(|frame| self.update_frame() >= frame)
        {
            self.entities[id].ai.path_request.queue_at = None;
            if !self.path_service.queue(id) {
                eprintln!("Original pathfinder queue exhausted: 511 pending objects");
            }
        }
    }

    pub(super) fn process_path_service(&mut self) {
        if self
            .path_service
            .zone_recalculate_at
            .is_some_and(|frame| self.tick >= frame)
        {
            self.navigation.recalculate_source_zones();
            if let Some(grid) = &mut self.cliff_navigation {
                grid.recalculate_source_zones();
            }
            self.rebuild_rubble();
            self.path_service.zone_recalculate_at = None;
            return;
        }
        self.path_service.cumulative_cells = 0;
        while self.path_service.cumulative_cells < 5000
            && self.path_service.head != self.path_service.tail
        {
            let head = self.path_service.head;
            let id = self.path_service.entries[head].take();
            if let Some(id) = id
                && self.entities.get(id).is_some_and(super::Entity::exists)
                && !matches!(
                    self.catalog[self.entities[id].type_index].ai.update,
                    super::UnitAiUpdate::Absent
                )
                && let Some(request) = self.entities[id].ai.path_request.pending.take()
            {
                self.do_ground_pathfind(id, request);
            }
            // Source advances the head after doPathfind, including callbacks.
            self.path_service.head = (head + 1) % self.path_service.entries.len();
        }
    }

    fn do_ground_pathfind(&mut self, id: usize, request: Request) {
        if matches!(request.kind, Kind::Safe) {
            self.compute_queued_safe_path(id);
            return;
        }
        let ordinary = matches!(request.kind, Kind::Move | Kind::Recovery);
        self.entities[id].ai.path_request.attack_path = false;
        if let Kind::Attack(target) = request.kind
            && self.attack_path_satisfied(id, target, request.goal)
        {
            // computeAttackPath's in-range branch destroys the old
            // path and succeeds without changing its timestamp.
            let actor = &mut self.entities[id];
            actor.ai.path.clear();
            actor.ai.ground_path = None;
            actor.ai.path_endpoint = None;
            actor.ai.motion.clear_goal();
            actor.ai.path_request.failed = false;
            actor.ai.path_request.last_goal = Some([request.goal[0], request.goal[1]]);
            actor.ai.path_request.last_endpoint = None;
            actor.ai.path_request.attack_path = true;
        } else if let Some(attack_path) = self.compute_air_attack_path(id, &request) {
            self.entities[id].ai.path_request.attack_path = attack_path;
        } else {
            let result = self.compute_queued_path(id, &request);
            self.path_service.cumulative_cells += result.cells;
            self.receive_ground_path(id, request, result.route);
        }
        if ordinary && self.entities[id].ai.path_request.pending.is_none() {
            self.wake_ai_now(id);
        }
    }

    fn compute_queued_path(&mut self, id: usize, request: &Request) -> SearchResult {
        if matches!(request.kind, Kind::Move | Kind::Recovery)
            && !self.entities[id].ai.blocking.stuck
        {
            let actor = &mut self.entities[id];
            actor.ai.path.clear();
            actor.ai.ground_path = None;
            actor.ai.motion.clear_goal();
        }
        let goal = [request.goal[0], request.goal[1]];
        if matches!(request.kind, Kind::Recovery) {
            return self.recovery_route_counted(id, request.goal);
        }
        if let Kind::Attack(target) = request.kind {
            return self.attack_approach_path_counted(id, target, request.goal);
        }
        if matches!(request.kind, Kind::Tighten) {
            return self.closest_queued_path(id, request.goal, 0.2);
        }
        if !self.entities[id].ai.path_request.final_goal
            && self.ground_line_passable_on(
                id,
                self.entities[id].position,
                goal,
                self.entities[id].layer,
            )
        {
            return self.quick_queued_path(id, request.goal);
        }
        if self.approaches_ignored_object(id) {
            return self.object_approach_path_world(id, request.goal);
        }
        let actor = &self.entities[id];
        if actor.ai.waypoints.exit_quick_pending && matches!(actor.ai.order, Order::Exit(_)) {
            return self.quick_queued_path(id, request.goal);
        }
        let mut result = self.approach_path_world(id, actor.position, request.goal, request.range);
        if result.route.is_none() && request.range < CELL_SIZE * 0.5 {
            let next = self.closest_queued_path(id, request.goal, 0.0);
            result.cells += next.cells;
            result.route = next.route;
        }
        result
    }

    fn quick_queued_path(&self, id: usize, destination: [f32; 3]) -> SearchResult {
        SearchResult {
            info_cleanup: Vec::new(),
            retained_infos: Vec::new(),
            route: self
                .cell_at(self.entities[id].position)
                .zip(self.cell_at([destination[0], destination[1]]))
                .map(|(start, goal)| {
                    let mut route = Route::direct_on(
                        crate::grid::LayerCell {
                            cell: start,
                            layer: self.entities[id].layer,
                        },
                        crate::grid::LayerCell {
                            cell: goal,
                            layer: 1,
                        },
                    );
                    route.exact_endpoint = Some(destination);
                    route.quick_start = Some([
                        self.entities[id].position[0],
                        self.entities[id].position[1],
                        destination[2],
                    ]);
                    route
                }),
            cells: 0,
        }
    }

    pub(in crate::skirmish) fn closest_queued_path(
        &mut self,
        id: usize,
        goal: [f32; 3],
        cost_factor: f32,
    ) -> SearchResult {
        let from = self.object_position(id);
        let mut query = self.hierarchy_query(id, from, goal);
        query.closest = true;
        let Some(movement) = self.path_query(id, from, goal, Some(cost_factor)) else {
            return SearchResult::empty();
        };
        let (units, _) = self.layer_route_context(id);
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let tunneling = navigation.initial_tunneling(
            movement.start,
            movement.radius,
            movement.centered,
            &units,
        );
        drop(units);
        let hierarchy = if tunneling {
            crate::grid::HierarchyResult {
                blocks: None,
                search: SearchResult::empty(),
            }
        } else {
            self.hierarchical_path(&query)
        };
        let mut result =
            self.search_closest_queued_path(id, &movement, cost_factor, hierarchy.blocks.as_ref());
        result.cells += hierarchy.search.cells;
        self.finish_path_search(result)
    }

    fn search_closest_queued_path(
        &self,
        id: usize,
        query: &crate::grid::LayerQuery,
        cost_factor: f32,
        corridor: Option<&std::collections::BTreeSet<Cell>>,
    ) -> SearchResult {
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        if navigation.has_bridge_layers() {
            let (mut units, unavailable) = self.layer_route_context(id);
            for context in units.values_mut() {
                context.corridor = corridor.cloned();
            }
            return navigation.path_for_layers_counted(query, &units, &unavailable);
        }
        let mut units = self.route_units(id);
        units.corridor = corridor.cloned();
        if cost_factor.partial_cmp(&0.2) == Some(std::cmp::Ordering::Equal) {
            navigation.closest_path_counted(
                query.start.cell,
                query.goal.cell,
                query.radius,
                query.centered,
                &self.unavailable_goals(id),
                &units,
            )
        } else {
            navigation.recovery_path_counted(
                query.start.cell,
                query.goal.cell,
                query.radius,
                query.centered,
                &self.unavailable_goals(id),
                &units,
            )
        }
    }

    fn apply_ground_path_failure(&mut self, id: usize, request: Request, blocked_path: bool) {
        let ordinary = matches!(request.kind, Kind::Move | Kind::Recovery);
        if ordinary && blocked_path {
            let actor = &mut self.entities[id];
            actor.ai.path.clear();
            actor.ai.ground_path = None;
            actor.ai.motion.clear_goal();
            actor.ai.blocking.blocked = false;
            actor.ai.blocking.retry_at = Some(self.tick + 30);
            // setFinalPosition stores the snapped point with m_doFinalPosition=false;
            // it does not move the object in this failed-path branch.
            self.set_queue_for_path_time(id, 30);
        } else if !ordinary {
            let actor = &mut self.entities[id];
            actor.ai.path.clear();
            actor.ai.ground_path = None;
            actor.ai.motion.clear_goal();
        }
        let waiting = self.entities[id]
            .ai
            .path_request
            .queue_at
            .is_some_and(|frame| frame > self.update_frame());
        self.entities[id].ai.path_request.failed =
            self.entities[id].ai.path.raw.is_empty() && !waiting;
        if waiting {
            self.entities[id].ai.path_request.pending = Some(request);
        }
    }

    fn receive_ground_path(&mut self, id: usize, request: Request, route: Option<Route>) {
        let blocked_path =
            !self.entities[id].ai.path.raw.is_empty() && self.entities[id].ai.blocking.stuck;
        let endpoint = route.as_ref().and_then(|route| {
            route
                .exact_endpoint
                .map(|point| [point[0], point[1]])
                .or_else(|| {
                    route
                        .optimized
                        .back()
                        .map(|&cell| self.path_position(id, cell.cell))
                })
        });
        let actor = &mut self.entities[id];
        // Keep the callback outcome after onExit destroys the live path, so
        // diagnostics distinguish source closest-path success from no path.
        actor.ai.path_request.last_goal = Some([request.goal[0], request.goal[1]]);
        actor.ai.path_request.last_endpoint = endpoint;
        if !matches!(request.kind, Kind::Tighten) {
            actor.ai.blocking.path_frame = self.tick;
            actor.ai.blocking.frames = 0;
            actor.ai.blocking.stuck = false;
        }
        let Some(mut route) = route else {
            self.apply_ground_path_failure(id, request, blocked_path);
            return;
        };
        if actor.ai.motion.ultra_accurate() {
            route.exact_endpoint = Some(request.goal);
        }
        let destination = *route
            .optimized
            .back()
            .expect("successful source path has a destination");
        route.optimized.pop_front();
        if route.optimized.is_empty() {
            route.optimized.push_back(destination);
        }
        actor.ai.path = route;
        actor.ai.path_goal = Some(destination.cell);
        actor.ai.path_endpoint = Some([request.goal[0], request.goal[1]]);
        actor.ai.ground_path = None;
        actor.ai.blocking.retry_at = None;
        if matches!(request.kind, Kind::Tighten) {
            let goal = self.path_position(id, destination.cell);
            self.entities[id].ai.path_endpoint = Some(goal);
            self.entities[id].ai.order = Order::Move(goal);
            self.entities[id].ai.command.move_to.position = [
                goal[0],
                goal[1],
                self.movement_layer_height(destination.layer, goal),
            ];
        }
        let factory_exit = self.entities[id].ai.waypoints.exit_quick_pending
            && matches!(self.entities[id].ai.order, Order::Exit(_));
        let waiting = matches!(request.kind, Kind::Move | Kind::Recovery)
            && self.entities[id]
                .ai
                .path_request
                .queue_at
                .is_some_and(|frame| frame > self.update_frame());
        self.install_ground_path(id);
        if let Some(endpoint) = self.entities[id]
            .ai
            .ground_path
            .as_ref()
            .and_then(|path| path.positions().last())
        {
            self.entities[id].ai.path_request.last_endpoint = Some(endpoint);
        }
        if factory_exit {
            self.move_allies_from_exit(id, [request.goal[0], request.goal[1]]);
        } else if !matches!(request.kind, Kind::Tighten)
            && (!matches!(request.kind, Kind::Attack(_))
                || self.entities[id].ai.path.blocked_by_ally)
            && !self.catalog[self.entities[id].type_index].no_collide()
        {
            self.move_allies(id);
        }
        if waiting {
            self.entities[id].ai.path_request.pending = Some(request);
        }
        if self.entities[id].ai.path_request.final_goal {
            let point = self.entities[id].ai.path.exact_endpoint.map_or_else(
                || self.path_position(id, destination.cell),
                |point| [point[0], point[1]],
            );
            self.reserve_goal(id, point);
        } else {
            self.remove_goal(id);
        }
    }
}
