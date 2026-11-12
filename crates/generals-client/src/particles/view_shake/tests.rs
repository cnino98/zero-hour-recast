use super::Shake;
use crate::particles::{Pose, effects, tests::system};
use generals_assets::{content, virtual_assets::Asset};

#[test]
fn native_fx_parsing_draws_one_angle_and_applies_xy_falloff_before_fixed_decay() {
    let file = content::scan(Asset {
        path: "fx.ini".into(),
        origin: "source fixture".into(),
        bytes: b"FXList Shot\nViewShake\nEnd\nEnd\n".to_vec(),
    })
    .unwrap();
    let mut particles = system(Vec::new());
    let list = effects::parse_list(
        std::slice::from_ref(&file),
        &file.declarations[0].lines,
        &[],
        &mut particles.lists,
        true,
    )
    .unwrap();
    particles.lists.push(list);
    particles.shake.focus = [100.0, 200.0];
    let mut draws = 0;
    particles.fire(
        0,
        &Pose::at([175.0, 200.0, 5000.0]),
        &[],
        &|_| panic!("view shake does not sample terrain"),
        0.0,
        &mut effects::rng::real_only(|low, high| {
            assert_eq!(low.to_bits(), 0.0_f32.to_bits());
            assert_eq!(high.to_bits(), std::f32::consts::TAU.to_bits());
            draws += 1;
            0.0
        }),
    );
    assert_eq!(draws, 1);
    assert_eq!(
        particles.shake.offset.map(f32::to_bits),
        [0.0_f32; 2].map(f32::to_bits)
    );
    particles.shake.advance();
    assert_eq!(particles.camera_shake().x.to_bits(), 0.05_f32.to_bits());
    particles.shake.advance();
    assert_eq!(particles.shake.offset[0].to_bits(), (-0.375_f32).to_bits());
    particles.shake.advance();
    assert_eq!(particles.shake.offset[0].to_bits(), 0.28125_f32.to_bits());
}

#[test]
fn distant_events_replace_direction_without_intensity_and_cutoff_clears_offset() {
    let mut shake = Shake::new();
    shake.impulse(1, [0.0; 3], 0.0);
    shake.impulse(5, [151.0, 0.0, 0.0], std::f32::consts::PI);
    assert_eq!(shake.intensity.to_bits(), 1.0_f32.to_bits());
    shake.advance();
    assert!((shake.offset[0] + 1.0).abs() < 0.00001);
    for _ in 1..17 {
        shake.advance();
    }
    assert!(shake.offset[0].abs() > 0.0);
    shake.advance();
    assert_eq!(
        shake.offset.map(f32::to_bits),
        [0.0_f32; 2].map(f32::to_bits)
    );
}

#[test]
fn source_threshold_clamps_to_literal_three_and_generation_reset_keeps_loaded_rules() {
    let file = content::scan(Asset {
        path: "gamedata.ini".into(),
        origin: "source fixture".into(),
        bytes: b"GameData\nShakeNormalIntensity=7\nMaxShakeIntensity=7\nMaxShakeRange=500\nEnd\n"
            .to_vec(),
    })
    .unwrap();
    let mut particles = system(Vec::new());
    particles.shake = Shake::load(&file.declarations[0].lines).unwrap();
    particles.shake.impulse(1, [0.0; 3], 0.0);
    assert_eq!(particles.shake.intensity.to_bits(), 7.0_f32.to_bits());
    particles.shake.impulse(0, [0.0; 3], 0.0);
    assert_eq!(particles.shake.intensity.to_bits(), 3.0_f32.to_bits());
    particles.shake.advance();
    particles.begin(1, 1);
    assert_eq!(particles.shake.intensity.to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        particles.shake.offset.map(f32::to_bits),
        [0.0_f32; 2].map(f32::to_bits)
    );
    particles.shake.impulse(1, [250.0, 0.0, 0.0], 0.0);
    assert_eq!(particles.shake.intensity.to_bits(), 3.5_f32.to_bits());
    assert!(super::kind(Some("UNKNOWN")).is_err());
}
