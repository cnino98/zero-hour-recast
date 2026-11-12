use super::*;
use crate::particles::tests::{system, template};

fn fixture() -> (Particles, BTreeMap<(usize, usize), DrawState>) {
    let mut green = template("green");
    green.system_lifetime = 0.0;
    green.burst_delay.low = 2.0;
    green.burst_delay.high = 2.0;
    green.lifetime.low = 10.0;
    green.lifetime.high = 10.0;
    green.alpha[1].frame = 10;
    green.color[1].frame = 10;
    let mut blue = template("blue");
    blue.system_lifetime = 0.0;
    let mut particles = system(vec![green, blue]);
    particles.model_states.bindings.insert(
        4,
        vec![Draw {
            animated: false,
            states: vec![vec![], vec![0], vec![0], vec![1]],
            bones: vec![
                vec![],
                vec!["None".into()],
                vec!["None".into()],
                vec!["None".into()],
            ],
        }],
    );
    let states = BTreeMap::from([(
        (8, 0),
        DrawState {
            unit: 4,
            state: 1,
            in_view: true,
            stopped: false,
            pose: Pose::at([100.0, 200.0, 30.0]),
            offsets: Vec::new(),
        },
    )]);
    (particles, states)
}

fn advance(particles: &mut Particles, states: &BTreeMap<(usize, usize), DrawState>, frame: u64) {
    particles.update_draw_particles(states, frame, false, &|_| 0.0, &mut |_, high| high);
}

#[test]
fn aliases_keep_ids_but_distinct_states_destroy_and_recreate_even_with_identical_art() {
    let (mut particles, mut states) = fixture();
    advance(&mut particles, &states, 1);
    assert_eq!(particles.emitters[0].particles.len(), 1);
    let first = particles.emitters[0].id;
    advance(&mut particles, &states, 1);
    assert_eq!(particles.next_id, 1);
    assert_eq!(particles.emitters[0].particles.len(), 1);
    states.get_mut(&(8, 0)).unwrap().state = 2;
    advance(&mut particles, &states, 2);
    assert_eq!(particles.next_id, 2);
    assert_eq!(particles.emitters[0].id, first);
    assert!(particles.emitters[0].lifecycle.destroyed);
    assert!(!particles.emitters[0].particles.is_empty());
    assert_eq!(particles.emitters[1].particles.len(), 1);
    states.get_mut(&(8, 0)).unwrap().state = 3;
    advance(&mut particles, &states, 3);
    assert_eq!(particles.emitters.last().unwrap().template, 1);
    assert!(
        particles
            .emitters
            .iter()
            .take(2)
            .all(|emitter| emitter.lifecycle.destroyed)
    );
}

#[test]
fn hidden_and_offscreen_creation_waits_but_existing_hidden_emitters_keep_age_and_burst_clock() {
    let (mut particles, mut states) = fixture();
    states.get_mut(&(8, 0)).unwrap().stopped = true;
    let mut draws = 0;
    particles.update_draw_particles(&states, 0, false, &|_| 0.0, &mut |_, high| {
        draws += 1;
        high
    });
    assert_eq!(draws, 0);
    assert!(particles.emitters.is_empty());
    let state = states.get_mut(&(8, 0)).unwrap();
    state.stopped = false;
    state.in_view = false;
    advance(&mut particles, &states, 1);
    assert!(particles.emitters.is_empty());
    states.get_mut(&(8, 0)).unwrap().in_view = true;
    advance(&mut particles, &states, 2);
    let first = particles.emitters[0].id;
    let remaining = particles.emitters[0].particles[0].remaining;
    states.get_mut(&(8, 0)).unwrap().stopped = true;
    for frame in 3..=4 {
        advance(&mut particles, &states, frame);
    }
    assert_eq!(particles.emitters[0].id, first);
    assert_eq!(
        particles.emitters[0].burst_delay.to_bits(),
        2.0_f32.to_bits()
    );
    assert_eq!(
        particles.emitters[0].particles[0].remaining.to_bits(),
        (remaining - 2.0).to_bits()
    );
    states.get_mut(&(8, 0)).unwrap().stopped = false;
    for frame in 5..=7 {
        advance(&mut particles, &states, frame);
    }
    assert_eq!(particles.next_id, 1);
    assert_eq!(particles.emitters[0].particles.len(), 2);
}

#[test]
fn model_particles_update_after_draw_once_per_tick_and_preserve_paused_world_particles() {
    let (mut particles, mut states) = fixture();
    advance(&mut particles, &states, 1);
    let original = particles.emitters[0].particles[0].position;
    let remaining = particles.emitters[0].particles[0].remaining;
    // The earlier shared FixedUpdate must not update draw-owned emitters.
    particles.logic(2, &mut |_, high| high, &|_| 0.0);
    assert_eq!(
        particles.emitters[0].particles[0].remaining.to_bits(),
        remaining.to_bits()
    );
    states.get_mut(&(8, 0)).unwrap().pose.position = [120.0, 210.0, 40.0];
    particles.update_draw_particles(&states, 2, true, &|_| 0.0, &mut |_, high| high);
    assert_eq!(
        particles.emitters[0].particles[0].remaining.to_bits(),
        remaining.to_bits()
    );
    assert_eq!(
        particles.emitters[0].transform.position.map(f32::to_bits),
        [120.0, 210.0, 40.0_f32].map(f32::to_bits)
    );
    assert_eq!(
        particles.emitters[0].particles[0]
            .position
            .map(f32::to_bits),
        original.map(f32::to_bits)
    );
    advance(&mut particles, &states, 2);
    advance(&mut particles, &states, 2);
    assert_eq!(
        particles.emitters[0].particles[0].remaining.to_bits(),
        (remaining - 1.0).to_bits()
    );
    states.clear();
    advance(&mut particles, &states, 3);
    assert!(particles.model_states.active.is_empty());
    assert!(particles.emitters[0].lifecycle.destroyed);
    for frame in 4..=14 {
        advance(&mut particles, &states, frame);
    }
    assert!(particles.emitters.is_empty());
    particles.begin(1, 0);
    assert!(particles.model_states.active.is_empty());
    assert!(particles.model_states.frame.is_none());
    assert!(!particles.model_states.bindings.is_empty());
}

#[ignore = "requires original game installation"]
#[test]
fn original_installed_tractor_model_particle_dependencies_load_without_substitutes() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let particles = super::super::load(&assets, &battle.simulation.catalog).unwrap();
    let tractor = battle
        .simulation
        .catalog
        .iter()
        .find(|unit| unit.name == "GLAVehicleToxinTruck")
        .unwrap();
    let pending = Pending::load(&assets, std::slice::from_ref(tractor)).unwrap();
    let names: std::collections::BTreeSet<_> = pending.names().collect();
    assert_eq!(
        names,
        std::collections::BTreeSet::from(["ToxinPuddleContinuous", "AnthraxPuddleContinuous"])
    );
    assert_eq!(pending.names().count(), 12);
    for name in names {
        let template = particles
            .templates
            .iter()
            .find(|template| template.name == name)
            .unwrap();
        assert!(template.drawing.ground_aligned);
        assert_eq!(template.system_lifetime.to_bits(), 0.0_f32.to_bits());
        assert_eq!(template.lifetime.low.to_bits(), 100.0_f32.to_bits());
        assert_eq!(template.burst_delay.low.to_bits(), 10.0_f32.to_bits());
    }
}

#[test]
fn constructor_draws_follow_native_drawable_and_authored_binding_order() {
    let (mut particles, mut states) = fixture();
    let rules = &mut particles.templates[0];
    rules.initial_delay.low = 1.0;
    rules.initial_delay.high = 2.0;
    rules.wind_start.low = 3.0;
    rules.wind_start.high = 4.0;
    rules.wind_end.low = 5.0;
    rules.wind_end.high = 6.0;
    states.insert(
        (9, 0),
        DrawState {
            unit: 4,
            state: 1,
            in_view: true,
            stopped: false,
            pose: Pose::at([0.0; 3]),
            offsets: Vec::new(),
        },
    );
    let mut ranges = Vec::new();
    particles.sync_model_states(&states, &mut |low, high| {
        ranges.push([low.to_bits(), high.to_bits()]);
        high
    });
    let expected =
        [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0], [4.0, 6.0_f32]].map(|range| range.map(f32::to_bits));
    assert_eq!(ranges, [expected, expected].concat());
    assert_eq!(particles.emitters[0].drawable_attachment, Some((9, 0)));
    assert_eq!(particles.emitters[1].drawable_attachment, Some((8, 0)));
    particles.sync_model_states(&states, &mut |_, _| {
        panic!("same state drew constructor RNG")
    });
}

#[test]
fn required_state_particle_templates_must_resolve_before_runtime() {
    let pending = Pending {
        bindings: BTreeMap::from([(
            0,
            vec![ParticleDraw {
                animated: false,
                states: vec![vec!["missing".into()]],
                bones: vec![vec!["None".into()]],
            }],
        )]),
    };
    let Err(error) = pending.resolve(&[]) else {
        panic!("missing required state particle was silently discarded");
    };
    assert!(
        error
            .to_string()
            .contains("missing required model-state ParticleSystem missing")
    );
}

#[test]
fn named_bone_local_pose_is_sampled_at_creation_then_follows_drawable_without_resampling() {
    let (mut particles, mut states) = fixture();
    let initial = Pose {
        position: [7.0, 2.0, 3.0],
        axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let source_local = local_offset(&initial);
    states.get_mut(&(8, 0)).unwrap().offsets = vec![Some(initial)];
    advance(&mut particles, &states, 1);
    assert_eq!(
        particles.emitters[0].transform.position.map(f32::to_bits),
        [107.0_f32, 202.0, 33.0].map(f32::to_bits)
    );
    let state = states.get_mut(&(8, 0)).unwrap();
    state.pose.position = [200.0, 300.0, 40.0];
    state.pose.axes = [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    state.offsets[0].as_mut().unwrap().position = [999.0; 3];
    let expected = compose(&state.pose, &source_local);
    advance(&mut particles, &states, 2);
    assert_eq!(
        particles.emitters[0].wind_origin.unwrap().map(f32::to_bits),
        [207.0, 302.0, 43.0].map(f32::to_bits)
    );
    assert_eq!(particles.next_id, 1);
    assert_eq!(
        particles.emitters[0].transform.position.map(f32::to_bits),
        [198.0_f32, 307.0, 43.0].map(f32::to_bits)
    );
    assert_eq!(
        particles.emitters[0].transform.axes[0].map(f32::to_bits),
        expected.axes[0].map(f32::to_bits)
    );
    states.clear();
    advance(&mut particles, &states, 3);
    assert!(particles.emitters[0].lifecycle.destroyed);
    assert_eq!(
        particles.emitters[0].wind_origin.unwrap().map(f32::to_bits),
        [7.0, 2.0, 3.0].map(f32::to_bits)
    );
}

fn pose_bits(pose: &Pose) -> [[u32; 3]; 4] {
    [pose.position, pose.axes[0], pose.axes[1], pose.axes[2]].map(|row| row.map(f32::to_bits))
}

#[test]
fn animated_bones_use_full_world_axes_without_parent_twice_or_extra_random_draws() {
    let (mut particles, mut states) = fixture();
    particles.model_states.bindings.get_mut(&4).unwrap()[0].animated = true;
    let bone = Pose {
        position: [7.0, 2.0, 3.0],
        axes: [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]],
    };
    states.get_mut(&(8, 0)).unwrap().offsets = vec![Some(bone.clone())];
    advance(&mut particles, &states, 1);
    let id = particles.emitters[0].id;
    let first = compose(&states[&(8, 0)].pose, &bone);
    assert_eq!(
        pose_bits(&particles.emitters[0].transform),
        pose_bits(&first)
    );
    assert!(matches!(
        particles.emitters[0].drawable_offset,
        Some(Attachment::World(_))
    ));
    let old_particle = particles.emitters[0].particles[0].position;
    let state = states.get_mut(&(8, 0)).unwrap();
    state.pose.position = [200.0, 300.0, 40.0];
    state.pose.axes = [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let bone = state.offsets[0].as_mut().unwrap();
    bone.position = [11.0, 13.0, 17.0];
    bone.axes = [[0.0, 0.0, -1.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]];
    let expected = compose(&state.pose, bone);
    // Drawable::draw and the native view-region gate suppress doDrawModule;
    // the optional global animated-bone ClientUpdate is absent on the Dozer.
    for (in_view, stopped) in [(false, false), (true, true)] {
        let state = states.get_mut(&(8, 0)).unwrap();
        state.in_view = in_view;
        state.stopped = stopped;
        particles.update_draw_particles(&states, 1, true, &|_| 0.0, &mut |_, _| {
            panic!("hidden animated emitter consumed RNG")
        });
        assert_eq!(
            pose_bits(&particles.emitters[0].transform),
            pose_bits(&first)
        );
    }
    let state = states.get_mut(&(8, 0)).unwrap();
    state.in_view = true;
    state.stopped = false;
    particles.update_draw_particles(&states, 1, true, &|_| 0.0, &mut |_, _| {
        panic!("animated matrix update consumed RNG or recreated emitter")
    });
    let emitter = &particles.emitters[0];
    assert_eq!(emitter.id, id);
    assert_eq!(particles.next_id, 1);
    assert_eq!(pose_bits(&emitter.transform), pose_bits(&expected));
    assert_eq!(
        emitter.particles[0].position.map(f32::to_bits),
        old_particle.map(f32::to_bits)
    );
    assert_eq!(
        emitter.wind_origin.unwrap().map(f32::to_bits),
        std::array::from_fn::<_, 3, _>(
            |axis| states[&(8, 0)].pose.position[axis] + expected.position[axis]
        )
        .map(f32::to_bits)
    );
}

#[test]
fn animated_old_states_drain_at_last_world_pose_and_missing_bones_remain_drawable_local() {
    let (mut particles, mut states) = fixture();
    particles.model_states.bindings.get_mut(&4).unwrap()[0].animated = true;
    states.get_mut(&(8, 0)).unwrap().offsets = vec![Some(Pose::at([7.0, 2.0, 3.0]))];
    advance(&mut particles, &states, 1);
    let old = pose_bits(&particles.emitters[0].transform);
    let state = states.get_mut(&(8, 0)).unwrap();
    state.state = 2;
    state.offsets = vec![None];
    state.pose.position = [200.0, 300.0, 40.0];
    advance(&mut particles, &states, 2);
    assert!(particles.emitters[0].lifecycle.destroyed);
    assert!(!particles.emitters[0].particles.is_empty());
    assert_eq!(pose_bits(&particles.emitters[0].transform), old);
    assert!(particles.emitters[1].drawable_offset.is_none());
    assert_eq!(
        pose_bits(&particles.emitters[1].transform),
        pose_bits(&states[&(8, 0)].pose)
    );
    states.get_mut(&(8, 0)).unwrap().pose.position = [400.0, 500.0, 60.0];
    advance(&mut particles, &states, 3);
    assert_eq!(pose_bits(&particles.emitters[0].transform), old);
    assert_eq!(
        pose_bits(&particles.emitters[1].transform),
        pose_bits(&states[&(8, 0)].pose)
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_dozer_bindings_load_exact_smoke_dirt_and_usa_only_animated_policy() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let particles = super::super::load(&assets, &battle.simulation.catalog).unwrap();
    for (name, animated, bone) in [
        ("AmericaVehicleDozer", true, "EXHAUSTFX01"),
        ("ChinaVehicleDozer", false, "EXHAUST01"),
    ] {
        let index = crate::strategy_verification::template(&battle.simulation, name).unwrap();
        let draw = &particles.model_states.bindings[&index][0];
        assert_eq!(draw.animated, animated);
        assert!(draw.bones.iter().flatten().any(|name| name == bone));
        assert!(draw.bones.iter().flatten().any(|name| name == "DIRTFX01"));
        let names: std::collections::BTreeSet<_> = draw
            .states
            .iter()
            .flatten()
            .map(|&index| particles.templates[index].name.as_str())
            .collect();
        assert!(names.contains("DozerSmokeLight"));
        assert!(names.contains("DozerSmokeHeavy"));
        assert!(names.contains("DozerDirtFall"));
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_mig_particles_are_admitted_to_the_real_state_emitter() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let game = &battle.simulation;
    let index = crate::strategy_verification::template(game, "ChinaJetMIG").unwrap();
    let particles = crate::particles::load(&assets, &game.catalog).unwrap();
    let bindings = &particles.model_states.bindings[&index];
    assert!(
        bindings
            .iter()
            .flat_map(|draw| &draw.states)
            .flatten()
            .any(|&id| particles.templates[id].name == "JetContrail")
    );
    assert!(
        bindings
            .iter()
            .flat_map(|draw| &draw.states)
            .flatten()
            .any(|&id| particles.templates[id].name == "JetExhaust")
    );
}
