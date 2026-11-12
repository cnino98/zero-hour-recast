use super::*;
use generals_assets::{content, virtual_assets::Asset};

fn object(modules: &str) -> ContentFile {
    content::scan(Asset {
        path: "terrorist.ini".into(),
        origin: "car-bomb parser fixture".into(),
        bytes: format!("Object GLAInfantryTerrorist\n{modules}\nEnd\n").into_bytes(),
    })
    .expect("source object")
}

#[test]
fn converter_keeps_required_kind_flags_and_rejects_untranslated_collision_order() {
    let convert = "Behavior=ConvertToCarBombCrateCollide Convert\nRequiredKindOf=VEHICLE SELECTABLE\nForbiddenKindOf=BOAT\nEnd";
    let valid = object(&format!(
        "Behavior=PhysicsBehavior Physics\nEnd\n{convert}\nBehavior=SquishCollide Squish\nEnd"
    ));
    let rules = converter(&valid.declarations[0]).unwrap().unwrap();
    assert_eq!(rules.required, ["VEHICLE", "SELECTABLE"]);
    assert_eq!(rules.forbidden, ["BOAT"]);
    let invalid = object(&format!("Behavior=SquishCollide Squish\nEnd\n{convert}"));
    assert!(converter(&invalid.declarations[0]).is_err());
}

#[test]
fn unsupported_crate_conditions_are_not_silently_discarded() {
    let file = object(
        "Behavior=ConvertToCarBombCrateCollide Convert\nRequiredKindOf=VEHICLE\nHumanOnly=Yes\nEnd",
    );
    assert!(converter(&file.declarations[0]).is_err());
}

#[ignore = "requires original game installation"]
#[test]
fn installed_whiteout_truck_accepts_paid_terrorist_and_original_suicide_weapon() {
    use crate::{
        skirmish_data,
        skirmish_verification::{advance_until, construct, friendly_entity, produce_type},
        strategy_verification::template,
        terrain::Terrain,
        world_data,
    };
    use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
    use generals_sim::skirmish::{Faction, Kind};
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = MapData::parse(&assets.read("maps/whiteout/whiteout.map").unwrap().bytes).unwrap();
    let mut data = skirmish_data::load(&assets, &map.objects).unwrap();
    let settings = world_data::camera_settings(&assets, root).unwrap();
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let (layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    let mut game = data
        .into_simulation(
            terrain
                .classify_movement(&map.triggers, &map.objects)
                .unwrap(),
            layout,
        )
        .unwrap();
    game.reset(Faction::Gla, Faction::Usa).unwrap();
    let truck = game
        .entities
        .iter()
        .position(|actor| game.catalog[actor.type_index].name == "TruckWork")
        .unwrap();
    assert!(game.entities[truck].alive());
    assert!(!game.entities[truck].car_bomb());
    let builder = friendly_entity(&game, Kind::Builder, 0).unwrap();
    let barracks = construct(&mut game, builder, Kind::Barracks, 0).unwrap();
    let terrorist_type = template(&game, "GLAInfantryTerrorist").unwrap();
    let terrorist = produce_type(&mut game, barracks, terrorist_type, 0).unwrap();
    game.move_to(terrorist, game.entities[truck].position)
        .unwrap();
    advance_until(
        &mut game,
        18000,
        "paid Terrorist scouting original Whiteout truck",
        |game| game.can_make_car_bomb(terrorist, truck),
    )
    .unwrap();
    game.make_car_bomb(terrorist, truck).unwrap();
    advance_until(
        &mut game,
        18000,
        "paid Terrorist entering original Whiteout truck",
        |game| game.entities[truck].car_bomb(),
    )
    .unwrap();
    assert_eq!(game.entities[truck].owner, 0);
    assert!(!game.entities[terrorist].alive());
    assert_eq!(
        game.entities[truck]
            .active_weapon(&game.catalog[game.entities[truck].type_index])
            .name,
        "SuicideCarBomb"
    );
    let before = game.entities[truck].position;
    game.move_to(truck, game.entities[barracks].position)
        .unwrap();
    for _ in 0..180 {
        game.step();
    }
    assert!(
        game.entities[truck].position[0].to_bits() != before[0].to_bits()
            || game.entities[truck].position[1].to_bits() != before[1].to_bits()
    );
}
