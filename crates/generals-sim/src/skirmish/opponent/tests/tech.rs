use super::*;
use crate::skirmish::{
    Kind,
    command_button_hunt::{Button, Command, Definitions},
    special_ability::{Power, PowerType, capture_fixture},
};

fn tech_rule() -> OpponentRules {
    let mut rules = rules(vec![script(
        "Tech search",
        vec![call(
            "SKIRMISH_TECH_BUILDING_WITHIN_DISTANCE",
            vec![
                parameter("<This Player>", 0),
                parameter("", 0),
                parameter("[Skirmish]MyInnerPerimeter", 0),
            ],
        )],
        vec![],
    )]);
    rules.map.areas.push(AiArea {
        name: "InnerPerimeter2".into(),
        points: vec![
            [300.0, 200.0],
            [500.0, 200.0],
            [500.0, 400.0],
            [300.0, 400.0],
        ],
    });
    rules
}

#[test]
fn tech_detection_caches_the_first_valid_query_and_ignores_owned_allied_and_off_map_providers() {
    let mut game = crate::skirmish::tests::game();
    game.slots[0].team = Some(1);
    game.slots[1].team = Some(1);
    let kind = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[kind].kind_of.push("TECH_BUILDING".into());
    game.spawn(kind, 1, [400.0, 300.0], true);
    game.spawn(kind, 0, [410.0, 300.0], true);
    let neutral = game.neutral_owner();
    let provider = game.spawn(kind, neutral, [450.0, 300.0], true);
    game.entities[provider].set_off_map(true);
    game.update_ground_contacts();
    let rules = tech_rule();
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    assert_eq!(
        scripts::conditions(&game, &rules, &mut state, 0, None),
        Some(false)
    );
    game.entities[provider].set_off_map(false);
    game.update_ground_contacts();
    assert_eq!(
        scripts::conditions(&game, &rules, &mut state, 0, None),
        Some(false)
    );
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    assert_eq!(
        scripts::conditions(&game, &rules, &mut state, 0, None),
        Some(true)
    );
    game.entities[provider].owner = 1;
    assert_eq!(
        scripts::conditions(&game, &rules, &mut state, 0, None),
        Some(true)
    );
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    assert_eq!(
        scripts::conditions(&game, &rules, &mut state, 0, None),
        Some(false)
    );
}

#[test]
fn missing_tech_perimeter_does_not_poison_the_condition_cache() {
    let mut game = crate::skirmish::tests::game();
    let mut rules = tech_rule();
    let area = rules.map.areas.pop().unwrap();
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    assert_eq!(
        scripts::conditions(&game, &rules, &mut state, 0, None),
        Some(false)
    );
    assert!(state.tech_conditions.is_empty());
    rules.map.areas.push(area);
    assert_eq!(
        scripts::conditions(&game, &rules, &mut state, 0, None),
        Some(false)
    );
    assert_eq!(state.tech_conditions.len(), 1);
}

#[test]
fn sequential_capture_wait_stalls_until_research_unpauses_the_original_power() {
    let mut game = crate::skirmish::tests::game();
    let kind = game.type_for(1, Kind::Infantry).unwrap();
    capture_fixture::bind(
        &mut game.catalog[kind],
        PowerType::SpecialInfantryCaptureBuilding,
        [1, 1, 1, 5],
        true,
    );
    let actor = game.spawn(kind, 1, [400.0, 300.0], true);
    game.configure_command_buttons(Definitions {
        buttons: vec![Button {
            name: "Command_AmericaRangerCaptureBuilding".into(),
            command: Command::SpecialPower,
            weapon_slot: 0,
            power: Some(Power {
                name: "InfantryCapture".into(),
                kind: PowerType::SpecialInfantryCaptureBuilding,
            }),
            upgrade: None,
            needs_object: true,
        }],
        sets: BTreeMap::new(),
    });
    let wait = call(
        "SKIRMISH_WAIT_FOR_COMMANDBUTTON_AVAILABLE_ALL",
        vec![
            parameter("unused source player", 0),
            parameter("<This Team>", 0),
            parameter("Command_AmericaRangerCaptureBuilding", 0),
        ],
    );
    let rules = rules(vec![script(
        "Capture orders",
        vec![],
        vec![
            wait,
            call(
                "SET_COUNTER",
                vec![parameter("advanced", 0), parameter("", 1)],
            ),
        ],
    )]);
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    state.teams.push(Team::new(None, 1, 0));
    state.teams[0].members = vec![actor];
    state.teams[0].native_instance = Some(game.team_membership.default_team(1));
    state.teams[0].sequence = Some((0, 0));
    teams::sequence(&mut game, &rules, &mut state, 0);
    assert_eq!(state.teams[0].sequence, Some((0, 0)));
    assert!(!state.counters.contains_key("advanced"));
    // The native button's upgrade branch can be ready while its power is paused.
    // It tests whether the offered upgrade would execute, not whether it is owned.
    game.command_buttons.buttons[0].upgrade = Some("Capture".into());
    let wait = &rules.factions[0].scripts[0].actions[0];
    assert_eq!(
        commands::button_ready(&mut game, &rules, &state, wait, Some(0)),
        Some(true)
    );
    game.command_buttons.buttons[0].upgrade = None;
    game.complete_player_upgrade(1, "Capture");
    // This fixture's original power still has five frames left on its timer.
    for _ in 0..5 {
        game.step();
    }
    assert_eq!(
        game.special_power_percent(actor, "InfantryCapture")
            .to_bits(),
        1.0_f32.to_bits()
    );
    teams::sequence(&mut game, &rules, &mut state, 0);
    assert!(state.teams[0].sequence.is_none());
    assert_eq!(state.counters["advanced"].value, 1);
    assert!(state.unsupported.is_empty());
}
