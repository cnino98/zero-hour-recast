use crate::skirmish::{Kind, Order};

#[test]
fn formation_toggle_preserves_source_offsets_and_consumes_ids_on_removal() {
    let mut game = crate::skirmish::tests::game();
    let unit = game.type_for(0, Kind::Tank).unwrap();
    let left = game.spawn(unit, 0, [105.0, 105.0], true);
    let right = game.spawn(unit, 0, [155.0, 125.0], true);
    game.group_create_formation(&[left, right]).unwrap();
    assert_eq!(game.entities[left].ai.command.formation_id, Some(1));
    assert_eq!(game.entities[right].ai.command.formation_id, Some(1));
    assert_eq!(
        game.entities[left]
            .ai
            .command
            .formation_offset
            .map(f32::to_bits),
        [-25.0, -10.0].map(f32::to_bits)
    );
    assert_eq!(
        game.entities[right]
            .ai
            .command
            .formation_offset
            .map(f32::to_bits),
        [25.0, 10.0].map(f32::to_bits)
    );
    game.group_create_formation(&[left, right]).unwrap();
    assert!(game.entities[left].ai.command.formation_id.is_none());
    assert_eq!(game.next_formation_id, 3);
    game.group_create_formation(&[left]).unwrap();
    assert_eq!(game.entities[left].ai.command.formation_id, Some(3));
    game.group_create_formation(&[left]).unwrap();
    assert!(game.entities[left].ai.command.formation_id.is_none());
}

#[test]
fn formation_move_preserves_offsets_without_individual_distance_cap() {
    let mut game = crate::skirmish::tests::game();
    let unit = game.type_for(0, Kind::Tank).unwrap();
    let left = game.spawn(unit, 0, [105.0, 105.0], true);
    let right = game.spawn(unit, 0, [305.0, 105.0], true);
    game.group_create_formation(&[left, right]).unwrap();
    game.group_move_to(&[left, right], [500.0, 400.0], false)
        .unwrap();
    let goal = |id: usize| match game.entities[id].ai.order {
        Order::Move(point) => point,
        Order::FollowPath => {
            let point = game.entities[id].ai.waypoints.points.last().unwrap();
            [point[0], point[1]]
        }
        _ => panic!("formation must issue a source move or follow command"),
    };
    let left_goal = goal(left);
    let right_goal = goal(right);
    assert_eq!(
        (right_goal[0] - left_goal[0]).to_bits(),
        200.0_f32.to_bits()
    );
    assert_eq!(right_goal[1].to_bits(), left_goal[1].to_bits());
    assert_eq!(game.entities[left].ai.command.formation_id, Some(1));
    assert_eq!(game.entities[right].ai.command.formation_id, Some(1));
}

#[test]
fn penalized_member_does_not_slow_formation_and_first_member_id_controls_toggle() {
    let mut game = crate::skirmish::tests::game();
    let unit = game.type_for(0, Kind::Tank).unwrap();
    let first = game.spawn(unit, 0, [105.0, 105.0], true);
    let other = game.spawn(unit, 0, [155.0, 105.0], true);
    game.entities[first].health = game.entities[first].max_health * 0.1;
    game.entities[first].ai.command.formation_id = Some(17);
    game.entities[other].ai.command.formation_id = Some(22);
    game.update_formation_speed(&[first, other]);
    assert_eq!(
        game.entities[first].ai.command.formation_speed,
        Some(game.entities[other].upgraded_speed(&game.catalog[unit]))
    );
    game.group_create_formation(&[first, other]).unwrap();
    assert!(game.entities[first].ai.command.formation_id.is_none());
    assert!(game.entities[other].ai.command.formation_id.is_none());
}

#[test]
fn formation_center_includes_immobile_ai_but_excludes_objects_without_ai() {
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let turret = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[turret].ai.update = crate::skirmish::UnitAiUpdate::Ordinary;
    game.catalog[turret].speed = 0.0;
    let building = game.type_for(0, Kind::CommandCenter).unwrap();
    game.catalog[building].ai.update = crate::skirmish::UnitAiUpdate::Absent;
    let mobile = game.spawn(tank, 0, [105.0, 105.0], true);
    let stationary = game.spawn(turret, 0, [205.0, 105.0], true);
    let no_ai = game.spawn(building, 0, [1005.0, 1005.0], true);
    game.group_create_formation(&[no_ai, mobile, stationary])
        .unwrap();
    assert!(game.entities[no_ai].ai.command.formation_id.is_none());
    assert_eq!(
        game.entities[mobile]
            .ai
            .command
            .formation_offset
            .map(f32::to_bits),
        [-50.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(
        game.entities[stationary]
            .ai
            .command
            .formation_offset
            .map(f32::to_bits),
        [50.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(
        game.entities[stationary].ai.command.formation_id,
        game.entities[mobile].ai.command.formation_id
    );
}
