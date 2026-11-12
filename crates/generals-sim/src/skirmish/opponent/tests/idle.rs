//! Native `Team::updateState` prior-idle gate and real Hacker on-idle callback.
use super::*;
use crate::skirmish::{Kind, Order};

fn setup(
    mut idle: AiScript,
    hacking: Option<crate::skirmish::income::HackerRules>,
) -> (Skirmish, OpponentRules, State, usize) {
    let mut game = crate::skirmish::tests::game();
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    if hacking.is_some() {
        game.catalog[infantry].behavior.hacking = hacking;
        crate::skirmish::income::tests::bind_hacker(&mut game.catalog[infantry]);
    }
    let id = game.spawn(infantry, 1, [400.0, 300.0], true);
    idle.flags |= 32;
    let mut template = recipe();
    template.on_destroyed.clear();
    template.on_idle.clone_from(&idle.name);
    let mut rules = rules(vec![idle]);
    rules.factions[0].teams.push(template);
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    let mut team = Team::new(Some(0), 0, 1);
    let native = game.team_membership.create_team(state.team_prototypes[0]);
    team.native_instance = Some(native);
    game.team_membership.set_team(id, Some(native));
    game.team_membership.activate(native);
    team.members.push(id);
    state.teams.push(team);
    (game, rules, state, id)
}

#[test]
fn idle_callback_requires_previous_idle_then_repeats_until_nonidle_or_no_living_ai() {
    let (mut game, rules, mut state, id) = setup(
        script(
            "Idle",
            vec![call("CONDITION_TRUE", vec![])],
            vec![call(
                "INCREMENT_COUNTER",
                vec![parameter("", 1), parameter("Ran", 0)],
            )],
        ),
        None,
    );
    let building = game.type_for(1, Kind::CommandCenter).unwrap();
    let building = game.spawn(building, 1, [700.0, 500.0], true);
    state.teams[0].members.push(building);
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert!(!state.counters.contains_key("Ran"));
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 1);
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 2);
    game.move_to(id, [500.0, 400.0]).unwrap();
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 2);
    game.stop(id).unwrap();
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 2);
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 3);
    game.entities[id].health = 0.0;
    for _ in 0..3 {
        lifecycle::update_states(&mut game, &rules, &mut state);
    }
    assert_eq!(state.counters["Ran"].value, 3);
}

#[test]
fn original_hacker_idle_callback_begins_income_then_leaves_pending_state_untouched() {
    let (mut game, rules, mut state, id) = setup(
        script(
            "China Behavior - Hack Idle",
            vec![call("CONDITION_TRUE", vec![])],
            vec![call(
                "TEAM_USE_COMMANDBUTTON_ABILITY",
                vec![
                    parameter("<This Team>", 0),
                    parameter("Command_ChinaInfantryHackerInternetHack", 0),
                ],
            )],
        ),
        Some(crate::skirmish::income::HackerRules {
            unpack: 0,
            pack: 0,
            variation: 0.0,
            delay: 0,
            contained_delay: 1,
            amounts: [5; 4],
            experience: 1,
        }),
    );
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert!(matches!(game.entities[id].ai.order, Order::HackInternet));
    assert_eq!(game.presentation.hacking_events.len(), 1);
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(game.presentation.hacking_events.len(), 1);
    game.step();
    game.step();
    assert_eq!(game.players[1].statistics.money_earned, 5);
    game.move_to(id, [500.0, 400.0]).unwrap();
    lifecycle::update_states(&mut game, &rules, &mut state);
    game.step();
    assert!(!game.entities[id].ai_idle());
    lifecycle::update_states(&mut game, &rules, &mut state);
    game.step();
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
}
