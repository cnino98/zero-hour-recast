use super::*;

#[test]
fn authored_hacker_command_starts_income_without_replacing_team_sequence() {
    let mut game = crate::skirmish::tests::game();
    let index = game.type_for(1, Kind::Infantry).unwrap();
    game.catalog[index].behavior.hacking = Some(crate::skirmish::income::HackerRules {
        unpack: 0,
        pack: 0,
        variation: 0.0,
        delay: 0,
        contained_delay: 1,
        amounts: [5; 4],
        experience: 1,
    });
    crate::skirmish::income::tests::bind_hacker(&mut game.catalog[index]);
    let id = game.spawn(index, 1, [500.0, 300.0], true);
    let mut rules = rules(Vec::new());
    rules.factions[0].teams.push(recipe());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    state.teams.push(Team::new(Some(0), 0, 0));
    state.teams[0].members = vec![id];
    state.teams[0].sequence = Some((2, 3));
    let hack = call(
        "TEAM_USE_COMMANDBUTTON_ABILITY",
        vec![
            parameter("<This Team>", 0),
            parameter("Command_ChinaInfantryHackerInternetHack", 0),
        ],
    );
    assert!(teams::action(&mut game, &rules, &mut state, &hack, Some(0)).is_some());
    assert!(matches!(game.entities[id].ai.order, Order::HackInternet));
    assert_eq!(state.teams[0].sequence, Some((2, 3)));
    game.step();
    game.step();
    assert_eq!(game.players[1].statistics.money_earned, 5);
    let other = call(
        "TEAM_USE_COMMANDBUTTON_ABILITY",
        vec![parameter("<This Team>", 0), parameter("UnportedCommand", 0)],
    );
    assert!(teams::action(&mut game, &rules, &mut state, &other, Some(0)).is_none());
}
use crate::skirmish::{Kind, Order, ProductionItem, Research};

fn scripted_approach(as_team: bool, aggressive: bool) -> (Skirmish, usize, usize) {
    use crate::skirmish::{AiWaypoint, GroundAppearance, ground_movement::tests::ground_rules};
    use std::sync::Arc;
    let mut game = crate::skirmish::tests::game();
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    game.catalog[infantry].locomotor.ground = Some(ground_rules(GroundAppearance::Legs));
    game.catalog[infantry].ai.acquisition = 0;
    let first = game.spawn(infantry, 1, [350.0, 350.0], true);
    let trailing = game.spawn(infantry, 1, [235.0, 350.0], true);
    if aggressive {
        // Keep the starting base outside acquisition range of the whole route.
        for id in 0..game.entities.len() {
            if game.entities[id].owner == 0 {
                let spacing = 30.0 * f32::from(u16::try_from(id).unwrap());
                game.set_object_position(id, [65.0, 45.0 + spacing, 0.0]);
            }
        }
        game.slots[1].difficulty = Some(OpponentDifficulty::Hard);
        let enemy_type = game.type_for(0, Kind::Infantry).unwrap();
        let enemy = game.spawn(enemy_type, 0, [385.0, 320.0], true);
        game.entities[enemy].health = 1.0;
        game.catalog[infantry].ai.vision_range = 180.0;
        game.entities[first].ai.mood.attitude = 2;
        game.entities[trailing].ai.mood.attitude = 2;
    }
    game.update_ground_contacts();
    let mut rules = rules(Vec::new());
    rules.factions[0].teams.push(recipe());
    rules.map.waypoints = vec![
        AiWaypoint {
            id: 41,
            position: [435.0, 350.0],
            labels: vec!["Attack1".into()],
            links: vec![42],
            bidirectional: false,
        },
        AiWaypoint {
            id: 42,
            position: [620.0, 350.0],
            labels: Vec::new(),
            links: Vec::new(),
            bidirectional: false,
        },
    ];
    let rules = Arc::new(rules);
    game.opponent_rules = Some(Arc::clone(&rules));
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    state.enemy_start = 0;
    let prototype = game.team_membership.add_prototype(1, "Approach".into());
    let native = game.team_membership.create_team(prototype);
    game.team_membership.set_team(trailing, Some(native));
    game.team_membership.set_team(first, Some(native));
    state.teams.push(Team::new(Some(0), 0, 0));
    state.teams[0].native_instance = Some(native);
    let command = call(
        "SKIRMISH_FOLLOW_APPROACH_PATH",
        vec![
            parameter("<This Team>", 0),
            parameter("Attack", 0),
            parameter("", i32::from(as_team)),
        ],
    );
    assert!(teams::action(&mut game, &rules, &mut state, &command, Some(0)).is_some());
    (game, first, trailing)
}

#[test]
fn scripted_approach_waypoints_follow_independently_or_as_a_team() {
    for (as_team, aggressive) in [(false, false), (true, false), (true, true)] {
        let (mut game, first, trailing) = scripted_approach(as_team, aggressive);
        assert!(matches!(game.entities[first].ai.order, Order::FollowPath));
        assert_eq!(
            game.entities[first].ai.command.last_command_source,
            crate::skirmish::ai_goals::COMMAND_FROM_SCRIPT
        );
        let goal = |id: usize, game: &Skirmish| game.entities[id].ai.command.move_to.position[0];
        if as_team {
            assert!(goal(first, &game) > 435.0 && goal(trailing, &game) < 435.0);
        } else {
            assert!((goal(first, &game) - 435.0).abs() < 0.01);
            assert!((goal(trailing, &game) - 435.0).abs() < 0.01);
        }
        let mut completed = [false; 2];
        let mut step = |game: &mut Skirmish| {
            game.step();
            for (seen, id) in completed.iter_mut().zip([first, trailing]) {
                *seen |= game.entities[id].ai.completed_waypoint == Some(42);
            }
            completed == [true; 2]
        };
        for _ in 0..300 {
            if goal(first, &game) > 550.0 {
                break;
            }
            step(&mut game);
        }
        assert!(
            goal(first, &game) > 550.0,
            "as_team={as_team} aggressive={aggressive} first={}, trailer={}, kills={}, first_pos={:?}, trailer_pos={:?}, follow={}",
            goal(first, &game),
            goal(trailing, &game),
            game.players[1].statistics.units_destroyed,
            game.entities[first].position,
            game.entities[trailing].position,
            game.entities[first].ai.follow_waypoint.is_some()
        );
        if as_team {
            step(&mut game);
            assert!(goal(trailing, &game) > 550.0);
        } else {
            assert!(goal(trailing, &game) < 500.0);
        }
        for _ in 0..500 {
            if step(&mut game) {
                break;
            }
        }
        assert_eq!(
            completed,
            [true; 2],
            "as_team={as_team} aggressive={aggressive}, members={:?}",
            [first, trailing].map(|id| {
                let actor = &game.entities[id];
                (
                    actor.position,
                    actor.health,
                    actor.ai.command.move_to.position,
                    actor.ai.follow_waypoint.is_some(),
                    actor.ai.current_victim,
                    actor.ai.path_request.pending.is_some(),
                    actor.ai.ground_path.is_some(),
                )
            })
        );
        if aggressive {
            assert!(game.entities[first].last_command_from_ai());
            assert!(game.players[1].statistics.units_destroyed > 0);
        }
    }
}

#[test]
fn original_team_button_hunt_captures_neutral_tech_after_research() {
    use crate::skirmish::{
        command_button_hunt::{Button, Command, Definitions, Rules},
        special_ability::{Power, PowerType, capture_fixture},
        tests::{game, register_update},
    };
    let mut game = game();
    let rules = std::sync::Arc::new(rules(Vec::new()));
    game.opponent_rules = Some(std::sync::Arc::clone(&rules));
    game.slots[1].difficulty = Some(OpponentDifficulty::Hard);
    game.slots[0].team = Some(1);
    game.slots[1].team = Some(1);
    let kind = PowerType::SpecialInfantryCaptureBuilding;
    let button = "Command_AmericaRangerCaptureBuilding";
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    let unit = &mut game.catalog[infantry];
    unit.ai.acquisition = 0;
    unit.command_set = "AmericaInfantryRangerCommandSet".into();
    capture_fixture::bind(unit, kind, [1, 1, 1, 5], true);
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers =
        vec!["Upgrade_InfantryCaptureBuilding".into()];
    unit.behavior.command_button_hunts.push(Rules {
        scan_frames: 7,
        scan_range: 250.0,
    });
    register_update(unit, "CommandButtonHuntUpdate");
    let mut slots = [None; 18];
    slots[17] = Some(0);
    game.configure_command_buttons(Definitions {
        buttons: vec![Button {
            name: button.into(),
            command: Command::SpecialPower,
            weapon_slot: 0,
            power: Some(Power {
                name: "InfantryCapture".into(),
                kind,
            }),
            needs_object: true,
            upgrade: None,
        }],
        sets: std::collections::BTreeMap::from([("AmericaInfantryRangerCommandSet".into(), slots)]),
    });
    let actor = game.spawn(infantry, 1, [400.0, 300.0], true);
    let provider = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[provider]
        .kind_of
        .extend(["STRUCTURE".into(), "CAPTURABLE".into()]);
    let neutral = game.neutral_owner();
    let target = game.spawn(provider, neutral, [485.0, 300.0], true);
    let ally = game.spawn(provider, 0, [400.0, 260.0], true);
    let own = game.spawn(provider, 1, [380.0, 300.0], true);
    game.update_ground_contacts();
    let state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    let command = call(
        "TEAM_HUNT_WITH_COMMAND_BUTTON",
        vec![
            parameter(&rules.factions[0].default_team, 0),
            parameter(button, 0),
        ],
    );
    assert!(
        super::super::commands::button_action(&mut game, &rules, &state, &command, None).is_some()
    );
    for _ in 0..20 {
        game.step();
    }
    assert_eq!(game.entities[target].owner, neutral);
    assert!(!game.native_ability_active(actor, 0));

    game.complete_player_upgrade(1, "Upgrade_InfantryCaptureBuilding");
    assert!(
        super::super::commands::button_action(&mut game, &rules, &state, &command, None).is_some()
    );
    for _ in 0..300 {
        if game.entities[target].owner == 1 {
            break;
        }
        game.step();
    }
    assert_eq!(game.entities[target].owner, 1);
    assert_eq!(game.entities[ally].owner, 0);
    assert_eq!(game.entities[own].owner, 1);
}

#[test]
fn original_nearest_tanks_script_disguises_a_bomb_truck() {
    use crate::skirmish::{
        command_button_hunt::{Button, Command, Definitions, add_test_power},
        special_ability::{Power, PowerType},
        stealth::{DisguiseRules, test_rules},
        tests::{fixture_type, game, register_update},
    };
    let mut game = game();
    let truck_type = game.catalog.len();
    let mut truck = fixture_type(crate::skirmish::Faction::Gla, Kind::Tank);
    truck.name = "GLAVehicleBombTruck".into();
    truck.kind_of.push("DISGUISER".into());
    let button = "Command_DisguiseAsVehicle";
    let power = Power {
        name: "SpecialAbilityDisguiseAsVehicle".into(),
        kind: PowerType::SpecialDisguiseAsVehicle,
    };
    add_test_power(&mut truck, &power);
    let ability = &mut truck.behavior.special_abilities[0];
    ability.approach.start_range = 1_000_000.0;
    ability.timing.preparation = 0;
    let mut stealth = test_rules();
    stealth.disguise = Some(DisguiseRules {
        transition: 60,
        reveal_transition: 30,
        effect: String::new(),
        reveal_effect: String::new(),
        sounds: std::array::from_fn(|_| String::new()),
    });
    truck.behavior.stealth = Some(stealth);
    register_update(&mut truck, "StealthUpdate");
    game.catalog.push(truck);
    game.configure_command_buttons(Definitions {
        buttons: vec![Button {
            name: button.into(),
            command: Command::SpecialPower,
            weapon_slot: 0,
            power: Some(power),
            needs_object: true,
            upgrade: None,
        }],
        sets: std::collections::BTreeMap::new(),
    });
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].name = "ChinaTankBattleMaster".into();
    game.catalog[tank].kind_of.push("VEHICLE".into());
    game.catalog[tank].ai.acquisition = 0;
    game.catalog[truck_type].ai.acquisition = 0;
    game.spawn(tank, 0, [500.0, 300.0], true);
    let actor = game.spawn(truck_type, 1, [400.0, 300.0], true);
    game.update_ground_contacts();
    let mut rules = rules(Vec::new());
    rules.factions[0].teams.push(recipe());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    let prototype = game
        .team_membership
        .add_prototype(1, "Bomb Truck Rush".into());
    let native = game.team_membership.create_team(prototype);
    game.team_membership.set_team(actor, Some(native));
    state.teams.push(Team::new(Some(0), 0, 0));
    state.teams[0].native_instance = Some(native);
    state
        .object_lists
        .insert("Tanks".into(), vec!["ChinaTankBattleMaster".into()]);
    let command = call(
        "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_OBJECTTYPE",
        vec![
            parameter("<This Team>", 0),
            parameter(button, 0),
            parameter("Tanks", 0),
        ],
    );
    assert!(
        super::super::commands::button_action(&mut game, &rules, &state, &command, Some(0))
            .is_some()
    );
    for _ in 0..100 {
        if game.entities[actor].disguised_appearance().is_some() {
            break;
        }
        game.step();
    }
    let appearance = game.entities[actor].disguised_appearance().unwrap();
    assert_eq!((appearance.type_index, appearance.owner), (tank, 0));
    assert_eq!(game.entities[actor].owner, 1);
}

#[test]
fn scripted_nearest_type_capture_dispatches_named_and_default_teams_in_list_order() {
    use crate::skirmish::{
        command_button_hunt::{Button, Command, Definitions},
        special_ability::{Power, PowerType, capture_fixture},
    };
    for named in [false, true] {
        let mut game = crate::skirmish::tests::game();
        let infantry = game.type_for(1, Kind::Infantry).unwrap();
        game.catalog[infantry].ai.acquisition = 0;
        let kind = PowerType::SpecialInfantryCaptureBuilding;
        capture_fixture::bind(&mut game.catalog[infantry], kind, [1, 1, 1, 5], false);
        let power = Power {
            name: "InfantryCapture".into(),
            kind,
        };
        game.configure_command_buttons(Definitions {
            buttons: vec![Button {
                name: "Command_CaptureBuilding".into(),
                command: Command::SpecialPower,
                weapon_slot: 0,
                power: Some(power),
                needs_object: true,
                upgrade: None,
            }],
            sets: std::collections::BTreeMap::new(),
        });
        let left_type = game.type_for(0, Kind::Barracks).unwrap();
        let right_type = game.type_for(0, Kind::Factory).unwrap();
        for (index, name) in [(left_type, "Left Provider"), (right_type, "Right Provider")] {
            game.catalog[index].name = name.into();
            game.catalog[index].kind_of.push("CAPTURABLE".into());
            game.catalog[index].ai.update = crate::skirmish::UnitAiUpdate::Absent;
            game.catalog[index].update_modules.retain(|module| {
                !matches!(module, crate::skirmish::module_updates::ModuleRegistration::Behavior {family, ..} if family == "AIUpdateInterface")
            });
        }
        let actor = game.spawn(infantry, 1, [400.0, 300.0], true);
        let held = game.spawn(infantry, 1, [200.0, 300.0], true);
        game.set_disabled_held(held);
        let neutral = game.neutral_owner();
        let left = game.spawn(left_type, neutral, [300.0, 300.0], true);
        let right = game.spawn(right_type, neutral, [500.0, 300.0], true);
        let friendly = game.spawn(right_type, 1, [410.0, 260.0], true);
        let mut rules = rules(Vec::new());
        rules.factions[0].teams.push(recipe());
        let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
        let name = if named {
            let prototype = game
                .team_membership
                .add_prototype(1, "Capture Patrol".into());
            let native = game.team_membership.create_team(prototype);
            game.team_membership.set_team(held, Some(native));
            game.team_membership.set_team(actor, Some(native));
            state.teams.push(Team::new(Some(0), 0, 0));
            state.teams[0].native_instance = Some(native);
            "<This Team>"
        } else {
            &rules.factions[0].default_team
        };
        game.update_ground_contacts();
        game.tick = 10;
        state.object_lists.insert(
            "Providers".into(),
            vec!["Right Provider".into(), "Left Provider".into()],
        );
        let command = call(
            "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_OBJECTTYPE",
            vec![
                parameter(name, 0),
                parameter("Command_CaptureBuilding", 0),
                parameter("Providers", 0),
            ],
        );
        assert!(
            super::super::commands::button_action(
                &mut game,
                &rules,
                &state,
                &command,
                named.then_some(0)
            )
            .is_some()
        );
        assert!(game.native_ability_active(actor, 0));
        assert!(!game.native_ability_active(held, 0));
        for _ in 0..300 {
            if game.entities[right].owner == 1 {
                break;
            }
            game.step();
        }
        assert_eq!(game.entities[right].owner, 1);
        assert_eq!(game.entities[left].owner, neutral);
        assert_eq!(game.entities[friendly].owner, 1);
    }
}

#[test]
fn named_team_guard_moves_again_only_when_idle_guardee_axis_exceeds_two_cells() {
    use crate::skirmish::ai::GuardPhase;
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(1, Kind::Tank).unwrap();
    let first = game.spawn(tank, 1, [305.0, 335.0], true);
    let second = game.spawn(tank, 1, [305.0, 375.0], true);
    let target = game.spawn(tank, 1, [500.0, 350.0], true);
    game.map_objects.insert("Mobile Beacon".into(), target);
    let mut rules = rules(Vec::new());
    rules.factions[0].teams.push(recipe());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    state.teams.push(Team::new(Some(0), 0, 0));
    state.teams[0].members = vec![first, second];
    let prototype = game.team_membership.add_prototype(1, "Escort".into());
    let native = game.team_membership.create_team(prototype);
    game.team_membership.set_team(first, Some(native));
    game.team_membership.set_team(second, Some(native));
    state.teams[0].native_instance = Some(native);
    let guard = call(
        "TEAM_GUARD_OBJECT",
        vec![parameter("<This Team>", 0), parameter("Mobile Beacon", 0)],
    );
    assert!(teams::action(&mut game, &rules, &mut state, &guard, Some(0)).is_some());
    for id in [first, second] {
        assert!(matches!(game.entities[id].ai.order, Order::Guard(_)));
        assert_eq!(game.entities[id].ai.guard.object, Some(target));
        assert_eq!(
            game.entities[id].ai.command.last_command_source,
            crate::skirmish::ai_goals::COMMAND_FROM_SCRIPT
        );
        game.enter_guard_phase(id, GuardPhase::Idle);
        game.entities[id].ai.guard.next_scan = game.tick;
        game.update_guard(id, [500.0, 350.0]);
        assert!(matches!(
            game.entities[id].ai.guard.phase,
            GuardPhase::Return
        ));
        game.enter_guard_phase(id, GuardPhase::Idle);
    }
    game.set_object_position(target, [515.0, 365.0, 0.0]);
    game.entities[first].ai.guard.next_scan = game.tick;
    game.update_guard(first, [500.0, 350.0]);
    assert!(matches!(
        game.entities[first].ai.guard.phase,
        GuardPhase::Idle
    ));
    game.set_object_position(target, [525.0, 365.0, 0.0]);
    game.entities[first].ai.guard.next_scan = game.tick;
    game.update_guard(first, [500.0, 350.0]);
    assert!(matches!(
        game.entities[first].ai.guard.phase,
        GuardPhase::Return
    ));
    assert_eq!(
        game.entities[first].ai.guard.center.map(f32::to_bits),
        [525.0_f32, 365.0, 0.0].map(f32::to_bits)
    );
    game.destroy_maintenance_object(target);
    game.process_update_destruction();
    game.enter_guard_phase(first, GuardPhase::Return);
    assert_eq!(
        game.entities[first].ai.guard.center.map(f32::to_bits),
        [0; 3]
    );
    game.guard(first, [350.0, 350.0]).unwrap();
    assert!(game.entities[first].ai.guard.object.is_none());
}

#[test]
fn object_guard_outer_attack_refreshes_the_leash_and_uses_height_for_deadline_reset() {
    use crate::skirmish::{ai::GuardPhase, ai_goals::COMMAND_FROM_SCRIPT};
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(1, Kind::Tank).unwrap();
    let guard = game.spawn(tank, 1, [405.0, 405.0], true);
    let protected = game.spawn(tank, 1, [355.0, 405.0], true);
    let victim = game.spawn(tank, 0, [505.0, 405.0], true);
    game.native_guard_object_from(guard, protected, COMMAND_FROM_SCRIPT);
    game.entities[guard].ai.ai_target = Some(victim);
    game.enter_guard_phase(guard, GuardPhase::Outer);
    let deadline = game.entities[guard].ai.guard.deadline;
    game.set_object_position(protected, [450.0, 405.0, 0.0]);
    game.tick = 1;
    game.update_guard(guard, [355.0, 405.0]);
    assert!(matches!(
        game.entities[guard].ai.guard.phase,
        GuardPhase::Outer
    ));
    let next_deadline = game.entities[guard].ai.guard.deadline;
    assert!(next_deadline > deadline);
    game.set_object_position(protected, [450.0, 405.0, 500.0]);
    game.tick = 2;
    game.update_guard(guard, [450.0, 405.0]);
    assert_eq!(game.entities[guard].ai.guard.deadline, next_deadline);
}

fn region_game() -> (Skirmish, usize, usize, usize) {
    let mut game = crate::skirmish::tests::game();
    let mut rules = rules(Vec::new());
    rules.map.areas.push(AiArea {
        name: "Perimeter".into(),
        points: vec![[200.0, 100.0], [700.0, 300.0], [200.0, 500.0]],
    });
    game.opponent_rules = Some(Arc::new(rules));
    game.slots[1].difficulty = Some(OpponentDifficulty::Hard);
    let tank = game.type_for(1, Kind::Tank).unwrap();
    game.catalog[tank].ai.vision_range = 500.0;
    let guard = game.spawn(tank, 1, [320.0, 300.0], true);
    let outside = game.spawn(tank, 0, [510.0, 450.0], true);
    let inside = game.spawn(tank, 0, [680.0, 300.0], true);
    game.update_ground_contacts();
    (game, guard, outside, inside)
}

#[test]
fn polygon_guard_rejects_a_closer_enemy_outside_the_authored_region() {
    let (mut game, guard, outside, inside) = region_game();
    let area = &game.opponent_rules.as_ref().unwrap().map.areas[0];
    let (center, radius) = map::bounds(area);
    assert_eq!(
        center.map(f32::to_bits),
        [450.0_f32, 300.0].map(f32::to_bits)
    );
    assert_eq!(radius.to_bits(), 152_500.0_f32.sqrt().to_bits());
    assert!(!map::inside(area, game.entities[outside].position));
    game.guard_area(guard, 0).unwrap();
    game.entities[guard].ai.guard.next_scan = game.tick;
    game.update_guard(guard, center);
    assert_eq!(game.entities[guard].ai.ai_target, Some(inside));
    assert!(matches!(game.entities[guard].ai.order, Order::Guard(_)));
}

#[test]
fn polygon_scan_reopens_after_crossing_but_not_movement_within_the_same_region() {
    let (mut game, guard, outside, _) = region_game();
    game.tick = u64::from(game.ai_rules.guard_scan_frames) + 1;
    game.guard_area(guard, 0).unwrap();
    game.entities[guard].ai.guard.next_scan = game.tick;
    game.update_guard(guard, [450.0, 300.0]);
    assert!(game.entities[guard].ai.ai_target.is_none());
    game.set_object_position(outside, [500.0, 440.0, 0.0]);
    game.update_ground_contacts();
    assert_eq!(game.trigger_state.changed_at, 0);
    game.set_object_position(outside, [600.0, 300.0, 0.0]);
    game.update_ground_contacts();
    assert_eq!(game.trigger_state.changed_at, game.tick);
    game.entities[guard].ai.guard.next_scan = game.tick;
    game.update_guard(guard, [450.0, 300.0]);
    assert_eq!(game.entities[guard].ai.ai_target, Some(outside));
}

#[test]
fn computer_idle_acquisition_can_pursue_beyond_weapon_range() {
    let mut game = crate::skirmish::tests::game();
    game.slots[1].difficulty = Some(OpponentDifficulty::Hard);
    let tank = game.type_for(1, Kind::Tank).unwrap();
    game.catalog[tank].ai.vision_range = 300.0;
    game.catalog[tank].weapon.range = 30.0;
    let actor = game.spawn(tank, 1, [400.0, 500.0], true);
    let enemy = game.spawn(tank, 0, [520.0, 500.0], true);
    game.tick = 100;
    game.acquire_target(actor);
    assert!(matches!(game.entities[actor].ai.order, Order::Attack(id) if id == enemy));
    game.stop_actor(actor).unwrap();
    game.slots[1].difficulty = None;
    game.tick += 100;
    game.acquire_target(actor);
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
}

#[test]
fn launch_sequence_releases_previous_guard_before_its_first_instruction() {
    let (mut game, guard, _, _) = region_game();
    let mut rules = Arc::try_unwrap(game.opponent_rules.take().unwrap())
        .unwrap_or_else(|_| panic!("Fixture owns its rules"));
    rules.factions[0].teams.push(recipe());
    rules.factions[0].scripts.push(script(
        "Launch",
        Vec::new(),
        vec![
            call(
                "SKIRMISH_MOVE_TO_APPROACH_PATH",
                vec![parameter("<This Team>", 0), parameter("Absent", 0)],
            ),
            call("TEAM_HUNT", vec![parameter("<This Team>", 0)]),
        ],
    ));
    let rules = Arc::new(rules);
    game.opponent_rules = Some(Arc::clone(&rules));
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    state.enabled = 0;
    state.teams.push(Team::new(Some(0), 0, 1));
    state.teams[0].members.push(guard);
    let native = game.team_membership.create_team(state.team_prototypes[0]);
    state.teams[0].native_instance = Some(native);
    game.team_membership.set_team(guard, Some(native));
    game.guard_area(guard, 0).unwrap();
    scripts::execute(
        &mut game,
        &rules,
        &mut state,
        &call(
            "TEAM_EXECUTE_SEQUENTIAL_SCRIPT",
            vec![parameter("<This Team>", 0), parameter("Launch", 0)],
        ),
        Some(0),
    );
    assert!(matches!(game.entities[guard].ai.order, Order::Idle));
    assert!(game.entities[guard].ai.guard.area.is_none());
    teams::sequence(&mut game, &rules, &mut state, 0);
    // The final hunt is still running. The source cleans up the sequence
    // when the group next becomes idle and advances past that instruction.
    assert_eq!(state.teams[0].sequence, Some((0, 2)));
    assert!(matches!(
        game.entities[guard].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::Hunt
    ));
    assert_eq!(game.entities[guard].ai.command.last_command_source, 1);
    assert!(state.unsupported.is_empty());
    game.tick = 100;
    game.update_ground_contacts();
    game.update_ai_module(guard);
    assert!(game.entities[guard].ai.attack.is_some());
    assert!(game.entities[guard].ai.current_victim.is_some());
}

#[test]
fn priority_sets_ignore_zero_and_observe_later_script_changes() {
    let mut game = crate::skirmish::tests::game();
    let rules = rules(Vec::new());
    let state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    let tank = game.type_for(1, Kind::Tank).unwrap();
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    game.catalog[tank].name = "Tank".into();
    game.catalog[infantry].name = "Infantry".into();
    let actor = game.spawn(tank, 1, [400.0, 500.0], true);
    let near = game.spawn(tank, 0, [460.0, 500.0], true);
    let far = game.spawn(infantry, 0, [650.0, 500.0], true);
    for (name, priority) in [("Tank", 0), ("Infantry", 50)] {
        priorities::action(
            &mut game,
            &state,
            &call(
                "SET_ATTACK_PRIORITY_THING",
                vec![
                    parameter("Targets", 0),
                    parameter(name, 0),
                    parameter("", priority),
                ],
            ),
        )
        .unwrap();
    }
    game.entities[actor].ai.attack_priority = priorities::find(&game, "Targets");
    game.opponent_rules = Some(Arc::new(rules));
    assert_eq!(
        game.prioritized_target(actor, [near, far].into_iter()),
        Some(far)
    );
    priorities::action(
        &mut game,
        &state,
        &call(
            "SET_ATTACK_PRIORITY_THING",
            vec![
                parameter("Targets", 0),
                parameter("Tank", 0),
                parameter("", 100),
            ],
        ),
    )
    .unwrap();
    assert_eq!(
        game.prioritized_target(actor, [near, far].into_iter()),
        Some(near)
    );
}

fn recruitment_fixture() -> (Skirmish, OpponentRules, State, usize) {
    let mut game = crate::skirmish::tests::game();
    let builder = game.type_for(1, Kind::Builder).unwrap();
    game.catalog[builder].name = "Builder".into();
    let producer = game.entities[2].type_index;
    game.catalog[producer].buildable_objects = vec!["Builder".into()];
    let mut rules = rules(Vec::new());
    let mut target = recipe();
    target.name = "TargetCollectors".into();
    target.units[0].name = "Builder".into();
    target.recruitment = 2;
    target.execute_actions = false;
    let mut donor = recipe();
    donor.name = "DonorCollectors".into();
    donor.units[0].name = "Builder".into();
    donor.priority = 5;
    donor.recruitment = 1;
    rules.factions[0].teams = vec![target, donor];
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    for template in 0..2 {
        let mut team = Team::new(Some(template), 0, 0);
        team.native_instance = Some(
            game.team_membership
                .create_team(state.team_prototypes[template]),
        );
        state.teams.push(team);
    }
    state.teams[1].members.push(3);
    game.team_membership
        .set_team(3, state.teams[1].native_instance);
    (game, rules, state, builder)
}

#[test]
fn recruitment_respects_active_priority_and_instance_override() {
    let (mut game, rules, mut state, builder) = recruitment_fixture();
    let home = game.entities[3].position;
    assert!(recruitment::candidate(&game, &rules, &state, 0, builder, home, 100.0).is_none());
    game.team_membership
        .activate(state.teams[1].native_instance.unwrap());
    assert_eq!(
        recruitment::candidate(&game, &rules, &state, 0, builder, home, 100.0),
        Some(3)
    );
    state.teams[1].recruitable = Some(false);
    assert!(recruitment::candidate(&game, &rules, &state, 0, builder, home, 100.0).is_none());
    state.teams[1].recruitable = Some(true);
    state.priorities[1] = state.priorities[0];
    assert!(recruitment::candidate(&game, &rules, &state, 0, builder, home, 100.0).is_none());
}

#[test]
fn default_team_first_recruit_retains_native_outside_radius_exception() {
    let (mut game, rules, state, builder) = recruitment_fixture();
    let far = game.spawn(builder, 1, [700.0, 500.0], true);
    assert_eq!(
        recruitment::candidate(&game, &rules, &state, 0, builder, [100.0, 100.0], 10.0),
        Some(far)
    );
}

#[test]
fn queued_team_recruits_without_spending_and_suspends_previous_hunt_for_native_ai_move() {
    let (mut game, mut rules, mut state, builder) = recruitment_fixture();
    rules.factions[0].teams[0].home = Some(game.entities[3].position);
    game.team_membership
        .activate(state.teams[1].native_instance.unwrap());
    game.ai_hunt_from_ai(3);
    assert!(matches!(
        game.entities[3].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::Hunt
    ));
    game.entities[3].ai.attack_priority = Some(7);
    game.entities[3].ai.mood.attitude = 2;
    state.queue_request(
        0,
        vec![WorkOrder {
            type_index: builder,
            remaining: 1,
            producer: None,
            required: true,
            resource_gatherer: false,
        }],
        0,
        true,
    );
    let cash = game.players[1].cash;
    recruitment::fill(&mut game, &rules, &mut state, 0, 0);
    assert_eq!(state.teams[0].members, [3]);
    assert!(state.teams[1].members.is_empty());
    assert!(matches!(
        game.entities[3].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::Hunt
    ));
    assert!(game.entities[3].ai.blocking.temporary_move_to());
    assert_eq!(state.requests[0].orders[0].remaining, 0);
    assert_eq!(game.players[1].cash, cash);
    assert_eq!(game.entities[3].ai.attack_priority, Some(7));
    assert_eq!(
        game.entities[3].ai.mood.attitude,
        rules.factions[0].teams[0].aggression
    );
    game.entities[3].ai.mood.attitude = 2;
    recruitment::disband(&mut game, &mut state.teams[0]);
    assert_eq!(game.entities[3].ai.mood.attitude, 0);
    assert_eq!(game.entities[3].ai.attack_priority, Some(7));
}

fn reinforcement_fixture() -> (Skirmish, OpponentRules, State, usize) {
    let (mut game, rules, mut state, builder) = recruitment_fixture();
    state.teams[1].members.clear();
    state.teams[0].members.push(3);
    game.team_membership
        .set_team(3, state.teams[0].native_instance);
    let native = state.teams[0].native_instance.unwrap();
    game.team_membership.activate(native);
    assert!(game.team_membership.take_created(native));
    state.buildings.push(Building {
        name: "Center".into(),
        position: game.entities[2].position,
        angle: 0.0,
        remaining: 0,
        automatic: false,
        priority: false,
        object: Some(2),
        destroyed_at: None,
        supply: None,
    });
    (game, rules, state, builder)
}

#[test]
fn reinforcement_requires_strictly_higher_priority_and_preserves_active_team() {
    let (mut game, rules, mut state, builder) = reinforcement_fixture();
    let recruit = game.spawn(builder, 1, [700.0, 500.0], true);
    let cash = game.players[1].cash;
    assert!(!recruitment::reinforce(&mut game, &rules, &mut state, 10));
    assert!(recruitment::reinforce(&mut game, &rules, &mut state, 9));
    assert_eq!(state.requests.len(), 1);
    assert!(state.requests[0].reinforcement);
    assert_eq!(state.requests[0].reinforcement_unit, Some(recruit));
    assert_eq!(game.players[1].cash, cash);
    state.flags.insert("GLA Team is Building".into(), true);
    lifecycle::check_building(&mut game, &rules, &mut state);
    lifecycle::check_ready(&mut game, &rules, &mut state);
    let native = state.teams[0].native_instance.unwrap();
    assert!(game.team_membership.active(native));
    assert!(!game.team_membership.take_created(native));
    assert!(state.flags["GLA Team is Building"]);
    assert!(state.build_queue.is_empty() && state.ready_queue.is_empty());
}

#[test]
fn reinforcement_without_a_recruit_charges_normal_production() {
    let (mut game, rules, mut state, builder) = reinforcement_fixture();
    let cash = game.players[1].cash;
    assert!(recruitment::reinforce(&mut game, &rules, &mut state, 9));
    assert_eq!(game.players[1].cash, cash - game.catalog[builder].cost);
    assert_eq!(state.requests[0].orders[0].producer, Some(2));
    assert!(
        matches!(game.entities[2].queue[0].item, ProductionItem::Unit(index) if index == builder)
    );
    assert!(!recruitment::reinforce(&mut game, &rules, &mut state, 0));
}

fn builder_variation(game: &mut Skirmish, wrapper: usize) -> usize {
    let mut unit = crate::skirmish::tests::fixture_type(game.players[1].faction, Kind::Builder);
    unit.name = "BuilderVariation".into();
    let variant = game.catalog.len();
    game.catalog.push(unit);
    game.catalog[wrapper].variations = vec!["BuilderVariation".into()];
    variant
}

#[test]
fn build_variation_production_completes_its_original_ai_work_order() {
    let (mut game, rules, mut state, builder) = recruitment_fixture();
    let center = game.entities[2].type_index;
    game.catalog[center].energy = 1;
    game.update_power();
    let variant = builder_variation(&mut game, builder);
    state.queue_request(
        0,
        vec![WorkOrder {
            type_index: builder,
            remaining: 1,
            producer: Some(2),
            required: true,
            resource_gatherer: false,
        }],
        0,
        true,
    );
    game.opponents[1] = Some(Opponent {
        rules: Arc::new(rules),
        state,
    });
    let cash = game.players[1].cash;
    game.train(2, builder).unwrap();
    for _ in 0..150 {
        game.update_production(2);
    }
    assert!(game.entities[2].queue.is_empty());
    assert_eq!(game.players[1].cash, cash - game.catalog[builder].cost);
    let id = game.entities.len() - 1;
    assert_eq!(game.entities[id].type_index, variant);
    let mut opponent = game.opponents[1].take().unwrap();
    assert_eq!(opponent.state.teams[0].members, [id]);
    assert_eq!(opponent.state.requests[0].orders[0].remaining, 0);
    assert!(opponent.state.requests[0].orders[0].producer.is_none());
    lifecycle::check_building(&mut game, &opponent.rules, &mut opponent.state);
    assert!(opponent.state.build_queue.is_empty());
    assert_eq!(opponent.state.ready_queue, [0]);
}

#[test]
fn variation_recruitment_and_reinforcement_preserve_the_authored_team_size() {
    let (mut game, mut rules, mut state, builder) = reinforcement_fixture();
    let variant = builder_variation(&mut game, builder);
    state.teams[0].members.clear();
    state.teams[1].members.push(3);
    let id = game.spawn(variant, 1, [650.0, 400.0], true);
    state.teams[0].members.push(id);
    let recruit = game.spawn(variant, 1, [700.0, 500.0], true);
    assert_eq!(
        recruitment::candidate(&game, &rules, &state, 0, builder, [700.0, 500.0], 100.0),
        Some(recruit)
    );
    rules.factions[0].teams[0].units[0].maximum = 1;
    assert!(!recruitment::reinforce(&mut game, &rules, &mut state, 0));
    rules.factions[0].teams[0].units[0].maximum = 2;
    let cash = game.players[1].cash;
    assert!(recruitment::reinforce(&mut game, &rules, &mut state, 0));
    assert_eq!(game.players[1].cash, cash);
    assert_eq!(state.requests[0].reinforcement_unit, Some(recruit));
    assert_eq!(state.teams[0].members, [id, recruit]);
}

#[test]
fn script_object_count_matches_variations_and_counts_overlapping_list_entries_once() {
    let (mut game, mut rules, mut state, builder) = recruitment_fixture();
    let variant = builder_variation(&mut game, builder);
    game.spawn(variant, 1, [650.0, 400.0], true);
    state.object_lists.insert(
        "Builders".into(),
        vec!["Builder".into(), "BuilderVariation".into()],
    );
    for name in ["Builder", "Builders"] {
        let condition = script(
            "Count",
            vec![call(
                "PLAYER_HAS_OBJECT_COMPARISON",
                vec![
                    parameter("<This Player>", 0),
                    parameter("", 2),
                    parameter("", 2),
                    parameter(name, 0),
                ],
            )],
            vec![],
        );
        rules.factions[0].scripts.push(condition);
        let index = rules.factions[0].scripts.len() - 1;
        assert_eq!(
            scripts::conditions(&game, &rules, &mut state, index, None),
            Some(true)
        );
    }
}

#[test]
fn script_research_uses_paid_queue_once_and_rejects_object_scope() {
    let mut game = crate::skirmish::tests::game();
    let center = game.entities[2].type_index;
    game.catalog[center].maintenance.research = vec![
        Research {
            name: "PlayerUpgrade".into(),
            cost: 500,
            frames: 60,
            player: true,
        },
        Research {
            name: "ObjectUpgrade".into(),
            cost: 300,
            frames: 60,
            player: false,
        },
    ];
    let rules = rules(Vec::new());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Easy, 0);
    state.buildings.push(Building {
        name: "Center".into(),
        position: game.entities[2].position,
        angle: 0.0,
        remaining: 0,
        automatic: false,
        priority: false,
        object: Some(2),
        destroyed_at: None,
        supply: None,
    });
    let cash = game.players[1].cash;
    for upgrade in ["PlayerUpgrade", "PlayerUpgrade", "ObjectUpgrade"] {
        scripts::execute(
            &mut game,
            &rules,
            &mut state,
            &call(
                "AI_PLAYER_BUILD_UPGRADE",
                vec![parameter("<This Player>", 0), parameter(upgrade, 0)],
            ),
            None,
        );
    }
    assert_eq!(game.players[1].cash, cash - 500);
    assert_eq!(game.entities[2].queue.len(), 1);
    assert!(matches!(
        game.entities[2].queue[0].item,
        ProductionItem::Upgrade(0)
    ));
    assert!(state.unsupported.is_empty());
}

#[test]
fn original_default_team_battle_plan_command_reaches_existing_centers_without_recruiting_them() {
    use crate::skirmish::battle_plan::{Plan, Rules};
    let mut game = crate::skirmish::tests::game();
    let mut unit = crate::skirmish::tests::fixture_type(Faction::Gla, Kind::TechBuilding);
    unit.battle_plan = Some(Rules {
        animation_frames: [2; 3],
        transition_idle_frames: 1,
        paralyze_frames: 7,
        valid_kind_of: vec!["INFANTRY".into(), "VEHICLE".into()],
        invalid_kind_of: vec!["DOZER".into()],
        armor_scalar: 0.8,
        sight_scalar: 1.2,
        center_sight_scalar: 2.0,
        center_health_scalar: 2.0,
        center_detects_stealth: true,
    });
    crate::skirmish::tests::register_update(&mut unit, "BattlePlanUpdate");
    let index = game.catalog.len();
    game.catalog.push(unit);
    let center = game.spawn(index, 1, [500.0, 400.0], true);
    let recruited = game.spawn(index, 1, [450.0, 400.0], true);
    let other = game.spawn(index, 0, [200.0, 400.0], true);
    let mut rules = rules(Vec::new());
    rules.factions[0].teams.push(recipe());
    let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
    state.teams.push(Team::new(Some(0), 0, 0));
    state.teams[0].members = vec![recruited];
    state.teams[0].sequence = Some((2, 3));
    let command = call(
        "TEAM_USE_COMMANDBUTTON_ABILITY",
        vec![
            parameter("teamSkirmishGLA", 0),
            parameter("Command_InitiateBattlePlanHoldTheLine", 0),
        ],
    );
    assert!(teams::action(&mut game, &rules, &mut state, &command, None).is_some());
    assert!(game.entities[center].battle_plan_desired() == Some(Plan::HoldTheLine));
    assert!(game.entities[recruited].battle_plan_desired().is_none());
    assert!(game.entities[other].battle_plan_desired().is_none());
    assert_eq!(state.teams[0].sequence, Some((2, 3)));
    for _ in 0..3 {
        game.step();
    }
    assert!(game.entities[center].battle_plan() == Some(Plan::HoldTheLine));
}

#[test]
fn mapped_recruitability_filters_actual_ai_but_not_objects_without_ai() {
    for with_ai in [true, false] {
        let (mut game, rules, state, _) = recruitment_fixture();
        let mut unit = crate::skirmish::tests::fixture_type(Faction::China, Kind::Infantry);
        unit.name = "MappedRecruit".into();
        if !with_ai {
            unit.ai.update = crate::skirmish::UnitAiUpdate::Absent;
            unit.behavior_modules.clear();
            unit.update_modules.clear();
        }
        let type_index = game.catalog.len();
        game.catalog.push(unit);
        let record_index = game.layout.neutral_objects.len();
        game.layout
            .neutral_objects
            .push(crate::skirmish::NeutralObject {
                initial_health: None,
                attitude: None,
                recruitable: Some(false),
                original_team: "team".into(),
                script_name: None,
                name: "MappedRecruit".into(),
                position: [500.0, 300.0, 0.0],
                angle: 0.0,
                script_targetable: false,
                indestructible: false,
                selectable: None,
            });
        let blocked = game.spawn_neutral_object(record_index, false).unwrap();
        game.transfer_maintenance_owner(blocked, 1);
        assert_eq!(game.entities[blocked].ai.registration.is_some(), with_ai);
        assert_eq!(
            recruitment::candidate(&game, &rules, &state, 0, type_index, [500.0, 300.0], 100.0),
            (!with_ai).then_some(blocked)
        );
        game.layout.neutral_objects[record_index].recruitable = Some(true);
        let allowed = game.spawn_neutral_object(record_index, false).unwrap();
        game.transfer_maintenance_owner(allowed, 1);
        assert_eq!(
            recruitment::candidate(&game, &rules, &state, 0, type_index, [500.0, 300.0], 100.0),
            Some(if with_ai { allowed } else { blocked })
        );
    }
}
