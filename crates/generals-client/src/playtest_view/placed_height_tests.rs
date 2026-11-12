//! Original Rocky Rampage placement through catalog, map admission and draw coordinates.
use super::*;
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::Faction;

#[ignore = "requires original game installation"]
#[test]
fn rocky_rampage_garage_keeps_authored_negative_height_in_simulation_and_draw_position() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = MapData::parse(
        &assets
            .read("Maps\\Rocky Rampage\\Rocky Rampage.map")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let record = map
        .objects
        .iter()
        .find(|record| {
            record.template == "EuropeanParkingGarage"
                && record.position[2].to_bits() == (-2.0_f32).to_bits()
        })
        .unwrap();
    let original = record.position;
    let angle = record.angle;
    let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
    let terrain = Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
    let (layout, _) = crate::world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    let partition_cell_size = layout.partition_cell_size;
    let movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    let mut game = data.into_simulation(movement, layout).unwrap();
    terrain
        .configure_projectiles(&mut game, partition_cell_size)
        .unwrap();
    for _ in 0..2 {
        game.reset_sandbox(Faction::Usa).unwrap();
        let (id, actor) = game
            .entities
            .iter()
            .enumerate()
            .find(|(_, actor)| {
                game.catalog[actor.type_index].name == "EuropeanParkingGarage"
                    && actor.position.map(f32::to_bits)
                        == [original[0], original[1]].map(f32::to_bits)
            })
            .unwrap();
        assert_eq!(actor.facing.to_bits(), angle.to_bits());
        assert_eq!(
            actor.placed_height_offset().unwrap().to_bits(),
            (-2.0_f32).to_bits()
        );
        let expected_z = game.ground_height(actor.position) + original[2];
        assert_eq!(game.object_position(id)[2].to_bits(), expected_z.to_bits());
        let draw = unit_position(&game, id);
        assert_eq!(draw.x.to_bits(), (original[0] * 0.1).to_bits());
        assert_eq!(draw.z.to_bits(), (-original[1] * 0.1).to_bits());
        let expected_draw_height = expected_z * 0.1;
        assert!((draw.y - expected_draw_height).abs() < 0.0001);
        let ground = terrain.sample(Vec2::from_array(actor.position) * 0.1);
        assert!((draw.y - ground + 0.2).abs() < 0.0001);
    }
}
