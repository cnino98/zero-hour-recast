use super::*;

#[test]
fn relative_tolerance_retains_exact_boundary_and_rejects_next_float_outside() {
    assert!(same_position([0.0; 2], [900.0, 0.0], [1000.0, 0.0]));
    assert!(!same_position(
        [0.0; 2],
        [f32::from_bits(900.0_f32.to_bits() - 1), 0.0],
        [1000.0, 0.0]
    ));
    assert!(same_position([0.0; 2], [990.0, 0.0], [1000.0, 0.0]));
    assert!(!same_position([950.0, 0.0], [990.0, 0.0], [1000.0, 0.0]));
    assert!(same_position([2.0, 3.0], [2.0, 3.0], [2.0, 3.0]));
}
