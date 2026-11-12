use super::super::{FeedbackEffect, hacker_disable_tests::setup};
use crate::skirmish::hacker_disable::Effect;

#[test]
fn source_small_footprint_fx_toggle_survives_stop_and_does_not_change_logic_rng() {
    let (mut game, hacker, target) = setup();
    let index = game.entities[target].type_index;
    game.catalog[index].geometry.major = 4.0;
    game.catalog[index].geometry.minor = 4.0;
    let random = game.random;
    game.disable_from_native_ability(hacker, 0, target);
    assert!(game.entities[target].hacked());
    assert!(
        !game
            .presentation
            .ability_feedback
            .iter()
            .any(|event| matches!(event.effect, FeedbackEffect::DisableParticles { .. }))
    );
    game.exit_native_ability(hacker, 0, false);
    game.disable_from_native_ability(hacker, 0, target);
    assert!(
        game.presentation
            .ability_feedback
            .iter()
            .any(|event| event.target == target
                && matches!(
                    event.effect,
                    FeedbackEffect::DisableParticles { lifetime: 12, .. }
                ))
    );
    assert_eq!(game.random, random);
}

#[test]
fn multiple_hackers_assign_the_latest_deadline_instead_of_accumulating_durations() {
    let (mut game, hacker, target) = setup();
    let second = game.spawn(game.entities[hacker].type_index, 0, [180.0, 200.0], true);
    game.tick = 10;
    game.disable_from_native_ability(hacker, 0, target);
    game.tick = 11;
    game.disable_from_native_ability(second, 0, target);
    assert_eq!(game.entities[target].hacked_until, 17);
    assert_eq!(
        game.presentation
            .disable_events
            .iter()
            .filter(|event| matches!(event.effect, Effect::DisabledEdge { .. }))
            .count(),
        1
    );
}
