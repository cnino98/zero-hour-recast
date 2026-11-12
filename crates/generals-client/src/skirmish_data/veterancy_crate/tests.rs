use super::*;
use crate::{playtest::MAP_NAME, skirmish_data, terrain::Terrain, world_data};
use generals_assets::{
    content,
    map::MapData,
    virtual_assets::{Asset, VirtualAssets},
};
use generals_sim::skirmish::{
    Faction, Kind, MapObstacle, MapWorldPlacement, NeutralObject, Skirmish,
};

fn assets() -> VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

#[ignore = "requires original game installation"]
#[test]
fn original_pilot_binds_exact_crate_fields_and_starting_veterancy() {
    let files = super::super::source_files(&assets()).unwrap();
    let object = super::super::definition(&files, "Object", "AmericaInfantryPilot").unwrap();
    let mut unit = super::super::unit_type(&files, object, Faction::Usa, Kind::Infantry).unwrap();
    super::super::veterancy::load(&files, object, &mut unit).unwrap();
    let rules = &unit.behavior.veterancy_crates;
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].restrictions.required, ["VEHICLE"]);
    assert_eq!(rules[0].restrictions.forbidden, ["DOZER"]);
    assert!(rules[0].adds_owner_veterancy && rules[0].is_pilot);
    assert!(!rules[0].allow_multi_pickup && !rules[0].restrictions.forbid_owner);
    assert!(rules[0].execute_fx.is_empty());
    assert_eq!(unit.veterancy.starting[0].level, 1);
    let scan = &unit.behavior.pilot_find_vehicle[0];
    assert_eq!(scan.scan_frames, 30);
    assert_eq!(scan.scan_range.to_bits(), 300.0_f32.to_bits());
    assert_eq!(scan.min_health.to_bits(), 0.5_f32.to_bits());
}

fn original_map_with_pilot() -> Skirmish {
    let root = &generals_assets::game_root().unwrap();
    let assets = assets();
    let map = MapData::parse(&assets.read(MAP_NAME).unwrap().bytes).unwrap();
    let mut data = skirmish_data::load(&assets, &map.objects).unwrap();
    let settings = world_data::camera_settings(&assets, root).unwrap();
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let (mut layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    // Actual map placement creates the parsed templates and their modules;
    // it supplies source names and controlling teams without a test spawn API.
    let [x, y] = layout.starts[0].position;
    for (name, template, offset) in [
        ("Ace", "AmericaInfantryPilot", 180.0),
        ("Crusader", "AmericaTankCrusader", 240.0),
    ] {
        let index = layout.neutral_objects.len();
        layout.neutral_objects.push(NeutralObject {
            initial_health: None,
            attitude: None,
            recruitable: None,
            original_team: "teamplayer0".into(),
            script_name: Some(name.into()),
            name: template.into(),
            position: [x + offset, y, 0.0],
            angle: 0.0,
            script_targetable: false,
            indestructible: false,
            selectable: None,
        });
        layout.obstacle_order.push(MapObstacle::Neutral(index));
        layout
            .world_object_order
            .push(MapWorldPlacement::Neutral(index));
    }
    let movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    let animations = crate::model_animation::load(&assets, &mut data.models, &data.types).unwrap();
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations).unwrap();
    let mut game = data.into_simulation(movement, layout).unwrap();
    terrain
        .configure_projectiles(&mut game, projectiles.partition_cell_size)
        .unwrap();
    game
}

#[ignore = "requires original game installation"]
#[test]
fn original_pilot_scheduled_entry_promotes_crusader_and_destroys_without_die() {
    let mut game = original_map_with_pilot();
    let pilot = game.named_map_object("Ace").unwrap();
    let vehicle = game.named_map_object("Crusader").unwrap();
    assert_eq!(game.entities[pilot].owner, 0);
    assert_eq!(game.entities[vehicle].owner, 0);
    assert_eq!(game.entities[pilot].veterancy, 1);
    let required = game.catalog[game.entities[vehicle].type_index]
        .veterancy
        .required[1];
    game.create_selection(0, &[], &[pilot], true);
    game.step();
    game.enter(pilot, vehicle).unwrap();
    for _ in 0..600 {
        game.step();
        if !game.entities[pilot].exists() {
            break;
        }
    }
    assert!(!game.entities[pilot].exists());
    assert_eq!(game.entities[vehicle].veterancy, 1);
    assert_eq!(game.entities[vehicle].experience, required);
    assert_eq!(game.entities[vehicle].owner, 0);
    assert_eq!(game.named_map_object("Ace"), Some(vehicle));
    assert!(game.named_map_object("Crusader").is_none());
    assert!(game.selected_objects(0).is_empty());
    assert!(game.deselected_objects.contains(&pilot));
}

#[test]
fn unsupported_radius_and_world_animation_remain_explicit() {
    for authored in ["EffectRange=12.5", "ExecuteAnimation=CrateSpin"] {
        let file = content::scan(Asset {
            path: "pilot.ini".into(),
            origin: "bounded crate source fields".into(),
            bytes: format!(
                "Object Pilot\nBehavior=VeterancyCrateCollide Tag\n{authored}\nEnd\nEnd\n"
            )
            .into_bytes(),
        })
        .unwrap();
        assert!(load(std::slice::from_ref(&file), &file.declarations[0]).is_err());
    }
}

#[test]
fn inherited_fx_parser_stores_null_for_unresolved_names() {
    let file = content::scan(Asset {
        path: "crate-fx.ini".into(), origin: "native INI parseFXList pointer resolution".into(),
        bytes: b"FXList Pickup\nEnd\nObject Crate\nBehavior=VeterancyCrateCollide Missing\nExecuteFX=Absent\nEnd\nBehavior=VeterancyCrateCollide Known\nExecuteFX=Pickup\nEnd\nEnd\n".to_vec(),
    }).unwrap();
    let rules = load(std::slice::from_ref(&file), &file.declarations[1]).unwrap();
    assert!(rules[0].execute_fx.is_empty());
    assert_eq!(rules[1].execute_fx, "Pickup");
}
