use crate::grid::{Cell, Grid, LayerCell, ZoneCellChange, ZoneModification};
use crate::terrain::CellType;

#[test]
fn native_partial_zone_fill_precedes_post_classification_and_deferred_rebuild() {
    let original = Grid::new(20, 20, []).unwrap();
    let cell = Cell { x: 5, y: 5 };
    let mut updated = original.clone();
    let event = ZoneModification {
        update_bounds: Some([[5, 5], [5, 5]]),
        cells: vec![ZoneCellChange {
            cell,
            kind: CellType::Obstacle,
            fence: false,
            reset_zone: true,
        }],
        post_cells: vec![ZoneCellChange {
            cell,
            kind: CellType::Clear,
            fence: false,
            reset_zone: false,
        }],
        pinched: vec![(cell, true)],
        force_dirty: true,
    };
    assert!(updated.inherit_source_zones(&original, &[event]));
    let node = LayerCell { cell, layer: 1 };
    // No same-type OBSTACLE neighbor supplied a raw zone during partial fill.
    // The later clear does not reset/flood again; full classification is deferred.
    assert_eq!(updated.source_zone(node), 0);
    updated.recalculate_source_zones();
    assert_ne!(updated.source_zone(node), 0);
}

#[test]
fn zero_owner_rubble_clear_does_not_reset_zone_or_force_dirty() {
    let original = Grid::new(20, 20, []).unwrap();
    let cell = Cell { x: 5, y: 5 };
    let mut updated = original.clone();
    let event = ZoneModification {
        update_bounds: None,
        cells: vec![ZoneCellChange {
            cell,
            kind: CellType::Clear,
            fence: false,
            reset_zone: false,
        }],
        post_cells: vec![],
        pinched: vec![],
        force_dirty: false,
    };
    assert!(!updated.inherit_source_zones(&original, &[event]));
    let node = LayerCell { cell, layer: 1 };
    assert_eq!(original.source_zone(node), updated.source_zone(node));
}
