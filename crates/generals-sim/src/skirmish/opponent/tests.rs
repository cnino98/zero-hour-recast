use super::*;

mod idle;
mod tech;
mod translated;

fn parameter(text: &str, integer: i32) -> AiParameter {
    AiParameter {
        text: text.into(),
        integer,
        real: 0.0,
        position: None,
    }
}

fn call(name: &str, parameters: Vec<AiParameter>) -> AiCall {
    AiCall {
        name: name.into(),
        parameters,
    }
}

fn script(name: &str, conditions: Vec<AiCall>, actions: Vec<AiCall>) -> AiScript {
    AiScript {
        name: name.into(),
        group: None,
        flags: 5,
        delay_frames: 0,
        conditions: vec![conditions],
        actions,
        false_actions: Vec::new(),
    }
}

pub(in crate::skirmish) fn rules(scripts: Vec<AiScript>) -> OpponentRules {
    OpponentRules {
        defense_extra_distance: 20.0,
        max_recruit_distance: 500.0,
        attack_priority_distance: 10.0,
        skirmish_group_fudge: 0.0,
        alert_range_modifier: 1.1,
        aggressive_range_modifier: 1.5,
        build_facilities: vec!["Fixture".into()],
        map: OpponentMap {
            areas: Vec::new(),
            waypoints: Vec::new(),
        },
        factions: vec![OpponentFaction {
            default_team: "teamSkirmishGLA".into(),
            defense_template: String::new(),
            faction: Faction::Gla,
            buildings: Vec::new(),
            gatherers: [5; 3],
            scripts,
            groups: Vec::new(),
            teams: Vec::new(),
        }],
        structure_seconds: 0.0,
        team_seconds: 10.0,
        wealthy: 7000,
        poor: 2000,
        structures_wealthy_rate: 2.0,
        structures_poor_rate: 0.6,
        teams_wealthy_rate: 2.0,
        teams_poor_rate: 0.6,
        team_resources_to_start: 0.1,
        rebuild_delay_seconds: 30,
        supply_center_safe_radius: 300.0,
    }
}

#[test]
fn timer_is_a_counter_and_decrements_before_conditions_until_minus_one() {
    let mut game = super::super::tests::game();
    let mut start = script(
        "Start",
        vec![call("CONDITION_TRUE", vec![])],
        vec![call(
            "SET_TIMER",
            vec![parameter("Clock", 0), parameter("", 3)],
        )],
    );
    start.flags |= 2;
    let mut expired = script(
        "Expired",
        vec![
            call("TIMER_EXPIRED", vec![parameter("Clock", 0)]),
            call(
                "COUNTER",
                vec![parameter("Clock", 0), parameter("", 2), parameter("", 0)],
            ),
        ],
        vec![call(
            "INCREMENT_COUNTER",
            vec![parameter("", 1), parameter("Ran", 0)],
        )],
    );
    expired.flags |= 2;
    let rules = rules(vec![start, expired]);
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    for frame in 1..=3 {
        game.tick = frame;
        scripts::update(&mut game, &rules, &mut state);
        assert!(!state.counters.contains_key("Ran"));
    }
    game.tick = 4;
    scripts::update(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 1);
    assert_eq!(state.counters["Clock"].value, 0);
    for frame in 5..=8 {
        game.tick = frame;
        scripts::update(&mut game, &rules, &mut state);
    }
    assert_eq!(state.counters["Clock"].value, -1);
    assert_eq!(state.counters["Ran"].value, 1);
}

#[test]
fn unknown_condition_executes_neither_branch_and_is_reported() {
    let mut game = super::super::tests::game();
    let action = || {
        call(
            "INCREMENT_COUNTER",
            vec![parameter("", 1), parameter("Ran", 0)],
        )
    };
    let mut unknown = script(
        "Unknown",
        vec![call("UNPORTED_CONDITION", vec![])],
        vec![action()],
    );
    unknown.false_actions.push(action());
    let rules = rules(vec![unknown]);
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    scripts::update(&mut game, &rules, &mut state);
    assert!(!state.counters.contains_key("Ran"));
    assert_eq!(state.unsupported, ["UNPORTED_CONDITION"]);
}

#[test]
fn enabling_a_name_enables_both_matching_group_and_script() {
    let mut game = super::super::tests::game();
    let mut guarded = script(
        "Shared",
        vec![call("CONDITION_TRUE", vec![])],
        vec![call(
            "SET_COUNTER",
            vec![parameter("Ran", 0), parameter("", 1)],
        )],
    );
    guarded.flags = 4;
    guarded.group = Some(0);
    let mut rules = rules(vec![guarded]);
    rules.factions[0].groups.push(AiScriptGroup {
        name: "Shared".into(),
        active: false,
        subroutine: false,
    });
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    scripts::update(&mut game, &rules, &mut state);
    assert!(!state.counters.contains_key("Ran"));
    scripts::execute(
        &mut game,
        &rules,
        &mut state,
        &call("ENABLE_SCRIPT", vec![parameter("Shared", 0)]),
        None,
    );
    scripts::update(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 1);
}

#[test]
fn team_generic_hook_ignores_global_gates_but_runs_once_per_team() {
    let mut game = super::super::tests::game();
    let mut generic = script(
        "Generic",
        vec![call("CONDITION_TRUE", vec![])],
        vec![call(
            "INCREMENT_COUNTER",
            vec![parameter("", 1), parameter("Ran", 0)],
        )],
    );
    generic.flags = 2 | 16; // Globally inactive, hard-only and one-shot.
    generic.delay_frames = 300;
    let mut rules = rules(vec![generic]);
    rules.factions[0].teams.push(AiTeam {
        recruitment: 0,
        home: None,
        singleton: false,
        attack_common_target: false,
        priority_success: 0,
        priority_failure: 0,
        execute_actions: false,
        destroyed_threshold: 1.0,
        aggression: 2,
        name: "Team".into(),
        units: vec![AiTeamUnit {
            name: "Fixture".into(),
            minimum: 1,
            maximum: 1,
        }],
        priority: 1,
        max_instances: 2,
        condition: String::new(),
        on_create: String::new(),
        on_idle: String::new(),
        on_destroyed: String::new(),
        generic_scripts: vec!["Generic".into()],
        initial_idle_frames: 0,
    });
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    state.enabled = 0;
    scripts::update(&mut game, &rules, &mut state);
    assert!(!state.counters.contains_key("Ran"));
    for _ in 0..2 {
        state.enabled = 2;
        teams::queue(&mut game, &rules, &mut state, 0, false);
    }
    state.enabled = 0;
    for team in &mut state.teams {
        game.team_membership.activate(team.native_instance.unwrap());
        team.members.push(3);
        team.scripts[0].next = 1000;
    }
    teams::update(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 2);
    teams::update(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 2);
}

#[test]
fn result_countdown_keeps_ai_production_but_blocks_global_scripts_and_player_orders() {
    let mut game = super::super::tests::game();
    let builder = game.type_for(1, super::super::Kind::Builder).unwrap();
    let center = game.entities[2].type_index;
    // The completed center supplies normal production during the result timer.
    game.catalog[center].energy = 1;
    game.update_power();
    game.catalog[builder].name = "Builder".into();
    game.catalog[center].name = "CommandCenter".into();
    game.catalog[center].buildable_objects = vec!["Builder".into()];
    game.train(2, builder).unwrap();
    for _ in 0..60 {
        game.step();
    }
    let mut rules = rules(vec![script(
        "Recruit",
        vec![call("CONDITION_TRUE", vec![])],
        vec![call(
            "INCREMENT_COUNTER",
            vec![parameter("", 1), parameter("Global", 0)],
        )],
    )]);
    rules.factions[0].buildings.push(AiBuilding {
        bounding_radius: 20.0,
        name: "CommandCenter".into(),
        position: [0.0; 2],
        angle: 0.0,
        rebuilds: 0,
        automatic: false,
    });
    rules.factions[0].teams.push(AiTeam {
        recruitment: 0,
        home: None,
        singleton: false,
        attack_common_target: false,
        name: "Builders".into(),
        units: vec![AiTeamUnit {
            name: "Builder".into(),
            minimum: 1,
            maximum: 1,
        }],
        // Equal to the default team: this queue must produce rather than recruit
        // the existing builders, so the end-game production assertion is valid.
        priority: 0,
        priority_success: 0,
        priority_failure: 0,
        max_instances: 1,
        execute_actions: false,
        condition: "Recruit".into(),
        on_create: String::new(),
        on_idle: String::new(),
        on_destroyed: String::new(),
        generic_scripts: Vec::new(),
        initial_idle_frames: 0,
        destroyed_threshold: 1.0,
        aggression: 2,
    });
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    teams::queue(&mut game, &rules, &mut state, 0, false);
    let owner = state.owner;
    game.opponents[owner] = Some(Opponent {
        rules: Arc::new(rules),
        state,
    });
    game.entities[0].health = 0.0;
    game.step();
    assert_eq!(game.outcome, Some(1));
    let ended_at = game.ended_at.unwrap();
    game.start_end_game_timer();
    let cash = game.players[1].cash;
    assert!(game.train(2, builder).is_err());
    assert!(game.move_to(3, [700.0, 400.0]).is_err());
    assert!(game.group_move_to(&[3], [700.0, 400.0], false).is_err());
    for _ in 0..120 {
        game.step();
    }
    assert_eq!(game.tick, ended_at + 120);
    // The existing paid unit completes, then the AI queues its team's unit
    // during the banner despite the disabled player-command entry points.
    assert_eq!(game.players[1].statistics.units_built, 2);
    assert_eq!(game.players[1].cash, cash - game.catalog[builder].cost);
    assert_eq!(game.entities[2].queue.len(), 1);
    assert_eq!(
        game.opponents[1].as_ref().unwrap().state.counters["Global"].value,
        1
    );
    game.step();
    assert_eq!(game.tick, ended_at + 120);
    assert_eq!(game.players[1].cash, cash - game.catalog[builder].cost);
    assert_eq!(game.players[1].statistics.units_built, 2);
}

fn supply_placement_game() -> Skirmish {
    let mut game = super::super::tests::game();
    // Supply warehouses are live map objects. Remove only the starting
    // buildings and units through GameLogic's deferred destruction.
    for id in 0..game.entities.len() {
        if !game.supplies.iter().any(|supply| supply.object == id) {
            game.destroy_maintenance_object(id);
        }
    }
    game.process_update_destruction();
    game
}

#[test]
fn supply_request_uses_native_fine_scan_first_legal_edge_position() {
    let mut game = supply_placement_game();
    let rules = rules(Vec::new());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    // Right-hand warehouse [615,175], base [615,305], requested point [615,205].
    // Expanded warehouse radius36 plus supply radius25 rejects the first rings.
    // Native span60 visits [585,175] then [585,235]; the latter is first legal.
    construction::supply(&game, &mut state, "Fixture", 10_000);
    let building = &state.buildings[0];
    assert_eq!(
        building.position.map(f32::to_bits),
        [585.0_f32.to_bits(), 235.0_f32.to_bits()]
    );
    let supply = game.type_for(1, super::super::Kind::SupplyCenter).unwrap();
    assert!(game.can_place_at_angle(supply, building.position, building.angle, true));
    assert!(!game.can_place_at_angle(supply, [615.0, 205.0], building.angle, true));
    assert!(building.priority);
}

#[test]
fn initial_supply_overlap_test_does_not_apply_the_fine_scans_terrain_option() {
    let mut game = supply_placement_game();
    let supply = game.type_for(1, super::super::Kind::SupplyCenter).unwrap();
    game.catalog[supply]
        .kind_of
        .retain(|kind| kind != "CANNOT_BUILD_NEAR_SUPPLIES");
    // The center is 30 units from a radius-16 warehouse. A narrow supply
    // center fits there, leaving the blocked terrain cell as the only veto.
    game.catalog[supply].geometry.major = 10.0;
    game.catalog[supply].geometry.minor = 10.0;
    game.catalog[supply].geometry_bounds.sphere_radius = 10.0;
    game.grid = crate::Grid::new(80, 60, [crate::Cell { x: 61, y: 20 }]).unwrap();
    let rules = rules(Vec::new());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    construction::supply(&game, &mut state, "Fixture", 10_000);
    let building = &state.buildings[0];
    assert_eq!(
        building.position.map(f32::to_bits),
        [615.0_f32.to_bits(), 205.0_f32.to_bits()]
    );
    assert!(!game.can_place_at_angle(supply, building.position, building.angle, true));
}

pub(super) fn recipe() -> AiTeam {
    AiTeam {
        recruitment: 0,
        home: None,
        singleton: false,
        attack_common_target: false,
        name: "Collectors".into(),
        units: vec![AiTeamUnit {
            name: "Fixture".into(),
            minimum: 1,
            maximum: 2,
        }],
        priority: 10,
        priority_success: 3,
        priority_failure: 4,
        max_instances: 2,
        execute_actions: true,
        condition: "Production".into(),
        on_create: String::new(),
        on_idle: String::new(),
        on_destroyed: "Destroyed".into(),
        generic_scripts: Vec::new(),
        initial_idle_frames: 30,
        destroyed_threshold: 0.5,
        aggression: 2,
    }
}

#[test]
fn production_actions_ignore_conditions_then_failed_minimum_disbands_without_killing_members() {
    let mut game = super::super::tests::game();
    let mut rules = rules(vec![script(
        "Production",
        vec![call("CONDITION_FALSE", vec![])],
        vec![call(
            "INCREMENT_COUNTER",
            vec![parameter("", 1), parameter("Ran", 0)],
        )],
    )]);
    rules.factions[0].teams.push(recipe());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    teams::queue(&mut game, &rules, &mut state, 0, false);
    assert_eq!(state.counters["Ran"].value, 2);
    state.teams[0].members.push(3);
    lifecycle::check_building(&mut game, &rules, &mut state);
    assert_eq!(state.counters["Ran"].value, 3);
    game.tick = 31;
    lifecycle::check_building(&mut game, &rules, &mut state);
    assert!(state.teams[0].deleted);
    assert!(state.build_queue.is_empty());
    assert!(state.teams[0].members.is_empty());
    assert!(game.entities[3].alive());
    assert_eq!(state.counters["Ran"].value, 3);
}

#[test]
fn destroyed_threshold_fires_once_and_changes_shared_template_priority() {
    let mut game = super::super::tests::game();
    let mut destroyed = script(
        "Destroyed",
        vec![call("CONDITION_TRUE", vec![])],
        vec![call(
            "TEAM_DECREASE_PRIORITY",
            vec![parameter("<This Team>", 0)],
        )],
    );
    destroyed.flags |= 32;
    let mut rules = rules(vec![destroyed]);
    rules.factions[0].teams.push(recipe());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    let kind = game.type_for(1, super::super::Kind::Builder).unwrap();
    let members: Vec<_> = (0..4)
        .map(|_| game.spawn(kind, 1, [600.0, 400.0], true))
        .collect();
    let mut team = Team::new(Some(0), 0, 1);
    team.members = members;
    let native = game.team_membership.create_team(state.team_prototypes[0]);
    team.native_instance = Some(native);
    for &member in &team.members {
        game.team_membership.set_team(member, Some(native));
    }
    game.team_membership.activate(native);
    state.teams.push(team);
    lifecycle::update_states(&mut game, &rules, &mut state);
    game.entities[state.teams[0].members[0]].health = 0.0;
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.priorities[0], 10);
    game.entities[state.teams[0].members[1]].health = 0.0;
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.priorities[0], 6);
    game.entities[state.teams[0].members[2]].health = 0.0;
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.priorities[0], 6);
}

fn supply_entry(game: &mut Skirmish, state: &mut State, current: i32) -> usize {
    let kind = game.type_for(1, super::super::Kind::SupplyCenter).unwrap();
    let center = game.spawn(kind, 1, [615.0, 235.0], true);
    game.entities[center].collector_pending = false;
    state.buildings.push(Building {
        name: "Supply".into(),
        position: [615.0, 235.0],
        angle: 0.0,
        remaining: 0,
        automatic: false,
        priority: false,
        object: Some(center),
        destroyed_at: None,
        supply: Some(SupplyQuota {
            desired: 1,
            current,
        }),
    });
    center
}

#[test]
fn loose_collector_is_recruited_without_spending_and_lost_collector_is_replaced() {
    let mut game = super::super::tests::game();
    let mut rules = rules(Vec::new());
    rules.factions[0].gatherers = [0; 3];
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    let center = supply_entry(&mut game, &mut state, 0);
    let cash = game.players[1].cash;
    game.entities[3].ai.supply.force_wanting = true;
    collectors::queue(&mut game, &rules, &mut state);
    assert_eq!(game.players[1].cash, cash);
    assert!(state.build_queue.is_empty());
    assert!(
        game.entities[3].ai.supply.preferred_dock
            == Some(super::super::economy::DockTarget::Center(center))
    );
    assert!(matches!(
        game.entities[3].ai.order,
        super::super::Order::ReturnSupply
    ));
    game.entities[3].health = 0.0;
    collectors::queue(&mut game, &rules, &mut state);
    assert_eq!(state.buildings[0].supply.as_ref().unwrap().current, 0);
    collectors::queue(&mut game, &rules, &mut state);
    assert_eq!(state.build_queue.len(), 1);
    assert_eq!(game.players[1].cash, cash - 200);
    collectors::queue(&mut game, &rules, &mut state);
    assert_eq!(game.players[1].cash, cash - 200);
    assert_eq!(game.entities[center].queue.len(), 1);
}

#[test]
fn ordinary_worker_is_not_recruited_and_free_collector_order_costs_nothing() {
    let mut game = super::super::tests::game();
    let rules = rules(Vec::new());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    let center = supply_entry(&mut game, &mut state, -1);
    let cash = game.players[1].cash;
    collectors::queue(&mut game, &rules, &mut state);
    assert!(matches!(
        game.entities[3].ai.order,
        super::super::Order::Idle
    ));
    assert_eq!(game.players[1].cash, cash);
    assert!(game.entities[center].queue.is_empty());
    assert_eq!(state.build_queue.len(), 1);
    assert_eq!(
        state.requests[state.build_queue[0]].orders[0].producer,
        Some(center)
    );
    let owner = state.owner;
    game.opponents[owner] = Some(Opponent {
        rules: Arc::new(rules),
        state,
    });
    let kind = game.type_for(1, super::super::Kind::Builder).unwrap();
    let collector = game.spawn_produced_unit(center, kind, true).unwrap();
    assert!(matches!(
        game.entities[collector].ai.order,
        super::super::Order::Exit(_)
    ));
    assert!(
        game.entities[collector].ai.supply.preferred_dock
            == Some(super::super::economy::DockTarget::Center(center))
    );
    let state = &game.opponents[1].as_ref().unwrap().state;
    assert_eq!(state.buildings[0].supply.as_ref().unwrap().current, 1);
    assert_eq!(state.requests[0].orders[0].remaining, 0);
    let state = &mut game.opponents[1].as_mut().unwrap().state;
    state.buildings[0].supply.as_mut().unwrap().desired = 2;
    state.requests[0].orders[0].remaining = 1;
    state.requests[0].orders[0].producer = Some(center);
    let paid = game.spawn_produced_unit(center, kind, false).unwrap();
    assert!(matches!(
        game.entities[paid].ai.order,
        super::super::Order::ReturnSupply
    ));
    assert!(game.entities[paid].ai.ignored_producer.is_none());
}

#[test]
fn paid_command_center_worker_finishes_exit_as_a_builder() {
    let mut game = super::super::tests::game();
    let worker_type = game.type_for(1, super::super::Kind::Builder).unwrap();
    let worker = game.spawn_produced_unit(2, worker_type, false).unwrap();
    for _ in 0..300 {
        game.step();
    }
    assert!(matches!(
        game.entities[worker].ai.order,
        super::super::Order::Idle
    ));
    assert_eq!(game.entities[worker].cargo, 0);
    assert!(game.entities[worker].ai.supply.preferred_dock.is_none());
}

#[test]
fn random_timers_use_native_rng_bounds_and_seconds_conversion() {
    let mut game = super::super::tests::game();
    let rules = rules(Vec::new());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    let random = game.random;
    let equal = call(
        "SET_RANDOM_TIMER",
        vec![parameter("Equal", 0), parameter("", 17), parameter("", 17)],
    );
    scripts::execute(&mut game, &rules, &mut state, &equal, None);
    assert_eq!(game.random, random);
    assert_eq!(state.counters["Equal"].value, 17);
    let mut expected_random = random;
    let draw = crate::skirmish::startup::random_value(&mut expected_random);
    let ranged = call(
        "SET_RANDOM_TIMER",
        vec![parameter("Frames", 0), parameter("", 20), parameter("", 29)],
    );
    scripts::execute(&mut game, &rules, &mut state, &ranged, None);
    assert_eq!(game.random, expected_random);
    assert_eq!(
        state.counters["Frames"].value,
        20 + i32::try_from(draw % 10).unwrap()
    );
    let mut seconds = parameter("", 0);
    seconds.real = 0.035;
    let mut same = parameter("", 0);
    same.real = 0.035;
    scripts::execute(
        &mut game,
        &rules,
        &mut state,
        &call(
            "SET_RANDOM_MSEC_TIMER",
            vec![parameter("Seconds", 0), seconds, same],
        ),
        None,
    );
    assert_eq!(game.random, expected_random);
    assert_eq!(state.counters["Seconds"].value, 2);
    assert!(state.counters["Seconds"].countdown);
}

#[test]
fn expired_team_counts_pending_factory_before_later_factory_validation() {
    let mut game = super::super::tests::game();
    let mut rules = rules(Vec::new());
    rules.factions[0].teams.push(recipe());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    teams::queue(&mut game, &rules, &mut state, 0, false);
    state.requests[0]
        .orders
        .iter_mut()
        .find(|order| order.required)
        .unwrap()
        .producer = Some(2);
    game.entities[2].health = 0.0;
    game.tick = 31;
    lifecycle::check_building(&mut game, &rules, &mut state);
    assert!(matches!(state.requests[0].phase, RequestPhase::Building));
    teams::update(&mut game, &rules, &mut state);
    assert!(matches!(state.requests[0].phase, RequestPhase::Building));
    assert!(
        state.requests[0]
            .orders
            .iter()
            .all(|order| order.producer.is_none())
    );
    lifecycle::check_building(&mut game, &rules, &mut state);
    assert!(state.teams[0].deleted);
}

#[test]
fn singleton_build_requests_share_members_without_discarding_other_pending_production() {
    let mut game = super::super::tests::game();
    let mut rules = rules(Vec::new());
    let mut singleton = recipe();
    singleton.singleton = true;
    rules.factions[0].teams.push(singleton);
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    teams::queue(&mut game, &rules, &mut state, 0, true);
    teams::queue(&mut game, &rules, &mut state, 0, true);
    assert_eq!(state.teams.len(), 1);
    assert_eq!(state.requests.len(), 2);
    state.teams[0].members.push(3);
    teams::queue(&mut game, &rules, &mut state, 0, true);
    assert_eq!(state.requests.len(), 2);
    state.requests[1].phase = RequestPhase::Ready;
    state.ready_queue.push(1);
    lifecycle::check_ready(&mut game, &rules, &mut state);
    assert!(
        game.team_membership
            .active(state.teams[0].native_instance.unwrap())
    );
    assert!(matches!(state.requests[0].phase, RequestPhase::Building));
    assert!(!state.requests[0].orders.is_empty());
    game.move_to(3, [700.0, 400.0]).unwrap();
    scripts::execute(
        &mut game,
        &rules,
        &mut state,
        &call("TEAM_STOP_AND_DISBAND", vec![parameter("Collectors", 0)]),
        None,
    );
    assert!(matches!(
        game.entities[3].ai.order,
        super::super::Order::Idle
    ));
    assert!(game.entities[3].alive());
    assert!(state.teams[0].members.is_empty());
    teams::queue(&mut game, &rules, &mut state, 0, true);
    assert_eq!(state.teams.len(), 1);
    assert_eq!(state.requests.len(), 3);
}

#[test]
fn dormant_computer_reset_does_not_consume_logic_rng() {
    let mut game = super::super::tests::game();
    let mut periodic = script("Periodic", vec![call("CONDITION_TRUE", vec![])], Vec::new());
    periodic.delay_frames = 30;
    let rules = rules(vec![periodic]);
    let state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    let owner = state.owner;
    let rules = Arc::new(rules);
    game.opponent_rules = Some(Arc::clone(&rules));
    game.slots[owner].difficulty = Some(OpponentDifficulty::Hard);
    game.opponents[owner] = Some(Opponent { rules, state });
    game.occupied[1] = false;
    let random = game.random;
    game.reset_opponent();
    assert_eq!(game.random, random);
    game.occupied[1] = true;
    game.reset_opponent();
    assert_ne!(game.random, random);
}

#[test]
fn three_computers_query_each_other_recursively_before_committing_enemy_choices() {
    use crate::skirmish::{PlayerSlot, PlayerStart};
    let mut game = super::super::tests::game();
    game.layout.starts = [
        [100.0, 100.0],
        [500.0, 100.0],
        [500.0, 500.0],
        [100.0, 500.0],
    ]
    .into_iter()
    .map(|position| PlayerStart {
        position,
        rally: None,
    })
    .collect();
    let slots = (0..4)
        .map(|owner| PlayerSlot {
            faction: Some(Faction::Gla),
            occupied: true,
            difficulty: (owner > 0).then_some(OpponentDifficulty::Easy),
            team: None,
            start: Some(owner),
            color: Some(owner),
        })
        .collect();
    game.reset_slots(slots).unwrap();
    let rules = Arc::new(rules(Vec::new()));
    for owner in 1..4 {
        let mut state = State::new(&mut game, &rules, owner, OpponentDifficulty::Easy, 0);
        state.base_center = game.player_start(owner).unwrap().position;
        game.opponents[owner] = Some(Opponent {
            rules: Arc::clone(&rules),
            state,
        });
    }
    // Querying AI1 recursively lets AI3 choose human0, then AI2 choose AI1.
    // Native anti-ganging and retaliation costs therefore send AI1 to AI2.
    assert_eq!(game.ai_enemy(1), Some(2));
    assert_eq!(game.ai_enemy(2), Some(1));
    assert_eq!(game.ai_enemy(3), Some(0));
    for owner in 1..4 {
        assert_eq!(
            game.opponents[owner]
                .as_ref()
                .unwrap()
                .state
                .next_enemy_check,
            150
        );
    }
    for actor in &mut game.entities {
        if actor.owner == 2 {
            actor.health = 0.0;
        }
    }
    game.tick = 149;
    assert_eq!(game.ai_enemy(1), Some(2));
    game.tick = 150;
    assert_ne!(game.ai_enemy(1), Some(2));
}

#[test]
fn configured_ai_can_resume_after_a_sandbox_reset_with_new_difficulty() {
    let mut game = super::super::tests::game();
    game.opponent_rules = Some(Arc::new(rules(Vec::new())));
    game.reset_sandbox(Faction::Gla).unwrap();
    assert!(game.opponents[1].is_none());
    game.reset_with_difficulty(Faction::Gla, Faction::Gla, OpponentDifficulty::Hard)
        .unwrap();
    assert!(matches!(
        game.opponents[1].as_ref().unwrap().difficulty(),
        OpponentDifficulty::Hard
    ));
    assert!(matches!(
        game.slots[1].difficulty,
        Some(OpponentDifficulty::Hard)
    ));
}

#[test]
fn chinook_evacuation_activates_an_inactive_team_and_schedules_creation_once() {
    let (mut game, host) = crate::skirmish::chinook::tests::aircraft();
    let mut created = script(
        "Created",
        vec![call("CONDITION_TRUE", vec![])],
        vec![call(
            "INCREMENT_COUNTER",
            vec![parameter("", 1), parameter("CreatedCount", 0)],
        )],
    );
    created.flags |= 32;
    let mut rules = rules(vec![created]);
    let mut template = recipe();
    template.on_create = "Created".into();
    template.on_destroyed.clear();
    rules.factions[0].teams.push(template);
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    let native = game.team_membership.create_team(state.team_prototypes[0]);
    let mut team = Team::new(Some(0), 0, 1);
    team.native_instance = Some(native);
    team.members.push(host);
    game.team_membership.set_team(host, Some(native));
    state.teams.push(team);
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert!(!state.counters.contains_key("CreatedCount"));
    game.set_chinook_state(
        host,
        crate::skirmish::chinook::Phase::EvacTakeoff,
        None,
        None,
        crate::skirmish::ai_goals::COMMAND_FROM_AI,
    );
    assert!(game.team_membership.active(native));
    assert!(!state.counters.contains_key("CreatedCount"));
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.counters["CreatedCount"].value, 1);
    game.set_chinook_state(
        host,
        crate::skirmish::chinook::Phase::EvacTakeoff,
        None,
        None,
        crate::skirmish::ai_goals::COMMAND_FROM_AI,
    );
    lifecycle::update_states(&mut game, &rules, &mut state);
    assert_eq!(state.counters["CreatedCount"].value, 1);
}

#[test]
fn pilot_returns_once_to_actual_ai_build_list_center_without_reissuing_after_idle() {
    use crate::skirmish::{pilot_find_vehicle, veterancy_crate};
    let mut game = crate::skirmish::tests::game();
    game.slots[1].difficulty = Some(OpponentDifficulty::Hard);
    let unit = &mut game.catalog[5];
    unit.ai.acquisition = 0;
    unit.behavior.veterancy_crates.push(veterancy_crate::Rules {
        restrictions: veterancy_crate::Restrictions {
            required: vec!["VEHICLE".into()],
            forbidden: Vec::new(),
            forbid_owner: false,
            building_pickup: false,
            human_only: false,
            science: None,
        },
        adds_owner_veterancy: false,
        is_pilot: true,
        allow_multi_pickup: false,
        execute_fx: String::new(),
    });
    crate::skirmish::tests::register_behavior(unit, "VeterancyCrateCollide", 0x10);
    unit.behavior
        .pilot_find_vehicle
        .push(pilot_find_vehicle::Rules {
            scan_frames: 30,
            scan_range: 1.0,
            min_health: 0.5,
        });
    crate::skirmish::tests::register_update(unit, "PilotFindVehicleUpdate");
    let pilot = game.spawn(5, 1, [300.0, 200.0], true);
    assert!(game.pilot_ai_base_center(1).is_none());
    let mut rules = rules(Vec::new());
    rules.factions[0].buildings.push(AiBuilding {
        bounding_radius: 20.0,
        name: game.catalog[0].name.clone(),
        position: [0.0; 2],
        angle: 0.0,
        rebuilds: 0,
        automatic: false,
    });
    let state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    let expected = [state.base_center[0], state.base_center[1], 0.0];
    game.opponents[1] = Some(Opponent {
        rules: Arc::new(rules),
        state,
    });
    assert_eq!(
        game.pilot_ai_base_center(1).unwrap().map(f32::to_bits),
        expected.map(f32::to_bits)
    );
    assert_eq!(game.update_pilot_find_vehicle(pilot, 0), 30);
    assert!(matches!(
        game.entities[pilot].ai.order,
        crate::skirmish::Order::Move(_)
    ));
    assert_eq!(
        game.entities[pilot].ai_goal_position().map(f32::to_bits),
        expected.map(f32::to_bits)
    );
    game.native_idle_from(pilot, crate::skirmish::ai_goals::COMMAND_FROM_AI);
    let revision = game.entities[pilot].ai.command_revision;
    assert_eq!(game.update_pilot_find_vehicle(pilot, 0), 30);
    assert!(game.entities[pilot].ai_idle());
    assert_eq!(game.entities[pilot].ai.command_revision, revision);
}
