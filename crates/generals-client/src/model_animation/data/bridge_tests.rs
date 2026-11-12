use super::*;
use generals_assets::{map::MapData, virtual_assets::Asset};

#[ignore = "requires original game installation"]
#[test]
fn installed_laser_draws_use_their_beam_consumer_without_model_states() {
    let (assets, battle, animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let catalog = &battle.simulation.catalog;
    let laser = catalog
        .iter()
        .position(|unit| unit.name == "LaserBeam")
        .unwrap();
    assert!(catalog[laser].laser_update.is_some());
    for (index, unit) in catalog.iter().enumerate() {
        if unit.laser_update.is_some() {
            assert!(animations.draws[index].is_empty(), "{}", unit.name);
        }
    }
    crate::laser_beam::load(&assets, catalog).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
fn original_final_crusade_and_golden_bridges_have_no_condition_draws() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let settings = crate::world_data::camera_settings(&assets, root).unwrap();
    for (path, count) in [
        ("maps/final crusade/final crusade.map", 2),
        ("maps/golden oasis/golden oasis.map", 3),
    ] {
        let map = MapData::parse(&assets.read(path).unwrap().bytes).unwrap();
        let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
        let index = data
            .types
            .iter()
            .position(|unit| unit.name == "GenericBridge")
            .unwrap();
        assert!(data.models[index].meshes.is_empty());
        let animations = load(&assets, &mut data.models, &data.types).unwrap();
        assert!(animations.draws[index].is_empty(), "{path}");
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
            crate::world_data::install_bridges(&assets, &map.objects, &terrain, &mut movement)
                .unwrap();
        assert_eq!(bridges.len(), count);
    }
}

#[test]
fn empty_default_draw_uses_its_real_module_contract_for_every_template() {
    for (name, body, accepted) in [
        (
            "GenericBridge",
            "Draw = W3DDefaultDraw Tag\nEnd",
            Some(true),
        ),
        (
            "GenericBridge",
            "Draw = W3DDefaultDraw Tag\nModel = Test\nEnd",
            None,
        ),
        ("GenericBridge", "Draw = W3DModelDraw Tag\nEnd", Some(false)),
        (
            "GenericBridge",
            "Draw = W3DDefaultDraw A\nEnd\nDraw = W3DDefaultDraw B\nEnd",
            None,
        ),
        ("Other", "Draw = W3DDefaultDraw Tag\nEnd", Some(true)),
    ] {
        let file = content::scan(Asset {
            path: "bridge.ini".into(),
            origin: "original empty draw contract".into(),
            bytes: format!("Object {name}\n{body}\nEnd\n").into_bytes(),
        })
        .unwrap();
        let draws = blocks(&file.declarations[0].lines, 0, "Draw").unwrap();
        if let Some(accepted) = accepted {
            assert_eq!(external_draw(name, &draws).unwrap(), accepted);
            if !accepted {
                assert!(parse(draws[0]).is_err());
            }
        } else {
            assert!(external_draw(name, &draws).is_err());
        }
    }
}
