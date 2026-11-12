use crate::skirmish_data::{
    Faction, Kind, definition, finish_type, load, normal_model, source_files, unit_type,
};
use generals_assets::virtual_assets::VirtualAssets;

fn placed_original_structure() -> generals_sim::skirmish::Skirmish {
    use crate::{playtest::MAP_NAME, terrain::Terrain, world_data};
    use generals_assets::map::MapData;
    use generals_sim::skirmish::{MapObstacle, MapWorldPlacement, NeutralObject};
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = MapData::parse(&assets.read(MAP_NAME).unwrap().bytes).unwrap();
    let mut data = load(&assets, &map.objects).unwrap();
    let mut roster = vec![(
        "MogadishuHighrise01".into(),
        Faction::Civilian,
        Kind::TechBuilding,
    )];
    let (files, metadata) = source_files::with_runtime_dependencies(&assets, &mut roster).unwrap();
    let object = definition(&files, "Object", "MogadishuHighrise01").unwrap();
    if !data
        .types
        .iter()
        .any(|unit| unit.name == "MogadishuHighrise01")
    {
        let mut unit = unit_type(&files, object, Faction::Civilian, Kind::TechBuilding)
            .and_then(|unit| finish_type(&files, object, unit))
            .unwrap();
        unit.behavior_modules = metadata["MogadishuHighrise01"]
            .runtime_behaviors(&files)
            .unwrap();
        unit.update_modules = metadata["MogadishuHighrise01"].runtime_registrations(true);
        let model = normal_model(&assets, object).unwrap();
        assert!(!model.meshes.is_empty());
        data.types.push(unit);
        data.models.push(model);
    }
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
    let [x, y] = layout.starts[0].position;
    let index = layout.neutral_objects.len();
    layout.neutral_objects.push(NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        original_team: "teamPlyrCivilian".into(),
        script_name: Some("NativeCollapse".into()),
        name: "MogadishuHighrise01".into(),
        position: [x + 150.0, y, 0.0],
        angle: 0.0,
        script_targetable: false,
        indestructible: false,
        selectable: None,
    });
    layout.obstacle_order.push(MapObstacle::Neutral(index));
    layout
        .world_object_order
        .push(MapWorldPlacement::Neutral(index));
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
fn original_mogadishu_collapse_is_woken_by_ordered_die_and_retains_post_collapse_drawable() {
    use generals_sim::skirmish::map_scripts::{Action, Condition, Script};
    let mut game = placed_original_structure();
    let tower = game.named_map_object("NativeCollapse").unwrap();
    game.step();
    assert!(!game.entities[tower].post_collapse());
    game.install_map_scripts(vec![Script {
        owner: "player0".into(),
        name: "Collapse".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::NamedKill("NativeCollapse".into())],
        false_actions: Vec::new(),
    }]);
    game.step();
    assert!(!game.entities[tower].alive());
    assert!(!game.entities[tower].post_collapse());
    for _ in 0..1000 {
        game.step();
        if game.entities[tower].post_collapse() {
            break;
        }
    }
    assert!(game.entities[tower].post_collapse());
    assert!(game.entities[tower].exists() && game.entities[tower].has_drawable());
    assert!(
        game.model_flags(&game.entities[tower], [0.5, 0.1])
            .contains("POST_COLLAPSE")
    );
    game.step();
    assert!(
        !game
            .presentation
            .structure_events
            .iter()
            .any(|event| event.object == tower)
    );
}
