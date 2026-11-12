use super::*;
use generals_assets::virtual_assets::Asset;

fn installed() -> VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

#[ignore = "requires original game installation"]
#[test]
fn original_real_and_fake_arms_dealer_keep_static_snow_garrison_models() {
    let assets = installed();
    let files = source_files(&assets).unwrap();
    for name in ["GLAArmsDealer", "FakeGLAArmsDealer"] {
        let object = definition(&files, "Object", name).unwrap();
        let modules = blocks(&object.lines, 0, "Draw").unwrap();
        let mut draw = parse(modules[0]).unwrap();
        let mut models = Vec::new();
        let mut clips = BTreeMap::new();
        preload_draw(&assets, &mut draw, &mut models, &mut clips).unwrap();
        for (condition, model_name) in [
            ("DAMAGED", "ubarmdleg_ds"),
            ("REALLYDAMAGED", "ubarmdleg_es"),
        ] {
            let flags = [condition, "GARRISONED", "SNOW"].map(str::to_owned).into();
            let state = &draw.states[draw.select(&flags)];
            assert_eq!(state.name, model_name, "{name}: {condition}");
            assert!(state.hlod);
            let model = &models[state.model.unwrap()];
            assert!(model.hlod);
            assert!(!model.meshes.is_empty());
            assert_eq!(state.motions.len(), 1);
            assert_eq!(state.motions[0].name, format!("{model_name}.{model_name}"));
            assert!(!clips.contains_key(&state.motions[0].name));
            let bytes = assets.read(&format!("art/w3d/{model_name}.w3d")).unwrap();
            assert!(Clip::load(&bytes.bytes).unwrap().is_none());
        }
        assert!(clips.contains_key("ubarmdeal.ubarmdeal"));
        let mut animations = Animations::new(vec![vec![draw]], clips, [0.5, 0.1]);
        animations.retain_models(models, 0);
        let mut normal = crate::model_animation::Player::new([0.0; 2], 0);
        normal.state = animations.draws[0][0].select(&BTreeSet::new());
        let inspection = animations
            .inspect(&crate::model_animation::Players(vec![normal]), 0)
            .unwrap();
        assert_eq!(inspection.len(), 1);
        assert_eq!(inspection[0].clip, "ubarmdeal.ubarmdeal");
        for condition in ["DAMAGED", "REALLYDAMAGED"] {
            let flags = [condition, "GARRISONED", "SNOW"].map(str::to_owned).into();
            let draw = &animations.draws[0][0];
            let state_index = draw.select(&flags);
            let state = &draw.states[state_index];
            let model = &animations.models[state.model.unwrap()];
            let expected = model.controlled_pose(None, 0.0, &[]).unwrap();
            let mut player = crate::model_animation::Player::new([0.0; 2], 0);
            player.state = state_index;
            let pose = crate::model_animation::world_pose(
                &mut player,
                state,
                &animations.models,
                &animations.clips,
            )
            .unwrap();
            assert!(!pose.is_empty());
            assert_eq!(pose.len(), expected.len());
            for (actual, expected) in pose.iter().zip(&expected) {
                assert_eq!(actual.visible, expected.visible);
                assert_eq!(
                    actual
                        .positions
                        .iter()
                        .map(|p| p.map(f32::to_bits))
                        .collect::<Vec<_>>(),
                    expected
                        .positions
                        .iter()
                        .map(|p| p.map(f32::to_bits))
                        .collect::<Vec<_>>(),
                );
            }
            player.model = state.model;
            assert!(
                crate::model_animation::world_pose(
                    &mut player,
                    state,
                    &animations.models,
                    &animations.clips,
                )
                .is_none()
            );
            let players = crate::model_animation::Players(vec![player]);
            assert!(animations.inspect(&players, 0).unwrap().is_empty());
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn shared_render_object_rejects_mixed_real_and_missing_handles() {
    let assets = installed();
    for turret_changes in [false, true] {
        let turret = if turret_changes {
            "Turret=OtherTurret\n"
        } else {
            ""
        };
        let file = content::scan(Asset {
            path: "bounded.ini".into(),
            origin: "native retained RenderObj and null Get_HAnim contract".into(),
            bytes: format!(
                "Object Bounded\nDraw=W3DModelDraw Draw\nDefaultConditionState\nModel=UBArmDeal\nAnimation=UBArmDeal.UBArmDeal\nEnd\nConditionState=DAMAGED\nModel=UBArmDeal\nAnimation=UBArmDlEG_DS.UBArmDlEG_DS\n{turret}End\nEnd\nEnd\n"
            ).into_bytes(),
        }).unwrap();
        let modules = blocks(&file.declarations[0].lines, 0, "Draw").unwrap();
        let mut draw = parse(modules[0]).unwrap();
        let result = preload_draw(&assets, &mut draw, &mut Vec::new(), &mut BTreeMap::new());
        if turret_changes {
            result.unwrap();
            assert_ne!(draw.states[0].render_model, draw.states[1].render_model);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("mixes valid and unavailable animations")
            );
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn one_state_cannot_choose_between_real_and_missing_handles() {
    let assets = installed();
    let file = content::scan(Asset {
        path: "bounded.ini".into(),
        origin: "native retained RenderObj and null Get_HAnim contract".into(),
        bytes: b"Object Bounded\nDraw=W3DModelDraw Draw\nDefaultConditionState\nModel=UBArmDeal\nIdleAnimation=UBArmDeal.UBArmDeal\nIdleAnimation=UBArmDlEG_DS.UBArmDlEG_DS\nEnd\nEnd\nEnd\n".to_vec(),
    }).unwrap();
    let modules = blocks(&file.declarations[0].lines, 0, "Draw").unwrap();
    let mut draw = parse(modules[0]).unwrap();
    let error =
        preload_draw(&assets, &mut draw, &mut Vec::new(), &mut BTreeMap::new()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mixes valid and unavailable animations")
    );
}

#[ignore = "requires original game installation"]
#[test]
fn original_palace_mismatched_export_is_a_null_handle_on_its_distinct_model() {
    let assets = installed();
    let files = source_files(&assets).unwrap();
    let object = definition(&files, "Object", "GLAPalace").unwrap();
    let modules = blocks(&object.lines, 0, "Draw").unwrap();
    let mut draw = parse(modules[0]).unwrap();
    let mut models = Vec::new();
    let mut clips = BTreeMap::new();
    preload_draw(&assets, &mut draw, &mut models, &mut clips).unwrap();
    let flags = ["DAMAGED", "USER_1", "SNOW", "GARRISONED"]
        .map(str::to_owned)
        .into();
    let state = &draw.states[draw.select(&flags)];
    assert_eq!(state.name, "ubpalaceegx_ds");
    assert!(state.hlod);
    assert!(!models[state.model.unwrap()].meshes.is_empty());
    assert_eq!(state.motions.len(), 1);
    assert_eq!(state.motions[0].name, "ubpalaceegx_dsg.ubpalaceegx_ds");
    assert!(!clips.contains_key(&state.motions[0].name));
    let bytes = assets.read("art/w3d/ubpalaceegx_ds.w3d").unwrap();
    let exported = Clip::load(&bytes.bytes).unwrap().unwrap();
    assert!(
        exported
            .name
            .eq_ignore_ascii_case("ubpalaceegx_ds.ubpalaceegx_ds")
    );
    assert!(!exported.name.eq_ignore_ascii_case(&state.motions[0].name));
    assert!(clips.contains_key("ubpalace.ubpalace"));
    assert!(
        draw.states
            .iter()
            .filter(|other| other.render_model == state.render_model)
            .flat_map(|other| &other.motions)
            .all(|motion| !clips.contains_key(&motion.name))
    );
}

#[ignore = "requires original game installation"]
#[test]
fn mismatched_export_cannot_replace_a_valid_retained_animation() {
    let assets = installed();
    let file = content::scan(Asset {
        path: "bounded.ini".into(),
        origin: "native exact HAnim lookup and retained RenderObj".into(),
        bytes: b"Object Bounded\nDraw=W3DModelDraw Draw\nDefaultConditionState\nModel=UBArmDeal\nAnimation=UBArmDeal.UBArmDeal\nEnd\nConditionState=DAMAGED\nModel=UBArmDeal\nAnimation=WrongHierarchy.UBArmDeal\nEnd\nEnd\nEnd\n".to_vec(),
    }).unwrap();
    let modules = blocks(&file.declarations[0].lines, 0, "Draw").unwrap();
    let mut draw = parse(modules[0]).unwrap();
    let error =
        preload_draw(&assets, &mut draw, &mut Vec::new(), &mut BTreeMap::new()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mixes valid and unavailable animations")
    );
}

#[ignore = "requires original game installation"]
#[test]
fn original_repair_bay_missing_night_snow_asset_keeps_both_handles_null() {
    let assets = installed();
    let files = source_files(&assets).unwrap();
    let object = definition(&files, "Object", "TechRepairbay").unwrap();
    let modules = blocks(&object.lines, 0, "Draw").unwrap();
    let mut draw = parse(modules[0]).unwrap();
    let mut models = Vec::new();
    let mut clips = BTreeMap::new();
    preload_draw(&assets, &mut draw, &mut models, &mut clips).unwrap();
    let flags = ["NIGHT", "SNOW"].map(str::to_owned).into();
    let state = &draw.states[draw.select(&flags)];
    assert_eq!(state.name, "zbrprbay_ns");
    assert!(state.model.is_none());
    assert!(state.render_model.is_none());
    assert!(!state.hlod);
    assert_eq!(state.motions.len(), 1);
    assert_eq!(state.motions[0].name, "zbrprbay_ns.zbrprbay_ns");
    assert!(!clips.contains_key(&state.motions[0].name));
    assert!(matches!(
        assets.read("art/w3d/zbrprbay_ns.w3d"),
        Err(AssetError::Missing(path)) if path == "art/w3d/zbrprbay_ns.w3d"
    ));
    let normal = &draw.states[draw.select(&BTreeSet::new())];
    assert_eq!(normal.name, "zbrprbay");
    assert!(!models[normal.model.unwrap()].meshes.is_empty());
    assert!(clips.contains_key("zbrprbay.zbrprbay"));
}

#[ignore = "requires original game installation"]
#[test]
fn absent_file_cannot_replace_a_valid_retained_animation() {
    let assets = installed();
    let file = content::scan(Asset {
        path: "bounded.ini".into(),
        origin: "native absent HAnim file and retained RenderObj".into(),
        bytes: b"Object Bounded\nDraw=W3DModelDraw Draw\nDefaultConditionState\nModel=UBArmDeal\nAnimation=UBArmDeal.UBArmDeal\nEnd\nConditionState=DAMAGED\nModel=UBArmDeal\nAnimation=ZBRprBay_NS.ZBRprBay_NS\nEnd\nEnd\nEnd\n".to_vec(),
    }).unwrap();
    let modules = blocks(&file.declarations[0].lines, 0, "Draw").unwrap();
    let mut draw = parse(modules[0]).unwrap();
    let error =
        preload_draw(&assets, &mut draw, &mut Vec::new(), &mut BTreeMap::new()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mixes valid and unavailable animations")
    );
}

#[test]
fn animation_file_absence_is_distinct_from_io_and_malformed_payloads() {
    use generals_assets::virtual_assets::Layer;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    let root = std::env::temp_dir().join("generals-client-tests");
    fs::create_dir_all(&root).unwrap();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = root.join(format!("{}-{unique}", std::process::id()));
    fs::create_dir(&path).unwrap();
    fs::create_dir_all(path.join("art/w3d")).unwrap();
    let assets = VirtualAssets::from_layers(vec![Layer::Loose(path.clone())]);
    assert!(load_motion(&assets, "absent.absent").unwrap().is_none());
    fs::write(path.join("art/w3d/bad.w3d"), [0_u8; 3]).unwrap();
    let malformed = load_motion(&assets, "bad.bad");
    assert!(malformed.is_err());
    assert!(assets.read("art/w3d/bad.w3d").is_ok());
    fs::remove_dir_all(&path).unwrap();
    let Err(io) = load_motion(&assets, "absent.absent") else {
        panic!("removed source directory must remain an I/O error");
    };
    assert!(matches!(
        io.downcast_ref::<AssetError>(),
        Some(AssetError::Io(_))
    ));
}
