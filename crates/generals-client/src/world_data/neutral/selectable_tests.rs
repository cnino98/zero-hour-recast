//! Original Killing Fields Object properties use actual instance selection.
use generals_assets::{
    map::{MapData, MapValue},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::Faction;

#[ignore = "requires original game installation"]
#[test]
fn original_killing_fields_bridge_map_override_selects_actual_unattackable_object() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = MapData::parse(
        &assets
            .read("Maps\\Killing Fields\\Killing Fields.map")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let records: Vec<_> = map
        .objects
        .iter()
        .filter(|record| record.template == "MonumentTrainBridge")
        .collect();
    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].properties.get("objectSelectable"),
        Some(MapValue::Bool(true))
    ));
    let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
    let kind = data
        .types
        .iter()
        .position(|unit| unit.name == "MonumentTrainBridge")
        .unwrap();
    assert!(!data.types[kind].selectable);
    assert!(
        data.types[kind]
            .kind_of
            .iter()
            .any(|kind| kind == "UNATTACKABLE")
    );
    verify_original_portable_selection(&data.types);
    let settings = crate::world_data::camera_settings(&assets, root).unwrap();
    let terrain = crate::terrain::Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let mut movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    let bridges =
        crate::world_data::install_bridges(&assets, &map.objects, &terrain, &mut movement).unwrap();
    let (mut layout, _) = crate::world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    let placed = layout
        .neutral_objects
        .iter()
        .find(|record| record.name == "MonumentTrainBridge")
        .unwrap();
    assert_eq!(placed.selectable, Some(true));
    layout.regular_bridges = bridges;
    let mut game = data.into_simulation(movement, layout).unwrap();
    game.reset(Faction::Usa, Faction::China).unwrap();
    let actors: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter_map(|(id, actor)| (actor.type_index == kind).then_some(id))
        .collect();
    assert_eq!(actors.len(), 1);
    let id = actors[0];
    assert!(game.entities[id].selectable(&game.catalog[kind]));
    assert!(!game.entities[id].unselectable());
    assert!(
        game.catalog[kind]
            .kind_of
            .iter()
            .any(|kind| kind == "UNATTACKABLE")
    );
    let owner = game.entities[id].owner;
    assert_eq!(game.materialize_selection_group(owner, &[id]), [id]);
}

fn verify_original_portable_selection(types: &[generals_sim::skirmish::UnitType]) {
    for name in [
        "ChinaTankOverlordGattlingCannon",
        "ChinaTankOverlordPropagandaTower",
        "ChinaTankOverlordBattleBunker",
    ] {
        let unit = types.iter().find(|unit| unit.name == name).unwrap();
        assert!(
            !unit.selectable,
            "original portable {name} has no SELECTABLE bit"
        );
    }
}
