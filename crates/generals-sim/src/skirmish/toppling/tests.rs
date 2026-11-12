use super::*;
use crate::skirmish::{Faction, Kind, module_updates::ModuleRegistration, tests};

fn rules() -> Rules {
    Rules {
        topple_fx: "Topple".into(),
        bounce_fx: "Bounce".into(),
        stump: None,
        initial_velocity: 0.2,
        initial_acceleration: 0.01,
        bounce_velocity: 0.3,
        kill_on_start: false,
        left_or_right: false,
        finish: FinishRules {
            kill: false,
            kill_stump: false,
            reorient: false,
        },
    }
}

fn fixture(rules: Rules) -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.name = "ToppleFixture".into();
    unit.geometry.height = 20.0;
    unit.behavior.toppling.push(rules);
    tests::register_update(&mut unit, "ToppleUpdate");
    unit.behavior_modules.last_mut().unwrap().interfaces = 0x11;
    let registration = unit
        .update_modules
        .iter()
        .position(|module| {
            matches!(module,
        ModuleRegistration::Behavior { family, .. } if family == "ToppleUpdate")
        })
        .unwrap();
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, game.neutral_owner(), [400.0, 400.0], true);
    (game, id, registration)
}

#[test]
fn event_wakes_real_heap_and_bounce_finishes_before_next_callback_sleeps() {
    let (mut game, id, registration) = fixture(rules());
    assert_eq!(
        game.update_remaining_sleep(id, registration) + game.update_frame(),
        FOREVER
    );
    game.topple_object(id, [1.0, 0.0, 0.0], 10.0, 1);
    assert_eq!(game.update_remaining_sleep(id, registration), 1);
    assert!(game.entities[id].topple_stopped_sway());
    game.tick = 1;
    game.run_object_updates();
    assert!(matches!(
        game.entities[id].behaviors.toppling[0].phase,
        Phase::Down
    ));
    assert_eq!(game.update_remaining_sleep(id, registration), 1);
    let (position, basis) = game.object_transform(id);
    assert_eq!(
        position.map(f32::to_bits),
        [400.0_f32, 400.0, 0.0].map(f32::to_bits)
    );
    assert!(basis[2][0].abs() > 0.99 || basis[2][1].abs() > 0.99);
    assert!(!game.entities[id].shadows_enabled());
    game.tick = 2;
    game.run_object_updates();
    assert_eq!(
        game.update_remaining_sleep(id, registration) + game.update_frame(),
        FOREVER
    );
    assert!(game.entities[id].alive());
    assert_eq!(
        game.presentation
            .object_fx_events
            .iter()
            .filter(|event| event.object == id)
            .count(),
        1
    );
}

#[test]
fn bounce_fx_options_only_suppress_bounce_and_keep_start_fx() {
    for options in [0, 2] {
        let (mut game, id, _) = fixture(rules());
        game.topple_object(id, [1.0, 0.0, 0.0], 10.0, options);
        game.tick = 1;
        game.run_object_updates();
        assert!(matches!(
            game.entities[id].behaviors.toppling[0].phase,
            Phase::Falling
        ));
        assert!(game.entities[id].behaviors.toppling[0].velocity < 0.0);
        let effects: Vec<_> = game
            .presentation
            .object_fx_events
            .iter()
            .filter(|event| event.object == id)
            .map(|event| event.effect.as_str())
            .collect();
        assert_eq!(
            effects,
            if options == 0 {
                vec!["Topple", "Bounce"]
            } else {
                vec!["Topple"]
            }
        );
    }
}

#[test]
fn first_authored_module_handles_stationary_collision_without_invented_minimum_speed() {
    let (mut game, original, _) = fixture(rules());
    let kind = game.entities[original].type_index;
    game.catalog[kind].behavior.toppling.push(rules());
    tests::register_update(&mut game.catalog[kind], "ToppleUpdate");
    game.catalog[kind]
        .behavior_modules
        .last_mut()
        .unwrap()
        .interfaces = 0x11;
    let id = game.spawn(kind, game.neutral_owner(), [450.0, 400.0], true);
    let crusher_kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[crusher_kind].crush.crusher_level = 2;
    let crusher = game.spawn(crusher_kind, 0, [440.0, 400.0], true);
    game.object_contact(id, crusher, false);
    assert!(matches!(
        game.entities[id].behaviors.toppling[0].phase,
        Phase::Falling
    ));
    assert!(matches!(
        game.entities[id].behaviors.toppling[1].phase,
        Phase::Upright
    ));
    assert_eq!(game.entities[id].behaviors.toppling[0].turns, 1);
    assert_eq!(
        game.entities[id].behaviors.toppling[0].velocity.to_bits(),
        0.0_f32.to_bits()
    );
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(
        game.entities[id].behaviors.toppling[0]
            .accumulation
            .to_bits(),
        0.0_f32.to_bits()
    );
    assert!(game.entities[id].alive());
}

#[test]
fn named_direction_fence_path_removal_and_stump_team_follow_source_event_order() {
    use crate::skirmish::map_scripts::{Action, Condition, Script};
    let mut rule = rules();
    rule.left_or_right = true;
    rule.stump = Some("ToppleStump".into());
    let (mut game, id, _) = fixture(rule);
    let mut stump = tests::fixture_type(Faction::Civilian, Kind::Effect);
    stump.name = "ToppleStump".into();
    game.catalog.push(stump);
    game.map_objects.insert("Fence".into(), id);
    game.install_map_scripts(vec![Script {
        owner: String::new(),
        name: "Wind-fallen fence".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::ToppleDirection("Fence".into(), [0.0, -3.0, 0.0])],
        false_actions: Vec::new(),
    }]);
    game.update_map_scripts();
    game.topple_object(id, [1.0, 0.0, 0.0], 10.0, 0);
    let state = &game.entities[id].behaviors.toppling[0];
    assert!(state.direction[1] < -0.99);
    let stump = state.stump.unwrap();
    assert_eq!(
        game.team_membership.team_of(stump),
        Some(game.team_membership.default_team(game.neutral_owner()))
    );
    assert_eq!(
        game.object_position(stump).map(f32::to_bits),
        game.object_position(id).map(f32::to_bits)
    );
    // A transform update must not restore the fence's explicitly removed obstacle.
    game.set_object_position(id, [400.0, 400.0, 1.0]);
    game.rebuild_navigation();
    assert!(
        !game
            .navigation
            .is_blocked(game.cell_at([400.0, 400.0]).unwrap())
    );
}

#[test]
fn kill_at_start_leaves_sway_and_model_untouched_and_creates_no_stump() {
    let mut rule = rules();
    rule.kill_on_start = true;
    rule.stump = Some("ToppleStump".into());
    let (mut game, id, registration) = fixture(rule);
    let mut stump = tests::fixture_type(Faction::Civilian, Kind::Effect);
    stump.name = "ToppleStump".into();
    game.catalog.push(stump);
    let count = game.entities.len();
    game.topple_object(id, [1.0, 0.0, 0.0], 10.0, 0);
    assert!(!game.entities[id].alive());
    assert!(!game.entities[id].topple_stopped_sway());
    assert_eq!(game.entities.len(), count);
    assert_eq!(
        game.update_remaining_sleep(id, registration) + game.update_frame(),
        FOREVER
    );
    assert!(
        !game.entities[id]
            .drawable
            .model_conditions
            .has_flag(ModelConditions::TOPPLED)
    );
}

#[test]
fn completion_damage_precedes_rubble_reorientation_and_stump_death() {
    let mut rule = rules();
    rule.finish = FinishRules {
        kill: true,
        reorient: true,
        kill_stump: true,
    };
    rule.stump = Some("ToppleStump".into());
    let (mut game, id, _) = fixture(rule);
    let mut stump = tests::fixture_type(Faction::Civilian, Kind::Effect);
    stump.name = "ToppleStump".into();
    game.catalog.push(stump);
    game.topple_object(id, [1.0, 0.0, 0.0], 10.0, 1);
    let stump = game.entities[id].behaviors.toppling[0].stump.unwrap();
    game.tick = 1;
    game.run_object_updates();
    assert!(matches!(
        game.entities[id].damage_state.death_type,
        DeathType::Toppled
    ));
    assert!(matches!(
        game.entities[stump].damage_state.death_type,
        DeathType::Toppled
    ));
    assert!(!game.entities[id].alive() && !game.entities[stump].alive());
    let (_, basis) = game.object_transform(id);
    assert_eq!(
        basis[2].map(f32::to_bits),
        [0.0_f32, 0.0, 1.0].map(f32::to_bits)
    );
}
