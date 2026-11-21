use super::*;
use crate::particles::tests::{system, template};

#[ignore = "requires original game installation"]
#[test]
fn installed_paid_mine_draws_bind_original_flares_on_eight_bones() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let types = &battle.simulation.catalog;
    let particles = crate::particles::load(&assets, types).unwrap();
    for (name, template) in [
        ("ChinaStandardMine", "MineFX"),
        ("ChinaEMPMine", "MineFXPurple"),
        ("ChinaClusterMine", "MineFX"),
    ] {
        let kind = types.iter().position(|unit| unit.name == name).unwrap();
        let [draw] = particles.model_states.bindings[&kind].as_slice() else {
            panic!("paid mine must have a single original draw");
        };
        let [normal, rubble] = draw.states.as_slice() else {
            panic!("paid mine must have normal and rubble draw states");
        };
        assert_eq!(normal.len(), 8);
        assert!(
            normal
                .iter()
                .all(|&id| particles.templates[id].name == template)
        );
        assert!(
            draw.bones[0]
                .iter()
                .zip(1..=8)
                .all(|(bone, index)| *bone == format!("Mine{index:02}"))
        );
        assert!(rubble.is_empty());
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_frenzy_marker_cloud_bursts_and_drains_after_native_deletion() {
    use crate::{
        input, progression_verification::earn_combat_rank, skirmish_verification::friendly_entity,
    };
    use generals_sim::skirmish::{Faction, Kind};

    let (assets, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let game = &mut battle.simulation;
    let mut particles = crate::particles::load(&assets, &game.catalog).unwrap();
    assert_stock_frenzy_bindings(&particles, game);
    game.set_match_options(20_000, 1);
    game.reset(Faction::China, Faction::China).unwrap();
    let (army, _) = earn_combat_rank(game, 3).unwrap();
    let fighter = army[0];
    let provider = friendly_entity(game, Kind::CommandCenter, 0).unwrap();
    game.purchase_science(0, "SCIENCE_Frenzy1").unwrap();
    let point = game.entities[fighter].position;
    let marker = game.entities.len();
    let tick = game.tick;
    let generation = battle.generation;
    let mut random = random(&mut animations, generation);
    particles.begin(generation, tick);
    particles.advance_frame(tick, &mut random, &|point| game.ground_height(point));

    let power = "SuperweaponFrenzy";
    let mut mouse = queue_frenzy(&mut battle, &assets, provider, point, power);
    assert_eq!(battle.simulation.entities.len(), marker);
    assert_eq!(
        battle.simulation.power_cooldown_remaining(provider, power),
        0
    );

    input::commands::step(&mut battle, &mut mouse);
    let game = &mut battle.simulation;
    assert_eq!(game.tick, tick + 1);
    assert_eq!(game.entities.len(), marker + 1);
    assert!(game.power_cooldown_remaining(provider, power) > 0);
    assert!(!game.entities[fighter].frenzy_tint());
    let actor = &game.entities[marker];
    assert_eq!(
        game.catalog[actor.type_index].name,
        "Frenzy_InvisibleMarker_Level1"
    );
    assert!(actor.has_drawable());
    assert!(matches!(
        game.visibility_status(0, marker),
        Status::Clear | Status::Partial
    ));
    let mut states = BTreeMap::from([(
        (marker, 0),
        DrawState {
            unit: actor.type_index,
            state: 0,
            in_view: true,
            stopped: false,
            pose: drawable_pose(game, marker),
            offsets: vec![None],
            wheels: None,
            tank: None,
        },
    )]);
    let [x, y] = point;
    let region = [[x - 200.0, y - 200.0], [x + 200.0, y + 200.0]];
    let birth = game.tick;
    particles.begin(generation, birth);
    particles.sync_drawables(&states, Some((game, region)), &mut random);
    particles.advance_frame(birth, &mut random, &|point| game.ground_height(point));
    let emitter = &particles.emitters[0];
    let system = emitter.id;
    let count = emitter.particles.len();
    assert!((50..=70).contains(&count));
    assert_eq!(emitter.drawable_attachment, Some((marker, 0)));
    assert_same_frenzy_draw(&mut particles, game, &states, region, (system, count));

    input::commands::step(&mut battle, &mut mouse);
    let game = &battle.simulation;
    assert_eq!(game.tick, birth + 1);
    assert!(!game.entities[marker].has_drawable());
    assert!(game.entities[fighter].frenzy_tint());
    states.clear();
    particles.begin(generation, game.tick);
    particles.sync_drawables(&states, Some((game, region)), &mut random);
    particles.advance_frame(game.tick, &mut random, &|point| game.ground_height(point));
    let emitter = &particles.emitters[0];
    assert_eq!(emitter.id, system);
    assert!(emitter.lifecycle.destroyed);
    assert!(emitter.drawable_attachment.is_none());
    assert_eq!(emitter.particles.len(), count);
    for particle in &emitter.particles {
        assert_eq!(particle.born, birth);
    }

    // The installed cloud particles live 30–50 frames. Finish that real
    // lifetime after the attached drawable stops the emitter.
    for _ in 0..50 {
        input::commands::step(&mut battle, &mut mouse);
        let game = &battle.simulation;
        particles.begin(generation, game.tick);
        particles.advance_frame(game.tick, &mut random, &|point| game.ground_height(point));
    }
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.id != system)
    );
}

fn assert_same_frenzy_draw(
    particles: &mut Particles,
    game: &Skirmish,
    states: &BTreeMap<(usize, usize), DrawState>,
    region: [[f32; 2]; 2],
    expected: (u64, usize),
) {
    // Degenerate ranges use no RNG in GameClientRandomValueReal.
    let mut no_extra_random = |low: f32, high: f32| {
        assert!(low >= high, "second same-frame draw sampled the source RNG");
        high
    };
    particles.sync_drawables(states, Some((game, region)), &mut no_extra_random);
    particles.advance_frame(game.tick, &mut no_extra_random, &|point| {
        game.ground_height(point)
    });
    let [emitter] = particles.emitters.as_slice() else {
        panic!("second same-frame draw created another Frenzy emitter");
    };
    assert_eq!((emitter.id, emitter.particles.len()), expected);
}

fn queue_frenzy(
    battle: &mut Match,
    assets: &VirtualAssets,
    provider: usize,
    point: [f32; 2],
    power: &str,
) -> crate::input::Mouse {
    use crate::{
        control_bar,
        input::{self, CommandMode},
        playtest_ui::{dispatch_command, selected_command},
    };
    use bevy::prelude::Vec2;

    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let mut mouse = input::load(assets, &options, &battle.simulation.catalog).unwrap();
    let ui = control_bar::load(assets, &battle.simulation.catalog, &options).unwrap();
    battle.select_objects(&[provider], true);
    let action = (1..=14)
        .filter_map(|slot| selected_command(battle, &ui, slot))
        .find(|command| command.action.power() == Some(power))
        .unwrap()
        .action;
    dispatch_command(battle, &mut mouse, action);
    assert!(
        matches!(mouse.pending_command, CommandMode::Power { caster, name, .. }
        if caster == provider && name == power)
    );
    input::targeted_order(battle, &mut mouse, Vec2::from_array(point), None);
    assert!(matches!(mouse.pending_command, CommandMode::Normal));
    mouse
}

fn assert_stock_frenzy_bindings(particles: &Particles, game: &Skirmish) {
    for name in [
        "Frenzy_InvisibleMarker_Level1",
        "Frenzy_InvisibleMarker_Level2",
        "Frenzy_InvisibleMarker_Level3",
    ] {
        let kind = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .unwrap();
        let [draw] = particles.model_states.bindings[&kind].as_slice() else {
            panic!("Frenzy marker must have a single stock draw");
        };
        assert_eq!(draw.states.len(), 1);
        assert_eq!(draw.states[0].len(), 1);
        assert_eq!(particles.templates[draw.states[0][0]].name, "FrenzyCloud");
        assert_eq!(draw.bones[0], ["None"]);
    }
}

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
            wheels: None,
            tank: None,
        },
    )]);
    (particles, states)
}

fn advance(particles: &mut Particles, states: &BTreeMap<(usize, usize), DrawState>, frame: u64) {
    particles.sync_model_states(states, &mut |_, high| high);
    particles.advance_frame(frame, &mut |_, high| high, &|_| 0.0);
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
    particles.sync_model_states(&states, &mut |_, high| {
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
    states.get_mut(&(8, 0)).unwrap().pose.position = [120.0, 210.0, 40.0];
    particles.sync_model_states(&states, &mut |_, high| high);
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
    assert!(particles.advanced.is_none());
    assert!(!particles.model_states.bindings.is_empty());
}

#[test]
fn shared_manager_emits_in_creation_order_across_drawable_and_free_systems() {
    let (mut particles, states) = fixture();
    particles.templates[0].size = super::super::Range {
        low: 10.0,
        high: 11.0,
    };
    particles.templates[1].size = super::super::Range {
        low: 20.0,
        high: 21.0,
    };
    particles.sync_model_states(&states, &mut |_, high| high);
    particles.create(1, Pose::at([0.0; 3]), None, &mut |_, high| high);
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.particles.is_empty())
    );
    let mut draws = Vec::new();
    particles.advance_frame(
        1,
        &mut |low, high| {
            let range = [low.to_bits(), high.to_bits()];
            if [[10.0, 11.0_f32], [20.0, 21.0]]
                .map(|values| values.map(f32::to_bits))
                .contains(&range)
            {
                draws.push(low.to_bits());
            }
            high
        },
        &|_| 0.0,
    );
    assert_eq!(draws, [10.0_f32.to_bits(), 20.0_f32.to_bits()]);
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.particles.len() == 1)
    );
    particles.advance_frame(1, &mut |_, _| panic!("second manager pass"), &|_| 0.0);
}

#[ignore = "requires original game installation"]
#[test]
fn newer_fire_draw_constructor_precedes_older_model_state_in_the_shared_view() {
    use crate::particles::fire::{Attachment as FireAttachment, Bindings};

    let (_, battle) = crate::cleanup_verification::tests::installed();
    let game = &battle.simulation;
    let (fire, actor) = game
        .entities
        .iter()
        .enumerate()
        .rev()
        .find(|(id, actor)| {
            *id > 0
                && actor.owner == 0
                && !actor.masked()
                && matches!(
                    game.visibility_status(0, *id),
                    Status::Clear | Status::Partial
                )
        })
        .expect("installed local starting object");
    let (mut particles, mut states) = fixture();
    let state = states.remove(&(8, 0)).unwrap();
    states.insert((0, 0), state);
    particles
        .fires
        .bindings
        .insert(actor.type_index, Bindings { draw: vec![1] });
    particles.sync_drawables(
        &states,
        Some((game, [[-10_000.0; 2], [10_000.0; 2]])),
        &mut |_, high| high,
    );
    assert!(
        matches!(particles.emitters[0].fire_attachment, Some(FireAttachment::Object(id)) if id == fire)
    );
    assert_eq!(
        particles.emitters.last().unwrap().drawable_attachment,
        Some((0, 0))
    );
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
            wheels: None,
            tank: None,
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
        wheels: BTreeMap::new(),
        tanks: BTreeMap::new(),
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
        particles.sync_model_states(&states, &mut |_, _| {
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
    particles.sync_model_states(&states, &mut |_, _| {
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

#[test]
fn replacement_of_same_object_and_state_retires_old_particle_incarnation() {
    let (mut particles, states) = fixture();
    particles.retire_replaced_drawables(BTreeMap::from([(8, 1)]));
    advance(&mut particles, &states, 1);
    let original = particles.emitters[0].id;
    particles.retire_replaced_drawables(BTreeMap::from([(8, 1)]));
    advance(&mut particles, &states, 2);
    assert_eq!(particles.next_id, 1);
    particles.retire_replaced_drawables(BTreeMap::from([(8, 20)]));
    particles.sync_model_states(&states, &mut |_, high| high);
    assert_eq!(particles.next_id, 2);
    assert_eq!(particles.emitters[0].id, original);
    assert!(particles.emitters[0].lifecycle.destroyed);
    assert!(!particles.emitters[0].particles.is_empty());
    assert!(!particles.emitters[1].lifecycle.destroyed);
    particles.begin(9, 0);
    assert!(particles.model_states.incarnations.is_empty());
}
