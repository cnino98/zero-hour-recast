use super::*;
use crate::particles::tests::{system, template};

fn sample(_: f32, high: f32) -> f32 {
    high
}

#[test]
fn ping_precedes_beacon_and_attached_origin_keeps_native_local_offset() {
    let mut particles = system(vec![template("Ping"), template("Beacon")]);
    particles.emit_detector_scans(
        vec![Scan {
            object: 7,
            pose: Pose {
                position: [25.0, 30.0, 45.0],
                axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            },
            offset: [0.0, 5.0, 15.0],
            wind_origin: [30.0, 35.0, 45.0],
            templates: [Some(0), Some(1)],
        }],
        &mut sample,
    );
    assert_eq!(particles.emitters.len(), 2);
    assert_eq!(
        particles
            .emitters
            .iter()
            .map(|emitter| emitter.template)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    for emitter in &particles.emitters {
        assert_eq!(
            emitter
                .object_attachment
                .as_ref()
                .map(|(id, pose)| (*id, pose.position.map(f32::to_bits))),
            Some((7, [0.0, 5.0, 15.0].map(f32::to_bits)))
        );
        assert_eq!(
            emitter.wind_origin.map(|point| point.map(f32::to_bits)),
            Some([30.0, 35.0, 45.0].map(f32::to_bits))
        );
        assert_eq!(
            emitter.transform.position.map(f32::to_bits),
            [25.0, 30.0, 45.0].map(f32::to_bits)
        );
    }
    particles.logic(1, &mut sample, &|_| 0.0);
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| !emitter.particles.is_empty())
    );
}

#[test]
fn unnamed_scan_particles_do_not_consume_randomness_or_create_replacements() {
    let mut particles = system(Vec::new());
    let mut calls = 0;
    particles.emit_detector_scans(
        vec![Scan {
            object: 1,
            pose: Pose::at([0.0; 3]),
            offset: [0.0; 3],
            wind_origin: [0.0; 3],
            templates: [None; 2],
        }],
        &mut |low, _| {
            calls += 1;
            low
        },
    );
    assert_eq!(calls, 0);
    assert!(particles.emitters.is_empty());
}
