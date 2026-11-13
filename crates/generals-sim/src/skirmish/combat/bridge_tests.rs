use crate::{
    skirmish::{
        Faction, Kind,
        tests::{fixture_type, game},
    },
    terrain::{BridgeGeometry, TerrainMap},
};

#[test]
fn bridge_weapon_range_uses_ordered_endpoints_without_bridge_body_radius() {
    let mut game = game();
    let mut bridge = fixture_type(Faction::Civilian, Kind::TechBuilding);
    bridge.kind_of = ["BRIDGE", "LANDMARK_BRIDGE", "STRUCTURE"]
        .map(str::to_owned)
        .to_vec();
    bridge.geometry.major = 500.0;
    game.catalog.push(bridge);
    let bridge = game.spawn(game.catalog.len() - 1, 1, [355.0, 205.0], true);
    let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
    terrain
        .add_bridge(
            BridgeGeometry {
                from: [300.0, 205.0, 80.0],
                to: [410.0, 205.0, 80.0],
                corners: [
                    [300.0, 230.0, 80.0],
                    [300.0, 180.0, 80.0],
                    [410.0, 230.0, 80.0],
                    [410.0, 180.0, 80.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    game.bind_map_bridges();
    game.catalog[6].geometry.major = 5.0;
    game.catalog[6].geometry.minor = 5.0;
    game.catalog[6].weapon.range = 60.0;
    game.catalog[6].weapon.min_range = 15.0;
    let tank = game.spawn(6, 0, [335.0, 205.0], true);
    // First endpoint (325,205) is too close; the second would be valid,
    // but the native function does not test it in this branch.
    assert!(!game.within_attack_range(tank, bridge));
    let bridge_position = game.object_position(bridge);
    assert!(!game.weapon_goal_in_range(tank, [335.0, 205.0, 0.0], Some(bridge), bridge_position));
    game.entities[tank].position = [435.0, 205.0];
    assert!(game.within_attack_range(tank, bridge));
    game.entities[tank].position = [455.0, 205.0];
    assert!(!game.within_attack_range(tank, bridge));
    let goal = [455.0, 205.0, 0.0];
    assert!(!game.weapon_goal_in_range(tank, goal, Some(bridge), bridge_position));
    let barracks = game.type_for(1, Kind::Barracks).unwrap();
    game.catalog[barracks].geometry.major = 55.0;
    let target = game.spawn(barracks, 1, [355.0, 205.0], true);
    assert!(game.weapon_goal_in_range(tank, goal, Some(target), game.object_position(target)));
    // Ground/deck altitude is ignored by ATTACK_RANGE_IS_2D.
    game.entities[tank].position = [350.0, 205.0];
    assert!(game.within_attack_range(tank, bridge));
}
