//! Native ten-cell raw zones and block/global surface equivalence tables.
#[cfg(test)]
mod reference_tests;

use crate::grid::{Cell, Grid};

pub(in crate::grid) const BLOCK_SIZE: u16 = 10;

// Grid snapshots copy the source cache together with their terrain/obstacle cells.
#[derive(Clone)]
pub(in crate::grid) struct Zones {
    raw: Vec<u32>,
    blocks: Vec<Block>,
    global: Combiners,
    hierarchical: Vec<u32>,
    terrain: Vec<u32>,
    upper: std::collections::BTreeMap<u8, u32>,
    block_height: usize,
}

#[derive(Clone)]
struct Block {
    first: u32,
    count: u32,
    combines: Combiners,
    bridge: bool,
}

#[derive(Clone)]
struct Combiners {
    cliff: Vec<u32>,
    water: Vec<u32>,
    rubble: Vec<u32>,
    crusher: Vec<u32>,
}

impl Combiners {
    fn identity(first: u32, count: u32) -> Self {
        let identity = || (first..first + count).collect();
        Self {
            cliff: identity(),
            water: identity(),
            rubble: identity(),
            crusher: identity(),
        }
    }

    fn block_pair(&mut self, from: (u32, u8, bool), to: (u32, u8, bool), first: u32) {
        let (left, right) = (from.0 - first, to.0 - first);
        if water_ground(from.1, to.1) {
            merge_block(&mut self.water, left, right);
        }
        if clear_pair(from.1, to.1, 3) {
            merge_block(&mut self.rubble, left, right);
        }
        if clear_pair(from.1, to.1, 2) {
            merge_block(&mut self.cliff, left, right);
        }
        if crusher_ground(from, to) {
            merge_block(&mut self.crusher, left, right);
        }
    }

    fn effective(&self, surfaces: u8, crusher: bool, mut zone: u32, first: u32) -> u32 {
        if crusher {
            zone = self.crusher[index(zone - first)];
        }
        if surfaces & 1 != 0 && surfaces & 4 != 0 {
            self.cliff[index(zone - first)]
        } else if surfaces & 1 != 0 && surfaces & 2 != 0 {
            self.water[index(zone - first)]
        } else if surfaces & 1 != 0 && surfaces & 16 != 0 {
            self.rubble[index(zone - first)]
        } else {
            zone
        }
    }
}

impl Zones {
    pub(in crate::grid) fn calculate(grid: &Grid) -> Self {
        let block_width = usize::from(grid.width.div_ceil(BLOCK_SIZE));
        let block_height = usize::from(grid.height.div_ceil(BLOCK_SIZE));
        let (raw, mut maximum) = raw_zones(grid);
        let mut upper = std::collections::BTreeMap::new();
        for layer in 0..=15 {
            upper.insert(layer, maximum);
            maximum += 1;
        }
        let mut blocks = Vec::new();
        for bx in 0..block_width {
            for by in 0..block_height {
                let cells = block_cells(grid, bx, by);
                let first = cells
                    .iter()
                    .map(|&cell| raw[cell_index(grid, cell)])
                    .min()
                    .expect("nonempty block");
                let last = cells
                    .iter()
                    .map(|&cell| raw[cell_index(grid, cell)])
                    .max()
                    .expect("nonempty block");
                let mut combines = Combiners::identity(first, last - first + 1);
                for cell in cells {
                    for adjacent in [
                        (usize::from(cell.x) > bx * 10).then(|| Cell {
                            x: cell.x - 1,
                            ..cell
                        }),
                        (usize::from(cell.y) > by * 10).then(|| Cell {
                            y: cell.y - 1,
                            ..cell
                        }),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let from = (
                            raw[cell_index(grid, cell)],
                            grid.hierarchy_kind(cell),
                            grid.hierarchy_fence(cell),
                        );
                        let to = (
                            raw[cell_index(grid, adjacent)],
                            grid.hierarchy_kind(adjacent),
                            grid.hierarchy_fence(adjacent),
                        );
                        if from.0 != to.0 {
                            combines.block_pair(from, to, first);
                        }
                    }
                }
                let bridge = block_cells(grid, bx, by)
                    .iter()
                    .any(|cell| grid.ground_connections.contains_key(cell));
                blocks.push(Block {
                    first,
                    count: last - first + 1,
                    combines,
                    bridge,
                });
            }
        }
        let mut zones = Self {
            raw,
            blocks,
            global: Combiners::identity(0, maximum),
            hierarchical: (0..maximum).collect(),
            terrain: (0..maximum).collect(),
            upper,
            block_height,
        };
        for (_, ends) in grid.active_bridge_endpoints() {
            for cell in ends {
                if grid.contains(cell) {
                    let index = usize::from(cell.x / 10) * block_height + usize::from(cell.y / 10);
                    zones.blocks[index].bridge = true;
                }
            }
        }
        zones.combine_global(grid);
        zones
    }

    fn combine_global(&mut self, grid: &Grid) {
        for y in 0..grid.height {
            for x in 0..grid.width {
                let cell = Cell { x, y };
                let zone = self.raw[cell_index(grid, cell)];
                if grid.hierarchy_kind(cell) == 0
                    && let Some(&layer) = grid.ground_connections.get(&cell)
                {
                    merge(&mut self.hierarchical, zone, self.upper[&layer]);
                }
                for adjacent in [
                    x.checked_sub(1).map(|x| Cell { x, y }),
                    y.checked_sub(1).map(|y| Cell { x, y }),
                ]
                .into_iter()
                .flatten()
                {
                    let other = self.raw[cell_index(grid, adjacent)];
                    if zone == other {
                        continue;
                    }
                    let from = (zone, grid.hierarchy_kind(cell), grid.hierarchy_fence(cell));
                    let to = (
                        other,
                        grid.hierarchy_kind(adjacent),
                        grid.hierarchy_fence(adjacent),
                    );
                    if from.1 == to.1 {
                        merge(&mut self.hierarchical, zone, other);
                        continue;
                    }
                    let terrain = terrain_kind(from.1) == terrain_kind(to.1);
                    let crusher = crusher_ground(from, to);
                    if terrain {
                        merge(&mut self.terrain, zone, other);
                    }
                    if crusher {
                        merge(&mut self.global.crusher, zone, other);
                    }
                    if !terrain && !crusher {
                        if water_ground(from.1, to.1) {
                            merge(&mut self.global.water, zone, other);
                        } else if clear_pair(from.1, to.1, 3) {
                            merge(&mut self.global.rubble, zone, other);
                        } else if clear_pair(from.1, to.1, 2) {
                            merge(&mut self.global.cliff, zone, other);
                        }
                    }
                }
            }
        }
        for zone in 1..self.hierarchical.len() {
            self.hierarchical[zone] = self.hierarchical[index(self.hierarchical[zone])];
        }
        for table in [
            &mut self.global.cliff,
            &mut self.global.water,
            &mut self.global.rubble,
            &mut self.global.crusher,
            &mut self.terrain,
        ] {
            flatten(table, &self.hierarchical);
        }
    }

    pub(in crate::grid) fn interacts_with_bridge(&self, block: Cell) -> bool {
        self.blocks[usize::from(block.x) * self.block_height + usize::from(block.y)].bridge
    }

    pub(in crate::grid) fn invalidate_cells(
        &mut self,
        grid: &Grid,
        cells: impl IntoIterator<Item = Cell>,
    ) {
        for cell in cells {
            self.raw[cell_index(grid, cell)] = 0;
        }
    }

    /// Native `updateZonesForModify` fills uninitialized IDs without allocating
    /// new zones or rebuilding the cached block/global equivalence tables.
    pub(in crate::grid) fn update_for_modify(&mut self, grid: &Grid, [lo, hi]: [[i32; 2]; 2]) {
        // Native expands the signed upper bound before clipping. A footprint
        // ending at -1 still touches cell zero in the partial-zone phase.
        let hi = [
            (hi[0] + 1).min(i32::from(grid.width) - 1),
            (hi[1] + 1).min(i32::from(grid.height) - 1),
        ];
        for bx in 0..grid.width.div_ceil(10) {
            for by in 0..grid.height.div_ceil(10) {
                let minimum = [lo[0].max(i32::from(bx) * 10), lo[1].max(i32::from(by) * 10)];
                let maximum = [
                    hi[0].min(i32::from(bx) * 10 + 9),
                    hi[1].min(i32::from(by) * 10 + 9),
                ];
                if minimum[0] > maximum[0] || minimum[1] > maximum[1] {
                    continue;
                }
                let cell = |point: [i32; 2]| Cell {
                    x: u16::try_from(point[0]).expect("intersected block X"),
                    y: u16::try_from(point[1]).expect("intersected block Y"),
                };
                let min = cell(minimum);
                let max = cell(maximum);
                self.blocks[usize::from(bx) * self.block_height + usize::from(by)].bridge = false;
                self.flood_modified(grid, min, max, false);
                self.flood_modified(grid, min, max, true);
            }
        }
    }

    fn flood_modified(&mut self, grid: &Grid, lo: Cell, hi: Cell, backwards: bool) {
        for iy in 0..=hi.y - lo.y {
            let y = if backwards { hi.y - iy } else { lo.y + iy };
            for ix in 0..=hi.x - lo.x {
                let x = if backwards { hi.x - ix } else { lo.x + ix };
                let cell = Cell { x, y };
                if self.raw(grid, cell) != 0 {
                    continue;
                }
                let horizontal = if backwards {
                    (x < hi.x).then(|| Cell { x: x + 1, y })
                } else {
                    (x > lo.x).then(|| Cell { x: x - 1, y })
                };
                if horizontal.is_some_and(|next| self.adopt_zone(grid, cell, next)) {
                    continue;
                }
                let vertical = if backwards {
                    (y < hi.y).then(|| Cell { x, y: y + 1 })
                } else {
                    (y > lo.y).then(|| Cell { x, y: y - 1 })
                };
                let Some(vertical) = vertical else {
                    continue;
                };
                if self.adopt_zone(grid, cell, vertical) {
                    continue;
                }
                if x < hi.x {
                    let diagonal = Cell {
                        x: x + 1,
                        y: vertical.y,
                    };
                    if grid.hierarchy_kind(cell) == grid.hierarchy_kind(diagonal)
                        && grid.hierarchy_kind(cell) == grid.hierarchy_kind(Cell { x: x + 1, y })
                    {
                        self.raw[cell_index(grid, cell)] = self.raw(grid, diagonal);
                    }
                }
            }
        }
    }

    fn adopt_zone(&mut self, grid: &Grid, cell: Cell, adjacent: Cell) -> bool {
        if grid.hierarchy_kind(cell) == grid.hierarchy_kind(adjacent) {
            let zone = self.raw(grid, adjacent);
            self.raw[cell_index(grid, cell)] = zone;
            zone != 0
        } else {
            false
        }
    }

    pub(in crate::grid) fn raw(&self, grid: &Grid, cell: Cell) -> u32 {
        self.raw[cell_index(grid, cell)]
    }
    pub(in crate::grid) fn terrain(&self, zone: u32) -> u32 {
        self.hierarchical[index(self.terrain[index(zone)])]
    }
    pub(in crate::grid) fn upper(&self, layer: u8) -> u32 {
        self.upper[&layer]
    }
    pub(in crate::grid) fn effective_global(&self, surfaces: u8, crusher: bool, zone: u32) -> u32 {
        if surfaces & 8 != 0 || surfaces & 7 == 7 {
            return 1;
        }
        let zone = self.global.effective(surfaces, crusher, zone, 0);
        if surfaces & 1 != 0 && surfaces & (4 | 2 | 16) != 0 {
            zone
        } else {
            self.hierarchical[index(zone)]
        }
    }
    pub(in crate::grid) fn effective_block(
        &self,
        grid: &Grid,
        surfaces: u8,
        crusher: bool,
        cell: Cell,
    ) -> u32 {
        let zone = self.raw(grid, cell);
        if zone == 0 {
            return 0;
        }
        if surfaces & 8 != 0 || surfaces & 7 == 7 {
            return 1;
        }
        let block =
            &self.blocks[usize::from(cell.x / 10) * self.block_height + usize::from(cell.y / 10)];
        if block.count < 2 || zone < block.first || zone >= block.first + block.count {
            return block.first;
        }
        block
            .combines
            .effective(surfaces, crusher, zone, block.first)
    }
}

fn raw_zones(grid: &Grid) -> (Vec<u32>, u32) {
    let width = usize::from(grid.width);
    let block_width = usize::from(grid.width.div_ceil(BLOCK_SIZE));
    let block_height = usize::from(grid.height.div_ceil(BLOCK_SIZE));
    let mut raw = vec![0; width * usize::from(grid.height)];
    let mut equivalent = vec![0_u32];
    for bx in 0..block_width {
        for by in 0..block_height {
            for y in by * 10..((by + 1) * 10).min(usize::from(grid.height)) {
                for x in bx * 10..((bx + 1) * 10).min(width) {
                    let current = y * width + x;
                    let cell = Cell {
                        x: u16::try_from(x).expect("bounded cell"),
                        y: u16::try_from(y).expect("bounded cell"),
                    };
                    let kind = grid.hierarchy_kind(cell);
                    for adjacent in [
                        (x > bx * 10).then(|| Cell {
                            x: cell.x - 1,
                            ..cell
                        }),
                        (y > by * 10).then(|| Cell {
                            y: cell.y - 1,
                            ..cell
                        }),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if grid.hierarchy_kind(adjacent) != kind {
                            continue;
                        }
                        let adjacent = usize::from(adjacent.y) * width + usize::from(adjacent.x);
                        let source = equivalent[index(raw[adjacent])];
                        if raw[current] == 0 {
                            raw[current] = source;
                        } else {
                            merge(&mut equivalent, raw[current], source);
                        }
                    }
                    if raw[current] == 0 {
                        let zone = u32::try_from(equivalent.len()).expect("bounded zones");
                        equivalent.push(zone);
                        raw[current] = zone;
                    }
                }
            }
        }
    }
    let mut collapsed = vec![0; equivalent.len()];
    let mut maximum = 1;
    for zone in 1..equivalent.len() {
        if equivalent[zone] == u32::try_from(zone).expect("zone") {
            collapsed[zone] = maximum;
            maximum += 1;
        } else {
            collapsed[zone] = collapsed[index(equivalent[zone])];
        }
    }
    for zone in &mut raw {
        *zone = collapsed[index(*zone)];
    }
    (raw, maximum)
}

fn cell_index(grid: &Grid, cell: Cell) -> usize {
    usize::from(cell.y) * usize::from(grid.width) + usize::from(cell.x)
}
fn block_cells(grid: &Grid, bx: usize, by: usize) -> Vec<Cell> {
    (by * 10..((by + 1) * 10).min(usize::from(grid.height)))
        .flat_map(|y| {
            (bx * 10..((bx + 1) * 10).min(usize::from(grid.width))).map(move |x| Cell {
                x: u16::try_from(x).expect("cell"),
                y: u16::try_from(y).expect("cell"),
            })
        })
        .collect()
}
fn index(value: u32) -> usize {
    usize::try_from(value).expect("bounded zone")
}
const fn terrain_kind(kind: u8) -> u8 {
    if kind == 4 { 0 } else { kind }
}
const fn clear_pair(first: u8, second: u8, kind: u8) -> bool {
    (first == 0 && second == kind) || (second == 0 && first == kind)
}
const fn water_ground(first: u8, second: u8) -> bool {
    (first == 0 && second & 1 != 0) || (second == 0 && first & 1 != 0)
}
const fn crusher_ground(first: (u32, u8, bool), second: (u32, u8, bool)) -> bool {
    (first.1 == 4 && first.2 && second.1 == 0) || (second.1 == 4 && second.2 && first.1 == 0)
}
fn merge(table: &mut [u32], first: u32, second: u32) {
    let first = table[index(first)];
    let second = table[index(second)];
    let final_zone = table[index(first.min(second))];
    for zone in table {
        if *zone == first || *zone == second {
            *zone = final_zone;
        }
    }
}
fn merge_block(table: &mut [u32], first: u32, second: u32) {
    let first = table[index(first)];
    let second = table[index(second)];
    let final_zone = first.min(second);
    for zone in table {
        if *zone == first || *zone == second {
            *zone = final_zone;
        }
    }
}
fn flatten(table: &mut [u32], hierarchical: &[u32]) {
    for i in 0..table.len() {
        let zone = hierarchical[index(table[i])];
        let zone = table[index(zone)];
        table[i] = hierarchical[index(zone)];
    }
    for index in 0..table.len() {
        if table[index] != hierarchical[index] {
            merge(table, table[index], hierarchical[index]);
        }
    }
}
