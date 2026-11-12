use crate::grid::{Cell, LayerCell};
use crate::skirmish::occupancy::Occupancy;

#[test]
fn pinned_native_info_lifecycles_preserve_coordinates_and_release_conditions() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/reference/path-info-reference.txt"
    ));
    let mut cases = 0;
    for line in fixture.lines().take(8) {
        let values: Vec<i32> = line
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect();
        let mode = values[0];
        let node = LayerCell {
            cell: Cell { x: 3, y: 4 },
            layer: 1,
        };
        let mut occupancy = Occupancy::new();
        assert_eq!(occupancy.claim_info(node), values[1] != 0);
        if mode & 2 != 0 {
            occupancy.positions.insert(node, 1);
        }
        if mode & 4 != 0 {
            occupancy.aircraft_goals.insert(node.cell, 2);
        }
        assert_eq!(occupancy.claim_info(node), values[2] != 0);
        occupancy.release_info(node, mode & 1 != 0);
        let coordinate = occupancy.infos.get(&node);
        assert_eq!(coordinate.is_some(), values[3] != 0);
        if let Some(coordinate) = coordinate {
            assert_eq!(i32::from(coordinate.coordinate.cell.x), values[4]);
            assert_eq!(i32::from(coordinate.coordinate.cell.y), values[5]);
        }
        occupancy.positions.clear();
        occupancy.aircraft_goals.clear();
        occupancy.release_info(node, false);
        assert_eq!(occupancy.infos.contains_key(&node), values[7] != 0);
        cases += 1;
    }
    assert_eq!(cases, 8);
    let pool: Vec<_> = fixture.lines().last().unwrap().split_whitespace().collect();
    assert_eq!(pool, ["pool", "30000", "0", "1"]);
}
