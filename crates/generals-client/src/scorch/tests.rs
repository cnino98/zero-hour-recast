use super::*;

#[test]
fn source_deduplication_uses_incoming_radius_strict_bounds_and_ignores_z() {
    let mut marks = Marks::new(true);
    marks.add([0.0; 3], 8.0, 0);
    marks.dirty = false;
    marks.add([1.0, 1.0, 999.0], 10.0, 0);
    assert_eq!(marks.entries.len(), 1);
    assert!(!marks.dirty);
    marks.add([2.0, 0.0, 0.0], 8.0, 0);
    marks.add([0.0; 3], 8.0, 1);
    assert_eq!(marks.entries.len(), 3);
    let mut reverse = Marks::new(true);
    reverse.add([0.0; 3], 10.0, 0);
    reverse.add([0.0; 3], 8.0, 0);
    assert_eq!(reverse.entries.len(), 2);
}

#[test]
fn native_fifo_retires_oldest_of_500_and_static_mode_retains_duplicates() {
    let mut marks = Marks::new(true);
    for index in 0..501_u16 {
        marks.add([f32::from(index) * 10.0, 0.0, 0.0], 4.0, -1);
    }
    assert_eq!(marks.entries.len(), 500);
    assert_eq!(
        marks.entries.front().unwrap().position[0].to_bits(),
        10.0_f32.to_bits()
    );
    assert_eq!(
        marks.entries.back().unwrap().position[0].to_bits(),
        5000.0_f32.to_bits()
    );
    assert!(marks.entries.iter().all(|mark| mark.kind == 0));
    let mut static_marks = Marks::new(false);
    for _ in 0..2 {
        static_marks.add([0.0; 3], 4.0, 4);
    }
    assert_eq!(static_marks.entries.len(), 2);
}

#[test]
fn resetting_before_new_fx_preserves_first_new_match_scorch() {
    let mut marks = Scorches::new(None);
    marks.add([10.0, 10.0, 0.0], 8.0, 1);
    marks.clear();
    marks.add([20.0, 20.0, 0.0], 8.0, 2);
    assert_eq!(marks.marks.entries.len(), 1);
    assert_eq!(marks.marks.entries[0].kind, 2);
    assert!(marks.marks.dirty);
}
