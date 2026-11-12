//! Original native empty/default/null and meshless HLOD consumers.
use super::*;

fn assets() -> VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

fn declaration(body: &str) -> Declaration {
    content::scan(generals_assets::virtual_assets::Asset {
        path: "native-empty-model.ini".into(),
        origin: "fixture".into(),
        bytes: format!("Object NativeModelFixture\n{body}\nEnd\n").into_bytes(),
    })
    .unwrap()
    .declarations
    .remove(0)
}

#[ignore = "requires original game installation"]
#[test]
fn default_and_none_draws_have_no_geometry_but_null_keeps_its_native_prototype_bounds() {
    let assets = assets();
    for draw in [
        "Draw = W3DDefaultDraw Tag\nEnd",
        "Draw = W3DModelDraw Tag\nConditionState = NONE\nModel = NONE\nEnd\nEnd",
    ] {
        let model = normal_model(&assets, &declaration(draw)).unwrap();
        assert!(model.meshes.is_empty());
        assert!(model.hierarchy.is_none());
        assert!(model.scene_sphere.is_none());
        assert_eq!(
            model.scene_bounds.min.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
    }
    let model = normal_model(
        &assets,
        &declaration("Draw = W3DModelDraw Tag\nConditionState = NONE\nModel = nUlL\nEnd\nEnd"),
    )
    .unwrap();
    assert!(model.meshes.is_empty());
    assert!(model.hierarchy.is_none());
    assert_eq!(
        model.scene_bounds.min.map(f32::to_bits),
        [-0.1_f32; 3].map(f32::to_bits)
    );
    assert_eq!(
        model.scene_bounds.max.map(f32::to_bits),
        [0.1_f32; 3].map(f32::to_bits)
    );
    assert_eq!(
        model.scene_sphere.unwrap().radius.to_bits(),
        0.1_f32.to_bits()
    );
    assert!(
        normal_model(
            &assets,
            &declaration("Draw = W3DDefaultDraw Tag\nModel = Missing\nEnd")
        )
        .is_err()
    );
    assert!(
        normal_model(
            &assets,
            &declaration(
                "Draw = W3DModelDraw Tag\nConditionState = DAMAGED\nModel = NULL\nEnd\nEnd"
            )
        )
        .is_err()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_invisible_objects_and_mine_group_use_original_default_and_meshless_models() {
    let assets = assets();
    let (files, _) = source_files::effective(
        &assets,
        &[
            "TNTStickyBomb",
            "BoobyTrap",
            "NeutronBlastObject",
            "ChinaEMPMine",
        ],
    )
    .unwrap();
    for name in ["TNTStickyBomb", "BoobyTrap", "NeutronBlastObject"] {
        let object = definition(&files, "Object", name).unwrap();
        let draw = blocks(&object.lines, 0, "Draw").unwrap();
        assert_eq!(draw[0][0].tokens[1], "W3DDefaultDraw");
        let model = normal_model(&assets, object).unwrap();
        assert!(model.meshes.is_empty(), "{name}");
        assert!(model.scene_sphere.is_none(), "{name}");
    }
    let object = definition(&files, "Object", "ChinaEMPMine").unwrap();
    let model = normal_model(&assets, object).unwrap();
    assert!(model.meshes.is_empty());
    assert!(model.hierarchy.is_some());
    assert_eq!(model.public_bone_indices("Mine").len(), 8);
    assert!(model.bone_position("Mine01").is_some());
    assert!(
        model
            .scene_bounds
            .max
            .iter()
            .zip(model.scene_bounds.min)
            .any(|(hi, lo)| *hi > lo)
    );
    assert_eq!(
        model.bounds.min.map(f32::to_bits),
        model.scene_bounds.min.map(f32::to_bits)
    );
    assert_eq!(
        model.bounds.max.map(f32::to_bits),
        model.scene_bounds.max.map(f32::to_bits)
    );
}
