use super::{
    Particles, Pose, Range,
    tests::{system, template},
};

fn fixture(above_ground: bool) -> Particles {
    let mut rules = template("terrain");
    rules.emit_above_ground_only = above_ground;
    rules.system_lifetime = 0.0;
    rules.start_size_rate = Range {
        low: 3.0,
        high: 3.0,
    };
    let mut particles = system(vec![rules]);
    particles.create(0, Pose::at([4.0, 5.0, -1.0]), None, &mut |_, high| high);
    particles
}

#[test]
fn rejected_below_ground_attempt_retains_attribute_rng_and_size_but_not_personality() {
    let mut rejected = fixture(true);
    let mut admitted = fixture(true);
    let mut rejected_draws = Vec::new();
    let mut admitted_draws = Vec::new();
    rejected.logic(
        0,
        &mut |low, high| {
            rejected_draws.push((low.to_bits(), high.to_bits()));
            high
        },
        &|point| {
            assert_eq!(point.map(f32::to_bits), [4.0_f32, 5.0].map(f32::to_bits));
            0.0
        },
    );
    // Equality is admitted by the original >= comparison.
    admitted.logic(
        0,
        &mut |low, high| {
            admitted_draws.push((low.to_bits(), high.to_bits()));
            high
        },
        &|_| -1.0,
    );
    assert_eq!(rejected_draws, admitted_draws);
    assert!(rejected.emitters[0].particles.is_empty());
    assert_eq!(rejected.emitters[0].personality, 0);
    assert_eq!(admitted.emitters[0].particles.len(), 1);
    assert_eq!(admitted.emitters[0].personality, 1);
    assert_eq!(rejected.emitters[0].size_bonus.to_bits(), 3.0_f32.to_bits());
    assert_eq!(
        rejected.emitters[0].size_bonus.to_bits(),
        admitted.emitters[0].size_bonus.to_bits()
    );
    rejected.logic(1, &mut |_, high| high, &|_| -1.0);
    assert_eq!(rejected.emitters[0].particles[0].personality, 0);
    assert_eq!(
        rejected.emitters[0].particles[0].size.to_bits(),
        4.0_f32.to_bits()
    );
}

#[test]
fn terrain_gate_precedes_attached_and_slave_creation_without_rechecking_forced_slave() {
    let mut master = template("master");
    master.emit_above_ground_only = true;
    master.system_lifetime = 0.0;
    master.slave = Some(1);
    master.attached = Some(2);
    let mut slave = template("slave");
    slave.emit_above_ground_only = true;
    slave.system_lifetime = 0.0;
    slave.slave_offset = [0.0, 0.0, -10.0];
    let mut child = template("child");
    child.system_lifetime = 0.0;
    let mut particles = system(vec![master, slave, child]);
    let parent = particles.create(0, Pose::at([4.0, 5.0, 2.0]), None, &mut |_, high| high);
    particles.logic(0, &mut |_, high| high, &|_| 3.0);
    assert_eq!(particles.emitters.len(), 2);
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.particles.is_empty() && emitter.personality == 0)
    );
    particles.logic(1, &mut |_, high| high, &|_| 2.0);
    assert_eq!(particles.emitters.len(), 3);
    let emitted = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == parent)
        .unwrap();
    assert_eq!(emitted.particles.len(), 1);
    assert!(emitted.particles[0].controlled.is_some());
    let emitted_slave = particles
        .emitters
        .iter()
        .find(|emitter| emitter.master)
        .unwrap();
    assert_eq!(emitted_slave.particles.len(), 1);
    assert!(emitted_slave.particles[0].position[2] < 2.0);
}

#[test]
fn ordinary_emission_keeps_below_ground_particles_without_querying_terrain() {
    let mut particles = fixture(false);
    particles.logic(0, &mut |_, high| high, &|_| {
        panic!("unrequested terrain query")
    });
    assert_eq!(particles.emitters[0].particles.len(), 1);
    assert_eq!(
        particles.emitters[0].particles[0].position[2].to_bits(),
        (-1.0_f32).to_bits()
    );
}
