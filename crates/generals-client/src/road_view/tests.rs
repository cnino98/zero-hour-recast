use super::*;
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};

#[ignore = "requires original game installation"]
#[test]
fn current_original_maps_fit_native_road_buffers_and_have_original_mip_textures() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    for name in [
        "Flash Fire",
        "Tournament Desert",
        "Dark Mountain",
        "Tournament Lake",
        "Tournament Tundra",
        "Whiteout",
    ] {
        let map = MapData::parse(
            &assets
                .read(&format!("Maps/{name}/{name}.map"))
                .unwrap()
                .bytes,
        )
        .unwrap();
        let terrain = crate::terrain::Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
        let parts =
            load(&assets, &map.objects, &terrain).unwrap_or_else(|error| panic!("{name}: {error}"));
        let vertices: usize = parts.iter().map(|part| part.mesh.count_vertices()).sum();
        for part in &parts {
            let texture = &part.texture.as_ref().unwrap().0;
            let image = crate::source_texture::load(&assets, texture).unwrap();
            assert!(
                image.texture_descriptor.mip_level_count >= 3,
                "{name}: {texture} lacks native road mip levels"
            );
            assert_eq!(
                part.material.depth_compare,
                Some(bevy::render::render_resource::CompareFunction::GreaterEqual)
            );
            assert!(!part.material.depth_write);
        }
        eprintln!(
            "{name}: {} original road batches, {vertices} vertices",
            parts.len()
        );
    }
}
