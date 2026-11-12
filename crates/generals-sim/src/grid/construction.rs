//! Source `prependCells/Path::optimize`; domain classification/LOS are supplied by caller.
use std::cmp::Ordering;

pub struct CellNode {
    pub(crate) cell: [u16; 2],
    pub(crate) position: [f32; 3],
    pub(crate) layer: u8,
    pub(crate) cliff: bool,
    pub(crate) pinched: bool,
    pub(crate) blocked_by_ally: bool,
}

pub struct Node {
    pub(crate) cell: [u16; 2],
    pub(crate) position: [f32; 3],
    pub(crate) layer: u8,
    pub(crate) can_optimize: bool,
    pub(crate) previous_layer: u8,
}

pub struct BuiltPath {
    pub(crate) nodes: Vec<Node>,
    pub(crate) optimized: Vec<usize>,
    pub(crate) blocked_by_ally: bool,
}

/// Raw cells are in start-to-goal parent-chain order, with original cell XYZ.
/// Raw caller storage is retained; node positions must not be resampled by layer.
pub fn prepend_cells(raw: &[CellNode], from: [f32; 3]) -> Option<BuiltPath> {
    let mut end = raw.len().checked_sub(1)?;
    if end > 0 && raw[end].pinched && !raw[end - 1].pinched {
        end -= 1;
    }
    prepend_group_cells(&raw[..=end], from)
}

/// buildGroundPath does not apply buildActualPath's pinched-goal trim.
pub fn prepend_group_cells(raw: &[CellNode], from: [f32; 3]) -> Option<BuiltPath> {
    let end = raw.len().checked_sub(1)?;
    let mut reversed: Vec<Node> = Vec::with_capacity(end + 1);
    let mut previous: Option<&CellNode> = None;
    let mut blocked_by_ally = false;
    for cell in raw[1..=end].iter().rev() {
        if let Some(prior) = previous
            && cell.cell == prior.cell
        {
            // The existing downstream node keeps its XYZ, including ground Z
            // at an upper-to-ground exit. Only its layer tag changes.
            reversed.last_mut()?.layer = if cell.layer == 1 {
                prior.layer
            } else {
                cell.layer
            };
            continue;
        }
        let mut can_optimize = true;
        if cell.cliff {
            if previous.is_some_and(|prior| !prior.cliff)
                && let Some(node) = reversed.last_mut()
            {
                node.can_optimize = false;
            }
        } else if previous.is_some_and(|prior| prior.cliff) {
            can_optimize = false;
        }
        reversed.push(Node {
            cell: cell.cell,
            position: cell.position,
            layer: cell.layer,
            can_optimize,
            previous_layer: 1,
        });
        blocked_by_ally |= cell.blocked_by_ally;
        previous = Some(cell);
    }
    if end == 0 {
        reversed.push(Node {
            cell: raw[0].cell,
            position: raw[0].position,
            layer: raw[0].layer,
            can_optimize: false,
            previous_layer: 1,
        });
    }
    if !same_xy(from, reversed.last()?.position) {
        reversed.push(Node {
            cell: raw[0].cell,
            position: from,
            layer: raw[0].layer,
            can_optimize: false,
            previous_layer: 1,
        });
    }
    reversed.reverse();
    let mut previous_layer = 1;
    for node in &mut reversed {
        node.previous_layer = previous_layer;
        previous_layer = node.layer;
    }
    Some(BuiltPath {
        nodes: reversed,
        optimized: Vec::new(),
        blocked_by_ally,
    })
}

pub struct LineCheck {
    pub(crate) passable: bool,
    pub(crate) unpinched_cliff: bool,
}

impl BuiltPath {
    /// check performs native isLinePassable(blocked,false) plus the destination
    /// cell's non-pinched cliff check using this exact selected layer and XYZ.
    pub(crate) fn optimize(&mut self, mut check: impl FnMut(u8, [f32; 3], [f32; 3]) -> LineCheck) {
        self.optimized.clear();
        if self.nodes.is_empty() {
            return;
        }
        self.optimized.push(0);
        let mut anchor = 0;
        while anchor + 1 < self.nodes.len() {
            let mut layer = self.nodes[anchor].layer;
            let mut current_layer = layer;
            let mut candidate = anchor + 1;
            while candidate + 1 < self.nodes.len() {
                let count = candidate - anchor;
                if current_layer == 1 {
                    if self.nodes[candidate].layer != current_layer {
                        layer = self.nodes[candidate].layer;
                        if count > 3 {
                            break;
                        }
                    }
                } else if self.nodes[candidate + 1].layer != current_layer && count > 3 {
                    break;
                }
                current_layer = self.nodes[candidate].layer;
                if !self.nodes[candidate].can_optimize {
                    break;
                }
                candidate += 1;
            }
            if anchor == 0 {
                layer = self.nodes[0].layer;
            }
            let mut next = anchor + 1;
            for index in (anchor + 1..=candidate).rev() {
                let answer = check(
                    layer,
                    self.nodes[anchor].position,
                    self.nodes[index].position,
                );
                if answer.passable
                    || answer.unpinched_cliff
                    || straight_fallback(&self.nodes[anchor..=index], true)
                {
                    next = index;
                    break;
                }
            }
            self.optimized.push(next);
            anchor = next;
        }
    }
}

fn same_xy(a: [f32; 3], b: [f32; 3]) -> bool {
    a[0].partial_cmp(&b[0]) == Some(Ordering::Equal)
        && a[1].partial_cmp(&b[1]) == Some(Ordering::Equal)
}

// Native Int subtraction conversion truncates after world-coordinate subtraction.
// Map coordinates fit exactly in the f32 integral range; retaining truncated f32
// here avoids a lossy-cast lint without changing any supported coordinate result.
fn integer_delta(a: &Node, b: &Node) -> [f32; 2] {
    [
        (b.position[0] - a.position[0]).trunc(),
        (b.position[1] - a.position[1]).trunc(),
    ]
}

fn equal(a: f32, b: f32) -> bool {
    a.partial_cmp(&b) == Some(Ordering::Equal)
}

fn straight_fallback(nodes: &[Node], allow_diagonal_step: bool) -> bool {
    let [mut dx, mut dy] = integer_delta(&nodes[0], &nodes[nodes.len() - 1]);
    let diagonal_step = equal(dx.abs(), 10.0) && equal(dy.abs(), 10.0);
    let mut might_be_passable = false;
    // Native walks from candidate's predecessor back toward (excluding) anchor.
    // Preserve dx/dy mutation and branch order, even for unusual short paths.
    if equal(dx, 0.0) {
        might_be_passable = true;
        for index in (1..nodes.len() - 1).rev() {
            dx = integer_delta(&nodes[index], &nodes[index + 1])[0];
            if !equal(dx, 0.0) {
                might_be_passable = false;
            }
        }
    }
    if equal(dy, 0.0) {
        might_be_passable = true;
        for index in (1..nodes.len() - 1).rev() {
            dy = integer_delta(&nodes[index], &nodes[index + 1])[1];
            if !equal(dy, 0.0) {
                might_be_passable = false;
            }
        }
    }
    if equal(dx, dy) {
        might_be_passable = true;
        for index in (1..nodes.len() - 1).rev() {
            [dx, dy] = integer_delta(&nodes[index], &nodes[index + 1]);
            if !equal(dx, dy) {
                might_be_passable = false;
            }
        }
    }
    if equal(dx, -dy) {
        might_be_passable = true;
        for index in (1..nodes.len() - 1).rev() {
            [dx, dy] = integer_delta(&nodes[index], &nodes[index + 1]);
            if !equal(dy, -dx) {
                might_be_passable = false;
            }
        }
    }
    (allow_diagonal_step && diagonal_step) || might_be_passable
}

pub struct CellFlags {
    pub(crate) cliff: bool,
    pub(crate) pinched: bool,
    pub(crate) blocked_by_ally: bool,
}

impl super::Grid {
    pub(crate) fn path_cell_flags(
        &self,
        node: super::LayerCell,
        blocked_by_ally: bool,
    ) -> CellFlags {
        let node = self.resolve_layer(node);
        CellFlags {
            cliff: node.layer == 1
                && matches!(
                    self.source_kinds[self.source_index(node.cell)],
                    crate::terrain::CellType::Cliff
                ),
            pinched: node.layer == 1 && self.pinched.contains(&node.cell),
            blocked_by_ally,
        }
    }

    pub(crate) fn unpinched_line(&self, from: super::LayerCell, to: super::LayerCell) -> bool {
        super::search::line_cells(self, from.cell, to.cell)
            .into_iter()
            .all(|cell| {
                !self
                    .path_cell_flags(
                        super::LayerCell {
                            cell,
                            layer: from.layer,
                        },
                        false,
                    )
                    .pinched
            })
    }

    // Cell-domain previews share construction and optimization with the world
    // path. Their zero Z is not an actor sampling input or final terrain height.
    pub(super) fn preview_path(
        &self,
        raw: &[super::LayerCell],
        flags: &[CellFlags],
        centered: bool,
        mut check: impl FnMut(super::LayerCell, super::LayerCell) -> bool,
    ) -> BuiltPath {
        assert_eq!(raw.len(), flags.len());
        let cells: Vec<_> = raw
            .iter()
            .zip(flags)
            .map(|(node, flags)| {
                let point = super::search::cell_position(node.cell, centered);
                CellNode {
                    cell: [node.cell.x, node.cell.y],
                    position: [point[0], point[1], 0.0],
                    layer: node.layer,
                    cliff: flags.cliff,
                    pinched: flags.pinched,
                    blocked_by_ally: flags.blocked_by_ally,
                }
            })
            .collect();
        let mut path =
            prepend_cells(&cells, cells[0].position).expect("successful raw search path");
        path.optimize(|layer, from, to| {
            // Only cell-domain preview points use this conversion; final
            // construction supplies the actor's exact world coordinates.
            let cell = |point: [f32; 3]| {
                use num_traits::ToPrimitive;
                super::LayerCell {
                    cell: super::Cell {
                        x: (point[0] / 10.0).to_u16().expect("bounded preview x"),
                        y: (point[1] / 10.0).to_u16().expect("bounded preview y"),
                    },
                    layer,
                }
            };
            let from = cell(from);
            let to = cell(to);
            let flags = self.path_cell_flags(to, false);
            LineCheck {
                passable: self.unpinched_line(from, to) && check(from, to),
                unpinched_cliff: flags.cliff && !flags.pinched,
            }
        });
        path
    }
}

impl BuiltPath {
    pub(crate) fn optimized_cells(&self) -> std::collections::VecDeque<super::LayerCell> {
        self.optimized
            .iter()
            .map(|&index| {
                let node = &self.nodes[index];
                super::LayerCell {
                    cell: super::Cell {
                        x: node.cell[0],
                        y: node.cell[1],
                    },
                    layer: node.layer,
                }
            })
            .collect()
    }
}

impl BuiltPath {
    /// Native optimizeGroundPath uses the diameter callback and removes small
    /// intermediate turns. It has no ordinary first-layer or cliff exception.
    pub(crate) fn optimize_ground(
        &mut self,
        mut passable: impl FnMut(u8, [f32; 3], [f32; 3]) -> bool,
    ) {
        self.optimized.clear();
        if self.nodes.is_empty() {
            return;
        }
        self.optimized.push(0);
        let mut anchor = 0;
        while anchor + 1 < self.nodes.len() {
            let mut layer = self.nodes[anchor].layer;
            let mut current_layer = layer;
            let mut candidate = anchor + 1;
            while candidate + 1 < self.nodes.len() {
                let count = candidate - anchor;
                if current_layer == 1 {
                    if self.nodes[candidate].layer != current_layer {
                        layer = self.nodes[candidate].layer;
                        if count > 3 {
                            break;
                        }
                    }
                } else if self.nodes[candidate + 1].layer != current_layer && count > 3 {
                    break;
                }
                current_layer = self.nodes[candidate].layer;
                candidate += 1;
            }
            let mut next = anchor + 1;
            for index in (anchor + 1..=candidate).rev() {
                if passable(
                    layer,
                    self.nodes[anchor].position,
                    self.nodes[index].position,
                ) || straight_fallback(&self.nodes[anchor..=index], false)
                {
                    next = index;
                    break;
                }
            }
            self.optimized.push(next);
            anchor = next;
        }
        let mut index = 0;
        while index + 2 < self.optimized.len() {
            let a = self.nodes[self.optimized[index]].position;
            let b = self.nodes[self.optimized[index + 1]].position;
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let horizontal = dx * dx;
            let vertical = dy * dy;
            if horizontal + vertical < 10.0 * 10.0 * 3.9 {
                self.optimized.remove(index + 1);
            }
            index += 1;
        }
    }
}
