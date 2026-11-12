use super::*;
use crate::{
    skirmish_verification::{advance_until, construct, construct_type, friendly_entity},
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{Faction, Kind, visibility::Status};

#[ignore = "requires original game installation"]
#[test]
fn installed_internet_center_keeps_original_spy_timers_masks_and_command_transition() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let files = super::super::source_files(&assets).unwrap();
    let object = super::super::definition(&files, "Object", "ChinaInternetCenter").unwrap();
    let rules = load(&files, object).unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].trigger, "Upgrade_ChinaSatelliteHackOne");
    assert!(rules[0].command_centers_only);
    assert_eq!((rules[0].duration, rules[0].interval), (0, 0));
    assert_eq!(rules[1].trigger, "Upgrade_ChinaSatelliteHackTwo");
    assert!(!rules[1].command_centers_only);
    assert_eq!((rules[1].duration, rules[1].interval), (600, 7200));
    let index = template(&battle.simulation, "ChinaInternetCenter").unwrap();
    let unit = &battle.simulation.catalog[index];
    let command = unit
        .maintenance
        .command_sets
        .iter()
        .find(|rule| rule.upgrade == rules[0].trigger)
        .unwrap();
    assert_eq!(command.command_set, "ChinaInternetCenterCommandSetTwo");
    assert_eq!(
        command.alternate.as_ref().unwrap(),
        &(
            "Upgrade_ChinaMines".into(),
            "ChinaInternetCenterCommandSetTwoUpgrade".into()
        )
    );
}

#[ignore = "requires original game installation"]
#[test]
fn paid_satellite_research_reveals_actual_enemy_looker_areas_and_retains_pulse_timing() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let files = super::super::source_files(&assets).unwrap();
    let global = super::super::definition(&files, "GameData", "").unwrap();
    assert!(
        !crate::visibility_data::rules(&assets, &global.lines)
            .unwrap()
            .use_shroud
    );
    let game = &mut battle.simulation;
    game.reset(Faction::China, Faction::China).unwrap();
    let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
    construct(game, builder, Kind::PowerPlant, 0).unwrap();
    construct(game, builder, Kind::SupplyCenter, 0).unwrap();
    construct(game, builder, Kind::Factory, 0).unwrap();
    let center_type = template(game, "ChinaInternetCenter").unwrap();
    let center = construct_type(game, builder, center_type, 0).unwrap();
    let enemy_center = friendly_entity(game, Kind::CommandCenter, 1).unwrap();
    let enemy_builder = friendly_entity(game, Kind::Builder, 1).unwrap();
    let enemy_origin = game.entities[enemy_center].position;
    let wanted = [enemy_origin[0] + 700.0, enemy_origin[1] - 500.0];
    let power = game.type_for(1, Kind::PowerPlant).unwrap();
    let goal = game.placement_near(power, wanted).unwrap();
    assert!((goal[0] - enemy_origin[0]).hypot(goal[1] - enemy_origin[1]) > 500.0);
    game.move_to(enemy_builder, goal).unwrap();
    advance_until(
        game,
        7200,
        "ordinary enemy Dozer movement outside command-center sight",
        |game| {
            let actor = &game.entities[enemy_builder];
            (actor.position[0] - goal[0]).hypot(actor.position[1] - goal[1]) < 30.0
        },
    )
    .unwrap();
    game.stop(enemy_builder).unwrap();
    let point = game.entities[enemy_builder].position;
    assert!(matches!(game.visibility_cell(0, point), Status::Fogged));
    let spent = game.players[0].statistics.money_spent;
    research(game, center, "Upgrade_ChinaSatelliteHackOne").unwrap();
    assert_eq!(game.players[0].statistics.money_spent, spent + 1000);
    assert!(matches!(
        game.visibility_cell(0, enemy_origin),
        Status::Clear
    ));
    assert!(matches!(game.visibility_cell(0, point), Status::Fogged));
    assert!(
        game.entities[center]
            .upgrade_conditions(game.tick)
            .contains(&"USER_1")
    );
    assert_eq!(
        game.entities[center].command_set_override(),
        Some("ChinaInternetCenterCommandSetTwo")
    );
    research(game, center, "Upgrade_ChinaSatelliteHackTwo").unwrap();
    assert_eq!(game.players[0].statistics.money_spent, spent + 3500);
    assert!(matches!(game.visibility_cell(0, point), Status::Clear));
    assert!(
        game.entities[center]
            .upgrade_conditions(game.tick)
            .contains(&"USER_2")
    );
    let completed = game.tick;
    for _ in 0..600 {
        game.step();
    }
    assert_eq!(game.tick, completed + 600);
    for _ in 0..180 {
        game.step();
    }
    assert!(matches!(game.visibility_cell(0, point), Status::Fogged));
    assert!(matches!(
        game.visibility_cell(0, enemy_origin),
        Status::Clear
    ));
    advance_until(
        game,
        7500,
        "original recurring Satellite Hack Two pulse",
        |game| matches!(game.visibility_cell(0, point), Status::Clear),
    )
    .unwrap();
}
