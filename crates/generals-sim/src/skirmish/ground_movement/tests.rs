use super::super::locomotor::{GroundAppearance, GroundRules};
use super::super::{Kind, Order, Skirmish};
use super::distance_squared;

pub(in crate::skirmish) fn moving_tank() -> (Skirmish, usize) {
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].locomotor.ground = Some(ground_rules(GroundAppearance::Treads));
    let id = game.spawn(tank, 0, [300.0, 150.0], true);
    game.entities[id].ai.mood.next_check = u64::MAX;
    (game, id)
}

pub(in crate::skirmish) const fn ground_rules(appearance: GroundAppearance) -> GroundRules {
    GroundRules {
        wander: None,
        is_dozer: false,
        downhill_only: false,
        appearance,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.1,
        lateral_friction: 0.2,
        extra_friction: 0.0,
    }
}

#[test]
fn ordinary_ground_move_arrives_at_native_grid_goal_without_teleporting() {
    let (mut game, id) = moving_tank();
    let clicked = [453.0, 127.0];
    game.move_to(id, clicked).unwrap();
    assert!(game.entities[id].ai.path_request.pending.is_some());
    game.step();
    let cell = game.entities[id].ai.path_goal.unwrap();
    let destination = game.path_position(id, cell);
    for _ in 0..1200 {
        let before = game.entities[id].position;
        game.step();
        if matches!(game.entities[id].ai.order, Order::Idle) {
            // Arrival changes the state and lets maintain-position scrub the
            // driven velocity; it must not snap the actor to the final node.
            assert_eq!(
                game.entities[id].position.map(f32::to_bits),
                before.map(f32::to_bits)
            );
            break;
        }
    }
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(distance_squared(game.entities[id].position, destination) < 1.0);
    assert!(distance_squared(game.entities[id].position, clicked) > 1.0);
    assert!(game.entities[id].ai.ground_path.is_none());
}

#[test]
fn redirect_retains_native_velocity_until_the_next_locomotor_update() {
    let (mut game, id) = moving_tank();
    game.move_to(id, [450.0, 150.0]).unwrap();
    for _ in 0..30 {
        game.step();
    }
    let velocity = game.entities[id].ai.motion.velocity;
    assert!(velocity.iter().any(|component| component.abs() > 0.0));
    game.move_to(id, [400.0, 250.0]).unwrap();
    assert_eq!(
        game.entities[id].ai.motion.velocity.map(f32::to_bits),
        velocity.map(f32::to_bits)
    );
}

#[test]
fn intermediate_waypoint_keeps_motion_and_the_source_extra_distance() {
    let (mut game, id) = moving_tank();
    game.queue_move_to(id, [400.0, 150.0]).unwrap();
    game.queue_move_to(id, [500.0, 150.0]).unwrap();
    for _ in 0..900 {
        game.step();
        if game.entities[id].ai.waypoints.index == 1 {
            assert!(game.entities[id].ai.motion.velocity[0] > 0.0);
            assert_eq!(game.entities[id].ai.motion.command_frame, Some(game.tick));
            // The native update's final branch leaves the prior extra distance.
            assert_eq!(
                game.entities[id].ai.command.path_extra_distance.to_bits(),
                100.0_f32.to_bits()
            );
            assert!(game.entities[id].ai.ground_path.is_some());
            // The source queue now runs after AIUpdate in this same frame.
            assert!(game.entities[id].ai.path_request.pending.is_none());
            // The path request uses the cell center. The waypoint list keeps the
            // player's coordinates for display and any subsequent append.
            assert_eq!(
                game.entities[id].ai.path_request.last_goal,
                Some([505.0, 155.0])
            );
            let (points, index) = game.entities[id].waypoint_path().unwrap();
            assert_eq!(
                points[index].map(f32::to_bits),
                [500.0, 150.0, game.ground_height([500.0, 150.0])].map(f32::to_bits)
            );
            game.step();
            assert!(game.entities[id].ai.path_request.pending.is_none());
            assert_eq!(game.entities[id].ai.motion.command_frame, Some(game.tick));
            assert!(game.entities[id].ai.motion.velocity[0] > 0.0);
            return;
        }
    }
    panic!("The tank did not advance to its second waypoint");
}

#[test]
fn boneless_ground_docking_completes_a_paid_supply_trip_with_native_arrival() {
    let mut game = crate::skirmish::tests::game();
    // The docking scenario assumes normal construction, with supplied energy.
    game.catalog[0].energy = 1;
    game.update_power();
    let builder = game.entities[1].type_index;
    game.catalog[builder].locomotor.ground = Some(ground_rules(GroundAppearance::Legs));
    let center_type = game.type_for(0, Kind::SupplyCenter).unwrap();
    game.step();
    let position = game.placement_near(center_type, [185.0, 250.0]).unwrap();
    let center = game.build(1, center_type, position).unwrap();
    for _ in 0..1200 {
        game.step();
        if game.entities[center].complete() {
            break;
        }
    }
    assert!(game.entities[center].complete());
    let cash = game.players[0].cash;
    let boxes = game.supplies[0].boxes;
    game.harvest(1, 0).unwrap();
    let mut collected = false;
    for _ in 0..1800 {
        game.step();
        collected |= game.entities[1].cargo > 0;
        if collected && game.entities[1].cargo == 0 {
            assert!(game.supplies[0].boxes < boxes);
            assert!(game.players[0].cash >= cash + game.rules.supply_value);
            return;
        }
    }
    panic!("The ground collector failed to gather and deposit its source-valued cargo");
}

#[test]
fn produced_ground_unit_continues_to_user_rally_without_an_idle_physics_frame() {
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].locomotor.ground = Some(ground_rules(GroundAppearance::Treads));
    let factory_type = game.type_for(0, Kind::Factory).unwrap();
    let factory = game.spawn(factory_type, 0, [305.0, 405.0], true);
    game.set_rally_point(factory, [505.0, 405.0, 0.0]).unwrap();
    game.train(factory, tank).unwrap();
    let mut leaving_factory = false;
    for _ in 0..1200 {
        game.step();
        let Some(actor) = game.entities.iter().find(|actor| actor.type_index == tank) else {
            continue;
        };
        if actor.ai.ignored_producer == Some(factory) {
            leaving_factory = true;
        } else if leaving_factory {
            assert!(matches!(actor.ai.order, Order::Exit(_)));
            assert!(actor.ai.ground_path.is_some());
            assert_eq!(actor.ai.motion.command_frame, Some(game.tick));
            assert!(actor.ai.motion.velocity[0] > 0.0);
            assert!(actor.ai.path_request.pending.is_none());
            assert_eq!(actor.ai.path_request.last_goal, Some([505.0, 405.0]));
            let id = game
                .entities
                .iter()
                .position(|actor| actor.type_index == tank)
                .unwrap();
            game.step();
            assert!(game.entities[id].ai.path_request.pending.is_none());
            assert_eq!(game.entities[id].ai.motion.command_frame, Some(game.tick));
            assert!(game.entities[id].ai.motion.velocity[0] > 0.0);
            return;
        }
    }
    panic!("The produced tank did not continue from the factory exit to its rally");
}

#[test]
fn factory_exit_rebuild_retains_detour_after_its_first_quick_path() {
    let (mut game, _) = moving_tank();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let factory_type = game.type_for(0, Kind::Factory).unwrap();
    let factory = game.spawn(factory_type, 0, [305.0, 405.0], true);
    let id = game.spawn_produced_unit(factory, tank, false).unwrap();
    assert!(game.entities[id].ai.path_request.pending.is_some());
    game.process_path_service();
    assert_eq!(
        game.entities[id]
            .ai
            .ground_path
            .as_ref()
            .unwrap()
            .positions()
            .count(),
        2
    );
    assert!(!game.entities[id].ai.waypoints.exit_quick_pending);

    // A second structure blocks the ordinary recovery route after the first
    // factory quick path. Only the original producer remains ignored.
    game.spawn(factory_type, 0, [405.0, 405.0], true);
    game.rebuild_navigation();
    game.refresh_occupancy();
    let from = game.entities[id].position;
    let destination = [505.0, 405.0];
    assert!(!game.ground_line_passable_on(id, from, destination, 1));
    let replacement = game
        .movement_path_world(id, from, [destination[0], destination[1], 0.0])
        .route
        .unwrap();
    assert!(replacement.quick_start.is_none());
    let original_raw = replacement.raw.clone();
    game.entities[id].ai.path_goal = Some(replacement.optimized.back().unwrap().cell);
    game.entities[id].ai.path_endpoint = Some(destination);
    game.entities[id].ai.path = replacement;
    let expected = game.ordinary_ground_nodes(id);
    assert!(
        expected.len() > 2,
        "the blocking structure requires a real detour"
    );
    game.entities[id].ai.ground_path = None;
    game.install_ground_path(id);
    let recovered: Vec<_> = game.entities[id]
        .ai
        .ground_path
        .as_ref()
        .unwrap()
        .positions()
        .collect();
    assert_eq!(recovered.len(), expected.len());
    for (actual, expected) in recovered.iter().zip(&expected) {
        assert_eq!(
            actual.map(f32::to_bits),
            [expected.position[0], expected.position[1]].map(f32::to_bits)
        );
    }
    assert!(game.entities[id].ai.path.raw == original_raw);
    assert!(matches!(game.entities[id].ai.order, Order::Exit(_)));
    assert_eq!(game.entities[id].ai.ignored_producer, Some(factory));
    assert!(!game.entities[id].ai.waypoints.exit_quick_pending);
    game.stop(id).unwrap();
    assert!(!game.entities[id].ai.waypoints.exit_quick_pending);
}

#[test]
fn production_exit_skips_a_following_rally_within_one_pathfind_cell() {
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].locomotor.ground = Some(ground_rules(GroundAppearance::Treads));
    let factory_type = game.type_for(0, Kind::Factory).unwrap();
    let factory = game.spawn(factory_type, 0, [305.0, 405.0], true);
    // The fixture's natural exit is x=360 after its source 20-unit extension.
    game.set_rally_point(factory, [365.0, 405.0, 0.0]).unwrap();
    let id = game.spawn_produced_unit(factory, tank, false).unwrap();
    for _ in 0..900 {
        game.step();
        let actor = &game.entities[id];
        if actor.ai.ignored_producer.is_none() {
            assert!(matches!(actor.ai.order, Order::Idle));
            assert!(actor.ai.ground_path.is_none());
            assert!(distance_squared(actor.position, [365.0, 405.0]) > 1.0);
            return;
        }
    }
    panic!("The ground unit did not complete its factory exit");
}

#[test]
fn stuck_route_patches_around_new_occupancy_and_retains_its_destination() {
    let (mut game, id) = moving_tank();
    game.move_to(id, [505.0, 155.0]).unwrap();
    game.process_path_service();
    let goal = game.entities[id].ai.path_goal.unwrap();
    let tank = game.entities[id].type_index;
    let blocker = game.spawn(tank, 0, [345.0, 155.0], true);
    game.reserve_goal(blocker, [345.0, 155.0]);
    game.refresh_occupancy();
    game.tick = 10;
    game.entities[id].ai.blocking.stuck = true;
    assert!(!game.recover_ground_path(id));
    assert!(game.entities[id].ai.path_request.pending.is_some());
    game.tick += 1;
    game.process_path_service();
    assert!(game.entities[id].ai.path_goal == Some(goal));
    assert!(
        game.entities[id]
            .ai
            .path
            .raw
            .iter()
            .any(|cell| cell.cell.y != 15)
    );
    assert!(
        !game.entities[id]
            .ai
            .path
            .raw
            .contains(&crate::grid::LayerCell {
                cell: crate::Cell { x: 34, y: 15 },
                layer: 1
            })
    );
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
    for _ in 0..1200 {
        game.step();
        if matches!(game.entities[id].ai.order, Order::Idle) {
            break;
        }
    }
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(distance_squared(game.entities[id].position, game.path_position(id, goal)) < 1.0);
}

#[test]
fn repeated_stuck_request_waits_without_losing_the_move_order() {
    let (mut game, id) = moving_tank();
    game.tick = 10;
    game.move_to(id, [505.0, 155.0]).unwrap();
    game.process_path_service();
    game.tick = 11;
    game.entities[id].ai.blocking.stuck = true;
    assert!(!game.recover_ground_path(id));
    assert!(game.entities[id].ai.path_request.pending.is_some());
    assert_eq!(game.entities[id].ai.blocking.ignore_until, 71);
    assert!(game.entities[id].ai.ground_path.is_some());
    for _ in 0..29 {
        game.step();
        assert!(game.entities[id].ai.path_request.pending.is_some());
    }
    // Deadline 41 wakes during AI state work and is serviced later that frame.
    game.step();
    assert_eq!(game.tick, 41);
    assert!(game.entities[id].ai.path_request.pending.is_none());
    assert!(game.entities[id].ai.blocking.retry_at.is_none());
    assert!(matches!(game.entities[id].ai.order, Order::Move(_)));
    game.stop_actor(id).unwrap();
    assert!(game.entities[id].ai.blocking.retry_at.is_none());
    // AIInternalMoveToState::onExit ends locomotion but retains the path.
    assert!(game.entities[id].ai.ground_path.is_some());
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(!game.entities[id].ai.motion.is_moving());
}

#[test]
fn quick_attack_retains_deck_layers_and_original_node_height() {
    let (mut game, id) = moving_tank();
    let start = game.cell_at(game.entities[id].position).unwrap();
    let end = game.cell_at([350.0, 150.0]).unwrap();
    game.entities[id].layer = 2;
    let mut route = crate::grid::Route::direct_on(
        crate::grid::LayerCell {
            cell: start,
            layer: 2,
        },
        crate::grid::LayerCell {
            cell: end,
            layer: 2,
        },
    );
    route.quick_start = Some([300.0, 150.0, 23.0]);
    route.exact_endpoint = Some([350.0, 150.0, 23.0]);
    game.entities[id].ai.path = route;
    game.entities[id].ai.path_goal = Some(end);
    game.install_ground_path(id);
    assert!(
        game.entities[id]
            .ai
            .path
            .raw
            .iter()
            .all(|node| node.layer == 2)
    );
    let mut path = game.entities[id].ai.ground_path.take().unwrap();
    let sample = path.sample_layer([350.0, 150.0, 23.0], 2, |_, _, _| true);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [350.0_f32, 150.0, 23.0].map(f32::to_bits)
    );
}

#[test]
fn contact_endpoint_keeps_intermediate_route_and_authored_tail_height() {
    let (mut game, id) = moving_tank();
    // The source optimizer removes an unobstructed detour. This water block
    // makes the existing intermediate-route assertion physically necessary.
    let width = game.grid.width();
    let height = game.grid.height();
    let water = (35..41).flat_map(|x| (10..21).map(move |y| crate::grid::Cell { x, y }));
    let terrain = crate::terrain::TerrainMap::classify(width, height, [], water).unwrap();
    game.grid = crate::grid::Grid::new(width, height, terrain.blocked_ground_cells()).unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    let cells = [
        [300.0, 150.0],
        [300.0, 250.0],
        [450.0, 250.0],
        [450.0, 150.0],
    ]
    .map(|point| crate::grid::LayerCell {
        cell: game.cell_at(point).unwrap(),
        layer: 1,
    });
    let mut route = crate::grid::Route::direct_on(cells[0], cells[3]);
    route.raw = cells.to_vec();
    route.optimized = cells.into_iter().collect();
    route.exact_endpoint = Some([453.25, 150.25, 19.0]);
    game.entities[id].ai.path = route;
    game.entities[id].ai.path_goal = Some(cells[3].cell);
    game.install_ground_path(id);
    let mut path = game.entities[id].ai.ground_path.take().unwrap();
    assert!(path.positions().any(|point| point[1] > 200.0));
    let sample = path.sample_layer([453.25, 150.25, 19.0], 1, |_, _, _| true);
    assert_eq!(
        sample.position.map(f32::to_bits),
        [453.25_f32, 150.25, 19.0].map(f32::to_bits)
    );
}

#[test]
fn unrelated_structure_registration_retains_active_route_and_requested_goal() {
    let (mut game, actor) = moving_tank();
    game.move_to(actor, [500.0, 150.0]).unwrap();
    game.process_path_service();
    let route = game.entities[actor].ai.path.optimized.clone();
    let goal = game.entities[actor].ai.path_goal;
    let requested = game.entities[actor]
        .ai
        .command
        .goal_position
        .map(f32::to_bits);
    let nodes: Vec<_> = game.entities[actor]
        .ai
        .ground_path
        .as_ref()
        .unwrap()
        .positions()
        .map(|point| point.map(f32::to_bits))
        .collect();
    let building = game.type_for(0, Kind::CommandCenter).unwrap();
    game.spawn(building, 0, [705.0, 505.0], true);
    game.rebuild_navigation();
    assert!(game.entities[actor].ai.path.optimized == route);
    assert!(game.entities[actor].ai.path_goal == goal);
    assert_eq!(
        game.entities[actor]
            .ai
            .command
            .goal_position
            .map(f32::to_bits),
        requested
    );
    let retained: Vec<_> = game.entities[actor]
        .ai
        .ground_path
        .as_ref()
        .unwrap()
        .positions()
        .map(|point| point.map(f32::to_bits))
        .collect();
    assert_eq!(retained, nodes);
}

#[test]
fn noncentered_destination_resolves_its_layer_after_the_native_half_cell_shift() {
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].locomotor.ground = Some(ground_rules(GroundAppearance::Treads));
    game.catalog[tank].geometry.major = 10.0;
    game.catalog[tank].geometry.minor = 10.0;
    game.catalog[tank].geometry.shape = crate::collision_geometry::Shape::Sphere;
    let id = game.spawn(tank, 0, [105.0, 205.0], true);
    let mut terrain =
        crate::terrain::TerrainMap::classify(game.grid.width(), game.grid.height(), [], [])
            .unwrap();
    terrain
        .add_bridge(
            crate::terrain::BridgeGeometry {
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
    let destination = [296.0, 200.0, 80.0];
    let from = game.object_position(id);
    let hierarchy = game.hierarchy_query(id, from, destination);
    let ordinary = game.path_query(id, from, destination, None).unwrap();
    assert!(!ordinary.centered);
    assert_eq!(hierarchy.goal.layer, 1);
    assert_eq!(ordinary.goal.layer, 2);
    assert_eq!([ordinary.goal.cell.x, ordinary.goal.cell.y], [30, 20]);
}

#[test]
fn ordinary_shift_overflow_fails_but_closest_clips_the_shifted_goal() {
    let mut game = crate::skirmish::tests::game();
    let unit = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[unit].locomotor.ground = Some(ground_rules(GroundAppearance::Treads));
    game.catalog[unit].geometry.major = 10.0;
    game.catalog[unit].geometry.minor = 10.0;
    game.catalog[unit].geometry.shape = crate::collision_geometry::Shape::Sphere;
    let id = game.spawn(unit, 0, [105.0, 205.0], true);
    let from = game.object_position(id);
    let width = f32::from(game.grid.width()) * 10.0;
    let target = [width - 1.0, 205.0, 0.0];
    assert!(game.path_query(id, from, target, None).is_none());
    let closest = game.path_query(id, from, target, Some(0.0)).unwrap();
    assert_eq!(closest.goal.cell.x, game.grid.width() - 1);
    assert_eq!(closest.goal.layer, 1);
}

fn bridge_construction_game(from_x: f32) -> (Skirmish, usize, u8) {
    let (mut game, id) = moving_tank();
    let width = game.grid.width();
    let height = game.grid.height();
    let water = (32..40).flat_map(|x| (0..height).map(move |y| crate::grid::Cell { x, y }));
    let mut terrain = crate::terrain::TerrainMap::classify(width, height, [], water).unwrap();
    let layer = terrain
        .add_bridge(
            crate::terrain::BridgeGeometry {
                from: [from_x, 205.0, 80.0],
                to: [410.0, 205.0, 80.0],
                corners: [
                    [from_x, 230.0, 80.0],
                    [from_x, 180.0, 80.0],
                    [410.0, 230.0, 80.0],
                    [410.0, 180.0, 80.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    game.grid = crate::grid::Grid::new(width, height, terrain.blocked_ground_cells()).unwrap();
    game.configure_movement_terrain(terrain).unwrap();
    game.entities[id].position = [275.25, 205.75];
    game.refresh_occupancy();
    (game, id, layer)
}

#[test]
fn ordinary_bridge_raw_transitions_keep_survivor_height_and_raw_predecessor() {
    let (mut game, id, layer) = bridge_construction_game(300.0);
    let route = game
        .movement_path(id, game.entities[id].position, [435.0, 205.0])
        .unwrap();
    let transitions: Vec<_> = route
        .raw
        .windows(2)
        .filter(|pair| pair[0].layer != pair[1].layer)
        .collect();
    assert_eq!(transitions.len(), 2);
    let entry = transitions[0];
    assert_eq!([entry[0].layer, entry[1].layer], [1, layer]);
    assert!(entry[0].cell == entry[1].cell);
    assert_eq!(
        game.movement_terrain.ground_bridge(entry[0].cell),
        Some(layer)
    );
    let exit = transitions[1];
    assert_eq!([exit[0].layer, exit[1].layer], [layer, 1]);
    if exit[0].cell == exit[1].cell {
        assert!(
            game.movement_terrain
                .bridge_cell(layer, exit[0].cell)
                .unwrap()
                .connect_ground
        );
    } else {
        assert!(exit[0].cell.x.abs_diff(exit[1].cell.x) <= 1);
        assert!(exit[0].cell.y.abs_diff(exit[1].cell.y) <= 1);
        assert!(
            game.navigation.resolve_layer(crate::grid::LayerCell {
                cell: exit[1].cell,
                layer,
            }) == exit[1]
        );
        assert!(!game.navigation.layer_blocked(exit[1]));
    }
    let original_raw = route.raw.clone();
    game.entities[id].ai.path_goal = Some(route.optimized.back().unwrap().cell);
    game.entities[id].ai.path = route;
    let raw = game.raw_ground_nodes(id, &game.navigation);
    let canonical =
        crate::grid::construction::prepend_cells(&raw, game.object_position(id)).unwrap();
    let nodes = game.ordinary_ground_nodes(id);
    game.install_ground_path(id);
    assert!(game.entities[id].ai.path.raw == original_raw);
    assert_eq!(
        nodes[0].position.map(f32::to_bits),
        game.object_position(id).map(f32::to_bits)
    );
    let last = nodes.last().unwrap();
    assert_eq!(last.layer, 1);
    assert_eq!(last.position[2].to_bits(), 0.0_f32.to_bits());
    for node in nodes {
        let original = canonical
            .nodes
            .iter()
            .find(|original| original.position[..2] == node.position[..2])
            .unwrap();
        assert_eq!(
            node.position.map(f32::to_bits),
            original.position.map(f32::to_bits)
        );
        assert_eq!(node.layer, original.layer);
        assert_eq!(node.previous_layer, original.previous_layer);
    }
}

#[test]
fn source_valid_bridge_connector_chain_retains_downstream_height_when_collapsed() {
    let (mut game, id, layer) = bridge_construction_game(295.0);
    let node = |x, layer| crate::grid::LayerCell {
        cell: crate::grid::Cell { x, y: 20 },
        layer,
    };
    // Both same-XY connections exist in this native-classified bridge. This
    // valid parent chain isolates collapse; the search may choose padding exit.
    for x in [29, 41] {
        let cell = node(x, layer).cell;
        assert!(
            game.movement_terrain
                .bridge_cell(layer, cell)
                .unwrap()
                .connect_ground
        );
        assert_eq!(game.movement_terrain.ground_bridge(cell), Some(layer));
    }
    let mut route = crate::grid::Route::new();
    route.raw = vec![node(27, 1), node(28, 1), node(29, 1)];
    route.raw.extend((29..=41).map(|x| node(x, layer)));
    route.raw.extend((41..=43).map(|x| node(x, 1)));
    for &cell in &route.raw {
        assert!(game.navigation.resolve_layer(cell) == cell);
        assert!(!game.navigation.layer_blocked(cell));
    }
    let original_raw = route.raw.clone();
    game.entities[id].ai.path = route;
    let raw = game.raw_ground_nodes(id, &game.navigation);
    let geometry = game.movement_terrain.bridge_geometry(layer).unwrap();
    assert!(geometry.contains([raw[3].position[0], raw[3].position[1]]));
    assert_eq!(raw[2].position[2].to_bits(), 0.0_f32.to_bits());
    assert_eq!(raw[3].position[2].to_bits(), 80.0_f32.to_bits());
    let canonical =
        crate::grid::construction::prepend_cells(&raw, game.object_position(id)).unwrap();
    for (x, height) in [(29, 80.0_f32), (41, 0.0_f32)] {
        let collapsed = canonical
            .nodes
            .iter()
            .find(|node| node.cell == [x, 20])
            .unwrap();
        assert_eq!(collapsed.layer, layer);
        assert_eq!(collapsed.position[2].to_bits(), height.to_bits());
    }
    assert!(
        canonical
            .nodes
            .windows(2)
            .all(|pair| pair[0].cell != pair[1].cell)
    );
    assert!(game.entities[id].ai.path.raw == original_raw);
}

#[test]
fn deck_locomotor_does_not_correct_away_from_the_ground_under_its_layer() {
    let mut velocities = Vec::new();
    for on_deck in [false, true] {
        let (mut game, id) = moving_tank();
        let mut terrain =
            crate::terrain::TerrainMap::classify(game.grid.width(), game.grid.height(), [], [])
                .unwrap();
        let layer = terrain
            .add_bridge(
                crate::terrain::BridgeGeometry {
                    from: [300.0, 205.0, 5.0],
                    to: [410.0, 205.0, 5.0],
                    corners: [
                        [300.0, 230.0, 5.0],
                        [300.0, 180.0, 5.0],
                        [410.0, 230.0, 5.0],
                        [410.0, 180.0, 5.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        game.configure_movement_terrain(terrain).unwrap();
        let position = [305.0, 205.0];
        assert!(!game.movement_terrain.valid_movement(position, 1));
        // These are exactly the nine offsets used by fixInvalidPosition.
        for y in -1_i8..=1 {
            for x in -1_i8..=1 {
                let offset = [10.0 * f32::from(x), 10.0 * f32::from(y)];
                let probe = [position[0] + offset[0], position[1] + offset[1]];
                assert_eq!(
                    game.movement_terrain.valid_movement(probe, 1),
                    x == -1,
                    "ground entry classification for offset [{x}, {y}]"
                );
                assert!(game.movement_terrain.valid_movement_on(layer, probe, 1));
            }
        }
        let actor = &mut game.entities[id];
        actor.position = position;
        actor.layer = if on_deck { layer } else { 1 };
        actor.facing = 0.0;
        actor
            .ai
            .motion
            .request_path(crate::skirmish::locomotor::GroundGoal {
                height: 5.0,
                position: [395.0, 205.0],
                distance: 90.0,
                desired_speed: 1.0,
                no_slow_down: false,
                ultra_accurate: false,
            });
        game.update_ground_locomotor(id);
        let actor = &mut game.entities[id];
        let unit = &game.catalog[actor.type_index];
        crate::skirmish::locomotor::integrate_ground(actor, unit, game.tick);
        velocities.push(actor.ai.motion.velocity[0]);
    }
    assert!(
        velocities[0] < 0.0,
        "ground actor must leave forbidden bridge terrain: {velocities:?}"
    );
    assert!(
        velocities[1] > 0.0,
        "deck actor must continue toward its positive-X goal: {velocities:?}"
    );
}
