use super::{Cell, HierarchyResult, LayerCell, List, Search, SearchResult, block};
use std::collections::BTreeSet;

mod boundary;
mod bridges;

impl Search<'_> {
    pub(super) fn run(&mut self) -> HierarchyResult {
        if self.query.goal_is_origin {
            return self.finish(None, &[]);
        }
        let start = self.grid.resolve_layer(self.query.start);
        let goal = self.grid.resolve_layer(self.query.goal);
        if !self.grid.contains(goal.cell) || !self.allocate(goal) {
            return self.finish(None, &[]);
        }
        if !self.grid.contains(start.cell) || !self.allocate(start) {
            return self.finish(None, &[goal]);
        }
        // The initial screen deliberately ignores the crusher argument.
        let zone = |node| {
            self.grid
                .effective_source_zone(self.query.surfaces, false, self.grid.source_zone(node))
        };
        if zone(start) != zone(goal) {
            return self.finish(None, &[goal, start]);
        }
        let remaining =
            super::super::search::heuristic(self.coordinate(start), self.coordinate(goal));
        self.insert(start, None, 0, remaining);
        if start.layer != 1 && !self.seed_bridge_start(start, goal) {
            return self.finish(None, &[goal]);
        }
        let goal_zone = self.block_zone(goal);
        let goal_block = (goal.layer == 1).then(|| block(self.coordinate(goal)));
        let mut closest = None;
        let mut closest_distance = 1.0e12_f32;
        let mut last = start;
        while let Some(parent) = self.pop() {
            last = parent;
            let parent_zone = self.block_zone(parent);
            let same_block = goal_block.is_none_or(|goal| block(self.coordinate(parent)) == goal);
            let reached = if parent_zone == goal_zone && same_block {
                true
            } else {
                let Some(reached) = self.cross_bridges(parent, goal, parent_zone, goal_zone) else {
                    return self.finish(None, &[goal]);
                };
                reached
            };
            if reached {
                if parent != goal {
                    self.set_parent(goal, parent);
                }
                return self.finish(Some(goal), &[parent, goal]);
            }
            let delta = [
                f32::from(self.coordinate(goal).x.abs_diff(self.coordinate(parent).x)),
                f32::from(self.coordinate(goal).y.abs_diff(self.coordinate(parent).y)),
            ];
            let squares = delta.map(|value| value * value);
            let distance = squares[0] + squares[1];
            if distance < closest_distance {
                closest = Some(parent);
                closest_distance = distance;
            }
            self.close(parent);
            self.expand_sides(parent, goal, parent_zone);
        }
        let endpoint = self.query.closest.then_some(closest).flatten();
        self.finish(endpoint, &[last, goal])
    }

    fn finish(&self, endpoint: Option<LayerCell>, extra: &[LayerCell]) -> HierarchyResult {
        let mut open: Vec<_> = self
            .nodes
            .iter()
            .filter(|(_, node)| matches!(node.list, List::Open))
            .map(|(&key, node)| (node.info.total, node.insertion, key))
            .collect();
        open.sort_unstable();
        let mut released: Vec<_> = open.into_iter().map(|(_, _, key)| key).collect();
        released.extend(self.closed.iter().rev().copied());
        let cells = released.len();
        released.extend_from_slice(extra);
        let mut seen = BTreeSet::new();
        let info_cleanup = released
            .into_iter()
            .filter_map(|key| {
                if !seen.insert(key) {
                    return None;
                }
                self.nodes.get(&key).map(|node| (key, node.info))
            })
            .collect();
        let retained_infos = self
            .nodes
            .iter()
            .filter(|(key, _)| !seen.contains(*key))
            .map(|(&key, node)| (key, node.info))
            .collect();
        HierarchyResult {
            blocks: endpoint.map(|endpoint| self.corridor(endpoint)),
            search: SearchResult {
                retained_infos,
                route: None,
                cells,
                info_cleanup,
            },
        }
    }

    fn corridor(&self, mut node: LayerCell) -> BTreeSet<Cell> {
        let mut marked = BTreeSet::new();
        loop {
            marked.insert(block(self.coordinate(node)));
            let Some(parent) = self.nodes[&node].parent else {
                break;
            };
            node = parent;
        }
        // buildHierarchicalPath expands ten cells around the actual start.
        let [low, high] = self.query.start_expansion;
        for horizontal in low.x..=high.x {
            for vertical in low.y..=high.y {
                marked.insert(block(Cell {
                    x: horizontal,
                    y: vertical,
                }));
            }
        }
        marked
    }
}
