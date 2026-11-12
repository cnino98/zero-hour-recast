use crate::skirmish::{
    Faction, GroundAppearance, GroundRules, Kind, LocomotorVariant,
    tests::{fixture_type, game},
};
use crate::{
    Grid,
    terrain::{BridgeGeometry, TerrainMap},
};

#[test]
fn single_and_multiple_normal_templates_sample_actual_sloped_ground_and_bridge_xyz() {
    for additional in [false, true] {
        for bridge in [false, true] {
            let mut game = game();
            let tank = game.type_for(0, Kind::Tank).unwrap();
            let unit = &mut game.catalog[tank];
            assert!(unit.locomotor.wander_length.is_some());
            unit.locomotor.ground = Some(GroundRules {
                wander: None,
                is_dozer: false,
                downhill_only: false,
                appearance: GroundAppearance::Treads,
                acceleration: 100.0 / 900.0,
                acceleration_damaged: 100.0 / 900.0,
                braking: 100.0 / 900.0,
                min_speed: 0.0,
                close_enough: 1.0,
                mass: 50.0,
                forward_friction: 0.1,
                lateral_friction: 0.2,
                extra_friction: 0.0,
            });
            if additional {
                let mut water = fixture_type(Faction::Usa, Kind::Tank).locomotor;
                water.legal_surfaces = 2;
                unit.locomotor_sets.normal.push(LocomotorVariant {
                    speed: 10.0,
                    locomotor: water,
                });
            }
            let heights = (0_u16..60)
                .flat_map(|_| {
                    (0_u16..80).map(|x| {
                        let rise = f32::from(x) * 2.0;
                        20.0 + rise
                    })
                })
                .collect();
            game.set_projectile_terrain(80, 60, [0.0; 2], heights, 10.0)
                .unwrap();
            let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
            let layer = if bridge {
                terrain
                    .add_bridge(
                        BridgeGeometry {
                            from: [250.0, 150.0, 200.0],
                            to: [550.0, 150.0, 210.0],
                            corners: [
                                [250.0, 200.0, 200.0],
                                [250.0, 100.0, 200.0],
                                [550.0, 200.0, 210.0],
                                [550.0, 100.0, 210.0],
                            ],
                        },
                        |point| game.ground_height(point),
                    )
                    .unwrap()
            } else {
                1
            };
            game.grid = Grid::new(80, 60, []).unwrap();
            game.configure_movement_terrain(terrain).unwrap();
            let id = game.spawn(tank, 0, [300.0, 150.0], true);
            game.entities[id].layer = layer;
            let actual = game.object_position(id);
            assert!(actual[2] > 0.0);
            let goal = [
                450.0,
                150.0,
                game.movement_layer_height(layer, [450.0, 150.0]),
            ];
            game.move_to_world(id, goal).unwrap();
            game.process_path_service();
            game.advance_ground(id);
            let path = game.entities[id].ai.ground_path.as_ref().unwrap();
            let cache = path.cache.as_ref().unwrap();
            assert_eq!(
                cache.input.map(f32::to_bits),
                actual.map(f32::to_bits),
                "additional={additional} bridge={bridge}"
            );
            assert_eq!(cache.output.layer, layer);
        }
    }
}
