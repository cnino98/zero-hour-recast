use crate::skirmish::{Skirmish, tests::game};

fn slope() -> (Skirmish, usize) {
    let mut game = game();
    let heights = (0_u16..60)
        .flat_map(|_| (0_u16..80).map(|x| f32::from(x) * 2.0))
        .collect();
    game.set_projectile_terrain(80, 60, [0.0; 2], heights, 10.0)
        .unwrap();
    let id = game.spawn(5, 0, [305.0, 405.0], true);
    (game, id)
}

fn block_goal(game: &mut Skirmish) -> [f32; 3] {
    let goal = [505.0, 405.0, game.ground_height([505.0, 405.0])];
    game.spawn(0, 0, [goal[0], goal[1]], true);
    game.rebuild_navigation();
    game.refresh_occupancy();
    goal
}

fn assert_adjusted_height(game: &Skirmish, actual: [f32; 3], original: [f32; 3]) {
    assert_ne!(
        [actual[0], actual[1]].map(f32::to_bits),
        [original[0], original[1]].map(f32::to_bits)
    );
    assert_eq!(
        actual[2].to_bits(),
        game.ground_height([actual[0], actual[1]]).to_bits()
    );
    assert_ne!(actual[2].to_bits(), original[2].to_bits());
}

#[test]
fn clear_human_single_keeps_original_xyz_and_computer_uses_cell_height() {
    let (mut game, human) = slope();
    game.materialize_selection_group(0, &[human]);
    let original = [503.25, 405.75, 137.0];
    let actual = game.adjusted_destination_world(human, original).unwrap();
    assert_eq!(actual.map(f32::to_bits), original.map(f32::to_bits));
    let computer = game.spawn(5, 1, [305.0, 455.0], true);
    game.slots[1].difficulty = Some(crate::skirmish::OpponentDifficulty::Hard);
    assert!(game.computer_player(1));
    let actual = game.adjusted_destination_world(computer, original).unwrap();
    assert_adjusted_height(&game, actual, original);
}

#[test]
fn obstructed_world_move_and_guard_return_keep_fresh_adjusted_height() {
    let (mut game, id) = slope();
    let original = block_goal(&mut game);
    game.move_to_world(id, original).unwrap();
    let actual = game.entities[id].ai_goal_position();
    assert_adjusted_height(&game, actual, original);
    game.stop(id).unwrap();
    game.guard_world(id, original).unwrap();
    assert_eq!(
        game.entities[id].ai.guard.center.map(f32::to_bits),
        original.map(f32::to_bits)
    );
    let actual = game.entities[id].ai.guard.return_position;
    assert_adjusted_height(&game, actual, original);
    let query = game.movement_goal_position(id, [actual[0], actual[1]]);
    assert_eq!(query.map(f32::to_bits), actual.map(f32::to_bits));
}

#[test]
fn final_waypoint_readjustment_and_formation_keep_fresh_height() {
    let (mut game, id) = slope();
    let original = block_goal(&mut game);
    let first = game.object_position(id);
    game.native_follow_path_from(
        id,
        vec![first, original],
        None,
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
        false,
    );
    game.process_path_service();
    game.update_native_follow_path(id);
    assert_eq!(game.entities[id].ai.waypoints.index, 1);
    assert_eq!(
        game.entities[id].ai.waypoints.points[1].map(f32::to_bits),
        original.map(f32::to_bits)
    );
    let actual = game.entities[id].ai.command.move_to.position;
    assert_adjusted_height(&game, actual, original);
    game.stop(id).unwrap();
    game.group_create_formation(&[id]).unwrap();
    let route = [first, [385.0, 405.0, 77.0], [425.0, 405.0, 85.0], original];
    game.move_formation(&[id], original, Some(&route)).unwrap();
    let actual = *game.entities[id].ai.waypoints.points.last().unwrap();
    assert_adjusted_height(&game, actual, original);
}

#[test]
fn snapped_bridge_destination_uses_selected_layer_and_ground_wins_height_tie() {
    use crate::terrain::{BridgeGeometry, TerrainMap};
    let mut game = game();
    let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
    let layer = terrain
        .add_bridge(
            BridgeGeometry {
                from: [400.0, 205.0, 100.0],
                to: [550.0, 205.0, 100.0],
                corners: [
                    [400.0, 255.0, 100.0],
                    [400.0, 155.0, 100.0],
                    [550.0, 255.0, 100.0],
                    [550.0, 155.0, 100.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    let id = game.spawn(5, 0, [350.0, 205.0], true);
    for (height, expected_layer, expected_height) in [(80.0, layer, 100.0_f32), (50.0, 1, 0.0)] {
        let actual = game
            .adjusted_destination_world(id, [455.25, 205.75, height])
            .unwrap();
        assert_eq!(actual[2].to_bits(), expected_height.to_bits());
        assert_eq!(
            game.movement_terrain.get_layer_for_destination(actual, 0.0),
            expected_layer
        );
    }
}

#[test]
fn bridge_edge_adjustment_uses_ground_height_when_requested_layer_has_no_cell() {
    use crate::{
        grid::{Cell, LayerCell},
        terrain::{BridgeGeometry, TerrainMap},
    };
    let mut game = game();
    let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
    let layer = terrain
        .add_bridge(
            BridgeGeometry {
                from: [400.0, 205.0, 100.0],
                to: [550.0, 205.0, 100.0],
                corners: [
                    [400.0, 255.0, 100.0],
                    [400.0, 155.0, 100.0],
                    [550.0, 255.0, 100.0],
                    [550.0, 155.0, 100.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    let id = game.spawn(5, 0, [350.0, 205.0], true);
    let requested = [545.0, 205.0, 100.0];
    assert_eq!(
        game.movement_terrain
            .get_layer_for_destination(requested, 0.0),
        layer
    );
    let candidate = LayerCell {
        cell: Cell { x: 58, y: 20 },
        layer,
    };
    assert_eq!(game.navigation_for(id).resolve_layer(candidate).layer, 1);
    // A spiral candidate beyond the bridge's allocated cells resolves to ground,
    // even though the original request selected the upper bridge layer.
    let actual = game
        .check_adjust(id, candidate.cell, [requested[0], requested[1]], layer)
        .unwrap();
    assert!(game.cell_at([actual[0], actual[1]]) == Some(candidate.cell));
    assert_eq!(
        actual[2].to_bits(),
        game.ground_height([actual[0], actual[1]]).to_bits()
    );
    assert_ne!(actual[2].to_bits(), requested[2].to_bits());
    assert_eq!(
        game.movement_terrain
            .get_layer_for_destination(actual, actual[2]),
        1
    );
}

#[test]
fn infantry_columns_reselect_priority_after_actual_locomotor_upgrade() {
    use crate::skirmish::{Faction, Kind, MovementUpgrade, tests};
    let mut game = game();
    let mut pilot = tests::fixture_type(Faction::Usa, Kind::Infantry);
    pilot.locomotor.move_priority = 0;
    let mut upgraded = tests::fixture_type(Faction::Usa, Kind::Infantry).locomotor;
    upgraded.move_priority = 2;
    pilot.maintenance.movement_upgrade = Some(MovementUpgrade {
        trigger: "Upgrade_Veterancy_HEROIC".into(),
        speed: 25.0,
        locomotor: upgraded,
    });
    tests::register_behavior(&mut pilot, "LocomotorSetUpgrade", 0x80);
    pilot.behavior_modules.last_mut().unwrap().upgrade.triggers =
        vec!["Upgrade_Veterancy_HEROIC".into()];
    let kind = game.catalog.len();
    game.catalog.push(pilot);
    let pilot = game.spawn(kind, 0, [300.0, 300.0], true);
    let mut members = vec![pilot];
    for y in [275.0, 325.0, 350.0] {
        members.push(game.spawn(5, 0, [300.0, y], true));
    }
    let route = [
        [300.0, 300.0, 0.0],
        [400.0, 300.0, 0.0],
        [500.0, 300.0, 0.0],
        [650.0, 300.0, 0.0],
    ];
    assert!(game.move_columns(&members, [300.0, 300.0], route[3], &route, true));
    let previous = *game.entities[pilot].ai.waypoints.points.last().unwrap();
    game.entities[pilot].veterancy_upgrades |= 1 << 3;
    game.activate_upgrade_modules(pilot);
    assert!(game.native_upgrade_executed(pilot, "LocomotorSetUpgrade", 0));
    assert!(game.move_columns(&members, [300.0, 300.0], route[3], &route, true));
    let current = *game.entities[pilot].ai.waypoints.points.last().unwrap();
    assert!(
        current[0] > previous[0],
        "upgraded FRONT priority must affect the actual column goal"
    );
}
