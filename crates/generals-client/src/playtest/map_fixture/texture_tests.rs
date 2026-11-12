//! Actual mesh references preserve the native missing-surface policy.
use super::*;
use bevy::{image::ImageSampler, render::render_resource::TextureFormat};
use generals_assets::{AssetError, virtual_assets::VirtualAssets, w3d::Model};

#[ignore = "requires original game installation"]
#[test]
fn installed_housecolor_snow_mesh_binds_native_missing_texture_without_alias() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let model = Model::load(&assets, "nbptower_s").unwrap();
    let parts = crate::model_render::parts(&model).unwrap();
    let texture = parts
        .iter()
        .filter_map(|part| part.texture.as_ref())
        .find(|(name, _)| name.eq_ignore_ascii_case("Housecolor2_S.tga"))
        .unwrap();
    for extension in ["dds", "tga"] {
        assert!(matches!(
            assets.read(&format!("art/textures/housecolor2_s.{extension}")),
            Err(AssetError::Missing(_))
        ));
    }
    assert!(crate::source_texture::load(&assets, &texture.0).is_err());
    assert!(crate::source_texture::load(&assets, "housecolor2.dds").is_ok());
    let image = crate::source_texture::load_mesh(&assets, &texture.0).unwrap();
    assert_eq!(image.width(), 128);
    assert_eq!(image.height(), 128);
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
    assert_eq!(image.texture_descriptor.mip_level_count, 8);
    let bytes = image.data.as_ref().unwrap();
    let mut offset = 0;
    for level in 0..8 {
        let extent = 128 >> level;
        let length = extent * extent * 4;
        let pixels = &bytes[offset..offset + length];
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 255, 127])
        );
        assert_eq!(
            &pixels[length - extent * 4..length],
            [255, 0, 255, 127].repeat(extent)
        );
        offset += length;
    }
    assert_eq!(offset, bytes.len());
    let ImageSampler::Descriptor(sampler) = image.sampler else {
        panic!("native mesh sampler");
    };
    assert_eq!(
        sampler.address_mode_u,
        bevy::image::ImageAddressMode::Repeat
    );
    assert_eq!(
        sampler.address_mode_v,
        bevy::image::ImageAddressMode::Repeat
    );
    assert_eq!(sampler.mag_filter, bevy::image::ImageFilterMode::Linear);
    assert_eq!(sampler.min_filter, bevy::image::ImageFilterMode::Linear);
    assert_eq!(sampler.mipmap_filter, bevy::image::ImageFilterMode::Linear);
}

#[ignore = "requires original game installation"]
#[test]
fn installed_tournament_desert_mesh_texture_closure_keeps_absent_name_inventory() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut fixture = Fixture::load(root, r"Maps\Tournament Desert\Tournament Desert.map").unwrap();
    let _effects = super::super::load_unit_effects(&assets, &mut fixture.data).unwrap();
    let mut textures = std::collections::BTreeSet::new();
    for model in &fixture.data.models {
        for part in crate::model_render::parts(model).unwrap() {
            if let Some((name, _)) = part.texture {
                textures.insert(name);
            }
        }
    }
    let mut missing = Vec::new();
    for name in &textures {
        crate::source_texture::load_mesh(&assets, name).unwrap();
        // The successful mesh load above excludes decode/I/O errors; a strict
        // load error for the same immutable assets therefore proves absence.
        if crate::source_texture::load(&assets, name).is_err() {
            missing.push(name.as_str());
        }
    }
    assert!(missing.contains(&"housecolor2_s.tga"));
    eprintln!(
        "Actual Tournament Desert mesh material closure: {} unique textures; native missing names={missing:?}",
        textures.len()
    );
}
