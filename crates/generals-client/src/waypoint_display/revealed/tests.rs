use super::*;
use generals_sim::skirmish::Kind;

fn fixture() -> (Match, usize, usize) {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    let outpost = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    let enemy = game
        .entities
        .iter()
        .position(|actor| actor.owner == 1 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    // Synthetic selection fixture over actual installed movers; Outpost's
    // separate paid/catalog admission remains a different acceptance gate.
    let kind = &mut game.catalog[game.entities[outpost].type_index];
    kind.kind_of.push("REVEALS_ENEMY_PATHS".into());
    kind.ai.vision_range = 10_000.0;
    battle.select_objects(&[outpost], true);
    (battle, outpost, enemy)
}

#[ignore = "requires original game installation"]
#[test]
fn revealed_route_uses_main_snapshot_not_current_enemy_pose_or_path_endpoint() {
    let (mut battle, outpost, enemy) = fixture();
    assert!(selected_outpost(&battle, outpost));
    assert!(points(&battle, outpost, Some(enemy)).is_none());
    let goal = battle.simulation.entities[enemy].position;
    battle.simulation.move_to(enemy, goal).unwrap();
    let snapshot = battle.simulation.entities[enemy].ai_goal_position();
    battle.simulation.entities[enemy].position[0] += 10.0;
    let route = points(&battle, outpost, Some(enemy)).unwrap();
    assert!(route.revealed);
    assert_eq!(route.marker_points.len(), 1);
    assert_eq!(
        route.marker_points[0].to_array().map(f32::to_bits),
        [snapshot[0] * 0.1, snapshot[2] * 0.1, -snapshot[1] * 0.1].map(f32::to_bits)
    );
    battle.simulation.stop(enemy).unwrap();
    assert!(points(&battle, outpost, Some(enemy)).is_none());
}

#[ignore = "requires original game installation"]
#[test]
fn reveal_requires_enemy_hover_and_full_three_dimensional_vision_range() {
    let (mut battle, outpost, enemy) = fixture();
    let goal = battle.simulation.entities[enemy].position;
    battle.simulation.move_to(enemy, goal).unwrap();
    assert!(points(&battle, outpost, None).is_none());
    assert!(points(&battle, outpost, Some(outpost)).is_none());
    let origin = battle.simulation.object_position(outpost);
    let target = battle.simulation.object_position(enemy);
    let distance = length([
        origin[0] - target[0],
        origin[1] - target[1],
        origin[2] - target[2],
    ]);
    let type_index = battle.simulation.entities[outpost].type_index;
    battle.simulation.catalog[type_index].ai.vision_range = distance;
    assert!(points(&battle, outpost, Some(enemy)).is_some());
    battle.simulation.catalog[type_index].ai.vision_range = f32::from_bits(distance.to_bits() - 1);
    assert!(points(&battle, outpost, Some(enemy)).is_none());
    battle.simulation.entities[outpost].owner = 1;
    assert!(!selected_outpost(&battle, outpost));
}

#[ignore = "requires original game installation"]
#[test]
fn remaining_waypoints_take_priority_over_the_main_goal_snapshot() {
    let (mut battle, outpost, enemy) = fixture();
    let position = battle.simulation.entities[enemy].position;
    battle.simulation.queue_move_to(enemy, position).unwrap();
    battle.simulation.queue_move_to(enemy, position).unwrap();
    let route = points(&battle, outpost, Some(enemy)).unwrap();
    assert_eq!(route.marker_points.len(), 2);
    assert_eq!(route.points.len(), 3);
}
