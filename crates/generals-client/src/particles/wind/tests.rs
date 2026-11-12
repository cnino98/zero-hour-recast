use super::*;
use crate::particles::{
    Pose, Range,
    tests::{system, template},
};

fn middle(low: f32, high: f32) -> f32 {
    let extent = high - low;
    let offset = extent * 0.5;
    low + offset
}

#[test]
fn ping_pong_reversal_draws_change_start_end_and_does_not_clamp_the_angle() {
    let mut rules = template("FireStormMain");
    rules.wind_motion = Motion::PingPong;
    rules.wind_change = Range {
        low: 0.1,
        high: 0.3,
    };
    rules.wind_start = Range {
        low: 0.5,
        high: 1.0,
    };
    rules.wind_end = Range {
        low: 5.0,
        high: 6.0,
    };
    let mut state = State::new(0.0, 4.0, 3.999);
    let mut draws = Vec::new();
    state.update(&rules, &mut |low, high| {
        draws.push([low, high].map(f32::to_bits));
        middle(low, high)
    });
    assert_eq!(state.angle.to_bits(), (3.999_f32 + 0.005).to_bits());
    assert!(!state.towards_end);
    assert_eq!(
        draws,
        [[0.1, 0.3], [0.5, 1.0], [5.0, 6.0]].map(|range| range.map(f32::to_bits))
    );
    assert_eq!(state.change.to_bits(), middle(0.1, 0.3).to_bits());
    state.angle = state.start + 0.001;
    let before = state.angle;
    state.update(&rules, &mut middle);
    assert!(state.towards_end);
    assert_eq!(state.angle.to_bits(), (before - 0.005).to_bits());
}

#[test]
fn circular_wind_preserves_constructor_rate_and_strict_single_wrap() {
    let mut rules = template("circular");
    rules.wind_motion = Motion::Circular;
    let mut state = State::new(0.0, 0.0, 0.0);
    state.update(&rules, &mut |_, _| {
        panic!("nonzero source rate does not resample")
    });
    assert_eq!(state.angle.to_bits(), 0.15_f32.to_bits());
    state.angle = 0.0;
    state.change = std::f32::consts::TAU;
    state.update(&rules, &mut middle);
    assert_eq!(state.angle.to_bits(), std::f32::consts::TAU.to_bits());
    state.change = 0.25;
    state.update(&rules, &mut middle);
    let next = std::f32::consts::TAU + 0.25;
    assert_eq!(
        state.angle.to_bits(),
        (next - std::f32::consts::TAU).to_bits()
    );
    state.angle = 0.0;
    state.change = -0.25;
    state.update(&rules, &mut middle);
    assert_eq!(
        state.angle.to_bits(),
        (-0.25 + std::f32::consts::TAU).to_bits()
    );
    state.change = 0.0;
    let mut count = 0;
    state.update(&rules, &mut |low, high| {
        count += 1;
        middle(low, high)
    });
    assert_eq!(count, 1);
}

#[test]
fn initial_delay_precedes_wind_but_stopping_emission_does_not_stop_wind() {
    let mut rules = template("delayed");
    rules.wind_motion = Motion::Circular;
    rules.initial_delay = Range {
        low: 1.0,
        high: 1.0,
    };
    rules.system_lifetime = 0.0;
    rules.wind_start = Range {
        low: 1.0,
        high: 2.0,
    };
    rules.wind_end = Range {
        low: 3.0,
        high: 4.0,
    };
    let mut particles = system(vec![rules]);
    let mut draws = Vec::new();
    particles.create(0, Pose::at([0.0; 3]), None, &mut |low, high| {
        draws.push([low, high].map(f32::to_bits));
        middle(low, high)
    });
    assert_eq!(
        draws,
        [[1.0, 1.0], [1.0, 2.0], [3.0, 4.0], [1.5, 3.5]].map(|range| range.map(f32::to_bits))
    );
    particles.emitters[0].shrouded = true;
    particles.logic(
        0,
        &mut |_, _| panic!("initial delay draws no wind RNG"),
        &|_| 0.0,
    );
    assert_eq!(
        particles.emitters[0].wind.angle.to_bits(),
        2.5_f32.to_bits()
    );
    particles.logic(
        1,
        &mut |_, _| panic!("stopped emission draws no burst RNG"),
        &|_| 0.0,
    );
    assert_eq!(
        particles.emitters[0].wind.angle.to_bits(),
        (2.5_f32 + 0.15).to_bits()
    );
    assert!(particles.emitters[0].particles.is_empty());
}

#[test]
fn wind_force_uses_three_dimensional_distance_and_source_boundaries() {
    for (distance, expected) in [
        (0.0, 2.0),
        (75.0, 2.0),
        (137.5, 1.0),
        (200.0, 0.0),
        (201.0, 0.0),
    ] {
        let mut position = [10.0, 20.0, 30.0 + distance];
        move_particle(&mut position, [10.0, 20.0, 30.0], 0.0, 1.0, 1.0);
        assert_eq!(
            position.map(f32::to_bits),
            [10.0 + expected, 20.0, 30.0 + distance].map(f32::to_bits)
        );
    }
    let mut position = [0.0; 3];
    move_particle(&mut position, [0.0; 3], 0.0, 0.7, 0.5);
    assert_eq!(
        position.map(f32::to_bits),
        [0.7, 0.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn wind_draw_follows_velocity_and_precedes_emitter_orientation_with_no_paused_motion() {
    let mut rules = template("upward");
    rules.up_towards_emitter = true;
    let mut particles = system(vec![rules]);
    particles.create(0, Pose::at([0.0; 3]), None, &mut middle);
    particles.logic(0, &mut middle, &|_| 0.0);
    let particle = &mut particles.emitters[0].particles[0];
    particle.position = [0.0, 1.0, 0.0];
    particle.velocity = [0.0; 3];
    particle.wind_randomness = 1.0;
    particle.draw(&particles.templates[0], 0.5, Some((0.0, [0.0; 3])));
    assert_eq!(
        particle.position.map(f32::to_bits),
        [1.0, 1.0, 0.0].map(f32::to_bits)
    );
    let expected = std::f32::consts::PI * 1.25;
    assert!((particle.angle - expected).abs() < 0.000_001);
    let before = particle.position;
    particle.draw(&particles.templates[0], 0.0, Some((0.0, [0.0; 3])));
    assert_eq!(
        particle.position.map(f32::to_bits),
        before.map(f32::to_bits)
    );
    particle.position = [74.0, 0.0, 0.0];
    particle.velocity = [1.0, 0.0, 0.0];
    particle.draw(&particles.templates[0], 1.0, Some((0.0, [0.0; 3])));
    assert_eq!(particle.position[0].to_bits(), 77.0_f32.to_bits());
}

#[test]
fn unused_wind_and_slave_merge_preserve_native_random_draws() {
    let rules = template("unused");
    let mut state = State::new(0.0, 1.0, 0.5);
    state.update(&rules, &mut |_, _| panic!("unused wind does not draw"));
    assert_eq!(state.angle.to_bits(), 0.5_f32.to_bits());
    let mut particles = system(vec![rules]);
    particles.create(0, Pose::at([0.0; 3]), None, &mut middle);
    let emitter = &mut particles.emitters[0];
    let mut draws = Vec::new();
    let mut master = emitter.generate(&particles.templates[0], 1.0, 0, 1, 0, &mut |low, high| {
        draws.push([low, high].map(f32::to_bits));
        middle(low, high)
    });
    assert_eq!(draws.last(), Some(&[0.7, 1.3].map(f32::to_bits)));
    let mut slave = emitter.generate(&particles.templates[0], 1.0, 0, 1, 0, &mut middle);
    master.wind_randomness = 0.7;
    slave.wind_randomness = 1.3;
    master.merge(&slave, [0.0; 3]);
    assert_eq!(master.wind_randomness.to_bits(), 0.7_f32.to_bits());
}
