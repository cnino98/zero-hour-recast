//! `checkDestination` and `snapClosestGoalPosition`, SH AIPathfind.cpp5009–5294.
use super::{CELL_SIZE, Cell, LayerCell, Skirmish, grid_coordinate, startup};

impl Skirmish {
    pub(in crate::skirmish) fn destination_available(&self, id: usize, center: LayerCell) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let above = radius + i32::from(centered);
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        for x in i32::from(center.cell.x) - radius..i32::from(center.cell.x) + above {
            for y in i32::from(center.cell.y) - radius..i32::from(center.cell.y) + above {
                let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                    return false;
                };
                let cell = Cell { x, y };
                if !navigation.contains(cell) {
                    return false;
                }
                let node = navigation.resolve_layer(LayerCell {
                    cell,
                    layer: center.layer,
                });
                if self.aircraft_goal(id) {
                    if self
                        .occupancy
                        .aircraft_goals
                        .get(&cell)
                        .is_some_and(|&owner| owner != id)
                    {
                        return false;
                    }
                    continue;
                }
                match navigation.source_cell_kind(node) {
                    Some(crate::terrain::CellType::Obstacle) => {
                        if navigation.ignored_obstacle(node) {
                            continue;
                        }
                        return false;
                    }
                    None
                    | Some(
                        crate::terrain::CellType::Impassable
                        | crate::terrain::CellType::BridgeImpassable,
                    ) => return false,
                    Some(_) => {}
                }
                let Some(&owner) = self.occupancy.goals.get(&node) else {
                    continue;
                };
                if owner == id
                    || Some(owner) == actor.ai.ignored_producer
                    || !self.entities[owner].exists()
                {
                    continue;
                }
                if self.object_relationship(id, owner) == 2
                    || (self.occupancy.positions.get(&node) == Some(&owner)
                        && !self.can_crush(id, owner))
                {
                    return false;
                }
            }
        }
        true
    }

    pub(in crate::skirmish) fn snap_closest_goal(&self, id: usize, original: [f32; 3]) -> [f32; 3] {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let offset = if centered { 0.0 } else { CELL_SIZE * 0.5 };
        let center = self.clipped_path_cell([original[0] + offset, original[1] + offset]);
        let layer = self
            .movement_terrain
            .get_layer_for_destination(original, self.ground_height([original[0], original[1]]));
        let first = self.destination_cell_position(center, centered, 1);
        if self.destination_available(
            id,
            LayerCell {
                cell: center,
                layer,
            },
        ) {
            return first;
        }
        let neighbors = || {
            (i32::from(center.x) - 1..i32::from(center.x) + 2).flat_map(|x| {
                (i32::from(center.y) - 1..i32::from(center.y) + 2).filter_map(move |y| {
                    let cell = Cell {
                        x: u16::try_from(x).ok()?,
                        y: u16::try_from(y).ok()?,
                    };
                    self.grid.contains(cell).then_some(cell)
                })
            })
        };
        for cell in neighbors() {
            if self.destination_available(id, LayerCell { cell, layer }) {
                return self.destination_cell_position(cell, centered, layer);
            }
        }
        if radius > 0 {
            return first;
        }
        for cell in neighbors() {
            let node = self.navigation.resolve_layer(LayerCell { cell, layer });
            if self
                .occupancy
                .goals
                .get(&node)
                .is_none_or(|&owner| owner == id)
            {
                return self.destination_cell_position(cell, centered, layer);
            }
        }
        for cell in neighbors() {
            let node = self.navigation.resolve_layer(LayerCell { cell, layer });
            let fixed = self
                .occupancy
                .positions
                .get(&node)
                .is_some_and(|owner| self.occupancy.goals.get(&node) == Some(owner));
            if !fixed {
                return self.destination_cell_position(cell, centered, layer);
            }
        }
        first
    }

    pub(in crate::skirmish) fn clipped_path_cell(&self, point: [f32; 2]) -> Cell {
        let axis = |value: f32, limit: u16| {
            grid_coordinate((value / CELL_SIZE).clamp(0.0, f32::from(limit - 1)), limit)
                .expect("finite movement coordinate and nonempty grid")
        };
        Cell {
            x: axis(point[0], self.grid.width()),
            y: axis(point[1], self.grid.height()),
        }
    }

    fn destination_cell_position(&self, cell: Cell, centered: bool, layer: u8) -> [f32; 3] {
        let point = super::super::movement::native_cell_position(cell, centered);
        let layer = self
            .navigation
            .resolve_layer(LayerCell { cell, layer })
            .layer;
        [point[0], point[1], self.movement_layer_height(layer, point)]
    }
}
