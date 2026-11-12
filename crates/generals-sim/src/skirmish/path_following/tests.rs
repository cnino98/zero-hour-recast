use super::{Node, Path};

fn path(points: &[[f32; 3]]) -> Path {
    Path::new(
        points
            .iter()
            .map(|&position| Node {
                position,
                layer: 1,
                previous_layer: 1,
                can_optimize: true,
            })
            .collect(),
    )
}

#[test]
fn bridge_segments_preserve_source_layer_selection_and_disable_ordinary_lookahead() {
    let mut path = Path::new(
        [
            ([0.0, 0.0, 0.0], 1),
            ([10.0, 0.0, 0.0], 1),
            ([10.0, 0.0, 20.0], 2),
            ([30.0, 0.0, 20.0], 2),
            ([30.0, 0.0, 0.0], 1),
            ([50.0, 0.0, 0.0], 1),
        ]
        .into_iter()
        .scan(1, |previous, (position, layer)| {
            let previous_layer = *previous;
            *previous = layer;
            Some(Node {
                position,
                layer,
                previous_layer,
                can_optimize: true,
            })
        })
        .collect(),
    );
    let mut tested_layers = Vec::new();
    let sample = path.sample_layer([20.0, 0.0, 20.0], 2, |layer, _, _| {
        tested_layers.push(layer);
        true
    });
    assert_eq!(sample.layer, 2);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [30.0, 0.0, 20.0].map(f32::to_bits)
    );
    assert!(tested_layers.iter().all(|&layer| layer == 2));
    let exited = path.sample_layer([40.0, 0.0, 0.0], 1, |layer, _, _| layer == 1);
    assert_eq!(exited.layer, 1);
}

#[test]
fn look_ahead_checks_the_next_segment_but_preserves_the_near_corner_exception() {
    let nodes = [[0.0, 0.0, 2.0], [10.0, 0.0, 3.0], [10.0, 10.0, 4.0]];
    let mut route = path(&nodes);
    let mut checked = Vec::new();
    let sample = route.sample_layer([6.0, 0.0, 2.0], 1, |_, _, goal| {
        checked.push(goal);
        goal[1] == 0.0
    });
    assert_eq!(checked.len(), 2);
    assert_eq!(
        sample.position.map(f32::to_bits),
        nodes[1].map(f32::to_bits)
    );
    assert_eq!(sample.distance.to_bits(), 14.0_f32.to_bits());

    checked.clear();
    let sample = route.sample_layer([9.5, 0.0, 2.0], 1, |_, _, goal| {
        checked.push(goal);
        goal[1] == 0.0
    });
    assert_eq!(checked.len(), 1);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [10.0, 5.0, 3.0].map(f32::to_bits)
    );
}

#[test]
fn nonoptimizable_node_blocks_early_look_ahead_but_not_native_close_override() {
    let mut route = path(&[[0.0; 3], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]]);
    route.nodes[1].can_optimize = false;
    let sample = route.sample_layer([6.0, 0.0, 0.0], 1, |_, _, _| true);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [10.0, 0.0, 0.0].map(f32::to_bits)
    );
    let sample = route.sample_layer([9.5, 0.0, 0.0], 1, |_, _, _| true);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [10.0, 5.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn blocked_off_path_motion_uses_source_projection_and_halfway_probe() {
    let mut route = path(&[[0.0, 0.0, 4.0], [100.0, 0.0, 8.0]]);
    let sample = route.sample_layer([20.0, 30.0, 6.0], 1, |_, _, goal| {
        goal[0].to_bits() == 60.0_f32.to_bits()
    });
    assert_eq!(
        sample.position.map(f32::to_bits),
        [60.0, 0.0, 4.0].map(f32::to_bits)
    );

    let mut route = path(&[[0.0, 0.0, 4.0], [100.0, 0.0, 8.0]]);
    let sample = route.sample_layer([20.0, 30.0, 6.0], 1, |_, _, _| false);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [20.0, 0.0, 4.0].map(f32::to_bits)
    );
    assert_eq!(sample.distance.to_bits(), 80.0_f32.to_bits());
}

#[test]
fn cache_expires_after_twenty_near_position_calls_and_checks_height() {
    let mut route = path(&[[0.0; 3], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]]);
    let sample = route.sample_layer([6.0, 0.0, 0.0], 1, |_, _, _| true);
    for _ in 0..20 {
        let cached = route.sample_layer([6.05, 0.0, 0.0], 1, |_, _, _| {
            panic!("A near-position sample should still use its bounded cache")
        });
        assert_eq!(
            cached.position.map(f32::to_bits),
            sample.position.map(f32::to_bits)
        );
        assert_eq!(cached.distance.to_bits(), sample.distance.to_bits());
    }
    let refreshed = route.sample_layer([6.05, 0.0, 0.0], 1, |_, _, _| false);
    assert_eq!(
        refreshed.position.map(f32::to_bits),
        [10.0, 0.0, 0.0].map(f32::to_bits)
    );
    let raised = route.sample_layer([6.05, 0.0, 0.11], 1, |_, _, _| true);
    assert_eq!(
        raised.position.map(f32::to_bits),
        sample.position.map(f32::to_bits)
    );
}

#[test]
fn ground_arrival_uses_final_distance_and_strict_locomotor_threshold() {
    let mut route = path(&[[0.0; 3], [10.0, 0.0, 0.0]]);
    let position = [11.0, 0.0, 0.0];
    let sample = route.sample_layer(position, 1, |_, _, _| true);
    // The projection is beyond the end, but the final-position correction must
    // keep the unit moving until its real distance is strictly below the cap.
    assert_eq!(sample.distance.to_bits(), 0.0_f32.to_bits());
    let distance = route.distance_to_goal(position, &sample);
    assert_eq!(distance.to_bits(), 1.0_f32.to_bits());
    assert!(!route.arrived(position, distance, 1.0));
    assert!(route.arrived([10.5, 0.0, 0.0], 0.5, 1.0));
    assert!(!route.arrived([51.0, 0.0, 0.0], 0.5, 1.0));

    let position = [0.0, 40.0, 0.0];
    let sample = route.sample_layer(position, 1, |_, _, _| true);
    assert_eq!(
        route.distance_to_goal(position, &sample).to_bits(),
        1700.0_f32.sqrt().to_bits()
    );
}

#[test]
fn coincident_nodes_keep_the_original_finite_segment_guard() {
    let mut route = path(&[[0.0; 3], [0.0; 3], [10.0, 0.0, 0.0]]);
    let sample = route.sample_layer([0.0; 3], 1, |_, _, _| true);
    assert!(sample.distance.is_finite());
    assert_eq!(sample.distance.to_bits(), 10.01_f32.to_bits());
    assert_eq!(
        sample.position.map(f32::to_bits),
        [5.0, 0.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn waypoint_arrival_uses_passed_segment_projection_with_ground_sanity_limit() {
    let mut route = path(&[[300.0, 405.0, 0.0], [405.0, 405.0, 0.0]]);
    let passed = [406.0, 418.0, 0.0];
    let sample = route.sample_layer(passed, 1, |_, _, _| true);
    assert!(route.distance_to_goal(passed, &sample) > 13.0);
    let distance = route.waypoint_distance_to_goal(passed);
    assert_eq!(distance.to_bits(), 0.0_f32.to_bits());
    assert!(route.arrived(passed, distance, 1.0));
    assert!(route.arrived([405.0, 445.0, 0.0], distance, 1.0));
    assert!(!route.arrived([406.0, 446.0, 0.0], distance, 1.0));
}

#[test]
fn flight_distance_keeps_passed_segments_retired_when_position_reverses() {
    let mut route = path(&[
        [0.0; 3],
        [100.0, 0.0, 0.0],
        [100.0, 100.0, 0.0],
        [200.0, 100.0, 0.0],
    ]);
    assert_eq!(
        route.flight_sample([101.0, 101.0, 0.0]).distance.to_bits(),
        99.0_f32.to_bits()
    );
    let reversed = route.flight_sample([50.0, 50.0, 1000.0]);
    assert_eq!(reversed.distance.to_bits(), 150.0_f32.to_bits());
    assert_eq!(
        reversed.position.map(f32::to_bits),
        [200.0_f32, 100.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(
        route
            .waypoint_distance_to_goal([50.0, 50.0, 1000.0])
            .to_bits(),
        150.0_f32.to_bits()
    );
}

#[test]
fn bridge_exit_uses_raw_predecessor_layer_after_optimization_skips_it() {
    let mut route = Path::new(vec![
        Node {
            position: [0.0, 0.0, 20.0],
            layer: 2,
            previous_layer: 1,
            can_optimize: true,
        },
        Node {
            position: [20.0, 0.0, 0.0],
            layer: 1,
            previous_layer: 1,
            can_optimize: true,
        },
        Node {
            position: [40.0, 0.0, 0.0],
            layer: 1,
            previous_layer: 1,
            can_optimize: true,
        },
    ]);
    // A ground node between the optimized first and second node was skipped.
    // Native getPrevious sees it, rather than the optimized upper-layer node.
    let sample = route.sample_layer([25.0, 0.0, 0.0], 1, |_, _, _| true);
    assert_eq!(sample.layer, 2);
}

#[test]
fn midpoint_probe_uses_next_node_layer_even_when_selected_layer_differs() {
    let mut route = Path::new(vec![
        Node {
            position: [0.0, 0.0, 20.0],
            layer: 2,
            previous_layer: 1,
            can_optimize: true,
        },
        Node {
            position: [10.0, 0.0, 0.0],
            layer: 1,
            previous_layer: 2,
            can_optimize: true,
        },
        Node {
            position: [20.0, 0.0, 0.0],
            layer: 1,
            previous_layer: 1,
            can_optimize: true,
        },
        Node {
            position: [30.0, 0.0, 0.0],
            layer: 1,
            previous_layer: 1,
            can_optimize: true,
        },
        Node {
            position: [30.0, 10.0, 0.0],
            layer: 1,
            previous_layer: 1,
            can_optimize: true,
        },
    ]);
    let mut layers = Vec::new();
    let sample = route.sample_layer([26.0, 0.0, 0.0], 1, |layer, _, _| {
        layers.push(layer);
        layer == 2
    });
    assert_eq!(layers, [2, 1]);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [30.0_f32, 0.0, 0.0].map(f32::to_bits)
    );
}
