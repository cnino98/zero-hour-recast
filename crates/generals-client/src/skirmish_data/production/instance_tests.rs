use super::*;
use generals_assets::{content, virtual_assets::Asset};

#[test]
fn first_interface_data_is_not_overwritten_or_merged_with_later_modules() {
    let file = content::scan(Asset {
        path: "production.ini".into(), origin: "independent ProductionUpdate data".into(),
        bytes: b"Object Factory\nBehavior=ProductionUpdate First\nMaxQueueEntries=3\nQuantityModifier=FirstUnit 2\nDisabledTypesToProcess=DISABLED_HELD\nNumDoorAnimations=1\nDoorOpeningTime=100\nEnd\nBehavior=ProductionUpdate Second\nMaxQueueEntries=7\nQuantityModifier=SecondUnit 4\nDisabledTypesToProcess=NONE\nNumDoorAnimations=2\nDoorOpeningTime=200\nEnd\nEnd\n".to_vec(),
    }).unwrap();
    let object = &file.declarations[0];
    assert_eq!(queue_limit(object).unwrap(), 3);
    assert_eq!(
        quantities(object).unwrap().unwrap(),
        [("FirstUnit".into(), 2)]
    );
    assert_eq!(disabled_types(object).unwrap(), 1 << 3);
    assert_eq!(doors(object).unwrap().opening, 3);
    let later = additional(object).unwrap();
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].queue_limit, 7);
    assert_eq!(later[0].quantities, [("SecondUnit".into(), 4)]);
    assert_eq!(later[0].disabled_types, 0);
    assert_eq!(later[0].doors.count, 2);
    assert_eq!(later[0].doors.opening, 6);
}

#[ignore = "requires original game installation"]
#[test]
fn installed_gattling_two_production_modules_advance_paid_mines_research_once_per_frame() {
    use crate::skirmish_verification::{construct, construct_type, friendly_entity};
    use generals_sim::skirmish::{Faction, Kind};
    use num_traits::ToPrimitive;

    let (mut game, _) = super::super::reinforcement_runtime_tests::original_game(
        "Maps\\Cairo Commandos\\Cairo Commandos.map",
    );
    game.reset_sandbox(Faction::China).unwrap();
    let builder = friendly_entity(&game, Kind::Builder, 0).unwrap();
    construct(&mut game, builder, Kind::PowerPlant, 0).unwrap();
    construct(&mut game, builder, Kind::Barracks, 0).unwrap();
    let kind = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaGattlingCannon")
        .unwrap();
    let modules: Vec<_> = game.catalog[kind]
        .behavior_modules
        .iter()
        .filter(|module| module.family == "ProductionUpdate")
        .map(|module| module.tag.as_str())
        .collect();
    assert_eq!(modules, ["ModuleTag_07", "ModuleTag_10"]);
    assert_eq!(game.catalog[kind].additional_production.len(), 1);
    let cannon = construct_type(&mut game, builder, kind, 0).unwrap();
    let research = game.catalog[kind]
        .maintenance
        .research
        .iter()
        .position(|research| research.name == "Upgrade_ChinaMines")
        .unwrap();
    let rules = &game.catalog[kind].maintenance.research[research];
    let frames = rules.frames;
    let cost = rules.cost;
    let cash = game.players[0].cash;
    game.queue_upgrade(cannon, research).unwrap();
    assert_eq!(game.players[0].cash, cash - cost);
    for elapsed in 1..=frames {
        game.step();
        if elapsed < frames {
            assert_eq!(game.entities[cannon].queue.len(), 1);
            let expected = (frames.to_f32().unwrap() - elapsed.to_f32().unwrap()) / 30.0;
            assert_eq!(
                game.entities[cannon].queue[0].remaining_seconds.to_bits(),
                expected.to_bits()
            );
            assert!(!game.object_has_upgrade(cannon, "Upgrade_ChinaMines"));
        }
    }
    assert!(game.entities[cannon].queue.is_empty());
    assert!(game.object_has_upgrade(cannon, "Upgrade_ChinaMines"));
}
