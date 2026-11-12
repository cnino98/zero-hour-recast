use super::*;
use crate::particles::tests::{system, template};

fn event(object: usize, module: usize, hidden: bool) -> Event {
    Event {
        object,
        module,
        frame: 10,
        action: Action::Particle {
            slot: 0,
            template: "Trail".into(),
            local: [2.0, 0.0, 3.0],
            position: [10.0, 20.0, 30.0],
            basis: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            hidden,
        },
    }
}

#[test]
fn source_callback_constructor_rng_uses_timeline_order_and_stop_is_not_later_shroud() {
    let mut trail = template("Trail");
    trail.system_lifetime = 0.0;
    let mut particles = system(vec![trail]);
    let mut requests = Vec::new();
    let mut random = |lo: f32, hi: f32| {
        requests.push([lo.to_bits(), hi.to_bits()]);
        lo
    };
    particles.record_bone_event(&event(7, 0, true), &mut random);
    assert!(!requests.is_empty()); // Actual system construction, including wind.
    let emitter = &particles.emitters[0];
    assert_eq!(
        emitter.transform.position.map(f32::to_bits),
        [10.0_f32, 22.0, 33.0].map(f32::to_bits)
    );
    assert_eq!(emitter.object_attachment.as_ref().unwrap().0, 7);
    assert!(emitter.lifecycle.stopped);
    assert!(!emitter.lifecycle.destroyed);
    particles.emitters[0].shrouded = false;
    particles.logic(10, &mut |lo, _| lo, &|_| 0.0);
    assert!(particles.emitters[0].particles.is_empty());
    assert!(particles.emitters[0].lifecycle.stopped);
}

#[test]
fn module_clear_is_independent_and_graceful_with_slave_destruction_and_remaining_particles() {
    let mut root = template("Trail");
    root.slave = Some(1);
    root.system_lifetime = 0.0;
    let mut slave = template("Slave");
    slave.system_lifetime = 0.0;
    let mut particles = system(vec![root, slave]);
    particles.record_bone_event(&event(7, 0, false), &mut |lo, _| lo);
    particles.record_bone_event(&event(7, 1, false), &mut |lo, _| lo);
    particles.logic(10, &mut |lo, _| lo, &|_| 0.0);
    let first = particles.bone_systems[&(7, 0)][0];
    let second = particles.bone_systems[&(7, 1)][0];
    let before = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == first)
        .unwrap()
        .particles
        .len();
    assert!(before > 0);
    particles.record_bone_event(
        &Event {
            object: 7,
            module: 0,
            frame: 11,
            action: Action::Clear,
        },
        &mut |lo, _| lo,
    );
    let emitter = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == first)
        .unwrap();
    assert_eq!(emitter.particles.len(), before);
    assert!(emitter.lifecycle.destroyed);
    let slave = emitter.slave.unwrap();
    assert!(
        particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == slave)
            .unwrap()
            .lifecycle
            .destroyed
    );
    assert!(
        !particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == second)
            .unwrap()
            .lifecycle
            .destroyed
    );
    particles.logic(11, &mut |lo, _| lo, &|_| 0.0);
    assert!(
        particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == first)
            .unwrap()
            .particles
            .len()
            <= before
    );
    particles.begin(1, 12);
    assert!(particles.bone_systems.is_empty());
}
