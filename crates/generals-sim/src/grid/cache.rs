//! Ordered `PathfindZoneManager` classification and deferred cache rebuilding.
use std::sync::Arc;
#[cfg(test)]
mod tests;

use super::{Cell, Grid, LayerCell, ZoneModification, hierarchy::zones::Zones};
use crate::terrain::CellType;

impl Grid {
    pub(super) fn source_index(&self, cell: Cell) -> usize {
        usize::from(cell.y) * usize::from(self.width) + usize::from(cell.x)
    }

    pub(super) fn hierarchy_kind(&self, cell: Cell) -> u8 {
        match self.source_kinds[self.source_index(cell)] {
            CellType::Clear => 0,
            CellType::Water => 1,
            CellType::Cliff => 2,
            CellType::Rubble => 3,
            CellType::Obstacle => 4,
            CellType::BridgeImpassable => 5,
            CellType::Impassable => 6,
        }
    }

    pub(super) fn hierarchy_fence(&self, cell: Cell) -> bool {
        self.fences.contains(&LayerCell { cell, layer: 1 })
    }

    pub(crate) fn recalculate_source_zones(&mut self) {
        self.source_zones = Some(Arc::new(Zones::calculate(self)));
    }

    /// Replay type changes against the previous cache in source notification
    /// order. Partial floods precede expanded footprint clearing and pinching.
    pub(crate) fn inherit_source_zones(
        &mut self,
        previous: &Self,
        modifications: &[ZoneModification],
    ) -> bool {
        let mut replay = previous.clone();
        let mut zones = replay.source_zones.take().expect("classified zone cache");
        let mut dirty = self.bridge_ends != previous.bridge_ends;
        for event in modifications {
            for change in &event.cells {
                replay.apply_source_change(change);
                if change.reset_zone {
                    Arc::make_mut(&mut zones).invalidate_cells(&replay, [change.cell]);
                }
            }
            if let Some(bounds) = event.update_bounds {
                Arc::make_mut(&mut zones).update_for_modify(&replay, bounds);
            }
            for change in &event.post_cells {
                replay.apply_source_change(change);
            }
            for &(cell, pinched) in &event.pinched {
                if pinched {
                    Arc::make_mut(&mut replay.pinched).insert(cell);
                } else {
                    Arc::make_mut(&mut replay.pinched).remove(&cell);
                }
            }
            dirty |= event.force_dirty;
        }
        self.source_zones = Some(zones);
        dirty
    }

    fn apply_source_change(&mut self, change: &super::ZoneCellChange) {
        let index = self.source_index(change.cell);
        Arc::make_mut(&mut self.source_kinds)[index] = change.kind;
        let node = LayerCell {
            cell: change.cell,
            layer: 1,
        };
        if change.fence {
            Arc::make_mut(&mut self.fences).insert(node);
        } else {
            Arc::make_mut(&mut self.fences).remove(&node);
        }
    }

    pub(super) fn source_zone(&self, node: LayerCell) -> u32 {
        let zones = self.source_zones.as_ref().expect("classified zone cache");
        let node = self.resolve_layer(node);
        if node.layer == 1 {
            zones.raw(self, node.cell)
        } else {
            zones.upper(node.layer)
        }
    }

    pub(super) fn effective_source_zone(&self, surfaces: u8, crusher: bool, zone: u32) -> u32 {
        self.source_zones
            .as_ref()
            .expect("classified zone cache")
            .effective_global(surfaces, crusher, zone)
    }

    pub(super) fn source_block_zone(&self, surfaces: u8, crusher: bool, cell: Cell) -> u32 {
        self.source_zones
            .as_ref()
            .expect("classified zone cache")
            .effective_block(self, surfaces, crusher, cell)
    }

    pub(super) fn source_layer_zone(&self, layer: u8) -> u32 {
        self.source_zones
            .as_ref()
            .expect("classified zone cache")
            .upper(layer)
    }

    pub(super) fn source_terrain_zone(&self, zone: u32) -> u32 {
        self.source_zones
            .as_ref()
            .expect("classified zone cache")
            .terrain(zone)
    }

    pub(super) fn bridge_block_interacts(&self, cell: Cell) -> bool {
        self.source_zones
            .as_ref()
            .expect("classified zone cache")
            .interacts_with_bridge(Cell {
                x: cell.x / 10,
                y: cell.y / 10,
            })
    }

    pub(super) fn bridge_endpoints(&self, layer: u8) -> Option<[Cell; 2]> {
        self.bridge_ends.get(&layer).copied()
    }
    pub(super) fn active_bridge_endpoints(&self) -> impl Iterator<Item = (u8, [Cell; 2])> + '_ {
        self.bridge_ends.iter().map(|(&layer, &ends)| (layer, ends))
    }
    pub(super) fn is_pinched(&self, cell: Cell) -> bool {
        self.pinched.contains(&cell)
    }
    pub(super) const fn block_extent(&self) -> Cell {
        Cell {
            x: self.width.div_ceil(10),
            y: self.height.div_ceil(10),
        }
    }
}
