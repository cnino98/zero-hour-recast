use super::*;

#[test]
fn native_point_cap_precedes_holes_and_does_not_invent_nozzle_or_single_point_lines() {
    let points = [
        [1.0; 3], [2.0; 3], [0.0; 3], [3.0; 3], [4.0; 3], [0.0; 3], [5.0; 3], [0.0; 3], [6.0; 3],
        [7.0; 3],
    ];
    let selected = runs(&points, 7);
    assert_eq!(selected.len(), 2);
    assert!(std::ptr::eq(selected[0].as_ptr(), points[3..].as_ptr()));
    assert!(std::ptr::eq(selected[1].as_ptr(), points[8..].as_ptr()));
    assert_eq!(runs(&points, 0).len(), 3);
    assert_eq!(runs(&points, 1).len(), 0);
}

#[test]
fn native_scroll_reuses_line_age_includes_hidden_elapsed_and_wraps_unsigned_clock() {
    let mut line = Line::new(100);
    assert_eq!(line.advance(100, 6.0).to_bits(), 0.0_f32.to_bits());
    let first = 6.0_f32 * 0.001 * 33.0;
    assert_eq!(line.advance(133, 6.0).to_bits(), first.to_bits());
    let elapsed = 6.0_f32 * 0.001 * 10_003.0;
    let next = first + elapsed;
    assert_eq!(
        line.advance(10_136, 6.0).to_bits(),
        (next - next.floor()).to_bits()
    );
    let mut wrapped = Line::new(u32::MAX - 9);
    assert_eq!(wrapped.advance(23, 6.0).to_bits(), first.to_bits());
    assert_eq!(
        logic_milliseconds(1, std::time::Duration::from_micros(32_800)),
        66
    );
}

#[test]
fn segmented_texture_preserves_the_shared_source_silhouette_and_original_streak_uv() {
    let points = [
        Vec3::new(-2.0, 1.0, -20.0),
        Vec3::new(0.0, 1.0, -20.0),
        Vec3::new(2.0, 1.0, -20.0),
    ];
    let streak =
        crate::particles::streak::geometry(&mut points.clone(), &[0.075; 3], &[0, 1, 1]).unwrap();
    let line =
        crate::particles::streak::segmented_geometry(&mut points.clone(), 0.075, 2.0).unwrap();
    assert_eq!(streak.indices, line.indices);
    assert_eq!(
        streak
            .vertices
            .iter()
            .map(|point| point.to_array().map(f32::to_bits))
            .collect::<Vec<_>>(),
        line.vertices
            .iter()
            .map(|point| point.to_array().map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        streak
            .uv
            .iter()
            .map(|point| point.map(f32::to_bits))
            .collect::<Vec<_>>(),
        [
            [0.0_f32, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 0.0]
        ]
        .map(|point| point.map(f32::to_bits))
    );
    assert_eq!(
        line.uv
            .iter()
            .map(|point| point.map(f32::to_bits))
            .collect::<Vec<_>>(),
        [
            [0.0_f32, 0.0],
            [1.0, 0.0],
            [0.0, 2.0],
            [1.0, 2.0],
            [0.0, 4.0],
            [1.0, 4.0]
        ]
        .map(|point| point.map(f32::to_bits))
    );
}
