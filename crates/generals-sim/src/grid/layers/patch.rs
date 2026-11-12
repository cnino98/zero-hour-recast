//! `patchPath` rejoin-tail search with typed layer nodes and native 2000 INFO cap.
use super::{BTreeMap, Grid, LayerCell, LayerQuery, Reverse, RouteUnits, Search, SearchResult};
use crate::grid::search::{cell_position, squared_distance};

pub struct LayerPatch<'a> {
    pub start: LayerCell,
    pub indexed_start: LayerCell,
    pub position: [f32; 2],
    pub radius: i32,
    pub centered: bool,
    pub raw: &'a [LayerCell],
    pub blocked: bool,
}

impl Grid {
    pub(crate) fn patch_layers_counted(
        &self,
        query: &LayerPatch<'_>,
        units: &BTreeMap<u8, RouteUnits>,
    ) -> SearchResult {
        if query.raw.len() < 2
            || !self.contains(query.start.cell)
            || !self.contains(query.indexed_start.cell)
        {
            return SearchResult::empty();
        }
        let Some(mut search) = Search::new(
            self,
            &LayerQuery {
                start: query.indexed_start,
                goal: query.indexed_start,
                radius: query.radius,
                centered: query.centered,
                closest_cost: None,
            },
            units,
            false,
            false,
        ) else {
            return SearchResult::empty();
        };
        let mut first = query.raw.len();
        let mut candidate = *query.raw.last().expect("nonempty old route");
        let mut nearest = squared_distance(
            query.position,
            cell_position(candidate.cell, query.centered),
        );
        for index in (1..query.raw.len()).rev() {
            let node = query.raw[index];
            let occupied = search.occupied(node);
            let transient = query.blocked
                && node.cell.x.abs_diff(query.indexed_start.cell.x) <= 2
                && node.cell.y.abs_diff(query.indexed_start.cell.y) <= 2;
            let above = query.radius + i32::from(query.centered);
            let inside = i32::from(node.cell.x) >= query.radius
                && i32::from(node.cell.y) >= query.radius
                && i32::from(node.cell.x) + above <= i32::from(self.width)
                && i32::from(node.cell.y) + above <= i32::from(self.height);
            if !inside || occupied.0 || occupied.2 || (transient && search.transient_occupied(node))
            {
                break;
            }
            first = index;
            let distance =
                squared_distance(query.position, cell_position(node.cell, query.centered));
            if distance < nearest {
                candidate = node;
                nearest = distance;
            }
        }
        if first == query.raw.len()
            || !search.reserve_info(candidate)
            || !search.insert(query.start, None, 0, 0)
        {
            return SearchResult::empty();
        }
        if !search.has_info(query.start) {
            search
                .nodes
                .get_mut(&query.start)
                .expect("inserted old route start")
                .coordinate = query.indexed_start;
        }
        search.use_heuristic = false;
        let mut allocations = 0;
        while let Some(Reverse((total, insertion, node))) = search.open.pop() {
            if search.nodes[&node].insertion != insertion {
                continue;
            }
            let coordinate = search.nodes[&node].coordinate;
            if let Some(index) = (first..query.raw.len())
                .rev()
                .find(|&index| query.raw[index] == coordinate)
            {
                let mut raw = search.route(node).raw;
                raw.extend_from_slice(&query.raw[index + 1..]);
                search.listed -= 1;
                return SearchResult {
                    retained_infos: Vec::new(),
                    info_cleanup: search.info_cleanup(),
                    route: Some(search.optimize(raw, query.blocked)),
                    cells: search.listed,
                };
            }
            if allocations < 2000 {
                if let Some(next) = self.connection(node)
                    && !search.nodes.contains_key(&next)
                {
                    search.insert(
                        next,
                        Some(node),
                        search.nodes[&node].cost,
                        total - search.nodes[&node].cost,
                    );
                }
                allocations += search.expand_neighbors(node);
            }
        }
        search.result(None, false)
    }
}
