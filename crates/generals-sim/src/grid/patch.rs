//! Ground-layer `Pathfinder::patchPath`: rejoin the unblocked old raw tail.
use super::{Cell, Grid, RouteUnits, search};

pub struct Patch<'a> {
    pub start: Cell,
    pub indexed_start: Cell,
    pub position: [f32; 2],
    pub radius: i32,
    pub centered: bool,
    pub raw: &'a [Cell],
    pub blocked: bool,
}

pub fn find_counted(grid: &Grid, units: &RouteUnits, query: &Patch<'_>) -> super::SearchResult {
    search::patch_counted(grid, units, query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_rejoins_the_old_tail_instead_of_selecting_a_new_goal() {
        let grid = Grid::new(30, 20, []).unwrap();
        let raw: Vec<_> = (5..=20).map(|x| Cell { x, y: 10 }).collect();
        let mut units = RouteUnits::new();
        units.fixed_allies.insert(Cell { x: 8, y: 10 });
        let query = Patch {
            start: raw[0],
            indexed_start: raw[0],
            position: [55.0, 105.0],
            radius: 0,
            centered: true,
            raw: &raw,
            blocked: true,
        };
        let path = find_counted(&grid, &units, &query)
            .route
            .expect("a clear old tail remains");
        assert!(path.raw.last().map(|node| node.cell).as_ref() == raw.last());
        assert!(
            path.raw
                .iter()
                .map(|node| node.cell)
                .collect::<Vec<_>>()
                .ends_with(&raw[5..])
        );
        assert!(path.raw.iter().any(|cell| cell.cell.y != 10));
        assert!(
            !path
                .raw
                .iter()
                .any(|node| node.cell == Cell { x: 8, y: 10 })
        );
    }

    #[test]
    fn occupied_final_tail_is_not_replaced_by_a_closest_destination() {
        let grid = Grid::new(30, 20, []).unwrap();
        let raw = [Cell { x: 5, y: 10 }, Cell { x: 20, y: 10 }];
        let mut units = RouteUnits::new();
        units.fixed_enemies.insert(raw[1]);
        let query = Patch {
            start: raw[0],
            indexed_start: raw[0],
            position: [55.0, 105.0],
            radius: 0,
            centered: true,
            raw: &raw,
            blocked: true,
        };
        assert!(find_counted(&grid, &units, &query).route.is_none());
    }

    #[test]
    fn near_start_transient_occupants_limit_the_rejoin_tail() {
        let grid = Grid::new(20, 20, []).unwrap();
        let raw = [Cell { x: 5, y: 5 }, Cell { x: 6, y: 5 }];
        let mut units = RouteUnits::new();
        units.transient_enemies.insert(raw[1]);
        let mut query = Patch {
            start: raw[0],
            indexed_start: raw[0],
            position: [55.0, 55.0],
            radius: 0,
            centered: true,
            raw: &raw,
            blocked: true,
        };
        assert!(find_counted(&grid, &units, &query).route.is_none());
        query.blocked = false;
        assert!(find_counted(&grid, &units, &query).route.is_some());
    }

    #[test]
    fn exhausted_patch_budget_does_not_fall_back_to_a_complete_new_search() {
        let grid = Grid::new(160, 160, []).unwrap();
        let raw = [Cell { x: 5, y: 5 }, Cell { x: 150, y: 150 }];
        let query = Patch {
            start: raw[0],
            indexed_start: raw[0],
            position: [55.0, 55.0],
            radius: 0,
            centered: true,
            raw: &raw,
            blocked: true,
        };
        assert!(
            find_counted(&grid, &RouteUnits::new(), &query)
                .route
                .is_none()
        );
        assert!(grid.path(raw[0], raw[1]).is_some());
    }
    #[test]
    fn preallocated_unit_cells_do_not_spend_the_native_patch_allocation_budget() {
        let grid = Grid::new(70, 70, []).unwrap();
        let raw = [Cell { x: 2, y: 2 }, Cell { x: 65, y: 65 }];
        let query = Patch {
            start: raw[0],
            indexed_start: raw[0],
            position: [25.0, 25.0],
            radius: 0,
            centered: true,
            raw: &raw,
            blocked: true,
        };
        let mut units = RouteUnits::new();
        assert!(find_counted(&grid, &units, &query).route.is_none());
        units
            .info_cells
            .extend((0..70).flat_map(|x| (0..70).map(move |y| Cell { x, y })));
        assert!(find_counted(&grid, &units, &query).route.is_some());
    }
}
