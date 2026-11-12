//! Chinook landing: default `PartitionManager` finder then `checkForLanding` spiral.
use super::Skirmish;
use crate::skirmish::{CELL_SIZE, startup};
use crate::{Cell, terrain::CellType};

impl Skirmish {
    pub(in crate::skirmish::chinook) fn adjust_chinook_landing(
        &self,
        id: usize,
        point: [f32; 2],
    ) -> Option<[f32; 2]> {
        let geometry = self.catalog[self.entities[id].type_index].footprint(point, 0.0);
        let (radius, centered) = startup::radius_and_center(&geometry);
        let offset = if centered { 0.0 } else { CELL_SIZE * 0.5 };
        let start = self.cell_at([point[0] + offset, point[1] + offset])?;
        let mut x = i32::from(start.x);
        let mut y = i32::from(start.y);
        if let Some(point) = self.landing_cell(x, y, radius, centered) {
            return Some(point);
        }
        let mut remaining = 400;
        let mut segment = 1;
        while remaining > 0 {
            for (direction, [dx, dy]) in [[1, 0], [0, 1], [-1, 0], [0, -1]].into_iter().enumerate()
            {
                for _ in 0..segment {
                    x += dx;
                    y += dy;
                    remaining -= 1;
                    if let Some(point) = self.landing_cell(x, y, radius, centered) {
                        return Some(point);
                    }
                }
                if direction % 2 == 1 {
                    segment += 1;
                }
            }
        }
        None
    }

    fn landing_cell(&self, x: i32, y: i32, radius: i32, centered: bool) -> Option<[f32; 2]> {
        let center = Cell {
            x: u16::try_from(x).ok()?,
            y: u16::try_from(y).ok()?,
        };
        if matches!(
            self.movement_terrain.cell(center)?.kind,
            CellType::Cliff | CellType::Water | CellType::Impassable
        ) {
            return None;
        }
        for column in x - radius..x + radius + i32::from(centered) {
            for row in y - radius..y + radius + i32::from(centered) {
                let cell = Cell {
                    x: u16::try_from(column).ok()?,
                    y: u16::try_from(row).ok()?,
                };
                // checkDestination(nullptr) tests impassable/obstacle cells;
                // its caller excludes water/cliffs only at the center.
                if matches!(
                    self.movement_terrain.cell(cell)?.kind,
                    CellType::Impassable | CellType::Obstacle | CellType::BridgeImpassable
                ) || self.navigation.is_obstacle(cell)
                    || self.occupancy.landing_claimed(cell)
                {
                    return None;
                }
            }
        }
        let offset = if centered { 0.5 } else { 0.05 };
        Some([
            (f32::from(center.x) + offset) * CELL_SIZE,
            (f32::from(center.y) + offset) * CELL_SIZE,
        ])
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::containment::tests::setup;
    use crate::{Cell, terrain::TerrainMap};

    #[test]
    fn landing_rejects_outer_buildings_and_claims_but_only_center_water() {
        let (mut game, _) = setup(0.0, 1.0);
        let outer = Cell { x: 51, y: 50 };
        game.movement_terrain =
            TerrainMap::classify(game.grid.width(), game.grid.height(), [], [outer]).unwrap();
        assert!(game.landing_cell(50, 50, 1, true).is_some());
        assert!(game.landing_cell(51, 50, 1, true).is_none());
        game.navigation = game.grid.with_obstacles([outer]);
        assert!(game.landing_cell(50, 50, 1, true).is_none());
        game.navigation = game.grid.clone();
        let rider = game.spawn(5, 0, [505.0, 505.0], true);
        game.reserve_goal(rider, [505.0, 505.0]);
        assert!(game.landing_cell(50, 50, 1, true).is_none());
    }
}
