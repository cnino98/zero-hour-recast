//! `AIInternalMoveToState` blocked requests and `AIUpdate::computePath` recovery.
use crate::grid::{SearchResult, patch};
use crate::skirmish::{Skirmish, startup};

impl Skirmish {
    pub(in crate::skirmish) fn object_approach_path_world(
        &mut self,
        id: usize,
        goal: [f32; 3],
    ) -> SearchResult {
        let position = self.entities[id].position;
        let mut result = self.movement_path_world(id, position, goal);
        if result.route.is_none() {
            let next = self.closest_queued_path(id, goal, 0.0);
            result.cells += next.cells;
            result.route = next.route;
        }
        result
    }

    pub(in crate::skirmish) fn recover_ground_path(&mut self, id: usize) -> bool {
        let actor = &self.entities[id];
        if actor.ai.blocking.temporary_goal().is_some() {
            return true;
        }
        if actor.ai.path_request.pending.is_some() {
            return false;
        }
        if let Some(frame) = actor.ai.blocking.retry_at {
            if self.tick < frame {
                return actor.ai.ground_path.is_some();
            }
        } else if !actor.ai.blocking.stuck && actor.ai.blocking.frames <= 60 {
            return true;
        }
        let goal = actor
            .ai
            .path_endpoint
            .or_else(|| actor.ai.path_goal.map(|cell| self.path_position(id, cell)));
        let Some(goal) = goal else {
            return true;
        };
        self.entities[id].ai.blocking.retry_at = None;
        self.request_ground_path(
            id,
            super::super::path_service::Request {
                goal: self.movement_goal_position(id, goal),
                range: 0.1,
                kind: super::super::path_service::Kind::Recovery,
            },
        );
        false
    }

    pub(in crate::skirmish) fn recovery_route_counted(
        &mut self,
        id: usize,
        requested: [f32; 3],
    ) -> SearchResult {
        if self.entities[id].ai.blocking.stuck && !self.entities[id].ai.path.raw.is_empty() {
            let result = self.search_patch_route(id);
            self.finish_path_search(result)
        } else {
            self.object_approach_path_world(id, requested)
        }
    }

    fn search_patch_route(&self, id: usize) -> SearchResult {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let Some(start) = self.cell_at(actor.position) else {
            return SearchResult::empty();
        };
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let units = self.route_units(id);
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));

        let indexed = patch_start_position(actor.position, centered);
        let Some(indexed_start) = self.cell_at(indexed) else {
            return SearchResult::empty();
        };
        if navigation.has_bridge_layers() {
            let (units, _) = self.layer_route_context(id);
            return navigation.patch_layers_counted(
                &crate::grid::LayerPatch {
                    start: crate::grid::LayerCell {
                        cell: start,
                        layer: actor.layer,
                    },
                    indexed_start: crate::grid::LayerCell {
                        cell: indexed_start,
                        layer: actor.layer,
                    },
                    position: actor.position,
                    radius,
                    centered,
                    raw: &actor.ai.path.raw,
                    blocked: true,
                },
                &units,
            );
        }
        let raw: Vec<_> = actor.ai.path.raw.iter().map(|node| node.cell).collect();
        let query = patch::Patch {
            start,
            indexed_start,
            position: actor.position,
            radius,
            centered,
            raw: &raw,
            blocked: true,
        };
        patch::find_counted(navigation, &units, &query)
    }
}

const fn patch_start_position(position: [f32; 2], centered: bool) -> [f32; 2] {
    let mut indexed = position;
    if !centered {
        indexed[0] += 5.0;
        indexed[0] += 5.0;
    }
    indexed
}
