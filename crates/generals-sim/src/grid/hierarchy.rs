//! `AIPathfind.cpp:7460–8145`: native hierarchical search, SH 2bfaea60.
//! Raw zone construction and equivalence tables belong to `zones`.
mod search;
#[cfg(test)]
mod tests;
pub(super) mod zones;

use super::{Cell, CellInfo, Grid, LayerCell, RouteUnits, SearchResult};
use num_traits::ToPrimitive;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

pub struct HierarchyQuery {
    pub start: LayerCell,
    pub goal: LayerCell,
    pub surfaces: u8,
    pub crusher: bool,
    pub closest: bool,
    pub logical_bounds: Option<[Cell; 2]>,
    pub start_expansion: [Cell; 2],
    pub goal_is_origin: bool,
}

pub struct HierarchyResult {
    pub blocks: Option<BTreeSet<Cell>>,
    pub search: SearchResult,
}

enum List {
    None,
    Open,
    Closed,
}

struct Node {
    info: CellInfo,
    parent: Option<LayerCell>,
    insertion: u64,
    list: List,
}

struct Search<'a> {
    grid: &'a Grid,
    query: &'a HierarchyQuery,
    units: &'a BTreeMap<u8, RouteUnits>,
    nodes: BTreeMap<LayerCell, Node>,
    open: BinaryHeap<Reverse<(u32, u64, LayerCell)>>,
    closed: Vec<LayerCell>,
    available: usize,
    allocated: usize,
    sequence: u64,
}

impl Grid {
    pub(crate) fn hierarchical_corridor(
        &self,
        query: &HierarchyQuery,
        units: &BTreeMap<u8, RouteUnits>,
    ) -> HierarchyResult {
        Search {
            grid: self,
            query,
            units,
            nodes: BTreeMap::new(),
            open: BinaryHeap::new(),
            closed: Vec::new(),
            available: units
                .values()
                .map(|units| units.info_available)
                .min()
                .unwrap_or(super::CELL_INFO_CAPACITY),
            allocated: 0,
            sequence: 0,
        }
        .run()
    }
}

impl Search<'_> {
    fn existing_info(&self, node: LayerCell) -> Option<CellInfo> {
        self.units.get(&node.layer).and_then(|units| {
            units
                .info_record(node.cell)
                .copied()
                .or_else(|| units.has_info(node.cell).then(|| CellInfo::new(node)))
        })
    }

    fn has_info(&self, node: LayerCell) -> bool {
        self.nodes.contains_key(&node) || self.existing_info(node).is_some()
    }

    fn allocate(&mut self, node: LayerCell) -> bool {
        if self.nodes.contains_key(&node) {
            return true;
        }
        let info = if let Some(info) = self.existing_info(node) {
            // Native INFO retains XY; layer belongs to its physical cell.
            CellInfo {
                coordinate: LayerCell {
                    cell: info.coordinate.cell,
                    layer: node.layer,
                },
                ..info
            }
        } else {
            if self.allocated == self.available {
                return false;
            }
            self.allocated += 1;
            CellInfo::new(node)
        };
        self.nodes.insert(
            node,
            Node {
                info,
                parent: None,
                insertion: 0,
                list: List::None,
            },
        );
        true
    }

    fn visited(&self, node: LayerCell) -> bool {
        self.nodes
            .get(&node)
            .is_some_and(|node| !matches!(node.list, List::None))
    }

    fn coordinate(&self, node: LayerCell) -> Cell {
        self.nodes[&node].info.coordinate.cell
    }

    fn set_parent(&mut self, node: LayerCell, parent: LayerCell) {
        self.nodes.get_mut(&node).expect("allocated INFO").parent = Some(parent);
    }

    fn close(&mut self, node: LayerCell) {
        self.nodes.get_mut(&node).expect("allocated INFO").list = List::Closed;
        self.closed.push(node);
    }

    fn insert(&mut self, node: LayerCell, parent: Option<LayerCell>, cost: u32, total: u32) {
        self.sequence += 1;
        let data = self.nodes.get_mut(&node).expect("allocated INFO");
        data.parent = parent;
        data.info.cost = cost;
        data.info.total = total;
        data.insertion = self.sequence;
        data.list = List::Open;
        self.open.push(Reverse((total, self.sequence, node)));
    }

    fn pop(&mut self) -> Option<LayerCell> {
        while let Some(Reverse((_, insertion, node))) = self.open.pop() {
            let data = self.nodes.get_mut(&node).expect("listed INFO");
            if data.insertion == insertion && matches!(data.list, List::Open) {
                data.list = List::None;
                return Some(node);
            }
        }
        None
    }

    fn block_zone(&self, node: LayerCell) -> u32 {
        if node.layer == 1 {
            self.grid.source_block_zone(
                self.query.surfaces,
                self.query.crusher,
                self.coordinate(node),
            )
        } else {
            self.grid.source_zone(node)
        }
    }

    fn cost(&self, first: LayerCell, second: LayerCell) -> u32 {
        hier_cost(self.coordinate(first), self.coordinate(second))
    }
}

const fn block(cell: Cell) -> Cell {
    Cell {
        x: cell.x / 10,
        y: cell.y / 10,
    }
}

fn hier_cost(first: Cell, second: Cell) -> u32 {
    // Native sqrt receives the integer squared distance and returns double.
    let horizontal = f64::from(first.x.abs_diff(second.x));
    let vertical = f64::from(first.y.abs_diff(second.y));
    let horizontal_squared = horizontal * horizontal;
    let vertical_squared = vertical * vertical;
    let distance = (horizontal_squared + vertical_squared).sqrt();
    let scaled = 10.0 * distance;
    (scaled + 0.5)
        .floor()
        .to_u32()
        .expect("bounded native hierarchy cost")
}

/// Native passable flags are shared by all layers; only ground cells consult them.
pub(super) fn passable(corridor: Option<&BTreeSet<Cell>>, cell: Cell) -> bool {
    corridor.is_none_or(|blocks| blocks.contains(&block(cell)))
}

/// clipIsPassable divides signed coordinates before checking block extents.
/// C++ truncates negative values toward zero, so -3 still addresses block zero.
pub(super) fn group_passable(grid: &Grid, corridor: Option<&BTreeSet<Cell>>, cell: Cell) -> bool {
    if passable(corridor, cell) {
        return true;
    }
    let Some(blocks) = corridor else {
        return true;
    };
    let extent = grid.block_extent();
    [[3, 3], [-3, 3], [3, -3], [-3, -3]]
        .into_iter()
        .any(|[horizontal, vertical]| {
            let block_x = (i32::from(cell.x) + horizontal) / 10;
            let block_y = (i32::from(cell.y) + vertical) / 10;
            block_x >= 0
                && block_y >= 0
                && block_x < i32::from(extent.x)
                && block_y < i32::from(extent.y)
                && blocks.contains(&Cell {
                    x: u16::try_from(block_x).expect("bounded block X"),
                    y: u16::try_from(block_y).expect("bounded block Y"),
                })
        })
}
