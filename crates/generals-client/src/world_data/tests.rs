use super::{Shape, map_footprint};
use generals_assets::{content, virtual_assets::Asset};

#[test]
fn map_footprints_preserve_inherited_geometry_and_original_sphere_default() {
    let file = content::scan(Asset {
        path: "map.ini".into(),
        origin: "Original map geometry inheritance fixture".into(),
        bytes: b"Object Round\nGeometry=CYLINDER\nGeometryMajorRadius=22\nGeometryMinorRadius=3\nEnd\nObjectReskin RoundSkin Round\nGeometryMajorRadius=26\nEnd\nObjectReskin BoxSkin Round\nGeometry=BOX\nEnd\nObject Default\nEnd\n".to_vec(),
    })
    .unwrap();
    let parent = &file.declarations[0];
    let round = map_footprint(&[&file.declarations[1], parent], [0.0; 2], 0.0).unwrap();
    assert!(matches!(round.shape, Shape::Cylinder));
    assert_eq!(round.major.to_bits(), 26.0_f32.to_bits());
    assert_eq!(round.minor.to_bits(), 3.0_f32.to_bits());
    let rectangle = map_footprint(&[&file.declarations[2], parent], [0.0; 2], 0.0).unwrap();
    assert!(matches!(rectangle.shape, Shape::Box));
    let fallback = map_footprint(&[&file.declarations[3]], [0.0; 2], 0.0).unwrap();
    assert!(matches!(fallback.shape, Shape::Sphere));
}

#[test]
fn map_admission_distinguishes_bridge_layers_roads_and_inherited_gameplay() {
    let file = content::scan(Asset {
        path: "map.ini".into(),
        origin: "Original placed-object admission fixture".into(),
        bytes: b"Object Defense\nKindOf=STRUCTURE CAN_ATTACK\nEnd\nObjectReskin DefenseSkin Defense\nEnd\nObject Tree\nKindOf=SHRUBBERY\nEnd\n".to_vec(),
    }).unwrap();
    let definitions = file
        .declarations
        .into_iter()
        .map(|object| (object.name.clone(), object))
        .collect();
    let mut record = generals_assets::map::MapObject {
        template: "Tree".into(),
        position: [0.0; 3],
        angle: 0.0,
        flags: 0,
        properties: std::collections::BTreeMap::new(),
    };
    assert!(super::validate_map_objects(std::slice::from_ref(&record), &definitions, &[]).is_ok());
    record.template = "DefenseSkin".into();
    assert!(
        super::validate_map_objects(std::slice::from_ref(&record), &definitions, &[])
            .unwrap_err()
            .to_string()
            .contains("gameplay object")
    );
    record.template = "OriginalRoad".into();
    record.flags = 0x2;
    assert!(super::validate_map_objects(std::slice::from_ref(&record), &definitions, &[]).is_ok());
    // Object admission defers bridge markers to source endpoint pairing and
    // original model geometry binding, tested against installed maps separately.
    for flags in [0x10, 0x20] {
        record.flags = flags;
        assert!(
            super::validate_map_objects(std::slice::from_ref(&record), &definitions, &[]).is_ok()
        );
    }
    record.flags = 0;
    record
        .properties
        .insert("scorchType".into(), generals_assets::map::MapValue::Int(2));
    assert!(super::validate_map_objects(std::slice::from_ref(&record), &definitions, &[]).is_ok());
    record.properties.insert(
        "scorchType".into(),
        generals_assets::map::MapValue::Ascii("2".into()),
    );
    assert!(
        super::validate_map_objects(&[record], &definitions, &[])
            .unwrap_err()
            .to_string()
            .contains("Missing placed Object")
    );
}

#[ignore = "requires original game installation"]
#[test]
fn prop_direct_model_name_uses_an_original_tree_models_native_unscaled_sphere() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = generals_assets::map::MapData::parse(
        &assets.read("maps/flash fire/flash fire.map").unwrap().bytes,
    )
    .unwrap();
    let definitions = super::object_definitions(&assets).unwrap();
    let original = map
        .objects
        .iter()
        .filter_map(|record| definitions.get(&record.template))
        .find(|object| {
            crate::skirmish_data::blocks(&object.lines, 0, "Draw")
                .unwrap()
                .iter()
                .any(|draw| {
                    draw[0]
                        .tokens
                        .get(1)
                        .is_some_and(|module| module == "W3DTreeDraw")
                })
        })
        .unwrap();
    let tree_draws = crate::skirmish_data::blocks(&original.lines, 0, "Draw").unwrap();
    let tree_draw = tree_draws
        .iter()
        .find(|draw| {
            draw[0]
                .tokens
                .get(1)
                .is_some_and(|module| module == "W3DTreeDraw")
        })
        .unwrap();
    let model_name = crate::skirmish_data::text(tree_draw, 1, "ModelName").unwrap();
    let original_model = crate::skirmish_data::normal_draw_model(&assets, tree_draw).unwrap();
    let native_sphere = original_model.scene_sphere.as_ref().unwrap();
    assert!(native_sphere.radius > 0.0);
    // No original map places W3DPropDraw. Exercise its direct ModelName contract
    // with original bytes, without substituting a mesh or approximating bounds.
    let file = content::scan(Asset {
        path: "prop-draw-contract.ini".into(),
        origin: "W3DPropDraw parser contract".into(),
        bytes: format!(
            "Object Prop\nDraw = W3DPropDraw ModuleTag_01\nModelName = {model_name}\nEnd\nEnd\n"
        )
        .into_bytes(),
    })
    .unwrap();
    let object = &file.declarations[0];
    let mut models = Vec::new();
    let draws = super::map_models(&assets, object, object, false, &mut models).unwrap();
    assert_eq!(draws.len(), 1);
    assert!(!draws[0].tree);
    assert_eq!(
        draws[0].prop_radius.unwrap().to_bits(),
        native_sphere.radius.to_bits()
    );
    assert_eq!(
        models[0]
            .scene_sphere
            .as_ref()
            .unwrap()
            .center
            .map(f32::to_bits),
        native_sphere.center.map(f32::to_bits)
    );
}

#[ignore = "requires original game installation"]
#[test]
fn original_bridge_endpoints_are_metadata_without_ordinary_object_templates() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let definitions = super::object_definitions(&assets).unwrap();
    for (path, flags, pair_template, pairs, orphans) in [
        (
            "maps/golden oasis/golden oasis.map",
            [0x10, 0x20, 0x10, 0x20, 0x10, 0x20, 0x50],
            "KandaharBridge",
            3,
            1,
        ),
        (
            "maps/final crusade/final crusade.map",
            [0x10, 0x20, 0x10, 0x20, 0x20, 0x10, 0x10],
            "EuropeanBridge",
            2,
            3,
        ),
    ] {
        let map = generals_assets::map::MapData::parse(&assets.read(path).unwrap().bytes).unwrap();
        // W3DBridgeBuffer::loadBridges requires consecutive serialized POINT1,
        // POINT2 records; the other original markers remain unpaired metadata.
        let actual_pairs: Vec<_> = map
            .objects
            .windows(2)
            .filter(|pair| pair[0].flags & 0x10 != 0 && pair[1].flags & 0x20 != 0)
            .collect();
        assert_eq!(actual_pairs.len(), pairs, "{path}");
        assert!(
            actual_pairs
                .iter()
                .all(|pair| pair[0].template == pair_template && pair[1].template == pair_template)
        );
        let paired_markers = actual_pairs.len() * 2;
        let endpoints: Vec<_> = map
            .objects
            .into_iter()
            .filter(|record| record.flags & 0x30 != 0)
            .collect();
        assert_eq!(
            endpoints
                .iter()
                .map(|record| record.flags)
                .collect::<Vec<_>>(),
            flags,
            "{path}"
        );
        assert_eq!(endpoints.len() - paired_markers, orphans, "{path}");
        assert!(
            endpoints
                .iter()
                .all(|record| !definitions.contains_key(&record.template))
        );
        super::validate_map_objects(&endpoints, &definitions, &[]).unwrap();
    }
}

#[ignore = "requires original game installation"]
#[test]
fn original_eastern_stone_arch_heights_survive_constructor_and_restart() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = generals_assets::map::MapData::parse(
        &assets
            .read("maps/eastern everglades/eastern everglades.map")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let expected: Vec<_> = map
        .objects
        .iter()
        .filter(|record| record.template == "StoneArch01")
        .map(|record| record.position)
        .collect();
    assert_eq!(
        expected
            .iter()
            .map(|position| position[2].to_bits())
            .collect::<Vec<_>>(),
        [-5.0_f32, -4.0, 2.0, 0.0].map(f32::to_bits)
    );
    let settings = super::camera_settings(&assets, root).unwrap();
    let terrain = crate::terrain::Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
    let (layout, _) = super::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    let mut game = data.into_simulation(movement, layout).unwrap();
    for restart in [false, true] {
        if restart {
            game.reset(
                generals_sim::skirmish::Faction::Usa,
                generals_sim::skirmish::Faction::Gla,
            )
            .unwrap();
        }
        let arches: Vec<_> = game
            .entities
            .iter()
            .enumerate()
            .filter(|(_, actor)| game.catalog[actor.type_index].name == "StoneArch01")
            .collect();
        assert_eq!(arches.len(), 4);
        for ((id, actor), position) in arches.iter().zip(&expected) {
            assert_eq!(
                actor.position.map(f32::to_bits),
                [position[0], position[1]].map(f32::to_bits)
            );
            let ground = game.ground_height(actor.position);
            assert_eq!(
                game.object_position(*id)[2].to_bits(),
                (ground + position[2]).to_bits()
            );
            assert_eq!(
                actor.placed_height_offset().unwrap().to_bits(),
                position[2].to_bits()
            );
        }
    }
}
