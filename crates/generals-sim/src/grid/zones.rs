//! Ground-only effective zone equivalence from `PathfindZoneManager`.
//! Zone numbers are private; queries retain only the source equality relation.

use super::{Cell, Grid};

impl Grid {
    /// Native build-preview reachability ignores structures and accepts zones
    /// still uninitialized between footprint modification and recalculation.
    pub(crate) fn quick_ui_path_exists(
        &self,
        from: super::LayerCell,
        to: super::LayerCell,
        surfaces: u8,
    ) -> bool {
        if !self.contains(from.cell) || !self.contains(to.cell) {
            return false;
        }
        let to = self.resolve_layer(to);
        if to.layer == 1 && self.hierarchy_kind(to.cell) == 2 {
            return false;
        }
        let raw = [from, to].map(|node| self.source_zone(node));
        let effective = raw.map(|zone| self.effective_source_zone(surfaces, false, zone));
        if effective.contains(&0) {
            return true;
        }
        let terrain = raw.map(|zone| {
            let zone = self.source_terrain_zone(zone);
            let zone = self.effective_source_zone(surfaces, false, zone);
            self.source_terrain_zone(zone)
        });
        terrain[0] == 0 || terrain[0] == terrain[1]
    }

    /// `clientSafeQuickDoesPathExist` uses the selected locomotor set's surfaces.
    pub(crate) fn quick_path_exists(&self, surfaces: u8, from: Cell, to: Cell) -> bool {
        self.quick_path_exists_layers(
            surfaces,
            super::LayerCell {
                cell: from,
                layer: 1,
            },
            super::LayerCell { cell: to, layer: 1 },
        )
    }

    pub(crate) fn quick_path_exists_layers(
        &self,
        surfaces: u8,
        from: super::LayerCell,
        to: super::LayerCell,
    ) -> bool {
        if !self.contains(from.cell) || !self.contains(to.cell) {
            return false;
        }
        let from = self.resolve_layer(from);
        let to = self.resolve_layer(to);
        let kind = |node: super::LayerCell| {
            if node.layer == 1 {
                self.hierarchy_kind(node.cell)
            } else if self.layer_obstacle(node) {
                4
            } else if self.layer_blocked(node) {
                5
            } else {
                0
            }
        };
        let destination_kind = kind(to);
        let acceptable = match destination_kind {
            0 => 1 | 8,
            1 => 2 | 8,
            2 => 4 | 8,
            3 => 16 | 8,
            _ => 8,
        };
        if destination_kind == 2
            || (!self.ignored_obstacles.contains(&to) && acceptable & surfaces == 0)
        {
            return false;
        }
        let raw = [from, to].map(|node| self.source_zone(node));
        let effective = raw.map(|zone| self.effective_source_zone(surfaces, false, zone));
        let from_obstacle = kind(from) == 4;
        // Zero Hour accepts an obstacle source whose zone is not yet initialized.
        if from_obstacle && effective[0] == 0 {
            return true;
        }
        if from_obstacle || destination_kind == 4 {
            let terrain = raw.map(|zone| {
                let zone = self.source_terrain_zone(zone);
                let zone = self.effective_source_zone(surfaces, false, zone);
                self.source_terrain_zone(zone)
            });
            terrain[0] == terrain[1]
        } else {
            effective[0] == effective[1]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_zone_query_respects_cardinal_connections_and_rebuilt_structures() {
        let terrain = Grid::new(12, 8, []).unwrap();
        let first = Cell { x: 2, y: 4 };
        let second = Cell { x: 9, y: 4 };
        assert!(terrain.quick_path_exists(1, first, second));
        let divided = terrain.with_obstacles((0..8).map(|y| Cell { x: 6, y }));
        assert!(!divided.quick_path_exists(1, first, second));
        let reopened = divided.without_layer_obstacles([crate::grid::LayerCell {
            cell: Cell { x: 6, y: 3 },
            layer: 1,
        }]);
        // clientSafeQuickDoesPathExist uses canonical zones. Ignoring an
        // obstacle only changes validMovementPosition; neither endpoint here
        // is CELL_OBSTACLE, so the terrain-zone branch is not selected.
        assert!(!reopened.quick_path_exists(1, first, second));
        assert!(!divided.quick_path_exists(1, first, second));
        let removed = terrain.with_obstacles((0..8).filter(|&y| y != 3).map(|y| Cell { x: 6, y }));
        assert!(removed.quick_path_exists(1, first, second));
        let diagonal = Grid::new(2, 2, [Cell { x: 0, y: 1 }, Cell { x: 1, y: 0 }]).unwrap();
        assert!(!diagonal.quick_path_exists(1, Cell { x: 0, y: 0 }, Cell { x: 1, y: 1 }));
    }

    #[test]
    fn ignored_obstacle_destination_uses_native_terrain_zone_equivalence() {
        let terrain = Grid::new(12, 8, []).unwrap();
        let obstacle = Cell { x: 6, y: 4 };
        let divided = terrain.with_obstacles((0..8).map(|y| Cell { x: 6, y }));
        let ignored = divided.without_layer_obstacles([crate::grid::LayerCell {
            cell: obstacle,
            layer: 1,
        }]);
        assert!(ignored.quick_path_exists(1, Cell { x: 2, y: 4 }, obstacle));
        assert!(!ignored.quick_path_exists(1, Cell { x: 2, y: 4 }, Cell { x: 9, y: 4 }));
    }

    #[test]
    fn obstacle_start_uses_terrain_zone_but_obstacle_destination_is_invalid() {
        let terrain = Grid::new(12, 8, []).unwrap();
        let obstacle = Cell { x: 6, y: 4 };
        let divided = terrain.with_obstacles((0..8).map(|y| Cell { x: 6, y }));
        assert!(divided.quick_path_exists(1, obstacle, Cell { x: 2, y: 4 }));
        assert!(divided.quick_path_exists(1, obstacle, Cell { x: 9, y: 4 }));
        assert!(!divided.quick_path_exists(1, Cell { x: 2, y: 4 }, obstacle));
        let terrain = Grid::new(12, 8, (0..8).map(|y| Cell { x: 8, y })).unwrap();
        let blocked = terrain.with_obstacles([obstacle]);
        assert!(!blocked.quick_path_exists(1, obstacle, Cell { x: 9, y: 4 }));
    }

    #[test]
    fn ordinary_query_rejects_disconnected_zones_but_can_leave_a_structure() {
        let terrain = Grid::new(12, 8, []).unwrap();
        let divided = terrain.with_obstacles((0..8).map(|y| Cell { x: 6, y }));
        let units = super::super::RouteUnits::new();
        assert!(
            divided
                .path_for_counted(1, Cell { x: 2, y: 4 }, Cell { x: 9, y: 4 }, 0, true, &units)
                .route
                .is_none()
        );
        let route = divided
            .path_for_counted(1, Cell { x: 6, y: 4 }, Cell { x: 9, y: 4 }, 0, true, &units)
            .route
            .unwrap();
        assert!(route.raw.first().map(|node| node.cell) == Some(Cell { x: 6, y: 4 }));
        assert!(route.raw.last().map(|node| node.cell) == Some(Cell { x: 9, y: 4 }));
    }
}

#[cfg(test)]
mod ui_tests {
    use crate::grid::{Cell, Grid, LayerCell, ZoneCellChange, ZoneModification};
    use crate::terrain::{BridgeGeometry, CellType, TerrainMap};

    fn ground(cell: Cell) -> LayerCell {
        LayerCell { cell, layer: 1 }
    }

    #[test]
    fn build_preview_ignores_structure_divider_without_allocating_a_route() {
        let grid = Grid::new(20, 10, [])
            .unwrap()
            .with_obstacles((0..10).map(|y| Cell { x: 10, y }));
        let from = ground(Cell { x: 3, y: 5 });
        let to = ground(Cell { x: 17, y: 5 });
        assert!(!grid.quick_path_exists(1, from.cell, to.cell));
        assert!(grid.quick_ui_path_exists(from, to, 1));
        assert!(grid.quick_ui_path_exists(from, ground(Cell { x: 10, y: 5 }), 1));
    }

    #[test]
    fn build_preview_accepts_native_uninitialized_cache_before_deferred_recalculation() {
        let original = Grid::new(20, 10, (0..10).map(|y| Cell { x: 10, y })).unwrap();
        let target = Cell { x: 17, y: 5 };
        let from = ground(Cell { x: 3, y: 5 });
        let to = ground(target);
        assert!(!original.quick_ui_path_exists(from, to, 1));
        let mut grid = original.clone();
        let event = ZoneModification {
            update_bounds: None,
            force_dirty: true,
            cells: vec![ZoneCellChange {
                cell: target,
                kind: CellType::Clear,
                fence: false,
                reset_zone: true,
            }],
            post_cells: vec![],
            pinched: vec![],
        };
        grid.inherit_source_zones(&original, &[event]);
        assert_eq!(grid.source_zone(to), 0);
        assert!(grid.quick_ui_path_exists(from, to, 1));
        grid.recalculate_source_zones();
        assert!(!grid.quick_ui_path_exists(from, to, 1));
    }

    #[test]
    fn build_preview_uses_bridge_zone_then_native_ground_fallback_after_layer_destruction() {
        let mut terrain =
            TerrainMap::classify(40, 40, (0..40).map(|y| Cell { x: 20, y }), []).unwrap();
        let layer = terrain
            .add_bridge(
                BridgeGeometry {
                    from: [100.0, 205.0, 20.0],
                    to: [300.0, 205.0, 20.0],
                    corners: [
                        [100.0, 230.0, 20.0],
                        [100.0, 180.0, 20.0],
                        [300.0, 230.0, 20.0],
                        [300.0, 180.0, 20.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        let grid = Grid::new(40, 40, terrain.blocked_ground_cells())
            .unwrap()
            .with_bridge_layers(&terrain);
        let deck = LayerCell {
            cell: Cell { x: 15, y: 20 },
            layer,
        };
        let opposite = ground(Cell { x: 32, y: 20 });
        assert!(grid.quick_ui_path_exists(deck, opposite, 1));
        assert!(!grid.quick_ui_path_exists(
            ground(Cell { x: 8, y: 20 }),
            ground(Cell { x: 20, y: 20 }),
            1
        ));
        assert!(!grid.quick_ui_path_exists(
            ground(Cell { x: 8, y: 20 }),
            ground(Cell { x: 20, y: 20 }),
            8
        ));
        terrain.set_bridge_destroyed(layer, true).unwrap();
        let destroyed = Grid::new(40, 40, terrain.blocked_ground_cells())
            .unwrap()
            .with_bridge_layers(&terrain);
        assert!(!destroyed.quick_ui_path_exists(deck, opposite, 1));
        let fallback = LayerCell {
            cell: Cell { x: 32, y: 20 },
            layer,
        };
        assert!(destroyed.quick_ui_path_exists(fallback, opposite, 1));
    }
}
