use super::*;

fn rules() -> Rules {
    Rules {
        stiffness: [0.1; 2],
        damping: [0.9; 2],
        axial_damping: 1.0,
        velocity: [0.2, 0.4, 0.0],
        acceleration: [0.5, 0.3],
        pitch_limits: [0.7, 0.5],
        rudder: [0.0; 2],
        elevator: [0.0; 2],
    }
}

#[test]
fn native_output_precedes_current_velocity_kick_and_render_repeats_do_not_integrate() {
    let rules = rules();
    let motion = Motion {
        rules: &rules,
        velocity: [2.0, 1.0, 0.0],
        acceleration: [0.2, -0.1, 0.0],
        motive: true,
    };
    let mut pose = Pose::new();
    pose.advance(20, 0.0, &motion);
    assert_eq!(pose.total.map(f32::to_bits), [0.0_f32; 3].map(f32::to_bits));
    pose.advance(21, 0.0, &motion);
    let output = pose.total;
    assert!(output[0] < 0.0 && output[1] < 0.0);
    let internal = pose.angle;
    for _ in 0..100 {
        pose.advance(21, 0.0, &motion);
    }
    assert_eq!(pose.total.map(f32::to_bits), output.map(f32::to_bits));
    assert_eq!(pose.angle.map(f32::to_bits), internal.map(f32::to_bits));
    pose.advance(24, 0.0, &motion);
    assert_eq!(
        pose.total.map(f32::to_bits),
        pose.previous.map(f32::to_bits)
    );
}

#[test]
fn inactive_motive_force_still_damps_saved_banking_without_new_motion_kicks() {
    let rules = rules();
    let mut pose = Pose::new();
    pose.angle = [-0.4, 0.3];
    let motion = Motion {
        rules: &rules,
        velocity: [100.0; 3],
        acceleration: [100.0; 3],
        motive: false,
    };
    for frame in 0..100 {
        pose.advance(frame, 0.0, &motion);
    }
    assert!(pose.total[0].abs() < 0.0001 && pose.total[1].abs() < 0.0001);
}

#[test]
fn hover_wing_banking_matches_extracted_original_cpp_bits() {
    // Verbatim native function, clang++ -ffp-contract=off; retained output:
    // output/aircraft-pose-native-reference.log (SH 2bfaea60).
    let rules = rules();
    let motion = Motion {
        rules: &rules,
        velocity: [2.0, 1.0, 0.0],
        acceleration: [0.2, -0.1, 0.0],
        motive: true,
    };
    let mut pose = Pose::new();
    let expected = [
        [0x0000_0000, 0x0000_0000, 0x0000_0000],
        [0xbebd_70a4, 0xbeb6_c8b5, 0x0000_0000],
        [0xbf33_3333, 0xbf2c_8b44, 0x0000_0000],
        [0xbf7e_3540, 0xbf74_8e8b, 0x0000_0000],
        [0xbfa0_624f, 0xbf9a_3a2a, 0x0000_0000],
        [0xbfbd_e9e2, 0xbfb6_934b, 0x0000_0000],
        [0xbfd8_1d7e, 0xbfcf_ba88, 0x0000_0000],
        [0xbfef_5d25, 0xbfe6_0c10, 0x0000_0000],
    ];
    for (frame, bits) in expected.into_iter().enumerate() {
        pose.advance(u64::try_from(frame).unwrap(), 0.0, &motion);
        assert_eq!(pose.total.map(f32::to_bits), bits);
    }
}
