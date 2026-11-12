//! Hierarchical queries retain authored XYZ and commit INFO before ground A*.
mod clip;

use crate::grid::{Cell, HierarchyQuery, HierarchyResult, LayerCell};
use crate::skirmish::{CELL_SIZE, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn hierarchy_query(
        &self,
        id: usize,
        from: [f32; 3],
        to: [f32; 3],
    ) -> HierarchyQuery {
        let layer = |position: [f32; 3]| {
            self.movement_terrain
                .get_layer_for_destination(position, self.ground_height([position[0], position[1]]))
        };
        let (clipped_from, clipped_to) = self.clip_path_coordinates(from, to);
        HierarchyQuery {
            start: LayerCell {
                cell: self.path_cell(clipped_from),
                layer: layer(from),
            },
            goal: LayerCell {
                cell: self.path_cell(clipped_to),
                layer: layer(clipped_to),
            },
            surfaces: self.entities[id]
                .locomotor_surfaces(&self.catalog[self.entities[id].type_index]),
            crusher: false,
            closest: false,
            logical_bounds: None,
            goal_is_origin: to[0].abs() <= 0.0 && to[1].abs() <= 0.0,
            start_expansion: [
                self.path_cell([from[0] - 100.0, from[1] - 100.0, from[2]]),
                self.path_cell([from[0] + 100.0, from[1] + 100.0, from[2]]),
            ],
        }
    }

    /// Ordinary/closest A* shifts non-centered destinations before resolving
    /// their layer. Hierarchy uses the original unshifted XYZ separately.
    pub(in crate::skirmish) fn path_query(
        &self,
        id: usize,
        from: [f32; 3],
        target: [f32; 3],
        closest_cost: Option<f32>,
    ) -> Option<crate::grid::LayerQuery> {
        let unit = &self.catalog[self.entities[id].type_index];
        let (radius, centered) =
            super::super::startup::radius_and_center(&unit.footprint([from[0], from[1]], 0.0));
        let offset = if centered { 0.0 } else { CELL_SIZE * 0.5 };
        let (from, target) = if closest_cost.is_some() {
            self.clip_path_coordinates(from, [target[0] + offset, target[1] + offset, target[2]])
        } else {
            let (from, target) = self.clip_path_coordinates(from, target);
            (from, [target[0] + offset, target[1] + offset, target[2]])
        };
        let goal = if closest_cost.is_some() {
            self.path_cell(target)
        } else {
            self.cell_at([target[0], target[1]])?
        };
        let layer = self
            .movement_terrain
            .get_layer_for_destination(target, self.ground_height([target[0], target[1]]));
        Some(crate::grid::LayerQuery {
            start: LayerCell {
                cell: self.path_cell(from),
                layer: self.entities[id].layer,
            },
            goal: LayerCell { cell: goal, layer },
            radius,
            centered,
            closest_cost,
        })
    }

    pub(in crate::skirmish) fn clip_path_coordinates(
        &self,
        from: [f32; 3],
        to: [f32; 3],
    ) -> ([f32; 3], [f32; 3]) {
        clip::world_line(
            from,
            to,
            Cell {
                x: self.grid.width(),
                y: self.grid.height(),
            },
        )
    }

    pub(in crate::skirmish) fn path_cell(&self, position: [f32; 3]) -> Cell {
        let clamp = |value: f32, limit: u16| {
            let value = (value / CELL_SIZE).clamp(0.0, f32::from(limit - 1));
            super::grid_coordinate(value, limit).expect("finite native world coordinate")
        };
        Cell {
            x: clamp(position[0], self.grid.width()),
            y: clamp(position[1], self.grid.height()),
        }
    }

    pub(in crate::skirmish) fn hierarchical_path(
        &mut self,
        query: &HierarchyQuery,
    ) -> HierarchyResult {
        let units = self.layer_path_info();
        let mut result = self.navigation.hierarchical_corridor(query, &units);
        drop(units);
        result.search = self.finish_path_search(result.search);
        result
    }
}
