use super::*;
use crate::skirmish::{
    WeaponRules, death_lifecycle,
    structure_death::{Phase, Rules, ToppleRules},
};

fn fixture() -> (Skirmish, usize, State) {
    let mut game = crate::skirmish::tests::game();
    let unit = &mut game.catalog[0];
    unit.geometry.major = 50.0;
    unit.geometry.minor = 50.0;
    unit.geometry.height = 100.0;
    let mut crushing = WeaponRules::unarmed();
    // Original ToppledStructureWeapon reach; zero damage isolates sample geometry.
    crushing.range = 100.0;
    unit.death_lifecycle = Some(death_lifecycle::Rules {
        on_die: Vec::new(),
        slow: Vec::new(),
        physics: None,
        weapons: vec![crushing],
    });
    unit.structure_death = Some(Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        transitions: Vec::new(),
        damage_thresholds: [0.5, 0.1],
        module: Module::Topple(ToppleRules {
            delay: [1, 1],
            burst_delay: [0, 0],
            integrity: 0.75,
            decay: 1.0,
            suppress_water_fx: true,
            start_effect: "Start".into(),
            delay_effect: "Delay".into(),
            done_effect: "Done".into(),
            crushing_effect: "Crush".into(),
            angles: vec![(0.35, "Angle".into())],
            crushing_weapon: 0,
        }),
        phases: std::array::from_fn(|_| Phase {
            effects: Vec::new(),
            objects: Vec::new(),
        }),
    });
    crate::skirmish::tests::register_update(&mut game.catalog[0], "StructureToppleUpdate");
    game.catalog[0]
        .behavior_modules
        .last_mut()
        .unwrap()
        .interfaces = 0x0003;
    let id = game.spawn(0, 0, [100.0, 200.0], true);
    (game, id, State::new(100.0))
}

#[test]
fn crush_samples_rows_and_right_edges_at_original_spacing_using_live_height() {
    let (mut game, id, mut state) = fixture();
    game.catalog[0].geometry.height = 1.0; // Native rubble geometry changes first.
    game.entities[id].facing = 0.0;
    state.direction = [1.0, 0.0];
    game.topple_crush(id, &mut state, 0.0);
    assert_eq!(game.presentation.shots.len(), 15); // Rows 0/25/50/75 and explicit 100 tip, 3 across.
    assert_eq!(
        game.presentation
            .shots
            .first()
            .unwrap()
            .to
            .map(f32::to_bits),
        [100.0_f32, 175.0].map(f32::to_bits)
    );
    assert_eq!(
        game.presentation.shots.last().unwrap().to.map(f32::to_bits),
        [200.0_f32, 225.0].map(f32::to_bits)
    );
    assert!(
        game.presentation
            .shots
            .iter()
            .all(|shot| shot.source_position[..2] == [100.0, 200.0])
    );
    game.topple_crush(id, &mut state, 0.0);
    assert_eq!(game.presentation.shots.len(), 18); // Tip is required again even when no new row fits.
}

#[test]
fn water_suppresses_effects_but_preserves_rotation_crushing_and_burst_callbacks() {
    let (mut game, id, mut state) = fixture();
    game.entities[id].damage_state.damage_type = DamageType::Water;
    game.begin_topple(id, &mut state);
    assert!(
        game.presentation
            .structure_events
            .iter()
            .all(|event| event.effect.is_none())
    );
    state.stage = Stage::Falling;
    state.angle = FRAC_PI_2 - 0.001;
    state.velocity = 0.01;
    state.direction = [0.0, 1.0];
    game.update_topple(id, &mut state);
    assert!(matches!(state.stage, Stage::Done));
    assert!(!game.presentation.shots.is_empty());
    assert!(game.presentation.object_fx_events.is_empty());
    assert!(
        game.presentation
            .structure_events
            .iter()
            .all(|event| event.effect.is_none())
    );
    assert!((state.basis[2][2] - 1.0).abs() < 0.000_01);
    assert!((game.entities[id].facing - FRAC_PI_2).abs() < 0.000_01);
    let count = game.presentation.shots.len();
    game.update_topple(id, &mut state);
    assert_eq!(game.presentation.shots.len(), count);
}

#[test]
fn isolated_waiting_transition_preserves_both_burst_checks() {
    let (mut game, id, mut state) = fixture();
    game.begin_topple(id, &mut state);
    game.tick += 1;
    game.update_topple(id, &mut state);
    let checks = game
        .presentation
        .structure_events
        .iter()
        .filter(|event| matches!(event.action, VisualAction::ToppleCheck { .. }))
        .count();
    assert_eq!(checks, 2);
    assert!(state.basis[2][0].abs() + state.basis[2][1].abs() > 0.0);
}

#[test]
fn removed_attacker_is_native_null_but_retained_dead_attacker_keeps_position() {
    let (mut game, id, mut state) = fixture();
    let attacker = game.spawn(1, 1, [50.0, 200.0], true);
    game.entities[id].damage_state.current_source = Some(attacker);
    game.entities[attacker].health = 0.0;
    let seed = game.random;
    game.begin_topple(id, &mut state);
    let retained = state.direction;
    game.destroy_maintenance_object(attacker);
    game.process_update_destruction();
    game.random = seed;
    let mut removed = State::new(100.0);
    game.begin_topple(id, &mut removed);
    game.entities[id].damage_state.current_source = None;
    game.random = seed;
    let mut absent = State::new(100.0);
    game.begin_topple(id, &mut absent);
    assert_eq!(
        removed.direction.map(f32::to_bits),
        absent.direction.map(f32::to_bits)
    );
    assert_ne!(
        retained.map(f32::to_bits),
        absent.direction.map(f32::to_bits)
    );
}

#[test]
fn ordered_topple_die_runs_scheduled_transform_notifications_and_sleeps_when_done() {
    let (mut game, id, _) = fixture();
    let revision = game.entities[id].drawable_transform_revision();
    game.kill_object(id, crate::skirmish::DeathType::Normal);
    assert!(game.entities[id].is_ai_dead());
    for _ in 0..1000 {
        game.step();
        if game.entities[id].post_collapse() {
            break;
        }
    }
    assert!(game.entities[id].post_collapse());
    let refresh = game
        .presentation
        .body_particles
        .iter()
        .find(|event| event.pose.object == id)
        .unwrap();
    assert!(
        !game
            .model_flags(&game.entities[id], [0.5, 0.1])
            .contains("RUBBLE")
    );
    // Native body refresh precedes doToppleDoneStuff's upright reset.
    assert!(refresh.pose.basis[2][2].abs() < 0.01);
    assert!(game.entities[id].drawable_transform_revision() > revision);
    assert!(game.entities[id].exists() && game.entities[id].has_drawable());
    let basis = game.entities[id].object_basis();
    assert_eq!(
        basis[2].map(f32::to_bits),
        [0.0_f32, 0.0, 1.0].map(f32::to_bits)
    );
    let final_revision = game.entities[id].drawable_transform_revision();
    game.step();
    assert_eq!(
        game.entities[id].drawable_transform_revision(),
        final_revision
    );
    assert!(game.presentation.structure_events.is_empty());
}

#[test]
fn script_direction_override_changes_fall_but_not_random_delay_burst_location() {
    use crate::skirmish::map_scripts::{Action, Condition, Script};
    let (mut game, id, mut state) = fixture();
    game.map_objects.insert("FallingTower".into(), id);
    game.install_map_scripts(vec![Script {
        owner: "player0".into(),
        name: "Direction".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::ToppleDirection(
            "FallingTower".into(),
            [0.0, 2.0, 0.0],
        )],
        false_actions: Vec::new(),
    }]);
    game.step();
    let seed = game.random;
    game.begin_topple(id, &mut state);
    assert_eq!(
        state.direction.map(f32::to_bits),
        [0.0_f32, 1.0].map(f32::to_bits)
    );
    let burst = state.burst_position;
    game.map_objects.remove("FallingTower");
    game.random = seed;
    let mut ordinary = State::new(100.0);
    game.begin_topple(id, &mut ordinary);
    assert_eq!(
        burst.map(f32::to_bits),
        ordinary.burst_position.map(f32::to_bits)
    );
    assert_ne!(
        state.direction.map(f32::to_bits),
        ordinary.direction.map(f32::to_bits)
    );
}
