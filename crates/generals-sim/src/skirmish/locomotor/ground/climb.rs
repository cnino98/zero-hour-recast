//! `Locomotor::moveTowardsPositionClimb`, SH 2bfaea60.
use super::{
    BIGNUM, CELL_SIZE, Entity, FRAC_PI_4, Force, GroundGoal, UnitType, change_speed, forward_speed,
    maximum_acceleration, normalize_angle, rotate_towards_angle,
};

const CLIMBING: u8 = 32;

pub(super) fn move_climber(
    actor: &mut Entity,
    unit: &UnitType,
    goal: &GroundGoal,
    distance: f32,
    mut desired_speed: f32,
    force: &Force<'_>,
) {
    let rules = actor
        .active_locomotor(unit)
        .ground
        .as_ref()
        .expect("Climber rules");
    let [height, goal_height, ahead_height] = actor.ai.motion.climb_heights;
    let dz = height - goal_height;
    if dz * dz > CELL_SIZE * CELL_SIZE {
        actor.ai.motion.set(CLIMBING, true);
    }
    if dz.abs() < 1.0 {
        actor.ai.motion.set(CLIMBING, false);
    }
    let climbing = actor.ai.motion.flags & CLIMBING != 0;
    let backwards = climbing && ahead_height < height - 0.1;
    if climbing {
        let slope = (ahead_height - height).abs().max(1.0);
        if slope > 1.0 {
            desired_speed /= slope * 4.0;
        }
    }
    actor
        .ai
        .motion
        .set(super::wheels::MOVING_BACKWARDS, backwards);
    let mut angle =
        (goal.position[1] - actor.position[1]).atan2(goal.position[0] - actor.position[0]);
    if backwards {
        angle = normalize_angle(angle - std::f32::consts::PI);
    }
    let relative = normalize_angle(angle - actor.facing);
    rotate_towards_angle(actor, unit, angle, force.terrain);
    let coefficient = (relative.abs() / FRAC_PI_4).min(1.0);
    let mut goal_speed = (1.0 - coefficient) * desired_speed;
    let mut actual = forward_speed(actor);
    if backwards {
        actual = -actual;
    }
    let braking = rules.braking.min(BIGNUM);
    let delta = actual - rules.min_speed;
    let slowdown = if delta <= 0.0 {
        0.0
    } else {
        let distance = (delta * delta / braking.abs()) * 0.5;
        distance * 1.05
    };
    if distance < slowdown && !goal.no_slow_down {
        goal_speed = rules.min_speed;
    }
    let change = if backwards {
        -goal_speed + actual
    } else {
        goal_speed - actual
    };
    let maximum = maximum_acceleration(actor, unit, rules);
    let acceleration = if backwards {
        if change < 0.0 { -maximum } else { braking }
    } else if change > 0.0 {
        maximum
    } else {
        -braking
    };
    change_speed(actor, force.mass, change, acceleration, force.frame);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{GroundAppearance, GroundRules};

    fn setup() -> (crate::skirmish::Skirmish, usize) {
        let mut game = crate::skirmish::tests::game();
        game.catalog[5].locomotor.ground = Some(GroundRules {
            wander: None,
            appearance: GroundAppearance::Climber,
            is_dozer: false,
            downhill_only: false,
            acceleration: 1.0,
            acceleration_damaged: 1.0,
            braking: 1.0,
            min_speed: 0.0,
            close_enough: 1.0,
            mass: 50.0,
            forward_friction: 0.0,
            lateral_friction: 0.0,
            extra_friction: 0.0,
        });
        let id = game.spawn(5, 0, [300.0, 400.0], true);
        game.entities[id].facing = 0.0;
        (game, id)
    }

    #[test]
    fn climb_hysteresis_and_uphill_slope_scale_preserve_exact_thresholds() {
        let (mut game, id) = setup();
        let unit = &game.catalog[5];
        let actor = &mut game.entities[id];
        let goal = GroundGoal {
            height: 0.0,
            position: [400.0, 400.0],
            distance: 100.0,
            desired_speed: 1.0,
            no_slow_down: false,
            ultra_accurate: false,
        };
        for (height_delta, climbing, expected) in [
            (10.0, false, 1.0_f32),
            (11.0, true, 0.125),
            (5.0, true, 0.125),
            (1.0, true, 0.125),
            (0.5, false, 1.0),
        ] {
            actor.ai.motion.acceleration = [0.0; 2];
            actor.ai.motion.climb_heights = [0.0, height_delta, 2.0];
            move_climber(
                actor,
                unit,
                &goal,
                100.0,
                1.0,
                &Force {
                    terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                        terrain: game.projectile_terrain.as_ref(),
                        layers: &game.movement_terrain,
                        height: 0.0,
                    },
                    mass: 50.0,
                    frame: 1,
                },
            );
            assert_eq!(actor.ai.motion.flags & CLIMBING != 0, climbing);
            assert_eq!(
                actor.ai.motion.acceleration[0].to_bits(),
                expected.to_bits()
            );
        }
    }

    #[test]
    fn descending_faces_backwards_but_applies_force_toward_goal() {
        let (mut game, id) = setup();
        let unit = &game.catalog[5];
        let actor = &mut game.entities[id];
        // Descend to the left while facing right. Native atan2 and the distant
        // direction point can round yaw around zero; its forward X still rounds
        // exactly to one, while the signed motive force must point left.
        actor.facing = 0.0;
        actor.ai.motion.climb_heights = [20.0, 0.0, 18.0];
        let goal = GroundGoal {
            height: 0.0,
            position: [200.0, 400.0],
            distance: 100.0,
            desired_speed: 1.0,
            no_slow_down: false,
            ultra_accurate: false,
        };
        move_climber(
            actor,
            unit,
            &goal,
            100.0,
            1.0,
            &Force {
                terrain: &crate::skirmish::physical_ocl::transform::TerrainContext {
                    terrain: game.projectile_terrain.as_ref(),
                    layers: &game.movement_terrain,
                    height: 0.0,
                },
                mass: 50.0,
                frame: 1,
            },
        );
        assert!(actor.ai.motion.moving_backwards());
        assert_eq!(actor.facing.cos().to_bits(), 1.0_f32.to_bits());
        // Analytic reverse acceleration is 1/(slope2 * 4). One relative
        // rounding unit near PI becomes four in the PI/4 angle coefficient;
        // allow four more for speed modulation and mass/force arithmetic.
        let straight_acceleration = 1.0 / (2.0 * 4.0);
        let roundoff = straight_acceleration * (8.0 * f32::EPSILON);
        assert!(actor.ai.motion.acceleration[0] < 0.0);
        assert!((actor.ai.motion.acceleration[0] + straight_acceleration).abs() <= roundoff);
        // Changing active surfaces preserves each original Locomotor's state.
        let mut saved = super::super::SavedGroundState::from_template(
            0,
            &unit.locomotor,
            &mut crate::skirmish::startup::seed_random(1),
        );
        actor.ai.motion.swap_surface_state(&mut saved);
        assert_eq!(actor.ai.motion.flags & CLIMBING, 0);
        actor.ai.motion.swap_surface_state(&mut saved);
        assert_ne!(actor.ai.motion.flags & CLIMBING, 0);
        assert!(actor.ai.motion.moving_backwards());
    }
}
