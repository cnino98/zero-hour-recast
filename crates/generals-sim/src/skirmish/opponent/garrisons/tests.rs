use super::*;
use crate::skirmish::{
    Kind, OpponentDifficulty, Order,
    containment::{Art, GarrisonRules, PassengerWeapons, Rules},
    opponent::{AiArea, AiParameter, Team, teams},
    tests::fixture_type,
    visibility::{Status, VisibilityRules},
};

pub(in crate::skirmish::opponent) fn fixture() -> (Skirmish, OpponentRules, State, usize) {
    host_fixture("GarrisonContain")
}

fn host_fixture(family: &str) -> (Skirmish, OpponentRules, State, usize) {
    let mut game = crate::skirmish::tests::game();
    let mut unit = fixture_type(game.players[1].faction, Kind::Barracks);
    unit.name = "ChinaBunker".into();
    unit.kind_of = vec!["STRUCTURE".into()];
    unit.geometry.major = 12.0;
    unit.geometry.minor = 12.0;
    unit.contain = Some(Rules {
        cave_index: None,
        payload: None,
        payload_templates: Vec::new(),
        experience_sink_for_rider: true,
        passengers_in_turret: false,
        orient_like_container_on_exit: false,
        assault_heal_ratio: None,
        heal: None,
        tunnel: None,
        garrison: (family == "GarrisonContain").then_some(GarrisonRules {
            allow_enemies: false,
            allow_neutral: true,
            immune_to_clear: false,
            heal_frames: None,
            stations: None,
        }),
        capacity: 2,
        allowed: vec!["INFANTRY".into()],
        forbidden: vec![],
        weapons: PassengerWeapons {
            pass_bonuses: false,
            fire: false,
            upgrade_transport: false,
        },
        relationships: crate::skirmish::containment::Relationships {
            allies: true,
            enemies: false,
            neutral: true,
        },
        exit_flags: 0,
        exit_frames: 0,
        door_frames: 0,
        exit_paths: 0,
        regen_percent: 0.0,
        death: crate::skirmish::containment::RiderDeath {
            damage: 0.0,
            burned: false,
            destroy_if_not_free: false,
        },
        damage_thresholds: [0.5, 0.1],
        art: std::array::from_fn(|_| Art {
            exits: vec![],
            fire_points: crate::skirmish::containment::tests::fire_points(&[[0.0, 0.0, 10.0]]),
        }),
    });
    crate::skirmish::tests::register_update(&mut unit, family);
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let rules = super::super::tests::rules(vec![]);
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    state.teams.push(Team::new(None, 0, 0));
    (game, rules, state, kind)
}

#[test]
fn hacker_uses_kind_only_center_filter_then_enter_rejects_foreign_ownership() {
    let (mut game, rules, mut state, kind) = host_fixture("InternetHackContain");
    game.catalog[kind].kind_of.push("FS_INTERNET_CENTER".into());
    let foreign = game.spawn(kind, 0, [400.0, 300.0], true);
    let own = game.spawn(kind, 1, [650.0, 300.0], true);
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    game.catalog[infantry].kind_of.push("MONEY_HACKER".into());
    let hacker = game.spawn(infantry, 1, [300.0, 300.0], true);
    state.teams[0].members = vec![hacker];
    game.update_ground_contacts();
    let command = call("TEAM_GARRISON_NEAREST_BUILDING", &["<This Team>"]);
    assert!(teams::action(&mut game, &rules, &mut state, &command, Some(0)).is_some());
    // privateEnter tests admission before clearing the previous order. The
    // kind-only assignment to the nearer foreign center therefore consumes
    // the hacker without sending it to the farther friendly center.
    assert!(!game.can_enter(hacker, foreign));
    assert!(game.can_enter(hacker, own));
    assert!(matches!(game.entities[hacker].ai.order, Order::Idle));
    assert!(game.entities[own].containment.waiting.is_empty());
    game.step();
    assert!(game.entities[hacker].contained_by().is_none());
    assert!(!matches!(game.entities[hacker].ai.order, Order::Enter(host) if host == own));
    assert!(game.passengers(own).is_empty());
}

fn call(name: &str, arguments: &[&str]) -> AiCall {
    AiCall {
        name: name.into(),
        parameters: arguments
            .iter()
            .map(|text| AiParameter {
                text: (*text).into(),
                integer: 0,
                real: 0.0,
                position: None,
            })
            .collect(),
    }
}

#[test]
fn assignment_spreads_infantry_by_occupant_count_and_skips_armed_vehicles() {
    let (mut game, rules, mut state, kind) = fixture();
    let first = game.spawn(kind, 1, [400.0, 300.0], true);
    let second = game.spawn(kind, game.neutral_owner(), [650.0, 300.0], true);
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    game.catalog[infantry].transport_slots = 9;
    let vehicle = game.type_for(1, Kind::Tank).unwrap();
    let tank = game.spawn(vehicle, 1, [300.0, 200.0], true);
    let riders = (0..3)
        .map(|_| game.spawn(infantry, 1, [300.0, 300.0], true))
        .collect::<Vec<_>>();
    state.teams[0].members = vec![riders[0], tank, riders[1], riders[2]];
    game.update_ground_contacts();
    let call = call("TEAM_GARRISON_NEAREST_BUILDING", &["<This Team>"]);
    assert!(teams::action(&mut game, &rules, &mut state, &call, Some(0)).is_some());
    assert!(matches!(game.entities[riders[0]].ai.order, Order::Enter(host) if host == first));
    assert!(matches!(game.entities[riders[1]].ai.order, Order::Enter(host) if host == first));
    assert!(matches!(game.entities[riders[2]].ai.order, Order::Enter(host) if host == second));
    assert!(matches!(game.entities[tank].ai.order, Order::Idle));
}

#[test]
fn approach_uses_first_member_and_polygon_not_team_centroid_or_owner() {
    let (mut game, mut rules, mut state, kind) = fixture();
    let close = game.spawn(kind, 0, [400.0, 300.0], true);
    let far = game.spawn(kind, 1, [650.0, 300.0], true);
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    let leader = game.spawn(infantry, 1, [300.0, 300.0], true);
    let tail = game.spawn(infantry, 1, [700.0, 300.0], true);
    state.teams[0].members = vec![leader, tail];
    rules.map.areas.push(AiArea {
        name: "Perimeter".into(),
        points: vec![
            [250.0, 200.0],
            [750.0, 200.0],
            [750.0, 400.0],
            [250.0, 400.0],
        ],
    });
    game.update_ground_contacts();
    assert_eq!(
        nearest_type(
            &game,
            game.entities[leader].position,
            "ChinaBunker",
            &rules.map.areas[0]
        ),
        Some(close)
    );
    let order = call(
        "TEAM_MOVE_TOWARDS_NEAREST_OBJECT_TYPE",
        &["<This Team>", "ChinaBunker", "Perimeter"],
    );
    assert!(teams::action(&mut game, &rules, &mut state, &order, Some(0)).is_some());
    for id in [leader, tail] {
        let Order::Move(goal) = game.entities[id].ai.order else {
            panic!("member did not receive source move")
        };
        assert!(
            distance_squared(goal, game.entities[close].position)
                < distance_squared(goal, game.entities[far].position)
        );
    }
    // Native canPlayerGarrison rejects an enemy, even though the approach action
    // deliberately has no relationship filter.
    assert!(!garrisonable(&game, 1, close));
}

#[test]
fn native_nearest_type_filter_accepts_a_reskinned_building() {
    let (mut game, _, _, kind) = fixture();
    let mut unit = fixture_type(game.players[1].faction, Kind::Barracks);
    unit.name = "BunkerReskin".into();
    unit.reskin = Some("ChinaBunker".into());
    let variant = game.catalog.len();
    game.catalog.push(unit);
    let closest = game.spawn(variant, 0, [400.0, 300.0], true);
    game.spawn(kind, 1, [650.0, 300.0], true);
    let area = AiArea {
        name: "Perimeter".into(),
        points: vec![
            [250.0, 200.0],
            [750.0, 200.0],
            [750.0, 400.0],
            [250.0, 400.0],
        ],
    };
    game.update_ground_contacts();
    assert_eq!(
        nearest_type(&game, [300.0, 300.0], "ChinaBunker", &area),
        Some(closest)
    );
}

#[test]
fn contained_garrison_occupant_keeps_looking_but_transport_occupant_does_not() {
    let (mut game, _, _, kind) = fixture();
    for unit in &mut game.catalog {
        unit.sight.clearing_range = 0.0;
    }
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    game.catalog[infantry].sight.clearing_range = 120.0;
    let host = game.spawn(kind, 1, [400.0, 300.0], true);
    let rider = game.spawn(infantry, 1, [400.0, 300.0], true);
    game.add_to_contain(rider, host);
    game.configure_visibility(VisibilityRules {
        cell_size: 40.0,
        unlook_persist_frames: 0,
        use_shroud: true,
    })
    .unwrap();
    game.update_visibility();
    assert!(game.entities[rider].masked());
    assert!(matches!(
        game.visibility_cell(1, [440.0, 300.0]),
        Status::Clear
    ));
    let mut transport = fixture_type(game.players[1].faction, Kind::Vehicle);
    transport.contain = Some(crate::skirmish::containment::tests::rules(0.0, 0.0));
    transport.sight.clearing_range = 0.0;
    crate::skirmish::tests::register_update(&mut transport, "TransportContain");
    let transport_type = game.catalog.len();
    game.catalog.push(transport);
    let carrier = game.spawn(transport_type, 1, [400.0, 300.0], true);
    game.remove_passenger(rider, host);
    game.add_to_contain(rider, carrier);
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    game.tick += 1;
    game.update_visibility();
    assert!(matches!(
        game.visibility_cell(1, [440.0, 300.0]),
        Status::Fogged
    ));
    game.remove_passenger(rider, carrier);
    assert!(matches!(
        game.visibility_cell(1, [440.0, 300.0]),
        Status::Clear
    ));
}

#[test]
fn rejected_damaged_host_consumes_assignment_and_preserves_previous_guard() {
    let (mut game, _, mut state, kind) = fixture();
    let host = game.spawn(kind, 1, [400.0, 300.0], true);
    game.change_body_health(host, -game.entities[host].max_health * 0.95);
    game.spawn(kind, 1, [600.0, 300.0], true);
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    let rider = game.spawn(infantry, 1, [300.0, 300.0], true);
    game.set_order(rider, Order::Guard([300.0, 300.0]));
    state.teams[0].members.push(rider);
    game.update_ground_contacts();
    assert!(!game.can_enter(rider, host));
    assert!(assign(&mut game, &mut state, 0).is_some());
    assert!(matches!(game.entities[rider].ai.order, Order::Guard(_)));
    assert!(game.entities[host].containment.waiting.is_empty());
}

#[test]
fn stationary_goal_loss_keeps_the_last_native_move_destination() {
    let (mut game, mut rules, mut state, kind) = fixture();
    let host = game.spawn(kind, 1, [400.0, 300.0], true);
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    let rider = game.spawn(infantry, 1, [300.0, 300.0], true);
    state.teams[0].members.push(rider);
    rules.map.areas.push(AiArea {
        name: "Perimeter".into(),
        points: vec![
            [250.0, 200.0],
            [750.0, 200.0],
            [750.0, 400.0],
            [250.0, 400.0],
        ],
    });
    game.update_ground_contacts();
    approach(&mut game, &rules, &mut state, 0, "ChinaBunker", "Perimeter").unwrap();
    let Order::Move(goal) = game.entities[rider].ai.order else {
        panic!("no source approach")
    };
    game.entities[host].owner = 0;
    game.entities[host].health = 0.0;
    game.rebuild_navigation();
    game.update_order(rider);
    assert!(
        matches!(game.entities[rider].ai.order, Order::Move(point) if point.map(f32::to_bits) == goal.map(f32::to_bits))
    );
}
