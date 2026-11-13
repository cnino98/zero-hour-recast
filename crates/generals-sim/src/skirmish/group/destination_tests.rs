use crate::skirmish::{
    GroupKind, Kind, Order, Skirmish, ground_movement::tests::ground_rules,
    locomotor::GroundAppearance, tests::game, weapons::Lock,
};

#[test]
fn mixed_group_attack_move_sends_the_unarmed_worker_on_an_ordinary_move() {
    let mut game = game();
    let worker_type = game.type_for(0, Kind::Builder).unwrap();
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[worker_type]
        .weapon_set_rules
        .replacements
        .push(crate::skirmish::weapons::WeaponTemplateSet {
            flags: 0,
            weapons: [None; 3],
            share_lock: false,
        });
    game.catalog[worker_type].locomotor.ground = Some(ground_rules(GroundAppearance::Legs));
    game.catalog[tank_type].locomotor.ground = Some(ground_rules(GroundAppearance::Treads));
    let worker = game.spawn(worker_type, 0, [305.0, 435.0], true);
    let tank = game.spawn(tank_type, 0, [305.0, 375.0], true);
    let victim = game.spawn(tank_type, 1, [445.0, 375.0], true);
    let destination = [605.0, 405.0, 0.0];
    assert!(!game.object_can_attack(worker));
    assert!(game.object_can_attack(tank));
    game.entities[worker].weapon_set.lock = Lock::Temporary;
    game.entities[tank].weapon_set.lock = Lock::Permanent;
    game.group_attack_move_world(&[tank, worker], destination)
        .unwrap();
    assert!(matches!(game.entities[worker].ai.order, Order::Move(_)));
    assert!(matches!(game.entities[tank].ai.order, Order::AttackMove(_)));
    assert!(matches!(game.entities[worker].weapon_set.lock, Lock::None));
    assert!(matches!(
        game.entities[tank].weapon_set.lock,
        Lock::Permanent
    ));
    for _ in 0..45 {
        game.step();
    }
    assert!(game.entities[worker].position[0] > 305.0);
    assert_ne!(game.entities[worker].ai.current_victim, Some(victim));
}

#[test]
fn fixed_wing_group_admission_uses_the_current_ground_movement_query() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].group_kind = GroupKind::Aircraft;
    game.catalog[kind].locomotor.ground = Some(ground_rules(GroundAppearance::Treads));
    let actor = game.spawn(kind, 0, [305.0, 405.0], true);
    assert!(game.attack_ground_movement(actor));
    assert!(game.allows_group_formation(actor));
    let mut alternate =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Tank);
    alternate.group_kind = GroupKind::Aircraft;
    alternate.locomotor.legal_surfaces = 8;
    alternate.locomotor.flight = Some(crate::skirmish::flight::tests::rules(
        crate::skirmish::flight::Appearance::Wings,
    ));
    let kind = game.catalog.len();
    game.catalog.push(alternate);
    let actor = game.spawn(kind, 0, [305.0, 455.0], true);
    assert!(!game.attack_ground_movement(actor));
    assert!(!game.allows_group_formation(actor));
}

#[test]
fn group_margin_clamp_resamples_slope_height_and_keeps_unclipped_picked_z() {
    let (game, _) = slope();
    let picked = [5.0, 405.0, 137.0];
    let clamped = game.clamp_group_destination(picked, 40.0);
    assert_eq!(
        clamped.map(f32::to_bits),
        [40.0, 405.0, game.ground_height([40.0, 405.0])].map(f32::to_bits)
    );
    let picked = [503.25, 405.75, 137.0];
    assert_eq!(
        game.clamp_group_destination(picked, 40.0).map(f32::to_bits),
        picked.map(f32::to_bits)
    );
}

#[test]
fn individual_group_keeps_blocked_ground_goal_and_flight_offset() {
    use crate::{grid::Cell, terrain::TerrainMap};
    let mut game = game();
    let cliffs = (30..=70).flat_map(|x| (20..=59).map(move |y| Cell { x, y }));
    game.configure_movement_terrain(TerrainMap::classify(80, 60, cliffs, []).unwrap())
        .unwrap();
    game.ai_rules.min_distance_for_group = 10_000.0;
    let tank_type = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank_type].locomotor.ground = Some(ground_rules(GroundAppearance::Treads));
    let mut plane = crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Tank);
    plane.group_kind = GroupKind::Aircraft;
    plane.kind_of.push("AIRCRAFT".into());
    plane.locomotor.legal_surfaces = 8;
    plane.locomotor.flight = Some(crate::skirmish::flight::tests::rules(
        crate::skirmish::flight::Appearance::Wings,
    ));
    let plane_type = game.catalog.len();
    game.catalog.push(plane);
    let tank = game.spawn(tank_type, 0, [200.0, 400.0], true);
    let plane = game.spawn(plane_type, 0, [200.0, 430.0], true);
    let click = [500.25, 400.75, 90.0];
    assert!(game.adjusted_destination_world(tank, click).is_none());
    game.group_move_to_world(&[tank, plane], click, false)
        .unwrap();
    assert_eq!(
        game.entities[tank].ai_goal_position().map(f32::to_bits),
        [click[0], click[1], 0.0].map(f32::to_bits)
    );
    assert_eq!(
        game.entities[plane].ai_goal_position().map(f32::to_bits),
        [click[0], click[1] + 30.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn flying_only_group_guard_uses_active_locomotor_instead_of_aircraft_kind() {
    let mut game = game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[infantry].locomotor.ground = Some(ground_rules(GroundAppearance::Legs));
    game.catalog[infantry].ai.vision_range = 300.0;
    game.catalog[infantry].weapon.anti_mask = u8::MAX;
    let tank = game.type_for(1, Kind::Tank).unwrap();
    let mut aircraft =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Tank);
    aircraft.kind_of.push("AIRCRAFT".into());
    let taxiing = game.catalog.len();
    game.catalog.push(aircraft);
    let mut flyer = crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Tank);
    flyer.locomotor.legal_surfaces = 8;
    flyer.locomotor.flight = Some(crate::skirmish::flight::tests::rules(
        crate::skirmish::flight::Appearance::Wings,
    ));
    let flying = game.catalog.len();
    game.catalog.push(flyer);
    let guard = game.spawn(infantry, 0, [400.0, 300.0], true);
    let ground = game.spawn(tank, 1, [425.0, 300.0], true);
    let parked = game.spawn(taxiing, 1, [450.0, 300.0], true);
    let airborne = game.spawn(flying, 1, [500.0, 300.0], true);
    game.update_ground_contacts();
    let center = [400.0, 300.0, 0.0];
    assert!(!game.entities[parked].is_using_airborne_locomotor(&game.catalog[taxiing]));
    assert!(game.entities[airborne].is_using_airborne_locomotor(&game.catalog[flying]));
    game.group_guard_world(&[guard], center, true).unwrap();
    game.entities[guard].ai.guard.next_scan = game.tick;
    game.update_guard(guard, [center[0], center[1]]);
    assert_eq!(game.entities[guard].ai.ai_target, Some(airborne));
    game.group_guard_world(&[guard], center, false).unwrap();
    game.entities[guard].ai.guard.next_scan = game.tick;
    game.update_guard(guard, [center[0], center[1]]);
    assert_eq!(game.entities[guard].ai.ai_target, Some(ground));
}

#[test]
fn immobile_ai_in_group_expands_gather_bounds_without_receiving_a_move() {
    let mut game = game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[infantry].locomotor.ground = Some(ground_rules(GroundAppearance::Legs));
    let static_ai = game.type_for(0, Kind::Barracks).unwrap();
    assert!(game.catalog[static_ai].immobile());
    game.ai_rules.group_move_gather_factor = 1.0;
    game.ai_rules.min_distance_for_group = 10_000.0;
    let a = game.spawn(infantry, 0, [300.0, 350.0], true);
    let b = game.spawn(infantry, 0, [300.0, 410.0], true);
    let fixed = game.spawn(static_ai, 0, [600.0, 380.0], true);
    assert!(game.entities[fixed].ai.registration.is_some());
    game.group_move_to(&[a, b, fixed], [450.0, 380.0], false)
        .unwrap();
    assert!(matches!(game.entities[fixed].ai.order, Order::Idle));
    assert!(
        game.entities[a]
            .ai
            .path_request
            .pending
            .as_ref()
            .is_some_and(|request| matches!(
                request.kind,
                crate::skirmish::path_service::Kind::Tighten
            ))
    );
    assert!(
        game.entities[b]
            .ai
            .path_request
            .pending
            .as_ref()
            .is_some_and(|request| matches!(
                request.kind,
                crate::skirmish::path_service::Kind::Tighten
            ))
    );
}

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
    game.move_formation(
        &[id],
        original,
        Some(&route),
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
    )
    .unwrap();
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
    assert!(game.move_columns(
        &members,
        [300.0, 300.0],
        route[3],
        &route,
        true,
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER
    ));
    let previous = *game.entities[pilot].ai.waypoints.points.last().unwrap();
    game.entities[pilot].veterancy_upgrades |= 1 << 3;
    game.activate_upgrade_modules(pilot);
    assert!(game.native_upgrade_executed(pilot, "LocomotorSetUpgrade", 0));
    assert!(game.move_columns(
        &members,
        [300.0, 300.0],
        route[3],
        &route,
        true,
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER
    ));
    let current = *game.entities[pilot].ai.waypoints.points.last().unwrap();
    assert!(
        current[0] > previous[0],
        "upgraded FRONT priority must affect the actual column goal"
    );
}
