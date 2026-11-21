use super::*;

fn fixed(value: f32) -> Range {
    Range {
        low: value,
        high: value,
    }
}
pub(super) fn template(name: &str) -> Template {
    Template {
        name: name.into(),
        texture: 0,
        drawing: Drawing {
            style: Style::Billboard,
            shader: Shader::Alpha,
            ground_aligned: false,
        },
        emit_above_ground_only: false,
        up_towards_emitter: false,
        lifetime: fixed(4.0),
        system_lifetime: 1.0,
        initial_delay: fixed(0.0),
        burst_delay: fixed(0.0),
        burst_count: fixed(1.0),
        volume: Volume::Point,
        velocity: Velocity::Ortho([fixed(0.0), fixed(0.0), fixed(2.0)]),
        velocity_damping: fixed(0.5),
        angle: fixed(0.0),
        angular_rate: fixed(0.0),
        angular_damping: fixed(1.0),
        gravity: -0.2,
        drift: [0.0; 3],
        size: fixed(1.0),
        start_size_rate: fixed(0.0),
        size_rate: fixed(1.0),
        size_rate_damping: fixed(1.0),
        alpha: std::array::from_fn(|i| AlphaKey {
            value: fixed(if i == 0 { 1.0 } else { 0.0 }),
            frame: if i == 1 { 2 } else { 0 },
        }),
        color: std::array::from_fn(|i| ColorKey {
            value: [1.0; 3],
            frame: if i == 1 { 2 } else { 0 },
        }),
        color_scale: fixed(0.0),
        slave: None,
        attached: None,
        slave_offset: [0.0; 3],
        wind_motion: wind::Motion::Unused,
        wind_change: Range {
            low: 0.15,
            high: 0.45,
        },
        wind_start: fixed(0.0),
        wind_end: fixed(0.0),
    }
}
pub(super) fn system(templates: Vec<Template>) -> Particles {
    Particles {
        object_emitters: super::object_effects::Attachments::new(),
        templates,
        textures: Vec::new(),
        emitters: Vec::new(),
        smudges: Vec::new(),
        next_id: 0,
        generation: 0,
        frame: None,
        processed: None,
        advanced: None,
        phase: 0.0,
        scale: 1.0,
        bindings: Vec::new(),
        missile_ignition: Vec::new(),
        bone_systems: std::collections::BTreeMap::new(),
        bodies: super::body_particles::Bodies::empty(),
        detectors: std::collections::BTreeMap::new(),
        gps_scramblers: std::collections::BTreeMap::new(),
        model_states: super::model_states::States::empty(),
        fires: super::fire::Fires::empty(),
        lists: Vec::new(),
        actor_fx: std::collections::BTreeMap::new(),
        propaganda: Vec::new(),
        sounds: Vec::new(),
        pulses: crate::light_pulse::Pool::new(),
        scorches: crate::scorch::Scorches::new(None),
        repair_sound: String::new(),
        shake: super::view_shake::Shake::new(),
        reported_streak: std::collections::BTreeSet::new(),
    }
}
fn sample(low: f32, high: f32) -> f32 {
    if low >= high {
        high
    } else {
        let width = high - low;
        let offset = width * 0.25;
        low + offset
    }
}

#[test]
fn zero_lifetime_controller_survives_attached_system_retirement_and_destroy_until_color_fades() {
    let mut parent = template("arms");
    parent.lifetime = fixed(0.0);
    parent.attached = Some(1);
    parent.drawing.shader = Shader::Additive;
    parent.color = std::array::from_fn(|_| ColorKey {
        value: [3.0 / 255.0, 0.0, 0.0],
        frame: 0,
    });
    parent.size = fixed(0.0);
    let mut particles = system(vec![parent, template("attached")]);
    let parent = particles.create(0, Pose::at([0.0; 3]), None, &mut sample);
    particles.logic(0, &mut sample, &|_| 0.0);
    assert!(particles.emitters[0].particles[0].controlled.is_some());
    for frame in 1..10 {
        particles.logic(frame, &mut sample, &|_| 0.0);
        particles.draw(1.0);
    }
    assert_eq!(particles.emitters.len(), 1);
    assert_eq!(particles.emitters[0].id, parent);
    assert!(particles.emitters[0].particles[0].controlled.is_none());
    particles.emitters[0].lifecycle.destroyed = true;
    particles.logic(10, &mut sample, &|_| 0.0);
    assert_eq!(
        particles.emitters[0].particles[0].remaining.to_bits(),
        0.0_f32.to_bits()
    );
    particles.emitters[0].particles[0].color = [0.0; 3];
    particles.logic(11, &mut sample, &|_| 0.0);
    assert!(particles.emitters.is_empty());
}

#[test]
fn particle_controlled_system_follows_at_logic_updates_and_stops_with_its_parent_particle() {
    let mut parent = template("smoke");
    parent.attached = Some(1);
    parent.lifetime = fixed(4.0);
    parent.alpha[1].frame = 10;
    let mut child = template("sparks");
    child.system_lifetime = 0.0;
    child.lifetime = fixed(8.0);
    child.alpha[1].frame = 7;
    let mut particles = system(vec![parent, child]);
    let parent = particles.create(0, Pose::at([10.0, 20.0, 30.0]), None, &mut sample);
    particles.logic(0, &mut sample, &|_| 0.0);
    let child = particles.emitters[0].particles[0].controlled.unwrap();
    assert_eq!(particles.emitters[1].control_particle, Some((parent, 0)));
    assert_eq!(
        particles.emitters[1]
            .wind_origin
            .map(|origin| origin.map(f32::to_bits)),
        Some([0.0; 3].map(f32::to_bits))
    );
    assert!(particles.emitters[1].particles.is_empty());
    particles.draw(1.0);
    let position = particles.emitters[0].particles[0].position;
    particles.logic(1, &mut sample, &|_| 0.0);
    let controlled = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == child)
        .unwrap();
    assert_eq!(
        controlled.transform.position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert_eq!(
        controlled.particles[0].position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    particles.draw(1.0);
    particles.logic(2, &mut sample, &|_| 0.0);
    let emitted = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == child)
        .unwrap()
        .personality;
    particles.draw(1.0);
    particles.logic(3, &mut sample, &|_| 0.0);
    let controlled = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == child)
        .unwrap();
    assert!(controlled.lifecycle.destroyed);
    assert!(controlled.control_particle.is_none());
    assert_eq!(controlled.personality, emitted);
    assert!(!controlled.particles.is_empty());
}

#[test]
fn appended_controlled_system_updates_on_birth_only_when_native_iterator_has_a_successor() {
    let mut parent = template("smoke");
    parent.attached = Some(1);
    let child = template("sparks");
    let mut successor = template("already registered");
    successor.burst_count = fixed(0.0);
    let mut particles = system(vec![parent, child, successor]);
    particles.create(0, Pose::at([10.0, 20.0, 30.0]), None, &mut sample);
    particles.create(2, Pose::at([0.0; 3]), None, &mut sample);
    particles.logic(0, &mut sample, &|_| 0.0);
    let parent = particles
        .emitters
        .iter()
        .find(|emitter| emitter.template == 0)
        .unwrap();
    let child = parent.particles[0].controlled.unwrap();
    let child = particles
        .emitters
        .iter()
        .find(|emitter| emitter.id == child)
        .unwrap();
    assert_eq!(child.particles.len(), 1);
    assert_eq!(child.particles[0].born, 0);
    assert_eq!(child.particles[0].remaining.to_bits(), 3.0_f32.to_bits());
    assert_eq!(
        child.particles[0].position.map(f32::to_bits),
        [10.0_f32, 20.0, 30.0].map(f32::to_bits)
    );
}

#[test]
fn finite_controlled_system_detaches_from_a_longer_lived_particle() {
    let mut parent = template("smoke");
    parent.attached = Some(1);
    parent.lifetime = fixed(20.0);
    parent.alpha[1].frame = 20;
    let mut child = template("sparks");
    child.lifetime = fixed(2.0);
    let mut particles = system(vec![parent, child]);
    particles.create(0, Pose::at([10.0, 20.0, 30.0]), None, &mut sample);
    for frame in 0..3 {
        particles.logic(frame, &mut sample, &|_| 0.0);
        particles.draw(1.0);
    }
    assert_eq!(particles.emitters.len(), 1);
    assert!(particles.emitters[0].particles[0].controlled.is_none());
}

#[test]
fn attached_constructor_randomness_precedes_fresh_master_info_for_slave() {
    let mut parent = template("AvengerDestroySmoke");
    parent.slave = Some(2);
    parent.attached = Some(1);
    parent.angle = Range {
        low: 55.0,
        high: 56.0,
    };
    let mut child = template("TankStruckSparks");
    child.initial_delay = Range {
        low: 33.0,
        high: 34.0,
    };
    let mut particles = system(vec![parent, child, template("SubExplosionFlameSlave01")]);
    particles.create(0, Pose::at([0.0; 3]), None, &mut sample);
    let mut draws = Vec::new();
    particles.logic(
        0,
        &mut |low, high| {
            if low.to_bits() == 55.0_f32.to_bits() || low.to_bits() == 33.0_f32.to_bits() {
                draws.push(low.to_bits());
            }
            sample(low, high)
        },
        &|_| 0.0,
    );
    assert_eq!(draws, [55.0_f32, 33.0, 55.0].map(f32::to_bits));
    assert_eq!(particles.emitters[0].particles.len(), 1);
    assert_eq!(particles.emitters[1].particles.len(), 1);
    assert_eq!(particles.emitters[2].control_particle, Some((0, 0)));
}

#[test]
fn birth_gravity_damping_and_render_phase_follow_original_lifetime() {
    let mut particles = system(vec![template("fixture")]);
    particles.create(0, Pose::at([0.0; 3]), None, &mut sample);
    particles.logic(0, &mut sample, &|_| 0.0);
    let first = &particles.emitters[0].particles[0];
    assert!((first.remaining - 3.0).abs() < f32::EPSILON);
    assert!((first.velocity[2] - 0.9).abs() < 1e-6);
    particles.draw(0.25);
    particles.draw(0.75);
    let first = &particles.emitters[0].particles[0];
    assert!((first.position[2] - 0.9).abs() < 1e-6);
    assert!((first.size - 2.0).abs() < 1e-6);
    assert!((first.alpha - 0.5).abs() < 1e-6);
    particles.logic(1, &mut sample, &|_| 0.0);
    particles.draw(1.0);
    particles.logic(2, &mut sample, &|_| 0.0);
    particles.draw(1.0);
    particles.logic(3, &mut sample, &|_| 0.0);
    assert!(particles.emitters.is_empty());
}

#[test]
fn source_slave_uses_fresh_master_info_and_multiplied_width_without_birth_update() {
    let mut master = template("master");
    master.slave = Some(1);
    master.size = fixed(0.25);
    master.system_lifetime = 0.0;
    let mut slave = template("slave");
    slave.size = fixed(60.0);
    slave.lifetime = fixed(3.0);
    slave.system_lifetime = 0.0;
    let mut particles = system(vec![master, slave]);
    let id = particles.create(0, Pose::at([10.0, 20.0, 30.0]), Some(9), &mut sample);
    particles.logic(0, &mut sample, &|_| 0.0);
    let slave = &particles.emitters[0];
    assert!(slave.master);
    assert!((slave.particles[0].size - 15.0).abs() < f32::EPSILON);
    // The slave updated before its master emitted this particle.
    assert!((slave.particles[0].remaining - 3.0).abs() < f32::EPSILON);
    assert!((slave.particles[0].velocity[2] - 2.0).abs() < f32::EPSILON);
    assert_eq!(particles.emitters[1].id, id);
    assert_eq!(particles.emitters[1].personality, 1);
    assert_eq!(slave.personality, 1);
}

#[test]
fn attached_fog_suppresses_new_emission_but_preserves_existing_particles() {
    let mut rules = template("exhaust");
    rules.system_lifetime = 0.0;
    let mut particles = system(vec![rules]);
    particles.create(0, Pose::at([0.0; 3]), Some(1), &mut sample);
    particles.logic(0, &mut sample, &|_| 0.0);
    particles.emitters[0].shrouded = true;
    particles.logic(1, &mut sample, &|_| 0.0);
    assert_eq!(particles.emitters[0].particles.len(), 1);
    assert!((particles.emitters[0].particles[0].remaining - 2.0).abs() < f32::EPSILON);
    particles.emitters[0].shrouded = false;
    particles.logic(2, &mut sample, &|_| 0.0);
    assert_eq!(particles.emitters[0].particles.len(), 2);
}

#[test]
fn outward_normalization_preserves_native_component_division_rounding() {
    assert_eq!(
        normalize([0.3, 0.4, 0.5]).map(f32::to_bits),
        [0x3ed9_3925, 0x3f10_d0c3, 0x3f35_04f3]
    );
}

#[test]
fn two_point_streak_projects_native_width_and_original_strip_topology() {
    let mut points = [Vec3::new(-2.0, 0.0, -10.0), Vec3::new(2.0, 0.0, -10.0)];
    let strip =
        streak::geometry(&mut points, &[1.0, 1.0], &[0, 1]).expect("straight source streak");
    assert_eq!(strip.indices, [0, 1, 2, 1, 3, 2]);
    assert_eq!(strip.vertices.len(), 4);
    // Native silhouette planes project the +/-1 cylinder edges toward the eye.
    let expected = 100.0 / 101.0;
    assert!((strip.vertices[0].y.abs() - expected).abs() < 1e-5);
    assert!((strip.vertices[1].y.abs() - expected).abs() < 1e-5);
    assert!(
        strip
            .uv
            .iter()
            .zip([[0.0, 0.0], [1.0, 0.0], [0.0, 0.0], [1.0, 0.0]])
            .all(|(actual, expected)| actual
                .iter()
                .zip(expected)
                .all(|(a, b)| (*a - b).abs() < f32::EPSILON))
    );
}

#[test]
fn flashbang_cylindrical_velocity_draws_radial_then_angle_then_normal() {
    let mut rules = template("FlashBangNormalSpark");
    rules.velocity = Velocity::Cylindrical(
        Range {
            low: 0.0,
            high: 3.0,
        },
        Range {
            low: 3.0,
            high: 5.0,
        },
    );
    rules.velocity_damping = fixed(1.0);
    rules.gravity = 0.0;
    let mut particles = system(vec![rules]);
    let mut draws = Vec::new();
    let mut random = |low: f32, high: f32| {
        if low < high {
            draws.push([low, high].map(f32::to_bits));
        }
        sample(low, high)
    };
    particles.create(0, Pose::at([0.0; 3]), None, &mut random);
    particles.logic(0, &mut random, &|_| 0.0);
    assert_eq!(
        &draws[..3],
        &[[0.0, 3.0], [0.0, std::f32::consts::TAU], [3.0, 5.0]]
            .map(|range| range.map(f32::to_bits))
    );
    let velocity = particles.emitters[0].particles[0].velocity;
    assert!(velocity[0].abs() < 1e-6);
    assert_eq!(velocity[1].to_bits(), 0.75f32.to_bits());
    assert_eq!(velocity[2].to_bits(), 3.5f32.to_bits());
}

#[test]
fn original_flashbang_zero_and_fractional_bursts_keep_native_truncation() {
    let mut rules = template("FlashBangBuildingFlash");
    rules.burst_count = Range {
        low: 0.0,
        high: 2.0,
    };
    rules.system_lifetime = 2.0;
    rules.burst_delay = fixed(1.0);
    let mut smoke = template("FlashBangBuildingSmoke");
    smoke.burst_count = fixed(0.0);
    smoke.system_lifetime = 10.0;
    let mut particles = system(vec![rules, smoke]);
    particles.create(0, Pose::at([0.0; 3]), None, &mut |low, _| low);
    particles.create(1, Pose::at([0.0; 3]), None, &mut |low, _| low);
    particles.logic(
        0,
        &mut |low, high| {
            let offset = (high - low) * 0.375;
            low + offset
        },
        &|_| 0.0,
    );
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.particles.is_empty())
    );
    particles.logic(1, &mut |low, _| low, &|_| 0.0);
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.template != 0)
    );
    particles.create(0, Pose::at([0.0; 3]), None, &mut |low, _| low);
    particles.logic(
        2,
        &mut |low, high| {
            let offset = (high - low) * 0.75;
            low + offset
        },
        &|_| 0.0,
    );
    assert_eq!(particles.emitters.last().unwrap().particles.len(), 1);
    assert!(
        particles
            .emitters
            .iter()
            .filter(|emitter| emitter.template == 1)
            .all(|emitter| emitter.particles.is_empty())
    );
}
