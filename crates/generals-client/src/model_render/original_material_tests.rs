//! Original WW3D passes retain authored textures, UVs and native blending.
use super::*;
use generals_assets::virtual_assets::VirtualAssets;

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
fn installed_town_house_disabled_detail_stage_preserves_primary_uvs_and_glow_pass() {
    let mut model = Model::load(&assets(), "cbtownhs01_en").unwrap();
    let mesh_index = model
        .meshes
        .iter()
        .position(|mesh| mesh.name == "CBTOWNHS01_EN.CBTOWNHS01")
        .unwrap();
    let source = &model.meshes[mesh_index];
    let materials = source.materials.as_ref().unwrap();
    assert_eq!(materials.passes.len(), 2);
    assert_eq!(materials.passes[0].stages.len(), 2);
    assert_eq!(materials.passes[0].stages[1].texture_ids, [0]);
    assert!(
        materials
            .shaders
            .iter()
            .all(|shader| shader[9] == 0 && shader[10] == 0)
    );
    let prepared = parts(&model).unwrap();
    let target: Vec<_> = prepared
        .iter()
        .filter(|part| part.binding.as_ref().unwrap().mesh == mesh_index)
        .collect();
    assert_eq!(target.len(), 2);
    for part in &target {
        let binding = part.binding.as_ref().unwrap();
        let pass = &materials.passes[binding.pass];
        assert!(pass.stages[0].face_coordinate_ids.is_none());
        let expected: Vec<_> = binding
            .vertices
            .iter()
            .map(|&vertex| pass.stages[0].coordinates[vertex])
            .collect();
        let Some(VertexAttributeValues::Float32x2(actual)) =
            part.mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("primary texture UVs missing");
        };
        assert_eq!(actual, &expected);
        let texture_id = usize::try_from(pass.stages[0].texture_ids[0]).unwrap();
        assert_eq!(
            part.texture.as_ref().unwrap().0,
            materials.textures[texture_id].name.to_ascii_lowercase()
        );
    }
    assert!(
        target
            .iter()
            .any(|part| matches!(part.material.alpha_mode, AlphaMode::Opaque))
    );
    assert!(
        target
            .iter()
            .any(|part| matches!(part.material.alpha_mode, AlphaMode::Add))
    );
    // The same original second stage becomes unsupported if either native
    // combiner is active. A color-only or alpha-only operation must not vanish.
    for channel in [9, 10] {
        model.meshes[mesh_index].materials.as_mut().unwrap().shaders[0][channel] = 2;
        let error = parts(&model)
            .err()
            .expect("active detail stage must remain gated");
        assert!(
            error
                .to_string()
                .contains("unsupported second texture stage")
        );
        model.meshes[mesh_index].materials.as_mut().unwrap().shaders[0][channel] = 0;
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_cairo_pyramid_retains_native_zero_source_color_destination_pass() {
    let model = Model::load(&assets(), "cmsmpyra").unwrap();
    let materials = model.meshes[0].materials.as_ref().unwrap();
    assert_eq!(materials.shaders[1][7], 0);
    assert_eq!(materials.shaders[1][3], 2);
    let prepared = parts(&model).unwrap();
    let pass = prepared
        .iter()
        .find(|part| part.binding.as_ref().unwrap().pass == 1)
        .unwrap();
    assert!(matches!(pass.material.alpha_mode, AlphaMode::Multiply));
    assert!(!pass.material.depth_write);
    assert_eq!(pass.material.parameters.primary_gradient, 0);
    assert_eq!(
        pass.material.parameters.alpha_test.to_bits(),
        (-1.0_f32).to_bits()
    );
    let texture = &materials.textures[1];
    assert_eq!(
        pass.texture.as_ref().unwrap().0,
        texture.name.to_ascii_lowercase()
    );
    crate::source_texture::load_mesh(&assets(), &texture.name).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
fn installed_hangar_active_add_detail_remains_explicitly_unsupported() {
    let model = Model::load(&assets(), "cbhangr03").unwrap();
    let mesh = model
        .meshes
        .iter()
        .find(|mesh| mesh.name == "CBHANGR03.LIGHTG")
        .unwrap();
    let materials = mesh.materials.as_ref().unwrap();
    assert_eq!(materials.passes[0].stages.len(), 2);
    assert_eq!(materials.shaders[0][9], 4);
    assert_eq!(materials.shaders[0][10], 0);
    let error = parts(&model)
        .err()
        .expect("original active detail remains gated");
    assert_eq!(
        error.to_string(),
        "CBHANGR03.LIGHTG uses an unsupported second texture stage"
    );
}
