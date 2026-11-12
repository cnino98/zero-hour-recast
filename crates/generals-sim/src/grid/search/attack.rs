//! Ground `Pathfinder::findAttackPath`, with the original bounded search.

use super::{Cell, Grid, Reverse, Route, RouteUnits, Search, cell_position, squared_distance};
use std::collections::BTreeSet;

pub struct Attack<'a> {
    pub from: [f32; 3],
    pub target: [f32; 2],
    pub range_targets: Option<[[f32; 2]; 2]>,
    pub radius: i32,
    pub centered: bool,
    pub weapon_range: [f32; 2],
    pub geometry_radius: f32,
    pub distance: f32,
    pub vehicle: bool,
    pub unavailable: &'a BTreeSet<Cell>,
}

pub struct AttackPoint {
    pub layer: u8,
    pub position: [f32; 2],
    pub on_grid: bool,
}

impl Grid {
    pub(crate) fn quick_attack_path(
        &self,
        attack: &Attack<'_>,
        mut view: impl FnMut(AttackPoint) -> bool,
    ) -> Option<Route> {
        quick(self, attack, &mut view)
    }

    pub(crate) fn attack_search_counted(
        &self,
        attack: &Attack<'_>,
        units: &RouteUnits,
        mut view: impl FnMut(AttackPoint) -> bool,
    ) -> super::SearchResult {
        let shift = if attack.centered { 5.0 } else { 0.0 };
        let position = [attack.from[0] + shift, attack.from[1] + shift];
        let Some(start) = world_cell(self, position.map(|value| value.max(0.0)), true) else {
            return super::SearchResult::empty();
        };
        let mut search = Search::new(
            self,
            units,
            start,
            start,
            attack.radius,
            attack.centered,
            false,
        );
        if !search.insert(start, None, 0, 0) {
            return super::SearchResult::empty();
        }
        let goal = world_cell(self, attack.target, false);
        if goal.is_none_or(|goal| !search.reserve_info(goal)) {
            // Native has reset the start INFO, but has not linked its open list.
            return super::SearchResult {
                route: None,
                cells: 0,
                info_cleanup: Vec::new(),
                retained_infos: search.info_cleanup(),
            };
        }
        let goal = goal.expect("allocated native attack goal");
        search.goal = goal;
        let distance = truncate(f64::from(attack.distance)) + 30;
        search.attack_cost = Some((distance, if attack.vehicle { 30 } else { 10 }));
        let mut allocations = 0;
        while let Some(Reverse((_, insertion, cell))) = search.open.pop() {
            if search.node(cell).insertion != insertion {
                continue;
            }
            let coordinate = search.coordinate(cell);
            let point = cell_position(coordinate, attack.centered);
            if cell != start
                && squared_distance(point, position) >= 25.0
                && attack.in_range(point)
                && attack.destination(self, coordinate)
                && view(AttackPoint {
                    layer: 1,
                    position: point,
                    on_grid: true,
                })
            {
                let endpoint = if attack.vehicle {
                    search.strip_attack_tail(cell)
                } else {
                    cell
                };
                search.listed -= 1;
                return search.result(Some(search.path(endpoint)));
            }
            // Native search stops expanding at 2500 new infos, then still
            // checks every node already on the open list for a valid goal.
            if allocations < 2500 {
                allocations += search.expand_neighbors_toward(cell, Some(goal));
            }
        }
        search.result(None)
    }
}

impl Attack<'_> {
    fn destination(&self, grid: &Grid, cell: Cell) -> bool {
        grid.destination_clear(cell, self.radius, self.centered)
            && !self.unavailable.contains(&cell)
    }

    pub(in crate::grid) fn in_range(&self, point: [f32; 2]) -> bool {
        // PartitionManager::distCalcProc_BoundaryAndBoundary_2D performs the
        // sqrt/subtraction/square, rather than enlarging a center-distance test.
        let minimum = self.weapon_range[0] + 2.5;
        let maximum = self.weapon_range[1] - 2.5;
        let maximum_squared = maximum * maximum;
        let distance = |target| {
            let center_squared = squared_distance(point, target);
            if self.geometry_radius > 0.0 {
                let separation = (center_squared.sqrt() - self.geometry_radius).max(0.0);
                separation * separation
            } else {
                center_squared
            }
        };
        let distance_squared = self.range_targets.map_or_else(
            || distance(self.target),
            |targets| {
                let first = distance(targets[0]);
                if first > maximum_squared {
                    distance(targets[1])
                } else {
                    first
                }
            },
        );
        distance_squared >= minimum * minimum && distance_squared <= maximum_squared
    }
}

impl Search<'_> {
    fn strip_attack_tail(&self, end: Cell) -> Cell {
        let mut current = Some(end);
        let mut last_blocked = None;
        let mut large = false;
        for _ in 0..12 {
            let Some(cell) = current else { break };
            let (radius, centered) = if large {
                (self.radius, self.centered)
            } else {
                (0, true)
            };
            let occupied = self.occupied_at(cell, radius, centered);
            let ally = (occupied.fixed || self.goal_occupied(cell, radius, centered))
                && !self.units.idle_positions.contains(&cell);
            let above = radius + i32::from(centered);
            let inside = i32::from(cell.x) >= radius
                && i32::from(cell.y) >= radius
                && i32::from(cell.x) + above <= i32::from(self.grid.width)
                && i32::from(cell.y) + above <= i32::from(self.grid.height);
            large = !inside || occupied.enemy || ally;
            if large {
                last_blocked = Some(cell);
            }
            current = self.node(cell).parent;
        }
        last_blocked.map_or(end, |cell| self.node(cell).parent.unwrap_or(cell))
    }
}

fn quick(
    grid: &Grid,
    attack: &Attack<'_>,
    view: &mut impl FnMut(AttackPoint) -> bool,
) -> Option<Route> {
    let delta = [
        attack.target[0] - attack.from[0],
        attack.target[1] - attack.from[1],
    ];
    let length = squared_distance(attack.target, [attack.from[0], attack.from[1]]).sqrt();
    let step = if length > 0.0 {
        delta.map(|value| value / length * 10.0)
    } else {
        [0.0; 2]
    };
    for index in 1..10_u16 {
        let offset = step.map(|value| value * f32::from(index) * 0.5);
        let point = [attack.from[0] + offset[0], attack.from[1] + offset[1]];
        let cell = world_cell(grid, point, false)?;
        if grid.is_blocked(cell) || !attack.destination(grid, cell) {
            break;
        }
        if attack.in_range(point)
            && view(AttackPoint {
                layer: 1,
                position: point,
                on_grid: false,
            })
        {
            let mut route = Route::direct(
                world_cell(grid, [attack.from[0], attack.from[1]], false)?,
                cell,
            );
            route.exact_endpoint = Some([point[0], point[1], attack.from[2]]);
            route.quick_start = Some(attack.from);
            return Some(route);
        }
    }
    None
}

pub(in crate::grid) fn remaining(from: Cell, goal: Cell, distance: u32) -> u32 {
    let dx = u64::from(from.x.abs_diff(goal.x));
    let dy = u64::from(from.y.abs_diff(goal.y));
    // floor(10 * sqrt(integer squared distance)); the integer square root
    // preserves this native integral heuristic without a lossy Rust cast.
    let radial = u32::try_from(((dx * dx + dy * dy) * 100).isqrt()).expect("bounded cell distance");
    radial.saturating_sub(distance / 2)
}

pub(in crate::grid) fn world_cell(grid: &Grid, point: [f32; 2], clipped: bool) -> Option<Cell> {
    let axis = |value: f32, extent: u16| {
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        let coordinate = value / 10.0;
        if coordinate >= f32::from(extent) {
            return clipped.then_some(extent - 1);
        }
        u16::try_from(truncate(f64::from(coordinate))).ok()
    };
    Some(Cell {
        x: axis(point[0], grid.width)?,
        y: axis(point[1], grid.height)?,
    })
}

pub(in crate::grid) fn truncate(value: f64) -> u32 {
    let (mut low, mut high) = (0_u32, u32::MAX);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if f64::from(middle) <= value {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

#[cfg(test)]
mod tests;
