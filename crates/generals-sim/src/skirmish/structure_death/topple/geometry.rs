//! `Matrix3D::In_Place_Pre_Rotate_*`: rotate rows, preserving native products.
pub(super) use crate::skirmish::physical_ocl::transform::pre_rotate;

pub(super) fn crushing_point(base: [f32; 2], direction: f32, lateral: f32) -> [f32; 2] {
    let x = lateral * direction.sin();
    let y = lateral * direction.cos();
    // Original doDamageLine uses plus for both terms, including the right edge.
    [base[0] + x, base[1] + y]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pre_rotation_changes_world_rows_and_preserves_existing_matrix_drift() {
        let mut basis = [[1.0, 0.125, 0.0], [0.0, 0.9, 0.1], [0.05, 0.0, 1.1]];
        let first = basis[0];
        pre_rotate(&mut basis, 0, std::f32::consts::FRAC_PI_2);
        assert_eq!(basis[0].map(f32::to_bits), first.map(f32::to_bits));
        assert!((basis[1][0] + 0.05).abs() < 0.000_001);
        assert!((basis[1][2] + 1.1).abs() < 0.000_001);
        assert!((basis[2][1] - 0.9).abs() < 0.000_001);
        assert!((basis[2][2] - 0.1).abs() < 0.000_001);
        let second = basis[1];
        pre_rotate(&mut basis, 1, 0.2);
        assert_eq!(basis[1].map(f32::to_bits), second.map(f32::to_bits));
    }

    #[test]
    fn native_diagonal_crush_uses_two_plus_terms_even_when_that_is_not_perpendicular() {
        let direction = std::f32::consts::FRAC_PI_4;
        let at = crushing_point([100.0, 200.0], direction, 25.0);
        assert!(at[0] > 100.0 && at[1] > 200.0);
        assert!(((at[0] - 100.0) - (at[1] - 200.0)).abs() < 0.000_02);
    }
}
