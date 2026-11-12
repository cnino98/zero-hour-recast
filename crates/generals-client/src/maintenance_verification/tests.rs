use super::*;
use crate::{model_animation, projectile_data, skirmish_data, terrain::Terrain, world_data};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};

#[ignore = "requires original game installation"]
#[test]
fn paid_gla_repair_then_defeating_worker_allows_free_hole_reconstruction() {
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
    game.reset(Faction::Gla, Faction::Usa).unwrap();
    let builder = friendly_entity(&game, Kind::Builder, 0).unwrap();
    let barracks = construct(&mut game, builder, Kind::Barracks, 0).unwrap();
    // Rebel is the first available infantry command on the original GLA Barracks.
    let infantry =
        crate::skirmish_verification::produce(&mut game, barracks, Kind::Infantry, 0).unwrap();
    assert_eq!(
        game.catalog[game.entities[infantry].type_index].name,
        "GLAInfantryRebel"
    );
    research(&mut game, barracks, "Upgrade_InfantryCaptureBuilding").unwrap();
    scout_oil(&mut game, infantry).unwrap();
    let attacker = enemy_infantry(&mut game).unwrap();
    repair_target(&mut game, builder, attacker, barracks).unwrap();
    assert!(game.entities[builder].alive());
    assert_eq!(
        game.entities[barracks].health.to_bits(),
        game.entities[barracks].max_health.to_bits()
    );
    let original_type = game.entities[barracks].type_index;
    let cash = game.players[0].cash;
    rebuild(&mut game, builder, attacker, barracks).unwrap();
    assert!(!game.entities[builder].alive());
    assert!(!game.entities[barracks].alive());
    assert!(game.entities[attacker].alive());
    assert_eq!(game.players[0].cash, cash);
    assert!(game.entities.iter().enumerate().any(|(id, actor)| {
        id != barracks && actor.owner == 0 && actor.type_index == original_type && actor.complete()
    }));
}
