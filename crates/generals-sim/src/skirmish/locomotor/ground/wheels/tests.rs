use super::{MOVING_BACKWARDS, THREE_POINT_TURN, move_wheels};
use crate::skirmish::locomotor::ground::{
    GroundAppearance, GroundGoal, GroundRules, integrate_ground,
};
use crate::skirmish::{Kind, Order, Skirmish};

fn wheeled_unit(can_move_backwards: bool) -> (Skirmish, usize) {
    let mut game = crate::skirmish::tests::game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[tank];
    unit.speed = 60.0;
    unit.locomotor.ground = Some(GroundRules {
        wander: None,
        appearance: GroundAppearance::Wheels {
            min_turn_speed: 20.0 / 30.0,
            can_move_backwards,
        },
        is_dozer: false,
        downhill_only: false,
        acceleration: 1000.0 / 900.0,
        acceleration_damaged: 1000.0 / 900.0,
        braking: 1000.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    let id = game.spawn(tank, 0, [100.0, 100.0], true);
    game.entities[id].facing = 0.0;
    (game, id)
}

fn goal(position: [f32; 2]) -> GroundGoal {
    GroundGoal {
        height: 0.0,
        position,
        distance: 100.0,
        desired_speed: 2.0,
        no_slow_down: false,
        ultra_accurate: false,
    }
}

#[test]
fn stopped_wheels_accelerate_before_they_can_turn() {
    let (mut game, id) = wheeled_unit(false);
    let unit = &game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    let goal = goal([100.0, 200.0]);
    move_wheels(
        actor,
        unit,
        &goal,
        100.0,
        2.0,
        &super::Force {
            terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0,
            },
            mass: 50.0,
            frame: 1,
        },
        &mut |_| true,
    );
    assert_eq!(actor.facing.to_bits(), 0.0_f32.to_bits());
    integrate_ground(actor, unit, 1);
    assert!((actor.ai.motion.velocity[0] - 20.0 / 30.0).abs() < 0.000_001);
    move_wheels(
        actor,
        unit,
        &goal,
        100.0,
        2.0,
        &super::Force {
            terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0,
            },
            mass: 50.0,
            frame: 2,
        },
        &mut |_| true,
    );
    assert!(actor.facing > 0.0);
}

#[test]
fn nearby_reverse_and_distant_three_point_turn_choose_different_headings() {
    for (distance, three_point) in [(20.0, false), (100.0, true)] {
        let (mut game, id) = wheeled_unit(true);
        let unit = &game.catalog[game.entities[id].type_index];
        let actor = &mut game.entities[id];
        let goal = goal([100.0 - distance, 100.0]);
        move_wheels(
            actor,
            unit,
            &goal,
            distance,
            2.0,
            &super::Force {
                terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                    terrain: game.projectile_terrain.as_ref(),
                    layers: &game.movement_terrain,
                    height: 0.0,
                },
                mass: 50.0,
                frame: 1,
            },
            &mut |_| true,
        );
        assert_ne!(actor.ai.motion.flags & MOVING_BACKWARDS, 0);
        assert_eq!(actor.ai.motion.flags & THREE_POINT_TURN != 0, three_point);
        integrate_ground(actor, unit, 1);
        assert!(actor.ai.motion.velocity[0] < 0.0);
        move_wheels(
            actor,
            unit,
            &goal,
            distance,
            2.0,
            &super::Force {
                terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                    terrain: game.projectile_terrain.as_ref(),
                    layers: &game.movement_terrain,
                    height: 0.0,
                },
                mass: 50.0,
                frame: 2,
            },
            &mut |_| true,
        );
        if three_point {
            assert!(actor.facing > 0.0);
        } else {
            assert_eq!(actor.facing.to_bits(), 0.0_f32.to_bits());
        }
    }
}

#[test]
fn invalid_turn_terrain_probes_half_first_then_turns_without_accelerating() {
    for invalid_probe in [1, 2] {
        let (mut game, id) = wheeled_unit(false);
        let unit = &game.catalog[game.entities[id].type_index];
        let actor = &mut game.entities[id];
        let initial = actor.position;
        let mut probes = Vec::new();
        let mut valid = |point| {
            probes.push(point);
            probes.len() != invalid_probe
        };
        move_wheels(
            actor,
            unit,
            &goal([100.0, 200.0]),
            100.0,
            2.0,
            &super::Force {
                terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                    terrain: game.projectile_terrain.as_ref(),
                    layers: &game.movement_terrain,
                    height: 0.0,
                },
                mass: 50.0,
                frame: 1,
            },
            &mut valid,
        );
        assert_eq!(probes.len(), invalid_probe);
        if invalid_probe == 2 {
            for axis in 0..2 {
                let full_offset = probes[1][axis] - initial[axis];
                let half_offset = probes[0][axis] - initial[axis];
                assert!(2.0_f32.mul_add(-half_offset, full_offset).abs() < 0.000_02);
            }
        }
        assert!(actor.facing > 0.0);
        integrate_ground(actor, unit, 1);
        assert_eq!(actor.ai.motion.velocity.map(f32::to_bits), [0; 2]);
        assert_eq!(actor.position.map(f32::to_bits), initial.map(f32::to_bits));
    }
}

#[test]
fn near_goal_donut_braking_starts_after_not_on_the_native_deadline() {
    let (mut game, id) = wheeled_unit(false);
    let unit = &game.catalog[game.entities[id].type_index];
    let actor = &mut game.entities[id];
    actor.ai.motion.start_move(100);
    let goal = goal([140.0, 100.0]);
    move_wheels(
        actor,
        unit,
        &goal,
        40.0,
        2.0,
        &super::Force {
            terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0,
            },
            mass: 50.0,
            frame: 175,
        },
        &mut |_| true,
    );
    assert!(!actor.ai.motion.braking());
    integrate_ground(actor, unit, 175);
    move_wheels(
        actor,
        unit,
        &goal,
        40.0,
        2.0,
        &super::Force {
            terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0,
            },
            mass: 50.0,
            frame: 176,
        },
        &mut |_| true,
    );
    assert!(actor.ai.motion.braking());
    integrate_ground(actor, unit, 176);
    actor.ai.motion.start_move(176);
    move_wheels(
        actor,
        unit,
        &goal,
        40.0,
        2.0,
        &super::Force {
            terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0,
            },
            mass: 50.0,
            frame: 177,
        },
        &mut |_| true,
    );
    assert!(!actor.ai.motion.braking());
}

#[test]
fn ordinary_order_after_long_idle_starts_a_new_wheel_move() {
    let (mut game, id) = wheeled_unit(false);
    for _ in 0..120 {
        game.step();
    }
    let start = game.entities[id].position;
    game.move_to(id, [135.0, 105.0]).unwrap();
    assert!(game.entities[id].ai.motion.movement_active);
    assert!(game.entities[id].ai.motion.donut_until > game.tick);
    for _ in 0..300 {
        game.step();
        if matches!(game.entities[id].ai.order, Order::Idle) {
            break;
        }
    }
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(game.entities[id].position[0] > start[0] + 20.0);
    assert!(!game.entities[id].ai.motion.movement_active);
}

#[test]
fn retained_path_rebuild_preserves_the_active_wheel_move_deadline() {
    let (mut game, id) = wheeled_unit(false);
    // Keep the remaining route inside DONUT_DISTANCE: farther travel itself
    // legitimately refreshes the timer, independently of move-state entry.
    game.move_to(id, [135.0, 105.0]).unwrap();
    for _ in 0..5 {
        game.step();
    }
    assert!(game.entities[id].ai.motion.movement_active);
    assert!(game.entities[id].ai.motion.velocity[0] > 0.0);
    let deadline = game.entities[id].ai.motion.donut_until;
    game.install_ground_path(id);
    game.step();
    assert_eq!(game.entities[id].ai.motion.donut_until, deadline);
    assert!(game.entities[id].ai.motion.movement_active);
    assert!(game.entities[id].ai.motion.velocity[0] > 0.0);
}

#[test]
fn appending_and_advancing_waypoints_keeps_the_same_wheel_move_deadline() {
    let (mut game, id) = wheeled_unit(false);
    game.queue_move_to(id, [115.0, 105.0]).unwrap();
    for _ in 0..60 {
        game.step();
        if game.entities[id].ai.motion.velocity[0] > 0.0 {
            break;
        }
    }
    // Capture after the queued route starts driving. While waiting, native
    // maintainCurrentPositionOther refreshes the timer.
    let deadline = game.entities[id].ai.motion.donut_until;
    assert!(game.entities[id].ai.motion.velocity[0] > 0.0);
    game.queue_move_to(id, [135.0, 105.0]).unwrap();
    assert_eq!(game.entities[id].ai.motion.donut_until, deadline);
    for _ in 0..600 {
        game.step();
        if game.entities[id].ai.waypoints.index == 1 {
            assert_eq!(game.entities[id].ai.motion.donut_until, deadline);
            assert!(game.entities[id].ai.motion.movement_active);
            assert!(game.entities[id].ai.motion.velocity[0] > 0.0);
            return;
        }
    }
    panic!("The wheel unit never advanced to its appended waypoint");
}

#[test]
fn waypoint_advance_resets_move_flags_before_destination_adjustment() {
    for blocked_destination in [false, true] {
        let (mut game, id) = wheeled_unit(false);
        game.queue_move_to(id, [115.0, 105.0]).unwrap();
        game.queue_move_to(id, [135.0, 105.0]).unwrap();
        if blocked_destination {
            let cells = (0..game.navigation.height()).flat_map(|y| {
                (0..game.navigation.width()).map(move |x| crate::grid::Cell { x, y })
            });
            game.navigation = game.navigation.with_obstacles(cells);
        }
        assert_eq!(
            game.adjusted_destination_world(id, [135.0, 105.0, 0.0])
                .is_none(),
            blocked_destination
        );
        game.tick = 100;
        let actor = &mut game.entities[id];
        // A failed segment with no retries advances through the actual state
        // update. Destination adjustment can then fail before the next request.
        actor.ai.command.move_to.waiting = true;
        actor.ai.path_request.pending = None;
        actor.ai.ground_path = None;
        actor.ai.waypoints.retries = [0; 4];
        actor.ai.motion.end_move();
        actor.ai.blocking.frames = 17;
        actor.ai.blocking.stuck = true;
        actor.ai.blocking.blocked = true;
        actor.ai.blocking.ignore_until = 53;
        let deadline = actor.ai.motion.donut_until;
        let result = game.update_follow_path_state(id);
        assert_eq!(
            matches!(
                result,
                crate::skirmish::ground_movement::MoveResult::Failure
            ),
            blocked_destination
        );
        let actor = &mut game.entities[id];
        assert_eq!(actor.ai.waypoints.index, 1);
        assert_eq!(actor.ai.motion.donut_until, deadline);
        assert!(actor.ai.motion.is_moving());
        assert!(!actor.ai.motion.take_completion());
        assert_eq!(actor.ai.blocking.frames, 0);
        assert!(!actor.ai.blocking.stuck);
        assert!(actor.ai.blocking.blocked);
        assert_eq!(actor.ai.blocking.ignore_until, 53);
        assert_eq!(actor.ai.path_request.pending.is_none(), blocked_destination);
    }
}
