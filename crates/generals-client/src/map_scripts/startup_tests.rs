//! Parsed authored build-list startup against the installed template catalog.
use generals_assets::{
    map::{MapData, MapValue},
    scripts::{MapBuildEntry, MapScripts, MapSide},
};
use generals_sim::skirmish::{Faction, Skirmish};
use std::collections::BTreeMap;

fn installed_npc_world() -> (Skirmish, u32, f32) {
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let bytes = assets.read("maps/flash fire/flash fire.map").unwrap().bytes;
    let map = MapData::parse(&bytes).unwrap();
    let root = &generals_assets::game_root().unwrap();
    let settings = crate::world_data::camera_settings(&assets, root).unwrap();
    let terrain = crate::terrain::Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
    let native_height = terrain.sample(bevy::prelude::Vec2::new(50.0, 30.0)) * 10.0;
    assert!(native_height > 0.0);
    let metadata = MapScripts {
        version: 3,
        sides: vec![MapSide {
            properties: BTreeMap::from([
                ("playerName".into(), MapValue::Ascii("AuthoredNPC".into())),
                (
                    "playerFaction".into(),
                    MapValue::Ascii("FactionCivilian".into()),
                ),
                ("playerIsHuman".into(), MapValue::Bool(false)),
                ("playerStartMoney".into(), MapValue::Int(9000)),
            ]),
            build_list: vec![MapBuildEntry {
                building_name: "AuthoredBarracks".into(),
                template_name: "AmericaBarracks".into(),
                position: [500.0, 300.0, 123.0],
                angle: 0.75,
                initially_built: 1,
                rebuilds: 2,
                script: None,
                health: Some(17),
                whiner: Some(1),
                unsellable: Some(1),
                repairable: Some(1),
            }],
        }],
        teams: Vec::new(),
        script_lists: Vec::new(),
    };
    let startup = super::load_startup(&metadata, &data.types, &assets).unwrap();
    assert_eq!(startup.sides[0].template_money, 0);
    let initial_cash = data.economy.starting_cash + 9000;
    assert_eq!(
        startup.build_lists[0].entries[0].position.map(f32::to_bits),
        [500.0_f32, 300.0].map(f32::to_bits)
    );
    let (mut layout, _) = crate::world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    layout.map_sides = startup.sides;
    layout.map_teams = startup.teams;
    layout.map_build_lists = startup.build_lists;
    layout.map_template_order = startup.template_order;
    let movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    (
        data.into_simulation(movement, layout).unwrap(),
        initial_cash,
        native_height,
    )
}

#[ignore = "requires original game installation"]
#[test]
fn installed_catalog_authored_civilian_structure_preserves_source_startup_and_restart() {
    let (mut game, initial_cash, native_height) = installed_npc_world();
    let type_index = game
        .catalog
        .iter()
        .position(|unit| unit.name == "AmericaBarracks")
        .unwrap();
    for restart in [false, true] {
        if restart {
            game.reset(Faction::Usa, Faction::China).unwrap();
        }
        assert_eq!(
            game.ground_height([500.0, 300.0]).to_bits(),
            native_height.to_bits()
        );
        let actors: Vec<_> = game
            .entities
            .iter()
            .enumerate()
            .filter(|(_, actor)| {
                actor.type_index == type_index
                    && actor.position.map(f32::to_bits) == [500.0_f32, 300.0].map(f32::to_bits)
            })
            .collect();
        assert_eq!(actors.len(), 1);
        let (id, actor) = actors[0];
        assert!(actor.complete());
        assert_eq!(actor.health.to_bits(), actor.max_health.to_bits());
        assert_eq!(actor.facing.to_bits(), 0.75_f32.to_bits());
        assert_eq!(actor.owner, 3);
        assert!(!game.can_sell(actor.owner, id));
        assert_eq!(game.players.len(), 4);
        assert_eq!(game.players[actor.owner].cash, initial_cash);
        assert_eq!(game.players[2].cash, 0);
    }
}
