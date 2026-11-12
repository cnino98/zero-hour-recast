use super::*;
use crate::{
    collision_geometry::Shape,
    skirmish::{
        Faction, Kind,
        tests::{fixture_type, game},
    },
};

#[test]
fn rotated_factory_raster_blocks_the_corner_that_trapped_the_third_rally_tank() {
    let grid = Grid::new(300, 300, []).unwrap();
    let footprint = Footprint {
        position: [775.0, 2145.0],
        angle: -135.0_f32.to_radians(),
        major: 40.0,
        minor: 62.0,
        shape: Shape::Box,
    };
    // Native 5-unit sample (column4,row0) is [832.98285,2115.301].
    // The old cell-center predicate incorrectly admitted this path waypoint.
    let corner = Cell { x: 83, y: 211 };
    assert!(!footprint.contains(super::super::cell_center(corner)));
    let sampled = cells(&footprint, &grid);
    assert!(sampled.contains(&corner));
    assert!(sampled.contains(&Cell { x: 83, y: 212 }));
    let navigation = grid.with_obstacles(sampled);
    let route = navigation
        .path(Cell { x: 71, y: 211 }, Cell { x: 88, y: 215 })
        .unwrap();
    assert!(!route.contains(&corner));
}

#[test]
fn circle_raster_keeps_native_point_four_cell_expansion_and_ignores_minor_axis() {
    let grid = Grid::new(20, 20, []).unwrap();
    let mut footprint = Footprint {
        position: [50.0, 50.0],
        angle: 0.0,
        major: 10.0,
        minor: 1.0,
        shape: Shape::Cylinder,
    };
    let expected = (4..=5)
        .flat_map(|x| (4..=5).map(move |y| Cell { x, y }))
        .collect();
    assert!(cells(&footprint, &grid) == expected);
    footprint.major = 12.0;
    let expanded = cells(&footprint, &grid);
    assert!(expanded.contains(&Cell { x: 3, y: 4 }));
    assert!(!footprint.contains([35.0, 45.0]));
    footprint.minor = 100.0;
    footprint.angle = 1.0;
    footprint.shape = Shape::Sphere;
    assert!(cells(&footprint, &grid) == expanded);
    footprint.position = [0.0, 0.0];
    assert!(
        cells(&footprint, &grid)
            .iter()
            .all(|cell| grid.contains(*cell))
    );
}

#[test]
fn footprint_raster_preserves_the_native_strict_inclusive_extent_check() {
    // newMap allocates hi+1 cells, but internal_classifyObjectFootprint uses
    // strict <hi. Preserve that source omission of the last allocated cells.
    let grid = Grid::new(4, 4, []).unwrap();
    let mut footprint = Footprint {
        position: [35.0, 35.0],
        angle: 0.0,
        major: 12.0,
        minor: 12.0,
        shape: Shape::Box,
    };
    assert!(grid.contains(Cell { x: 3, y: 3 }));
    for shape in [Shape::Box, Shape::Cylinder, Shape::Sphere] {
        footprint.shape = shape;
        assert!(cells(&footprint, &grid) == BTreeSet::from([Cell { x: 2, y: 2 }]));
    }
}

fn structure(game: &mut Skirmish, position: [f32; 2]) -> usize {
    let mut unit = fixture_type(Faction::Gla, Kind::Factory);
    unit.geometry.shape = Shape::Box;
    unit.geometry.major = 20.0;
    unit.geometry.minor = 20.0;
    let index = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(index, 0, position, true);
    game.rebuild_navigation();
    id
}

#[test]
fn ignored_owner_and_deletion_never_backfill_a_later_overlapping_object() {
    let mut game = game();
    let first = structure(&mut game, [355.0, 205.0]);
    let second = structure(&mut game, [375.0, 205.0]);
    let shared = Cell { x: 36, y: 20 };
    let second_only = Cell { x: 38, y: 20 };
    let mover = game.spawn(6, 0, [305.0, 205.0], true);
    game.entities[mover].ai.ignored_producer = Some(first);
    let ignored = game.ignored_navigation(mover).unwrap();
    assert!(!ignored.is_blocked(shared));
    assert!(ignored.is_blocked(second_only));
    game.entities[mover].ai.ignored_producer = Some(second);
    assert!(game.ignored_navigation(mover).unwrap().is_blocked(shared));
    game.destroy_maintenance_object(first);
    game.process_update_destruction();
    assert!(!game.navigation.is_blocked(shared));
    assert!(game.navigation.is_blocked(second_only));
    game.rebuild_navigation();
    assert!(!game.navigation.is_blocked(shared));
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert!(!game.navigation.is_blocked(shared));
    assert!(!game.navigation.is_blocked(second_only));
}

#[test]
fn native_structure_admission_excludes_small_mobile_raised_and_projectile_objects() {
    let mut game = game();
    let id = structure(&mut game, [355.0, 205.0]);
    let index = game.entities[id].type_index;
    let cell = Cell { x: 35, y: 20 };
    assert!(game.navigation.is_blocked(cell));
    game.catalog[index].geometry.is_small = true;
    game.rebuild_navigation();
    assert!(!game.navigation.is_blocked(cell));
    game.catalog[index].geometry.is_small = false;
    game.catalog[index]
        .kind_of
        .retain(|name| name != "IMMOBILE");
    game.rebuild_navigation();
    assert!(!game.navigation.is_blocked(cell));
    game.catalog[index].kind_of.push("IMMOBILE".into());
    game.set_object_position(id, [355.0, 205.0, 10.01]);
    game.rebuild_navigation();
    assert!(!game.navigation.is_blocked(cell));
    game.set_object_position(id, [355.0, 205.0, 10.0]);
    game.rebuild_navigation();
    assert!(game.navigation.is_blocked(cell));
    game.catalog[index].kind_of.push("PROJECTILE".into());
    game.rebuild_navigation();
    assert!(!game.navigation.is_blocked(cell));
    game.catalog[index]
        .kind_of
        .retain(|kind| kind != "PROJECTILE");
    game.catalog[index].kind_of.push("BLAST_CRATER".into());
    game.set_object_position(id, [355.0, 205.0, 10.01]);
    game.rebuild_navigation();
    assert!(game.navigation.is_blocked(cell));
    game.destroy_maintenance_object(id);
    assert!(game.navigation.is_blocked(cell));
}

#[test]
fn registration_and_ignored_producer_preserve_original_terrain_barriers() {
    let mut game = game();
    let water = Cell { x: 35, y: 20 };
    game.grid = Grid::new(80, 60, [water]).unwrap();
    game.movement_terrain = crate::terrain::TerrainMap::classify(80, 60, [], [water]).unwrap();
    let first = structure(&mut game, [355.0, 205.0]);
    assert!(!game.navigation.is_obstacle(water));
    let mover = game.spawn(6, 0, [305.0, 205.0], true);
    game.entities[mover].ai.ignored_producer = Some(first);
    assert!(game.ignored_navigation(mover).unwrap().is_blocked(water));
    game.destroy_maintenance_object(first);
    game.process_update_destruction();
    assert!(game.navigation.is_blocked(water));
}

#[test]
fn original_map_record_order_survives_neutral_object_id_adoption_and_removal() {
    use crate::skirmish::{MapObstacle, NeutralObject};
    let mut game = game();
    let mut neutral = fixture_type(Faction::Civilian, Kind::TechBuilding);
    neutral.name = "FixtureNeutralStructure".into();
    neutral.geometry.shape = Shape::Box;
    neutral.geometry.major = 20.0;
    neutral.geometry.minor = 20.0;
    game.layout
        .obstacles
        .push(neutral.footprint([375.0, 205.0], 0.0));
    let neutral_index = game.layout.neutral_objects.len();
    game.layout.neutral_objects.push(NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        original_team: "teamPlyrCivilian".into(),
        name: neutral.name.clone(),
        position: [355.0, 205.0, 0.0],
        angle: 0.0,
        script_name: Some("first-map-obstacle".into()),
        script_targetable: false,
        indestructible: false,
        selectable: None,
    });
    game.catalog.push(neutral);
    game.layout.obstacle_order = vec![
        MapObstacle::Neutral(neutral_index),
        MapObstacle::Static {
            index: 0,
            transparent: false,
        },
    ];
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    game.configure_movement_terrain(crate::terrain::TerrainMap::classify(80, 60, [], []).unwrap())
        .unwrap();
    let first = game.named_map_object("first-map-obstacle").unwrap();
    let mover = game.spawn(6, 0, [305.0, 205.0], true);
    game.entities[mover].ai.ignored_producer = Some(first);
    let shared = Cell { x: 36, y: 20 };
    assert!(
        !game.ignored_navigation(mover).unwrap().is_blocked(shared),
        "first={first}, actual={}",
        game.navigation_obstacles.owner_details(shared)
    );
    game.destroy_maintenance_object(first);
    game.process_update_destruction();
    assert!(!game.navigation.is_blocked(shared));
    game.layout.obstacle_order = vec![
        MapObstacle::Static {
            index: 0,
            transparent: false,
        },
        MapObstacle::Neutral(neutral_index),
    ];
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    let second = game.named_map_object("first-map-obstacle").unwrap();
    let mover = game.spawn(6, 0, [305.0, 205.0], true);
    game.entities[mover].ai.ignored_producer = Some(second);
    assert!(game.ignored_navigation(mover).unwrap().is_blocked(shared));
    game.destroy_maintenance_object(second);
    game.process_update_destruction();
    assert!(game.navigation.is_blocked(shared));
}

#[test]
fn installing_real_terrain_reclassifies_retained_owners_without_removing_barriers() {
    let mut game = game();
    let first = structure(&mut game, [355.0, 205.0]);
    let second = structure(&mut game, [375.0, 205.0]);
    let water = Cell { x: 35, y: 20 };
    let shared = Cell { x: 36, y: 20 };
    assert!(game.navigation.is_obstacle(water));
    game.grid = Grid::new(80, 60, [water]).unwrap();
    game.configure_movement_terrain(
        crate::terrain::TerrainMap::classify(80, 60, [], [water]).unwrap(),
    )
    .unwrap();
    assert!(!game.navigation.is_obstacle(water));
    let mover = game.spawn(6, 0, [305.0, 205.0], true);
    game.entities[mover].ai.ignored_producer = Some(first);
    let ignored = game.ignored_navigation(mover).unwrap();
    assert!(ignored.is_blocked(water));
    assert!(!ignored.is_blocked(shared));
    game.entities[mover].ai.ignored_producer = Some(second);
    assert!(game.ignored_navigation(mover).unwrap().is_blocked(shared));
}

#[test]
fn fence_sampling_uses_authored_offset_and_width_without_centered_geometry() {
    let grid = Grid::new(20, 20, []).unwrap();
    let mut fence = crate::skirmish::MapFence {
        position: [50.0, 50.0],
        angle: 0.0,
        width: 30.0,
        x_offset: 15.0,
    };
    let cells = raster::fence_cells(&fence, &grid);
    assert!(
        cells
            == BTreeSet::from([
                Cell { x: 3, y: 5 },
                Cell { x: 4, y: 5 },
                Cell { x: 5, y: 5 },
                Cell { x: 6, y: 5 }
            ])
    );
    fence.x_offset = 0.0;
    assert!(
        raster::fence_cells(&fence, &grid)
            == BTreeSet::from([
                Cell { x: 5, y: 5 },
                Cell { x: 6, y: 5 },
                Cell { x: 7, y: 5 },
                // fast_float_ceil(6) includes the far endpoint sample.
                Cell { x: 8, y: 5 }
            ])
    );
    fence.position = [195.0, 50.0];
    assert!(raster::fence_cells(&fence, &grid).is_empty());
}

#[test]
fn map_fence_blocks_actual_navigation_and_reclassifies_after_restart() {
    let mut game = game();
    game.layout.fences.push(crate::skirmish::MapFence {
        position: [355.0, 205.0],
        angle: 0.0,
        width: 30.0,
        x_offset: 15.0,
    });
    game.layout
        .obstacle_order
        .push(crate::skirmish::MapObstacle::Fence {
            index: 0,
            transparent: false,
        });
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    let cell = Cell { x: 35, y: 20 };
    assert!(game.navigation.is_obstacle(cell));
    let route = game
        .navigation
        .path(Cell { x: 33, y: 20 }, Cell { x: 39, y: 20 })
        .unwrap();
    assert!(!route.contains(&cell));
    game.configure_movement_terrain(crate::terrain::TerrainMap::classify(80, 60, [], []).unwrap())
        .unwrap();
    assert!(game.navigation.is_obstacle(cell));
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert!(game.navigation.is_obstacle(cell));
}

#[test]
fn ordinary_structure_marks_orthogonal_open_neighbors_pinched_then_clears_them() {
    let mut game = game();
    let building = structure(&mut game, [355.0, 205.0]);
    // Seven neighboring cells remain open; this is the separate one-cell
    // obstacle-expansion phase, not the narrow-passage neighbor-count rule.
    let adjacent = Cell { x: 35, y: 23 };
    let diagonal = Cell { x: 38, y: 23 };
    assert!(game.movement_terrain.cell(adjacent).unwrap().pinched);
    assert!(!game.movement_terrain.cell(diagonal).unwrap().pinched);
    game.destroy_maintenance_object(building);
    game.process_update_destruction();
    assert!(!game.movement_terrain.cell(adjacent).unwrap().pinched);
}

#[test]
fn structure_on_low_bridge_registers_ground_cells_and_respects_layer_altitude() {
    use crate::terrain::{BridgeGeometry, TerrainMap};
    let mut game = game();
    let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
    let layer = terrain
        .add_bridge(
            BridgeGeometry {
                from: [295.0, 205.0, 8.0],
                to: [410.0, 205.0, 8.0],
                corners: [
                    [295.0, 230.0, 8.0],
                    [295.0, 180.0, 8.0],
                    [410.0, 230.0, 8.0],
                    [410.0, 180.0, 8.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    // Cell 29 straddles the actual deck's entry edge. Its center is on the
    // eight-unit deck; the native entry connection keeps the ground cell clear.
    let building = structure(&mut game, [295.0, 205.0]);
    game.entities[building].layer = layer;
    game.entities[building].placed_height_offset = Some(0.0);
    assert_eq!(
        game.object_position(building)[2].to_bits(),
        8.0_f32.to_bits()
    );
    assert!(
        game.movement_terrain
            .bridge_cell(layer, Cell { x: 29, y: 20 })
            .unwrap()
            .connect_ground
    );
    game.rebuild_navigation();
    let cell = Cell { x: 29, y: 20 };
    assert!(
        game.navigation_obstacles
            .claims
            .at(LayerCell { cell, layer: 1 })
            .is_some()
    );
    assert!(
        game.navigation_obstacles
            .claims
            .at(LayerCell { cell, layer })
            .is_none()
    );
    // Object::calculateHeightAboveTerrain subtracts the current layer, so the
    // eight-unit deck is not part of Pathfinder's ten-unit altitude threshold.
    game.entities[building].placed_height_offset = Some(10.0);
    assert_eq!(
        game.object_position(building)[2].to_bits(),
        18.0_f32.to_bits()
    );
    assert_eq!(
        game.height_above_ground(building).to_bits(),
        10.0_f32.to_bits()
    );
    game.rebuild_navigation();
    assert!(game.navigation_obstacles.at(cell).is_some());
    game.entities[building].placed_height_offset = Some(10.01);
    assert!(game.height_above_ground(building) > 10.0);
    game.rebuild_navigation();
    assert!(game.navigation_obstacles.at(cell).is_none());
}

#[test]
fn obstacle_owner_transparency_stays_with_first_claim_and_disappears_without_backfill() {
    let mut game = game();
    let first = structure(&mut game, [355.0, 205.0]);
    let second = structure(&mut game, [375.0, 205.0]);
    let second_type = game.entities[second].type_index;
    game.catalog[second_type]
        .kind_of
        .push("CAN_SEE_THROUGH_STRUCTURE".into());
    game.rebuild_navigation();
    let shared = Cell { x: 36, y: 20 };
    let second_only = Cell { x: 38, y: 20 };
    let owner = game.navigation_obstacles.at(shared).unwrap();
    assert!(owner.id == OwnerId::Actor(first));
    assert!(!owner.transparent);
    assert!(
        game.navigation_obstacles
            .at(second_only)
            .unwrap()
            .transparent
    );
    game.destroy_maintenance_object(first);
    game.process_update_destruction();
    assert!(game.navigation_obstacles.at(shared).is_none());
    assert!(game.navigation_obstacles.at(second_only).unwrap().id == OwnerId::Actor(second));
    game.destroy_maintenance_object(second);
    game.process_update_destruction();
    assert!(game.navigation_obstacles.at(second_only).is_none());
}

#[test]
fn elevated_structure_does_not_claim_bridge_cells_or_replace_ground_owner() {
    let mut game = game();
    let mut terrain = crate::terrain::TerrainMap::classify(80, 60, [], []).unwrap();
    let layer = terrain
        .add_bridge(
            crate::terrain::BridgeGeometry {
                from: [300.0, 205.0, 80.0],
                to: [450.0, 205.0, 80.0],
                corners: [
                    [300.0, 245.0, 80.0],
                    [300.0, 165.0, 80.0],
                    [450.0, 245.0, 80.0],
                    [450.0, 165.0, 80.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    let lower = structure(&mut game, [355.0, 205.0]);
    let upper = structure(&mut game, [355.0, 205.0]);
    game.entities[upper].layer = layer;
    game.entities[upper].placed_height_offset = Some(10.01);
    assert!(game.height_above_ground(upper) > 10.0);
    game.rebuild_navigation();
    let cell = Cell { x: 35, y: 20 };
    assert!(game.attack_obstacle(1, cell).unwrap().id == OwnerId::Actor(lower));
    assert!(game.attack_obstacle(layer, cell).is_none());
    let mover = game.spawn(6, 0, [315.0, 205.0], true);
    game.entities[mover].layer = layer;
    game.entities[mover].ai.ignored_producer = Some(upper);
    assert!(game.ignored_navigation(mover).is_none());
    assert!(game.navigation.is_blocked(cell));
    assert!(!game.navigation.layer_blocked(LayerCell { cell, layer }));
    game.destroy_maintenance_object(upper);
    assert!(game.attack_obstacle(layer, cell).is_none());
    assert!(game.attack_obstacle(1, cell).unwrap().id == OwnerId::Actor(lower));
}

#[test]
fn explicit_remove_and_add_reclassify_cells_before_deferred_destruction() {
    let mut game = game();
    let id = structure(&mut game, [355.0, 205.0]);
    let center = Cell { x: 35, y: 20 };
    assert!(game.navigation_obstacles.at(center).unwrap().id == OwnerId::Actor(id));
    game.unregister_object_obstacle(id);
    assert!(!game.navigation.is_blocked(center));
    game.rebuild_navigation();
    assert!(!game.navigation.is_blocked(center));
    game.restore_object_obstacle(id);
    assert!(game.navigation.is_blocked(center));
    assert!(game.navigation_obstacles.at(center).unwrap().id == OwnerId::Actor(id));
    // Ordinary destroyObject retains the footprint until destruction-list drain.
    game.destroy_maintenance_object(id);
    game.rebuild_navigation();
    assert!(game.navigation.is_blocked(center));
    game.process_update_destruction();
    assert!(!game.navigation.is_blocked(center));
}
