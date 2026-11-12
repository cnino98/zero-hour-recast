use super::{LayerCell, Search, block};

impl Search<'_> {
    pub(super) fn seed_bridge_start(&mut self, start: LayerCell, goal: LayerCell) -> bool {
        let Some(ends) = self.grid.bridge_endpoints(start.layer) else {
            return true;
        };
        // Native validates both ground cells before removing the bridge start.
        if ends.iter().any(|&cell| !self.grid.contains(cell)) {
            return true;
        }
        self.close(start);
        for cell in ends {
            let node = LayerCell { cell, layer: 1 };
            if !self.allocate(node) {
                return false;
            }
            let cost = self.cost(node, start);
            let remaining = self.cost(node, goal);
            // The source omits cost-so-far from bridge seed total cost.
            self.insert(node, Some(start), cost, remaining);
        }
        true
    }

    pub(super) fn cross_bridges(
        &mut self,
        parent: LayerCell,
        goal: LayerCell,
        parent_zone: u32,
        goal_zone: u32,
    ) -> Option<bool> {
        let coordinate = self.coordinate(parent);
        if !self.grid.bridge_block_interacts(coordinate) {
            return Some(false);
        }
        let parent_block = block(coordinate);
        let bridges: Vec<_> = self.grid.active_bridge_endpoints().collect();
        for (layer, mut ends) in bridges {
            if block(ends[0]) != parent_block {
                ends.swap(0, 1);
            }
            if block(ends[0]) != parent_block {
                continue;
            }
            let bridge_zone =
                self.grid
                    .source_block_zone(self.query.surfaces, self.query.crusher, ends[0]);
            if bridge_zone != parent_zone {
                continue;
            }
            if self.grid.source_layer_zone(layer) == goal_zone {
                return Some(true);
            }
            let target = LayerCell {
                cell: ends[1],
                layer: 1,
            };
            if !self.grid.contains(target.cell) || self.visited(target) {
                continue;
            }
            let from = LayerCell {
                cell: ends[0],
                layer: 1,
            };
            if !self.grid.contains(from.cell) {
                continue;
            }
            if from != parent {
                if !self.allocate(from) {
                    return None;
                }
                self.set_parent(from, parent);
                if !self.visited(from) {
                    self.close(from);
                }
            }
            if !self.allocate(target) {
                return None;
            }
            // Merely assigning the endpoint's parent does not reset its old cost.
            let cost = self.nodes[&from].info.cost + self.cost(target, from);
            let total = cost + self.cost(target, goal);
            self.insert(target, Some(from), cost, total);
        }
        Some(false)
    }
}
