//! `DiscreteCircle` Bresenham edge generation, duplicate removal and scan order.

pub(super) fn cells(center: [i32; 2], radius: i32, dimensions: [u16; 2]) -> Vec<usize> {
    let mut edges = Vec::new();
    let mut x = 0;
    let mut y = radius;
    let mut decision = (1 - radius) * 2;
    while y >= 0 {
        if edges.last().is_some_and(|&(_, prior)| prior == y) {
            edges.pop();
        }
        edges.push((x, y));
        if decision + y > 0 {
            y -= 1;
            decision -= y * 2 - 1;
        }
        if x > decision {
            x += 1;
            decision += x * 2 + 1;
        }
    }
    let mut cells = Vec::new();
    for (x, y) in edges {
        line(
            &mut cells,
            center[0] - x,
            center[0] + x,
            center[1] + y,
            dimensions,
        );
        if y != 0 {
            line(
                &mut cells,
                center[0] - x,
                center[0] + x,
                center[1] - y,
                dimensions,
            );
        }
    }
    cells
}

fn line(cells: &mut Vec<usize>, left: i32, right: i32, y: i32, dimensions: [u16; 2]) {
    let Ok(y) = u16::try_from(y) else {
        return;
    };
    if y >= dimensions[1] {
        return;
    }
    for x in left.max(0)..=right.min(i32::from(dimensions[0]) - 1) {
        cells.push(
            usize::from(y) * usize::from(dimensions[0])
                + usize::try_from(x).expect("clipped column"),
        );
    }
}
