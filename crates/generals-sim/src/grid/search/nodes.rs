//! `PathfindCell::allocateInfo` allocates search state only for visited cells.
//! Keep indexed lookup without initializing a full map of nodes per search.
//! Native INFO allocation counts and open-list ordering remain in `Search`.

use super::Node;

const PAGE_SIZE: usize = 64;

pub(super) struct Pages {
    pages: Vec<Option<Box<[Option<Node>; PAGE_SIZE]>>>,
}

impl Pages {
    pub(super) fn new(cells: usize) -> Self {
        Self {
            pages: std::iter::repeat_with(|| None)
                .take(cells.div_ceil(PAGE_SIZE))
                .collect(),
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (usize, &Node)> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(index, page)| page.as_ref().map(|page| (index, page)))
            .flat_map(|(index, page)| {
                page.iter().enumerate().filter_map(move |(offset, node)| {
                    node.as_ref().map(|node| (index * PAGE_SIZE + offset, node))
                })
            })
    }

    pub(super) fn get(&self, index: usize) -> Option<&Node> {
        self.pages[index / PAGE_SIZE].as_ref()?[index % PAGE_SIZE].as_ref()
    }

    pub(super) fn get_mut(&mut self, index: usize) -> Option<&mut Node> {
        self.pages[index / PAGE_SIZE].as_mut()?[index % PAGE_SIZE].as_mut()
    }

    pub(super) fn insert(&mut self, index: usize, node: Node) {
        let page = self.pages[index / PAGE_SIZE]
            .get_or_insert_with(|| Box::new(std::array::from_fn(|_| None)));
        page[index % PAGE_SIZE] = Some(node);
    }
}

#[cfg(test)]
mod tests {
    use super::super::Cell;
    use super::*;

    #[test]
    fn sparse_visits_reopening_and_shifted_coordinates_match_dense_nodes() {
        let cells = 400 * 400;
        let mut pages = Pages::new(cells);
        let mut dense: Vec<Option<Node>> = std::iter::repeat_with(|| None).take(cells).collect();
        let indices = [0, 63, 64, 65, 399, 400, 79_999, 80_000, cells - 1];
        for sequence in 0..64_u16 {
            for &index in &indices {
                let node = || Node {
                    coordinate: Cell { x: sequence, y: 7 },
                    parent: (sequence != 0).then_some(Cell { x: 3, y: sequence }),
                    cost: 1000 - u32::from(sequence),
                    total: 2000 - u32::from(sequence),
                    insertion: u64::from(sequence),
                };
                pages.insert(index, node());
                dense[index] = Some(node());
            }
        }
        // Away/patch can change a starting node's coordinates without moving
        // its lookup slot. Neighbor slots and parent references stay distinct.
        pages.get_mut(63).unwrap().coordinate = Cell { x: 5, y: 6 };
        dense[63].as_mut().unwrap().coordinate = Cell { x: 5, y: 6 };
        for (index, expected) in dense.iter().enumerate() {
            let fields = |node: &Node| {
                (
                    node.coordinate,
                    node.parent,
                    node.cost,
                    node.total,
                    node.insertion,
                )
            };
            assert!(pages.get(index).map(fields) == expected.as_ref().map(fields));
        }
        // A short search touches only these pages, not 160,000 node slots.
        assert_eq!(pages.pages.iter().flatten().count(), 6);
    }

    #[test]
    fn final_partial_page_preserves_a_valid_edge_cell() {
        let mut pages = Pages::new(65);
        pages.insert(
            64,
            Node {
                coordinate: Cell { x: 12, y: 4 },
                parent: None,
                cost: 0,
                total: 0,
                insertion: 1,
            },
        );
        assert!(pages.get(0).is_none());
        assert!(pages.get(63).is_none());
        assert!(pages.get(64).is_some_and(|node| node.insertion == 1));
    }
}
