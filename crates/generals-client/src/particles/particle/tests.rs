use crate::particles::{
    Particle, Pose, Shader, Template,
    tests::{system, template},
};

fn generated(up: bool, origin: [f32; 3]) -> (Particle, Template) {
    let mut rules = template("orientation");
    rules.up_towards_emitter = up;
    let mut particles = system(vec![rules]);
    let mut random = |low, _| low;
    particles.create(0, Pose::at(origin), None, &mut random);
    let particle =
        particles.emitters[0].generate(&particles.templates[0], 1.0, 1, 1, 0, &mut random);
    // The emitter can move or disappear; existing particles keep its birth
    // position. Returning the particle exercises that independent lifetime.
    (particle, particles.templates.remove(0))
}

#[test]
fn zero_lifetime_stays_zero_and_preserves_birth_then_color_or_alpha_expiry() {
    for additive in [false, true] {
        let (mut particle, mut rules) = generated(false, [0.0; 3]);
        rules.drawing.shader = if additive {
            Shader::Additive
        } else {
            Shader::Alpha
        };
        particle.remaining = 0.0;
        particle.alpha_keys[1].1 = 0;
        particle.color_keys[1].1 = 0;
        particle.alpha = 0.0;
        particle.color = [0.0; 3];
        assert!(particle.update(&rules, particle.born));
        particle.alpha = 1.0;
        particle.color = [3.0 / 255.0, 0.0, 0.0];
        for frame in 1..100 {
            assert!(particle.update(&rules, particle.born + frame));
            assert_eq!(particle.remaining.to_bits(), 0.0_f32.to_bits());
        }
        particle.alpha = 0.0;
        particle.color = [0.0; 3];
        assert!(!particle.update(&rules, particle.born + 100));
        let (mut finite, _) = generated(false, [0.0; 3]);
        finite.remaining = 1.0;
        assert!(!finite.update(&rules, finite.born));
    }
}

#[test]
fn cleanup_trail_keeps_original_descending_color_keys_and_unsigned_interval() {
    let (mut particle, mut rules) = generated(false, [0.0; 3]);
    rules.drawing.shader = Shader::Additive;
    particle.color_keys[0] = ([64.0 / 255.0, 0.0, 1.0], 50);
    particle.color_keys[1] = ([0.0; 3], 10);
    particle.color = particle.color_keys[0].0;
    particle.color_scale = 0.0;
    particle.color_target = 1;
    particle.color_rate();
    let expected = particle.color.map(|value| (0.0 - value) / 4_294_967_296.0);
    assert_eq!(
        particle.color_rate.map(f32::to_bits),
        expected.map(f32::to_bits)
    );
    particle.draw(&rules, 1.0, None);
    assert_eq!(
        particle.color.map(f32::to_bits),
        particle.color_keys[0].0.map(f32::to_bits)
    );
}

#[test]
fn alpha_test_particles_keep_zero_alpha_and_black_color_until_native_lifetime_ends() {
    let (mut particle, mut rules) = generated(false, [0.0; 3]);
    rules.drawing.shader = Shader::AlphaTest;
    particle.remaining = 3.0;
    particle.alpha_keys[1].1 = 0;
    particle.color_keys[1].1 = 0;
    particle.alpha = 0.0;
    particle.color = [0.0; 3];
    assert!(particle.update(&rules, particle.born));
    assert!(particle.update(&rules, particle.born + 1));
    assert!(!particle.update(&rules, particle.born + 2));
    particle.remaining = 0.0;
    assert!(particle.update(&rules, particle.born + 3));
}

#[test]
fn up_towards_emitter_preserves_zero_and_orthogonal_source_angles() {
    let (mut particle, rules) = generated(true, [10.0, 20.0, 30.0]);
    particle.velocity = [0.0; 3];
    let pi = std::f32::consts::PI;
    for (direction, expected) in [
        ([0.0, 0.0], pi),
        ([1.0, 0.0], 2.0 * pi),
        ([-1.0, 0.0], pi),
        ([0.0, 1.0], pi),
        ([0.0, -1.0], 0.0),
        ([1.0, 1.0], 1.25 * pi),
        ([-1.0, 1.0], 0.75 * pi),
        ([1.0, -1.0], 1.75 * pi),
        ([-1.0, -1.0], 0.25 * pi),
    ] {
        particle.position = [10.0 + direction[0], 20.0 + direction[1], 999.0];
        particle.angle = -100.0;
        particle.draw(&rules, 0.0, None);
        assert!((particle.angle - expected).abs() < 0.000_001);
    }
}

#[test]
fn draw_reorients_after_movement_and_slave_promotion_keeps_master_emitter_state() {
    let (mut master, rules) = generated(true, [100.0, 200.0, 300.0]);
    let (slave, _) = generated(false, [500.0, 600.0, 700.0]);
    master.merge(&slave, [0.0; 3]);
    master.position = [100.0, 201.0, 300.0];
    master.velocity = [1.0, 0.0, 500.0];
    master.angular_rate = 99.0;
    master.draw(&rules, 1.0, None);
    let expected = std::f32::consts::PI * 1.25;
    assert!((master.angle - expected).abs() < 0.000_001);
    let (mut unflagged, rules) = generated(false, [0.0; 3]);
    unflagged.merge(&master, [0.0; 3]);
    unflagged.angle = 0.25;
    unflagged.angular_rate = 0.5;
    unflagged.draw(&rules, 1.0, None);
    assert_eq!(unflagged.angle.to_bits(), 0.75_f32.to_bits());
}
