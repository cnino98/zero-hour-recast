use crate::{model_animation, projectile_data, skirmish_data, terrain::Terrain, world_data};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};

#[ignore = "requires original game installation"]
#[test]
fn paid_eight_hackers_keep_independent_native_contained_payment_timers() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = MapData::parse(&assets.read(crate::playtest::MAP_NAME).unwrap().bytes).unwrap();
    let mut data = skirmish_data::load(&assets, &map.objects).unwrap();
    let settings = world_data::camera_settings(&assets, root).unwrap();
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let (layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    let animations = model_animation::load(&assets, &mut data.models, &data.types).unwrap();
    let projectiles =
        projectile_data::load(&assets, &mut data.types, &data.models, &animations).unwrap();
    let movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    let mut game = data.into_simulation(movement, layout).unwrap();
    terrain
        .configure_projectiles(&mut game, projectiles.partition_cell_size)
        .unwrap();
    super::verify(&mut game).unwrap();
}
