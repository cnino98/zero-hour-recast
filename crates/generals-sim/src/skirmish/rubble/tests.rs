use super::*;
use crate::Cell;
use crate::skirmish::{
    Faction, Kind,
    movement::obstacles::cells as footprint_cells,
    tests::{fixture_type, game},
};

fn retained_structure() -> (Skirmish, usize) {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.maintenance.rubble_height = Some(5.0);
    unit.scoring.victory = false;
    unit.geometry.major = 20.0;
    unit.geometry.minor = 10.0;
    unit.geometry.shape = crate::collision_geometry::Shape::Box;
    let index = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(index, game.neutral_owner(), [355.0, 205.0], true);
    game.rebuild_navigation();
    (game, id)
}

#[test]
fn construction_clears_native_removable_objects_but_preserves_inert_feedback_and_height() {
    let mut game = game();
    let position = [355.0, 205.0];
    let cases = [
        (vec!["SHRUBBERY"], false, 0.0, true),
        (vec!["CLEARED_BY_BUILD"], false, 0.0, true),
        (Vec::new(), true, 0.0, true),
        (vec!["INERT", "CLEARED_BY_BUILD"], false, 0.0, false),
        (vec!["ALWAYS_SELECTABLE", "SHRUBBERY"], false, 0.0, false),
        (vec!["SHRUBBERY"], false, 200.0, false),
        (Vec::new(), false, 0.0, false),
    ];
    let mut objects = Vec::new();
    for (flags, dead, height, removed) in cases {
        let mut unit = fixture_type(Faction::Civilian, Kind::Infantry);
        unit.kind_of = flags.into_iter().map(str::to_owned).collect();
        unit.geometry.shape = crate::collision_geometry::Shape::Box;
        if dead {
            unit.maintenance.rubble_height = Some(5.0);
        }
        let index = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(index, game.neutral_owner(), position, true);
        game.set_object_position(id, [position[0], position[1], height]);
        if dead {
            game.apply_hit(id, 1, 2000.0);
        }
        game.observe_ground_partition(id);
        objects.push((id, removed));
    }
    game.update_ground_contacts();
    game.clear_rubble_for_construction(3, position, 0.0);
    for &(id, removed) in &objects {
        assert_eq!(
            game.update_objects[game.entities[id].update_owner].destroy_requested, removed,
            "object {id}"
        );
        assert!(game.entities[id].exists());
    }
    game.process_update_destruction();
    for (id, removed) in objects {
        assert_eq!(!game.entities[id].exists(), removed, "object {id}");
    }
}

#[test]
fn retained_rubble_changes_surface_without_admitting_vehicles_or_live_commands() {
    let (mut game, building) = retained_structure();
    game.catalog[5].locomotor.legal_surfaces = 17;
    game.catalog[6].locomotor.legal_surfaces = 1;
    let infantry = game.spawn(5, 0, [305.0, 205.0], true);
    let vehicle = game.spawn(6, 0, [305.0, 175.0], true);
    let center = game.cell_at(game.entities[building].position).unwrap();
    assert!(game.navigation_for(infantry).is_blocked(center));
    game.apply_hit(building, infantry, 2000.0);
    assert!(game.retained_rubble(building));
    game.update_ground_contacts();
    assert!(
        game.partition
            .outward_objects([355.0, 205.0])
            .contains(&building)
    );
    assert!(!game.navigation_for(infantry).is_blocked(center));
    assert!(game.navigation_for(vehicle).is_blocked(center));
    assert!(game.movement_terrain.valid_movement([355.0, 205.0], 17));
    assert!(!game.movement_terrain.valid_movement([355.0, 205.0], 1));
    assert!(game.move_to(building, [350.0, 200.0]).is_err());
    assert!(game.attack(infantry, building).is_err());
    assert!(
        game.movement_path(infantry, [305.0, 205.0], [405.0, 205.0])
            .unwrap()
            .raw
            .contains(&crate::grid::LayerCell {
                cell: center,
                layer: 1
            })
    );
    assert!(
        !game
            .movement_path(vehicle, [305.0, 205.0], [405.0, 205.0])
            .unwrap()
            .raw
            .contains(&crate::grid::LayerCell {
                cell: center,
                layer: 1
            })
    );
}

#[test]
fn construction_clears_dead_geometry_and_restart_removes_rubble_surface() {
    let (mut game, building) = retained_structure();
    game.apply_hit(building, 1, 2000.0);
    game.update_ground_contacts();
    let position = game.entities[building].position;
    game.catalog[game.entities[1].type_index]
        .locomotor
        .legal_surfaces = 1;
    // Native terrain() merges OBSTACLE with CLEAR, but leaves RUBBLE separate.
    assert!(!game.build_site_reachable(1, position));
    assert!(game.build_site_reachable(1, [position[0] + 40.0, position[1]]));
    game.clear_rubble_for_construction(3, position, 0.0);
    assert!(game.retained_rubble(building));
    assert!(game.update_objects[game.entities[building].update_owner].destroy_requested);
    game.process_update_destruction();
    game.rebuild_navigation();
    assert!(!game.retained_rubble(building));
    assert!(game.entities[building].masked());
    assert!(!game.partition.outward_objects(position).contains(&building));
    assert!(game.movement_terrain.valid_movement(position, 1));
    let second = game.spawn(
        game.entities[building].type_index,
        game.neutral_owner(),
        [455.0, 205.0],
        true,
    );
    game.apply_hit(second, 1, 2000.0);
    assert!(!game.movement_terrain.valid_movement([455.0, 205.0], 1));
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert!(game.movement_terrain.valid_movement([455.0, 205.0], 1));
    assert!(game.rubble_navigation.is_none());
}

#[test]
fn ordinary_construction_preserves_native_rubble_overlap_gaps() {
    let (mut game, building) = retained_structure();
    let position = game.entities[building].position;
    let other = game.spawn(
        game.entities[building].type_index,
        game.neutral_owner(),
        [455.0, 205.0],
        true,
    );
    game.rebuild_navigation();
    // Ordinary destruction invalidates zones until the deferred recalculation;
    // startup rebuilds at frame zero calculate them immediately.
    game.step();
    game.apply_hit(building, 1, 2000.0);
    game.apply_hit(other, 1, 2000.0);
    game.update_ground_contacts();
    assert!(game.retained_rubble(building));
    assert!(!game.movement_terrain.valid_movement(position, 1));
    game.catalog[1].locomotor.legal_surfaces = 17;
    assert!(game.build_site_reachable(1, position));
    game.catalog[3].geometry.shape = crate::collision_geometry::Shape::Box;
    game.catalog[3].geometry.major = 40.0;
    game.catalog[3].geometry.minor = 20.0;
    let angle = std::f32::consts::FRAC_PI_4;
    let expected = footprint_cells(&game.catalog[3].footprint(position, angle), &game.grid);
    let rubble: BTreeSet<_> = game.movement_terrain.rubble_cells().collect();
    let gaps: BTreeSet<_> = expected.intersection(&rubble).copied().collect();
    let claimed: BTreeSet<_> = expected.difference(&rubble).copied().collect();
    assert!(!gaps.is_empty());
    assert!(!claimed.is_empty());
    let cash = game.players[0].cash;
    let replacement = game.build_facing(1, 3, position, angle).unwrap();
    assert_eq!(game.players[0].cash, cash - game.catalog[3].cost);
    assert!(game.entities[replacement].alive());
    assert!(!game.entities[replacement].complete());
    assert!(game.retained_rubble(building));
    assert!(game.update_objects[game.entities[building].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(game.entities[building].masked());
    assert!(game.movement_terrain.valid_movement(position, 1));
    assert!(game.retained_rubble(other));
    assert!(!game.movement_terrain.valid_movement([455.0, 205.0], 1));
    // setTypeAsObstacle refuses RUBBLE. Later removeObstacle clears that
    // ownerless rubble without backfilling the replacement's footprint.
    game.entities[1].ai.ignored_producer = Some(replacement);
    for phase in 0..2 {
        let owned: BTreeSet<_> = game
            .navigation_obstacles
            .obstacle_owners()
            .filter_map(|(node, owner)| {
                matches!(owner.id, crate::skirmish::movement::obstacles::OwnerId::Actor(id) if id == replacement)
                    .then_some(node.cell)
            })
            .collect();
        assert!(owned == claimed);
        for &cell in &expected {
            assert_eq!(game.navigation.is_blocked(cell), claimed.contains(&cell));
            assert_eq!(
                game.attack_obstacle(1, cell).is_some(),
                claimed.contains(&cell)
            );
        }
        let ignored = game.ignored_navigation(1).unwrap();
        assert!(expected.iter().all(|cell| !ignored.is_blocked(*cell)));
        assert!(gaps.iter().all(|cell| !game.navigation.is_blocked(*cell)));
        assert!(game.retained_rubble(other));
        assert!(!game.movement_terrain.valid_movement([455.0, 205.0], 1));
        if phase == 0 {
            game.rebuild_navigation();
        }
    }
}

#[test]
fn final_object_destruction_removes_retained_rubble_before_navigation_rebuild() {
    let (mut game, building) = retained_structure();
    game.apply_hit(building, 1, 2000.0);
    let position = game.entities[building].position;
    assert!(game.retained_rubble(building));
    assert!(!game.movement_terrain.valid_movement(position, 1));
    game.destroy_maintenance_object(building);
    assert!(game.retained_rubble(building));
    assert!(game.update_objects[game.entities[building].update_owner].destroy_requested);
    game.process_update_destruction();
    assert!(!game.retained_rubble(building));
    assert!(game.movement_terrain.valid_movement(position, 1));
    assert!(game.rubble_navigation.is_none());
    game.update_ground_contacts();
    assert!(!game.partition.outward_objects(position).contains(&building));
}

#[test]
fn source_box_sampling_uses_half_unit_offset_and_keeps_terrain_obstacles() {
    let (mut game, building) = retained_structure();
    let unit = &game.catalog[game.entities[building].type_index];
    let cells = footprint_cells(&unit.footprint([355.0, 205.0], 0.0), &game.grid);
    // BaseFunctions.h fast_float_ceil adds binary32 0x3f7fffff: the
    // integral step extents 8 and 4 round to 9 and 5, respectively. Native
    // samples include x=375 and y=195 (AIPathfind.cpp:4403–4416).
    assert_eq!(crate::native_real::ceil(8.0), 9);
    assert_eq!(crate::native_real::ceil(4.0), 5);
    assert!(
        cells
            == (19..=21)
                .flat_map(|y| (33..=37).map(move |x| Cell { x, y }))
                .collect()
    );
    let water = Cell { x: 35, y: 20 };
    game.configure_movement_terrain(
        crate::terrain::TerrainMap::classify(80, 60, [], [water]).unwrap(),
    )
    .unwrap();
    game.apply_hit(building, 1, 2000.0);
    assert!(!game.movement_terrain.valid_movement([355.0, 205.0], 17));
    assert!(game.movement_terrain.valid_movement([355.0, 205.0], 2));
}

#[test]
fn removing_an_overlapping_structure_clears_foreign_rubble_without_reinsertion() {
    let (mut game, first) = retained_structure();
    let second = game.spawn(
        game.entities[first].type_index,
        game.neutral_owner(),
        [375.0, 205.0],
        true,
    );
    game.rebuild_navigation();
    game.apply_hit(first, 1, 2000.0);
    let shared = [365.0, 205.0];
    let first_only = [335.0, 205.0];
    assert!(!game.movement_terrain.valid_movement(shared, 1));
    assert!(!game.movement_terrain.valid_movement(first_only, 1));
    game.destroy_maintenance_object(second);
    assert!(game.entities[second].exists());
    assert!(!game.movement_terrain.valid_movement(shared, 1));
    game.process_update_destruction();
    assert!(game.retained_rubble(first));
    assert!(game.movement_terrain.valid_movement(shared, 1));
    assert!(!game.movement_terrain.valid_movement(first_only, 1));
    game.rebuild_navigation();
    assert!(game.movement_terrain.valid_movement(shared, 1));
    assert!(!game.movement_terrain.valid_movement(first_only, 1));
    assert!(!game.entities[second].exists());
}
