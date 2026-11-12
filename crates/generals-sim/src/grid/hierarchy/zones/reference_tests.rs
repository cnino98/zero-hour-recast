use super::{Cell, Grid, Zones};
use crate::grid::LayerCell;
use crate::terrain::CellType;
use std::sync::Arc;

const MASKS: [u8; 14] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 16, 17, 19, 21, 31];
const WIDTH: u16 = 23;
const HEIGHT: u16 = 17;

fn environment(mode: u16) -> Grid {
    let mut grid = Grid::new(WIDTH, HEIGHT, []).unwrap();
    for x in 0..WIDTH {
        for y in 0..HEIGHT {
            let kind = match mode {
                1 => (x / 3 + y / 2) % 7,
                2 => {
                    if x % 10 == 9 {
                        4
                    } else {
                        u16::from(y == 6)
                    }
                }
                3 => {
                    if x < 8 {
                        2
                    } else if y < 7 {
                        1
                    } else if x < 17 {
                        3
                    } else {
                        0
                    }
                }
                4 => ((x * 13 + y * 7 + x * y) / 5) % 7,
                5 if x == 10 => 6,
                _ => 0,
            };
            let cell = Cell { x, y };
            let index = grid.source_index(cell);
            Arc::make_mut(&mut grid.source_kinds)[index] = match kind {
                1 => CellType::Water,
                2 => CellType::Cliff,
                3 => CellType::Rubble,
                4 => CellType::Obstacle,
                5 => CellType::BridgeImpassable,
                6 => CellType::Impassable,
                _ => CellType::Clear,
            };
            if kind == 4 && (x + y) % 2 == 0 {
                Arc::make_mut(&mut grid.fences).insert(LayerCell { cell, layer: 1 });
            }
        }
    }
    if mode == 5 {
        let endpoints = [Cell { x: 5, y: 5 }, Cell { x: 20, y: 12 }];
        Arc::make_mut(&mut grid.bridge_ends).insert(2, endpoints);
        for cell in endpoints {
            Arc::make_mut(&mut grid.ground_connections).insert(cell, 2);
        }
    }
    grid.recalculate_source_zones();
    grid
}

fn phase(grid: &mut Grid, phase: u16) {
    let mut zones = grid.source_zones.take().unwrap();
    match phase {
        1 => {
            let mut cells = Vec::new();
            for x in 8..=12 {
                for y in 4..=10 {
                    let cell = Cell { x, y };
                    let index = grid.source_index(cell);
                    Arc::make_mut(&mut grid.source_kinds)[index] = CellType::Obstacle;
                    Arc::make_mut(&mut grid.fences).remove(&LayerCell { cell, layer: 1 });
                    cells.push(cell);
                }
            }
            Arc::make_mut(&mut zones).invalidate_cells(grid, cells);
            Arc::make_mut(&mut zones).update_for_modify(grid, [[8, 4], [12, 10]]);
        }
        2 => {
            let cell = Cell { x: 9, y: 7 };
            let index = grid.source_index(cell);
            Arc::make_mut(&mut grid.source_kinds)[index] = CellType::Clear;
            Arc::make_mut(&mut zones).invalidate_cells(grid, [cell]);
        }
        3 => {
            let cell = Cell { x: 0, y: 0 };
            Arc::make_mut(&mut grid.source_kinds)[0] = CellType::Obstacle;
            Arc::make_mut(&mut zones).invalidate_cells(grid, [cell]);
            Arc::make_mut(&mut zones).update_for_modify(grid, [[-3, -3], [-1, -1]]);
        }
        4 => zones = Arc::new(Zones::calculate(grid)),
        _ => {}
    }
    grid.source_zones = Some(zones);
}

fn numbers(line: &str) -> Vec<u32> {
    line.split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect()
}

#[test]
fn pinned_native_raw_block_global_and_partial_zone_tables_match() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/reference/zone-reference.txt"
    ));
    let mut lines = fixture.lines();
    for mode in 0..6 {
        let mut grid = environment(mode);
        for step in 0..5 {
            phase(&mut grid, step);
            let zones = grid.source_zones.as_ref().unwrap();
            let maximum = zones.upper(15) + 1;
            assert_eq!(
                numbers(lines.next().unwrap()),
                vec![
                    u32::from(mode),
                    u32::from(step),
                    u32::from(WIDTH),
                    u32::from(HEIGHT),
                    maximum
                ]
            );
            let cells = || (0..HEIGHT).flat_map(|y| (0..WIDTH).map(move |x| Cell { x, y }));
            let raw: Vec<_> = cells().map(|cell| zones.raw(&grid, cell)).collect();
            assert_eq!(
                raw,
                numbers(lines.next().unwrap()),
                "raw mode {mode} phase {step}"
            );
            let bridges: Vec<_> = (0..3)
                .flat_map(|x| (0..2).map(move |y| Cell { x, y }))
                .map(|cell| u32::from(zones.interacts_with_bridge(cell)))
                .collect();
            assert_eq!(
                bridges,
                numbers(lines.next().unwrap()),
                "bridges mode {mode} phase {step}"
            );
            let upper: Vec<_> = (0..=15).map(|layer| zones.upper(layer)).collect();
            assert_eq!(upper, numbers(lines.next().unwrap()));
            let mut blocks = Vec::new();
            let mut global = Vec::new();
            for crusher in [false, true] {
                for mask in MASKS {
                    blocks.extend(
                        cells().map(|cell| zones.effective_block(&grid, mask, crusher, cell)),
                    );
                    global.extend(
                        (0..maximum).map(|zone| zones.effective_global(mask, crusher, zone)),
                    );
                }
            }
            assert_eq!(
                blocks,
                numbers(lines.next().unwrap()),
                "block mode {mode} phase {step}"
            );
            assert_eq!(
                global,
                numbers(lines.next().unwrap()),
                "global mode {mode} phase {step}"
            );
            let terrain: Vec<_> = (0..maximum).map(|zone| zones.terrain(zone)).collect();
            assert_eq!(
                terrain,
                numbers(lines.next().unwrap()),
                "terrain mode {mode} phase {step}"
            );
        }
    }
    assert!(lines.next().is_none());
}
