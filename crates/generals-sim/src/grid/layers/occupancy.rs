//! Immutable per-search occupancy bits, as in the existing ground search.
use super::{BTreeMap, Grid, LayerCell, RouteUnits};

#[cfg(test)]
mod tests;

pub(super) const FIXED: u8 = 1;
pub(super) const MOVING: u8 = 2;
pub(super) const ENEMY: u8 = 4;

pub(super) fn build(grid: &Grid, units: &BTreeMap<u8, RouteUnits>) -> Vec<Option<Vec<u8>>> {
    let occupied = |units: &RouteUnits| {
        !units.fixed_allies.is_empty()
            || !units.moving_allies.is_empty()
            || !units.fixed_enemies.is_empty()
    };
    let Some((&last, _)) = units.iter().rev().find(|(_, units)| occupied(units)) else {
        return Vec::new();
    };
    let mut layers: Vec<_> = (0..=last).map(|_| None).collect();
    for (&layer, units) in units.iter().filter(|(_, units)| occupied(units)) {
        let mut flags = vec![0; usize::from(grid.width) * usize::from(grid.height)];
        for (cells, mask) in [
            (&units.fixed_allies, FIXED),
            (&units.moving_allies, MOVING),
            (&units.fixed_enemies, ENEMY),
        ] {
            for &cell in cells {
                if grid.contains(cell) {
                    flags[grid.source_index(cell)] |= mask;
                }
            }
        }
        layers[usize::from(layer)] = Some(flags);
    }
    layers
}

// Initial movement screening makes one bounded query and never allocates a
// search snapshot. Keep its existing cell lookup and short-circuit predicates.
pub(super) fn screen(
    grid: &Grid,
    units: &BTreeMap<u8, RouteUnits>,
    node: LayerCell,
    radius: i32,
    centered: bool,
) -> (bool, bool, bool) {
    let mut flags = (false, false, false);
    let above = radius + i32::from(centered);
    for x in i32::from(node.cell.x) - radius..i32::from(node.cell.x) + above {
        for y in i32::from(node.cell.y) - radius..i32::from(node.cell.y) + above {
            if let Some(cell) = grid.signed_cell(x, y) {
                let node = grid.resolve_layer(LayerCell {
                    cell,
                    layer: node.layer,
                });
                let Some(units) = units.get(&node.layer) else {
                    continue;
                };
                flags.0 |= units.fixed_allies.contains(&cell);
                flags.1 |= units.moving_allies.contains(&cell);
                flags.2 |= units.fixed_enemies.contains(&cell);
            }
        }
    }
    flags
}
