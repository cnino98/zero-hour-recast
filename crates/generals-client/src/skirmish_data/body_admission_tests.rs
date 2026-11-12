use super::*;

#[ignore = "requires original game installation"]
#[test]
fn original_hedge_inherits_inactive_body_and_retains_real_topple_and_death_modules() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut roster = vec![("Hedge1".into(), Faction::Civilian, Kind::TechBuilding)];
    let (files, metadata) = source_files::with_runtime_dependencies(&assets, &mut roster).unwrap();
    let object = definition(&files, "Object", "Hedge1").unwrap();
    let mut unit = unit_type(&files, object, Faction::Civilian, Kind::TechBuilding)
        .and_then(|unit| finish_type(&files, object, unit))
        .unwrap();
    assert!(matches!(
        unit.body_kind,
        generals_sim::skirmish::BodyKind::Inactive
    ));
    assert_eq!(unit.max_health.to_bits(), 0.0_f32.to_bits());
    assert_eq!(unit.initial_health.to_bits(), 0.0_f32.to_bits());
    assert_eq!(unit.behavior.toppling.len(), 1);
    assert_eq!(unit.death_lifecycle.as_ref().unwrap().slow.len(), 1);
    unit.behavior_modules = metadata["Hedge1"].runtime_behaviors(&files).unwrap();
    unit.update_modules = metadata["Hedge1"].runtime_registrations(true);
    generals_sim::skirmish::module_updates::validate_registrations(&unit).unwrap();
    assert!(validate_health(&unit).is_ok());
    unit.body_kind = generals_sim::skirmish::BodyKind::Active;
    assert!(validate_health(&unit).is_err());
}

fn placed_original_hedge() -> generals_sim::skirmish::Skirmish {
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
    let mut roster = vec![("Hedge1".into(), Faction::Civilian, Kind::TechBuilding)];
    let (files, metadata) = source_files::with_runtime_dependencies(&assets, &mut roster).unwrap();
    let object = definition(&files, "Object", "Hedge1").unwrap();
    if !data.types.iter().any(|unit| unit.name == "Hedge1") {
        let mut unit = unit_type(&files, object, Faction::Civilian, Kind::TechBuilding)
            .and_then(|unit| finish_type(&files, object, unit))
            .unwrap();
        unit.behavior_modules = metadata["Hedge1"].runtime_behaviors(&files).unwrap();
        unit.update_modules = metadata["Hedge1"].runtime_registrations(true);
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
        script_name: Some("NativeHedge".into()),
        name: "Hedge1".into(),
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
fn original_inactive_hedge_keeps_pristine_drawable_until_actual_direct_deletion() {
    use generals_sim::skirmish::map_scripts::{Action, Condition, Script};
    let mut game = placed_original_hedge();
    let hedge = game.named_map_object("NativeHedge").unwrap();
    game.step();
    assert!(game.entities[hedge].exists() && game.entities[hedge].has_drawable());
    assert!(game.entities[hedge].is_effectively_dead());
    assert!(!game.entities[hedge].alive());
    assert!(matches!(
        game.visibility_status(0, hedge),
        generals_sim::skirmish::visibility::Status::Clear
            | generals_sim::skirmish::visibility::Status::Partial
    ));
    assert!(crate::playtest_view::drawable_present(&game, hedge));
    assert!(
        !game
            .model_flags(&game.entities[hedge], [0.5, 0.1])
            .contains("RUBBLE")
    );
    game.install_map_scripts(vec![Script {
        owner: "player0".into(),
        name: "DeleteHedge".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::NamedDelete("NativeHedge".into())],
        false_actions: Vec::new(),
    }]);
    game.step();
    assert!(!game.entities[hedge].exists());
    assert!(!game.entities[hedge].has_drawable());
    assert!(!crate::playtest_view::drawable_present(&game, hedge));
}
