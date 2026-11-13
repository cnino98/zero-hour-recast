use super::*;

fn collapse_game() -> (Skirmish, usize) {
    let mut game = super::super::tests::game();
    game.catalog[0].geometry.height = 1.0;
    game.catalog[0].geometry.shape = crate::collision_geometry::Shape::Box;
    game.catalog[0].body_damage_thresholds = [0.5, 0.1];
    game.catalog[0].structure_death = Some(Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        module: Module::Collapse(CollapseRules {
            delay: [1, 1],
            burst_delay: [1, 1],
            big_burst_frequency: 4,
            damping: 0.0,
            shudder: 0.6,
            gravity: -1.0,
        }),
        phases: std::array::from_fn(|_| Phase {
            effects: Vec::new(),
            objects: Vec::new(),
        }),
    });
    crate::skirmish::tests::register_update(&mut game.catalog[0], "StructureCollapseUpdate");
    game.catalog[0]
        .behavior_modules
        .last_mut()
        .unwrap()
        .interfaces = 0x0003;
    let id = game.spawn(0, 0, [100.0, 200.0], true);
    (game, id)
}

#[test]
fn collapse_transition_consumes_both_shudders_without_moving_collision_object() {
    let (mut game, id) = collapse_game();
    let position = game.entities[id].position;
    game.begin_structure_death(id);
    assert!(game.presentation.structure_events.is_empty());
    game.tick += 1;
    game.update_structure_death(id);
    assert_eq!(game.presentation.structure_events.len(), 2);
    assert!(
        game.presentation
            .structure_events
            .iter()
            .all(|event| matches!(event.action,
        VisualAction::CollapseShudder { height, .. } if height == 0.0))
    );
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    game.tick += 1;
    game.update_structure_death(id);
    assert!(game.entities[id].post_collapse());
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert!(matches!(
        game.presentation.structure_events.last().unwrap().action,
        VisualAction::Clear
    ));
}

#[test]
fn fixed_collapse_delays_leave_logic_rng_untouched_until_big_burst_choice() {
    let (mut game, id) = collapse_game();
    let random = game.random;
    game.begin_structure_death(id);
    game.tick += 1;
    game.update_structure_death(id);
    assert_eq!(game.random, random);
    game.tick += 1;
    game.update_structure_death(id);
    assert_ne!(game.random, random);
}

#[test]
fn transition_healing_and_filtered_damage_retire_previous_module_emitters() {
    let (mut game, id) = collapse_game();
    game.catalog[0].behavior.damage_transitions = vec![TransitionRules {
        suppress_water: true,
        suppress_water_fx: false,
        effects: std::array::from_fn(|_| Vec::new()),
        particles: std::array::from_fn(|state| {
            if state == 2 {
                vec![TransitionParticle {
                    slot: 1,
                    random_bone: false,
                    bone: "Fire02".into(),
                    template: "OriginalSmoke".into(),
                    position: [1.0, 2.0, 3.0],
                }]
            } else {
                Vec::new()
            }
        }),
    }];
    let maximum = game.entities[id].max_health;
    game.entities[id].health = maximum * 0.1;
    game.structure_damage_transition(id, maximum);
    assert_eq!(game.presentation.structure_events.len(), 2);
    assert!(matches!(
        game.presentation.structure_events[0].action,
        VisualAction::TransitionClear { module: 0 }
    ));
    assert!(matches!(
        game.presentation.structure_events[1].action,
        VisualAction::TransitionParticle { module: 0, .. }
    ));
    game.apply_healing(id, None, maximum);
    assert_eq!(game.presentation.structure_events.len(), 3);
    game.entities[id].damage_state.damage_type = super::super::DamageType::Water;
    game.entities[id].health = maximum * 0.1;
    game.structure_damage_transition(id, maximum);
    assert_eq!(game.presentation.structure_events.len(), 4);
    assert!(matches!(
        game.presentation.structure_events[3].action,
        VisualAction::TransitionClear { .. }
    ));
}

#[test]
fn transition_fx_precedes_numbered_smoke_without_consuming_logic_random() {
    use crate::skirmish::model_bones::{Draw, State};
    use std::collections::BTreeSet;
    let (mut game, id) = collapse_game();
    game.catalog[0].pristine_draws.push(Draw {
        ignored: BTreeSet::new(),
        states: vec![State {
            conditions: vec![BTreeSet::new()],
            transforms: [(
                "smoke01".into(),
                [
                    [1.0, 0.0, 0.0, 2.0],
                    [0.0, 1.0, 0.0, 3.0],
                    [0.0, 0.0, 1.0, 4.0],
                ],
            )]
            .into(),
        }],
    });
    let mut transition = TransitionRules {
        suppress_water: false,
        suppress_water_fx: false,
        effects: std::array::from_fn(|_| Vec::new()),
        particles: std::array::from_fn(|_| Vec::new()),
    };
    transition.effects[2].push(TransitionEffect {
        slot: 1,
        template: "OriginalDamageFx".into(),
        local: [2.0, -3.0, 4.0],
    });
    transition.particles[2].push(TransitionParticle {
        slot: 1,
        bone: "Smoke".into(),
        random_bone: true,
        template: "OriginalSmoke".into(),
        position: [0.0; 3],
    });
    game.catalog[0].behavior.damage_transitions.push(transition);
    let basis = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    game.set_object_transform(id, [100.0, 200.0, 30.0], basis);
    let maximum = game.entities[id].max_health;
    game.entities[id].health = maximum * 0.1;
    let expected_random = game.random;
    game.structure_damage_transition(id, maximum);
    assert_eq!(game.random, expected_random);
    let events = &game.presentation.structure_events;
    assert!(matches!(
        events[0].action,
        VisualAction::TransitionClear { .. }
    ));
    assert!(matches!(events[1].action, VisualAction::PositionFx));
    assert_eq!(
        events[1]
            .effect
            .as_ref()
            .unwrap()
            .position
            .map(f32::to_bits),
        [103.0_f32, 202.0, 34.0].map(f32::to_bits)
    );
    let VisualAction::TransitionParticle { ref positions, .. } = events[2].action else {
        panic!("transition smoke did not follow the positional FX");
    };
    assert_eq!(positions.len(), 1);
    assert_eq!(
        positions[0].map(f32::to_bits),
        [2.0_f32, 3.0, 4.0].map(f32::to_bits)
    );
}

#[test]
fn transition_mask_uses_retained_body_damage_not_the_incoming_anonymous_water_hit() {
    let (mut game, id) = collapse_game();
    game.catalog[0].behavior.damage_transitions = vec![TransitionRules {
        suppress_water: true,
        suppress_water_fx: false,
        effects: std::array::from_fn(|_| Vec::new()),
        particles: std::array::from_fn(|state| {
            if state == 2 {
                vec![TransitionParticle {
                    slot: 1,
                    bone: "None".into(),
                    random_bone: false,
                    template: "StructureTransitionMediumSmoke".into(),
                    position: [0.0; 3],
                }]
            } else {
                Vec::new()
            }
        }),
    }];
    game.tick = 10;
    game.record_body_damage(id, Some(id), super::super::DamageType::Explosion);
    game.record_body_damage(id, None, super::super::DamageType::Water);
    let maximum = game.entities[id].max_health;
    game.entities[id].health = maximum * 0.1;
    game.structure_damage_transition(id, maximum);
    assert!(matches!(
        game.presentation.structure_events.last().unwrap().action,
        VisualAction::TransitionParticle { module: 0, .. }
    ));
    game.entities[id].health = maximum;
    game.structure_damage_transition(id, maximum * 0.1);
    let before = game.presentation.structure_events.len();
    game.tick = 12;
    game.record_body_damage(id, None, super::super::DamageType::Water);
    game.entities[id].health = maximum * 0.1;
    game.structure_damage_transition(id, maximum);
    assert_eq!(game.presentation.structure_events.len(), before + 1);
    assert!(matches!(
        game.presentation.structure_events.last().unwrap().action,
        VisualAction::TransitionClear { module: 0 }
    ));
}

#[test]
fn actual_die_callback_wakes_once_then_sleeps_after_completion_and_deselects() {
    let (mut game, id) = collapse_game();
    // Both the constructor sleep and the DIE wake use the real module scheduler.
    game.step();
    assert!(game.presentation.structure_events.is_empty());
    assert_eq!(game.create_selection(0, &[], &[id], true), [id]);
    game.kill_object(id, crate::skirmish::DeathType::Normal);
    assert!(game.selected_objects(0).is_empty());
    assert!(game.entities[id].is_ai_dead());
    assert!(!game.entities[id].post_collapse());
    game.step();
    let shudders = game
        .presentation
        .structure_events
        .iter()
        .filter(|event| matches!(event.action, VisualAction::CollapseShudder { .. }))
        .count();
    assert_eq!(shudders, 2);
    assert!(!game.entities[id].post_collapse());
    game.step();
    assert!(game.entities[id].post_collapse());
    assert!(game.entities[id].exists() && game.entities[id].has_drawable());
    game.step();
    assert!(game.presentation.structure_events.is_empty());
}

#[test]
fn die_mux_rejection_keeps_constructor_asleep_and_does_not_deselect() {
    let (mut game, id) = collapse_game();
    game.catalog[0]
        .structure_death
        .as_mut()
        .unwrap()
        .conditions
        .veterancy_mask = 0;
    assert_eq!(game.create_selection(0, &[], &[id], true), [id]);
    game.kill_object(id, crate::skirmish::DeathType::Normal);
    assert_eq!(game.selected_objects(0), [id]);
    assert!(!game.entities[id].is_ai_dead());
    for _ in 0..4 {
        game.step();
    }
    assert!(game.presentation.structure_events.is_empty());
    assert!(!game.entities[id].post_collapse());
}
