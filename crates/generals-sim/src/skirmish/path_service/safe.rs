//! `AIUpdate::requestSafePath/doPathfind` retains the two most recent repulsors.
use super::{Kind, Request};
use crate::grid::{LayerCell, LayerQuery};
use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn request_safe_path(&mut self, id: usize, repulsor: usize) {
        let state = &mut self.entities[id].ai.path_request;
        if state.repulsors[0] != Some(repulsor) {
            state.repulsors[1] = state.repulsors[0];
        }
        state.repulsors[0] = Some(repulsor);
        state.final_goal = false;
        state.attack_path = false;
        state.failed = false;
        state.pending = Some(Request {
            goal: [0.0; 3],
            range: 0.0,
            kind: Kind::Safe,
        });
        if self.update_frame().wrapping_sub(3)
            < super::super::ai_dispatch::native_frame(self.entities[id].ai.blocking.path_frame)
        {
            self.set_queue_for_path_time(id, 60);
        } else if !self.path_service.queue(id) {
            eprintln!("Original pathfinder queue exhausted: 511 pending objects");
        }
    }

    pub(super) fn compute_queued_safe_path(&mut self, id: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let source = actor.position;
        let (radius, centered) =
            crate::skirmish::startup::radius_and_center(&unit.footprint(source, 0.0));
        let repulsors = actor.ai.path_request.repulsors;
        let first = repulsors[0]
            .filter(|&other| self.entities[other].exists())
            .map_or([-1000.0; 2], |other| self.entities[other].position);
        let second = repulsors[1]
            .filter(|&other| self.entities[other].exists())
            .map_or(first, |other| self.entities[other].position);
        let start = LayerCell {
            cell: self.clipped_path_cell(source),
            layer: actor.layer,
        };
        let query = LayerQuery {
            start,
            goal: start,
            radius,
            centered,
            closest_cost: None,
        };
        let safe_range = actor.vision_range(unit) + self.ai_rules.repulsed_distance;
        let (units, unavailable) = self.layer_route_context(id);
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let result =
            navigation.safe_path_counted(&query, [first, second], safe_range, &units, &unavailable);
        drop(units);
        let result = self.finish_path_search(result);
        self.path_service.cumulative_cells += result.cells;
        let actor = &mut self.entities[id];
        actor.ai.path.clear();
        actor.ai.ground_path = None;
        actor.ai.path_goal = None;
        actor.ai.path_endpoint = None;
        actor.ai.path_request.attack_path = false;
        actor.ai.motion.clear_goal();
        actor.ai.path_request.failed = result.route.is_none();
        if let Some(mut route) = result.route {
            let end = *route
                .optimized
                .back()
                .expect("successful safe path endpoint");
            route.optimized.pop_front();
            if route.optimized.is_empty() {
                route.optimized.push_back(end);
            }
            let point = self.path_position(id, end.cell);
            let actor = &mut self.entities[id];
            actor.ai.path = route;
            actor.ai.path_goal = Some(end.cell);
            actor.ai.path_endpoint = Some(point);
            self.install_ground_path(id);
        }
    }
}
