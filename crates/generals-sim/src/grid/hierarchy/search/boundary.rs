use super::{Cell, LayerCell, Search, block};
use std::collections::BTreeSet;

impl Search<'_> {
    pub(super) fn expand_sides(&mut self, parent: LayerCell, goal: LayerCell, parent_zone: u32) {
        let center = block(self.coordinate(parent));
        let extent = self.grid.block_extent();
        let present = [
            center.x > 0,
            center.x + 1 < extent.x,
            center.y > 0,
            center.y + 1 < extent.y,
        ];
        for (side, present) in present.into_iter().enumerate() {
            if !present {
                continue;
            }
            let mut examined = BTreeSet::new();
            for index in 1..=10 {
                let offset = (index / 2) * if index & 1 != 0 { -1 } else { 1 };
                let edge = if side % 2 == 0 { 0 } else { 9 };
                let [horizontal, vertical] = if side < 2 {
                    [
                        i32::from(center.x) * 10 + edge,
                        i32::from(center.y) * 10 + 5 + offset,
                    ]
                } else {
                    [
                        i32::from(center.x) * 10 + 5 + offset,
                        i32::from(center.y) * 10 + edge,
                    ]
                };
                let Some(scan) = self.grid.signed_cell(horizontal, vertical) else {
                    continue;
                };
                let node = LayerCell {
                    cell: scan,
                    layer: 1,
                };
                // This screen uses physical scan coordinates, even if its INFO
                // was previously claimed with a different indexed position.
                if self.visited(node)
                    && self
                        .grid
                        .source_block_zone(self.query.surfaces, self.query.crusher, scan)
                        == parent_zone
                {
                    break;
                }
                if self.query.logical_bounds.is_some_and(|[low, high]| {
                    scan.x < low.x || scan.y < low.y || scan.x > high.x || scan.y > high.y
                }) {
                    continue;
                }
                let delta = [[-1, 0], [1, 0], [0, -1], [0, 1]][side];
                self.process_boundary(scan, delta, parent, goal, parent_zone, &mut examined);
            }
        }
    }

    fn process_boundary(
        &mut self,
        scan: Cell,
        delta: [i32; 2],
        parent: LayerCell,
        goal: LayerCell,
        parent_zone: u32,
        examined: &mut BTreeSet<u32>,
    ) {
        if parent_zone == 0
            || self.grid.source_block_zone(1, self.query.crusher, scan) != parent_zone
        {
            return;
        }
        let node = LayerCell {
            cell: scan,
            layer: 1,
        };
        // Zero Hour/nonretail returns before allocateInfo if the boundary has
        // no existing INFO. Ordinary clear terrain alone cannot admit this step.
        if !self.has_info(node) || self.visited(node) || self.grid.source_zone(node) != parent_zone
        {
            return;
        }
        let Some(adjacent) = self
            .grid
            .signed_cell(i32::from(scan.x) + delta[0], i32::from(scan.y) + delta[1])
        else {
            return;
        };
        let next = LayerCell {
            cell: adjacent,
            layer: 1,
        };
        if self.visited(next) {
            return;
        }
        let zone = self.grid.source_block_zone(1, self.query.crusher, adjacent);
        if self.grid.effective_source_zone(1, self.query.crusher, zone)
            != self
                .grid
                .effective_source_zone(1, self.query.crusher, parent_zone)
            || examined.contains(&zone)
        {
            return;
        }
        assert!(self.allocate(node), "existing boundary INFO");
        self.close(node);
        // Failure here closes the boundary but does not abort the entire search.
        if !self.allocate(next) {
            return;
        }
        let mut cost = self.cost(next, parent);
        if self.grid.is_pinched(scan) || self.grid.is_pinched(adjacent) {
            cost += 20;
        } else {
            examined.insert(zone);
        }
        cost += self.nodes[&parent].info.cost;
        let total = cost + self.cost(next, goal);
        self.insert(next, Some(parent), cost, total);
    }
}
