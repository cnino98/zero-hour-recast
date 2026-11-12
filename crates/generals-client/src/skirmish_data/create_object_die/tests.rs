use super::*;
use generals_assets::{
    content,
    virtual_assets::{Asset, VirtualAssets},
};
use generals_sim::skirmish::{MapObstacle, MapWorldPlacement, NeutralObject};

fn assets() -> VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

fn killing_fields() -> generals_sim::skirmish::Skirmish {
    let root = &generals_assets::game_root().unwrap();
    let mut fixture = crate::playtest::map_fixture::Fixture::load(
        root,
        "Maps\\Killing Fields\\Killing Fields.map",
    )
    .unwrap();
    for name in ["FireHydrantRed", "AmericaVehicleScoutDrone"] {
        let index = fixture
            .data
            .types
            .iter()
            .position(|unit| unit.name == name)
            .unwrap();
        assert!(
            !fixture.data.models[index].meshes.is_empty(),
            "original {name} model"
        );
    }
    let [x, y] = fixture.layout.starts[0].position;
    for (offset, name, label) in [
        (150.0, "FireHydrantRed", "NativeHydrant"),
        (250.0, "AmericaVehicleScoutDrone", "NativeDrone"),
    ] {
        let index = fixture.layout.neutral_objects.len();
        fixture.layout.neutral_objects.push(NeutralObject {
            initial_health: None,
            attitude: None,
            recruitable: None,
            original_team: "teamPlyrCivilian".into(),
            script_name: Some(label.into()),
            name: name.into(),
            position: [x + offset, y, 0.0],
            angle: 0.0,
            script_targetable: false,
            indestructible: false,
            selectable: None,
        });
        fixture
            .layout
            .obstacle_order
            .push(MapObstacle::Neutral(index));
        fixture
            .layout
            .world_object_order
            .push(MapWorldPlacement::Neutral(index));
    }
    fixture.into_simulation().unwrap()
}

fn kill(game: &mut generals_sim::skirmish::Skirmish, name: &str) {
    use generals_sim::skirmish::map_scripts::{Action, Condition, Script};
    game.install_map_scripts(vec![Script {
        owner: "player0".into(),
        name: "NativeDeleteSource".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::NamedKill(name.into())],
        false_actions: Vec::new(),
    }]);
    game.step();
}

#[ignore = "requires original game installation"]
#[test]
fn installed_hydrant_creates_real_no_physics_model_and_lifetime_removes_it_at_five_seconds() {
    let mut game = killing_fields();
    let source = game.named_map_object("NativeHydrant").unwrap();
    let first = game.entities.len();
    kill(&mut game, "NativeHydrant");
    let id = (first..game.entities.len())
        .find(|&id| game.catalog[game.entities[id].type_index].name == "WaterSpray")
        .unwrap();
    assert_eq!(game.entities[id].producer(), Some(source));
    assert_eq!(game.entities[id].owner, game.entities[source].owner);
    assert!(
        game.catalog[game.entities[id].type_index]
            .physical
            .is_none()
    );
    assert!(
        game.catalog[game.entities[id].type_index]
            .live_physics
            .is_none()
    );
    assert!(game.entities[id].has_drawable() && game.entities[id].alive());
    let born = game.tick;
    while game.tick + 1 < born + 150 {
        game.step();
    }
    assert!(game.entities[id].exists() && game.entities[id].has_drawable());
    game.step();
    assert!(!game.entities[id].exists());
    assert!(!game.entities[id].has_drawable());
}

#[ignore = "requires original game installation"]
#[test]
fn installed_drone_mixed_ocl_preserves_object_then_debris_birth_order() {
    let mut game = killing_fields();
    let source = game.named_map_object("NativeDrone").unwrap();
    let first = game.entities.len();
    kill(&mut game, "NativeDrone");
    let created: Vec<_> = (first..game.entities.len())
        .filter(|&id| game.entities[id].producer() == Some(source))
        .collect();
    assert_eq!(
        created.len(),
        5,
        "original Scout hulk then four debris nuggets"
    );
    assert_eq!(
        game.catalog[game.entities[created[0]].type_index].name,
        "AmericaScoutDroneHulk"
    );
    assert!(
        created[1..]
            .iter()
            .all(|&id| game.catalog[game.entities[id].type_index].name == "GenericDebris")
    );
    let actual_models: Vec<_> = created[1..]
        .iter()
        .map(|&id| game.entities[id].debris_draw().unwrap().model.as_str())
        .collect();
    assert_eq!(
        actual_models,
        [
            "AVScoutDr_D2",
            "AVScoutDr_D3",
            "AVScoutDr_D3",
            "AVScoutDr_D4"
        ]
    );
    let files = crate::skirmish_data::source_files(&assets()).unwrap();
    let original =
        definition(&files, "ObjectCreationList", "OCL_AmericaScoutDroneExplode").unwrap();
    let actions = parse_list(original).unwrap();
    assert_eq!(actions.len(), 5);
    let models: Vec<_> = actions[1..]
        .iter()
        .map(|action| match action {
            super::Action::Physical(list) => list.nuggets[0].names[0].as_str(),
            super::Action::Objects(_) => panic!("original debris sequence"),
        })
        .collect();
    assert_eq!(
        models,
        [
            "AVScoutDr_D2",
            "AVScoutDr_D3",
            "AVScoutDr_D3",
            "AVScoutDr_D4"
        ]
    );
    assert!(matches!(actions[0], super::Action::Objects(_)));
    assert!(
        actions[1..]
            .iter()
            .all(|action| matches!(action, super::Action::Physical(_)))
    );
}

#[test]
fn native_nullable_lists_and_unsupported_transfer_are_explicit() {
    let parse = |fields: &str| {
        let file = content::scan(Asset {
            path: "native.ini".into(),
            origin: "CreateObjectDie field table".into(),
            bytes: format!("Object Source\nBehavior=CreateObjectDie Once\n{fields}\nEnd\nEnd\n")
                .into_bytes(),
        })
        .unwrap();
        load(std::slice::from_ref(&file), &file.declarations[0])
    };
    assert!(parse("").unwrap()[0].actions.is_empty());
    assert!(parse("CreationList=NONE").unwrap()[0].actions.is_empty());
    assert!(parse("TransferPreviousHealth=Yes").is_err());
    assert!(parse("TransferSelection=Yes").is_err());
}
