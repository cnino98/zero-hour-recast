use std::f32::consts::{PI, TAU};

use super::physical_ocl::transform::TerrainContext;
use super::{Entity, UnitType};

mod ground;
mod surfaces;
pub(super) use ground::SavedGroundState;
#[cfg(test)]
pub(super) use ground::wander::State as WanderState;
pub use ground::{GroundAppearance, GroundGoal, GroundMotion, GroundRules};
pub(super) use ground::{GroundRequest, integrate_ground, maintain_ground, move_ground, validate};
pub use surfaces::{LocomotorSets, LocomotorVariant};

/// Native `AIUpdate` `LocomotorSetType` values; controllers share the entity's current set.
pub mod set {
    pub const NORMAL: u8 = 0;
    pub const UPGRADED: u8 = 1;
    pub const FREEFALL: u8 = 2;
    pub const WANDER: u8 = 3;
    pub const PANIC: u8 = 4;
    pub const TAXI: u8 = 5;
    pub const SUPERSONIC: u8 = 6;
    pub const SLUGGISH: u8 = 7;
}

/// Installed locomotor caps used by the current movement adapter.
pub struct Locomotor {
    pub move_priority: i32,
    /// Native template constructor parameter, including zero-width and flight locomotors.
    /// `None` denotes an object with no native locomotor template.
    pub wander_length: Option<f32>,
    pub wander_about_point_radius: f32,
    pub wander_width: f32,
    pub stick_to_ground: bool,
    pub friction_when_airborne: bool,
    pub works_when_dead: bool,
    pub flight: Option<super::flight::Rules>,
    pub legal_surfaces: u8,
    /// Native integer threshold converted to Real for the strict height comparison.
    pub airborne_targeting_height: f32,
    pub ground: Option<GroundRules>,
    /// World units per second, like `UnitType::speed`.
    pub speed_damaged: f32,
    /// Radians per logic frame, following the INI angular-velocity conversion.
    pub turn_rate: f32,
    pub turn_rate_damaged: f32,
    /// Fraction of the object's bounding-circle radius, along its facing.
    pub turn_pivot_offset: f32,
    /// Health ratio at which the configured movement-penalty damage state begins.
    pub damage_health_threshold: f32,
}

pub(super) fn speed(actor: &Entity, unit: &UnitType) -> f32 {
    if actor.health / actor.max_health > actor.active_locomotor(unit).damage_health_threshold {
        actor.upgraded_speed(unit)
    } else {
        actor.active_locomotor(unit).speed_damaged
    }
}

pub(super) fn turn_rate(actor: &Entity, unit: &UnitType) -> f32 {
    let rate =
        if actor.health / actor.max_health > actor.active_locomotor(unit).damage_health_threshold {
            actor.active_locomotor(unit).turn_rate
        } else {
            actor.active_locomotor(unit).turn_rate_damaged
        }
        .min(99_999.0);
    if actor.ai.motion.ultra_accurate() {
        rate * 2.0
    } else {
        rate
    }
}

pub(super) fn turn_towards(
    actor: &mut Entity,
    unit: &UnitType,
    goal: [f32; 2],
    maximum: f32,
    terrain: &TerrainContext<'_>,
) -> (bool, f32) {
    rotate_actor(actor, unit, goal, maximum, None, terrain)
}

pub(super) fn turn_flight(
    actor: &mut Entity,
    unit: &UnitType,
    goal: [f32; 2],
    maximum: f32,
    pivot: f32,
    terrain: &TerrainContext<'_>,
) -> f32 {
    let radius = if matches!(unit.geometry.shape, crate::collision_geometry::Shape::Box) {
        let major = unit.geometry.major * unit.geometry.major;
        let minor = unit.geometry.minor * unit.geometry.minor;
        (major + minor).sqrt()
    } else {
        unit.geometry.major
    };
    let mut basis = actor.object_basis();
    let (_, amount) = rotate(
        &mut actor.position,
        &mut actor.facing,
        &mut basis,
        goal,
        maximum,
        pivot * radius,
        None,
    );
    let height = if pivot == 0.0 {
        terrain.orient(actor, unit, actor.facing)
    } else {
        terrain.rotated_basis(actor, basis)
    };
    actor.physics_control.turning = amount != 0.0;
    height
}

/// Ground `locoUpdate_moveTowardsAngle` uses a distant direction point,
/// rather than the enemy's position, when rotating around a noncentral pivot.
pub(super) fn turn_ground_angle(
    actor: &mut Entity,
    unit: &UnitType,
    angle: f32,
    frame: u64,
    terrain: &TerrainContext<'_>,
) -> bool {
    let blocked = actor.ai.blocking.begin_locomotor();
    actor.ai.motion.mark_angle(frame);
    let reached = rotate_towards_angle(actor, unit, angle, terrain).0;
    actor.ai.blocking.finish_locomotor(blocked);
    reached
}

fn rotate_towards_angle(
    actor: &mut Entity,
    unit: &UnitType,
    angle: f32,
    terrain: &TerrainContext<'_>,
) -> (bool, f32) {
    let dx = angle.cos() * 1000.0;
    let dy = angle.sin() * 1000.0;
    let wanted = [actor.position[0] + dx, actor.position[1] + dy];
    let maximum = turn_rate(actor, unit);
    rotate_actor(actor, unit, wanted, maximum, None, terrain)
}

fn rotate_actor(
    actor: &mut Entity,
    unit: &UnitType,
    goal: [f32; 2],
    maximum: f32,
    relative: Option<&mut f32>,
    terrain: &TerrainContext<'_>,
) -> (bool, f32) {
    let radius = if matches!(unit.geometry.shape, crate::collision_geometry::Shape::Box) {
        let major = unit.geometry.major * unit.geometry.major;
        let minor = unit.geometry.minor * unit.geometry.minor;
        (major + minor).sqrt()
    } else {
        unit.geometry.major
    };
    let offset = if actor.ai.motion.braking() {
        0.0
    } else {
        actor.active_locomotor(unit).turn_pivot_offset * radius
    };
    let mut basis = actor.object_basis();
    let result = rotate(
        &mut actor.position,
        &mut actor.facing,
        &mut basis,
        goal,
        maximum,
        offset,
        relative,
    );
    if offset == 0.0 {
        terrain.orient(actor, unit, actor.facing);
    } else {
        terrain.rotated_basis(actor, basis);
    }
    result
}

// Locomotor::rotateObjAroundLocoPivot preserves the full pose about a noncentral pivot.
// Native velocity-dependent wheeled turning remains outside this primitive.
fn rotate(
    position: &mut [f32; 2],
    facing: &mut f32,
    basis: &mut [[f32; 3]; 3],
    goal: [f32; 2],
    maximum: f32,
    pivot_offset: f32,
    relative: Option<&mut f32>,
) -> (bool, f32) {
    let mut pivot = *position;
    if pivot_offset != 0.0 {
        let displacement_x = facing.cos() * pivot_offset;
        let displacement_y = facing.sin() * pivot_offset;
        pivot[0] += displacement_x;
        pivot[1] += displacement_y;
        if (goal[0] - pivot[0]).abs() < 0.1 && (goal[1] - pivot[1]).abs() < 0.1 {
            return (true, 0.0);
        }
    }
    let desired = (goal[1] - pivot[1]).atan2(goal[0] - pivot[0]);
    let difference = normalize_angle(desired - *facing);
    if let Some(relative) = relative {
        *relative = difference;
    }
    let amount = difference.clamp(-maximum, maximum);
    if pivot_offset == 0.0 {
        *facing = normalize_angle(*facing + amount);
        *basis = super::physical_ocl::transform::yaw(*facing);
    } else {
        // Matrix3D: Translate(pivot), In_Place_Pre_Rotate_Z(amount),
        // Translate(-pivot), then multiply by the existing transform.
        let cosine = amount.cos();
        let sine = amount.sin();
        let translation_x = cosine * -pivot[0];
        let translation_y = -sine * -pivot[1];
        let offset_x = pivot[0] + (translation_x + translation_y);
        let translation_x = sine * -pivot[0];
        let translation_y = cosine * -pivot[1];
        let offset_y = pivot[1] + (translation_x + translation_y);
        let x = cosine * position[0];
        let y = -sine * position[1];
        let rotated_x = x + y;
        let x = sine * position[0];
        let y = cosine * position[1];
        position[0] = rotated_x + offset_x;
        position[1] = (x + y) + offset_y;
        let [row_x, row_y, _] = basis;
        for (column_x, column_y) in row_x.iter_mut().zip(row_y) {
            let x = cosine * *column_x;
            let y = -sine * *column_y;
            let rotated_x = x + y;
            let x = sine * *column_x;
            let y = cosine * *column_y;
            *column_x = rotated_x;
            *column_y = x + y;
        }
        *facing = basis[1][0].atan2(basis[0][0]);
    }
    (difference.abs() <= maximum, amount.abs())
}

// GameCommon::normalizeAngle uses (-PI, PI], including positive antipodal turns.
pub(super) fn normalize_angle(mut angle: f32) -> f32 {
    if angle.is_nan() {
        return 0.0;
    }
    // Keep the source's repeated subtraction/addition rather than replacing it
    // with remainder arithmetic, which changes boundary rounding.
    loop {
        if angle > PI {
            angle -= TAU;
        } else {
            break;
        }
    }
    loop {
        if angle <= -PI {
            angle += TAU;
        } else {
            break;
        }
    }
    angle
}

#[cfg(test)]
mod tests {
    use super::{TerrainContext, rotate, speed, turn_ground_angle, turn_rate};
    use crate::skirmish::{Kind, Order};
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn antipodal_turn_uses_the_native_positive_direction() {
        let mut position = [0.0, 0.0];
        let mut facing = PI;
        let mut basis = crate::skirmish::physical_ocl::transform::yaw(facing);
        let (finished, turned) = rotate(
            &mut position,
            &mut facing,
            &mut basis,
            [10.0, 0.0],
            0.1,
            0.0,
            None,
        );
        assert!(!finished);
        assert_eq!(turned.to_bits(), 0.1_f32.to_bits());
        assert!((facing - (-PI + 0.1)).abs() < 0.000_001);
        assert_eq!(position.map(f32::to_bits), [0.0_f32.to_bits(); 2]);
    }

    #[test]
    fn rear_pivot_moves_the_center_and_close_pivot_does_not_twitch() {
        let mut position = [10.0, 10.0];
        let mut facing = 0.0;
        let mut basis = crate::skirmish::physical_ocl::transform::yaw(facing);
        let (finished, _) = rotate(
            &mut position,
            &mut facing,
            &mut basis,
            [5.0, 20.0],
            FRAC_PI_2,
            -5.0,
            None,
        );
        assert!(finished);
        assert!((position[0] - 5.0).abs() < 0.000_01);
        assert!((position[1] - 15.0).abs() < 0.000_01);
        assert!((facing - FRAC_PI_2).abs() < 0.000_001);
        let previous = position;
        let (finished, turned) = rotate(
            &mut position,
            &mut facing,
            &mut basis,
            [5.0, 10.0],
            0.1,
            -5.0,
            None,
        );
        assert!(finished);
        assert_eq!(turned.to_bits(), 0.0_f32.to_bits());
        assert_eq!(position.map(f32::to_bits), previous.map(f32::to_bits));
    }

    #[test]
    fn rear_pivot_preserves_tilt_instead_of_rebuilding_yaw() {
        let mut position = [10.0, 10.0];
        let mut facing = 0.0;
        let mut basis = super::super::physical_ocl::transform::yaw(facing);
        super::super::physical_ocl::transform::rotate(&mut basis, [0.3, 0.0, 0.0]);
        basis[2][1] *= 1.000_01;
        let old = basis;
        rotate(
            &mut position,
            &mut facing,
            &mut basis,
            [5.0, 20.0],
            FRAC_PI_2,
            -5.0,
            None,
        );
        assert_eq!(basis[2].map(f32::to_bits), old[2].map(f32::to_bits));
        assert!(basis[0][2].abs() > 0.29);
        assert!(basis[2][1].abs() > 0.29);
        assert!((facing - FRAC_PI_2).abs() < 0.000_001);
    }

    #[test]
    fn angle_command_uses_direction_instead_of_a_near_target_at_the_rear_pivot() {
        let mut game = crate::skirmish::tests::game();
        let actor = &mut game.entities[1];
        let unit = &mut game.catalog[actor.type_index];
        actor.position = [100.0, 100.0];
        actor.facing = 0.0;
        unit.geometry.shape = crate::collision_geometry::Shape::Cylinder;
        unit.geometry.major = 10.0;
        unit.locomotor.turn_pivot_offset = -0.5;
        unit.locomotor.turn_rate = PI;
        assert!(turn_ground_angle(
            actor,
            unit,
            FRAC_PI_2,
            7,
            &TerrainContext {
                terrain: game.projectile_terrain.as_ref(),
                layers: &game.movement_terrain,
                height: 0.0
            }
        ));
        // A direct turn around the rear pivot toward the nearby target would
        // stop around PI/4; the native angle request instead faces nearly north.
        assert!((actor.facing - 1000.0_f32.atan2(5.0)).abs() < 0.000_01);
        assert_eq!(actor.ai.motion.command_frame, Some(7));
    }

    #[test]
    fn penalty_starts_at_the_source_threshold_of_current_maximum_health() {
        let mut game = crate::skirmish::tests::game();
        let actor = &mut game.entities[1];
        let unit = &mut game.catalog[actor.type_index];
        unit.locomotor.speed_damaged = 15.0;
        unit.locomotor.turn_rate_damaged = 0.05;
        actor.max_health = 200.0;
        actor.health = 70.0;
        assert_eq!(speed(actor, unit).to_bits(), 15.0_f32.to_bits());
        assert_eq!(turn_rate(actor, unit).to_bits(), 0.05_f32.to_bits());
        actor.health = f32::from_bits(70.0_f32.to_bits() + 1);
        assert_eq!(speed(actor, unit).to_bits(), unit.speed.to_bits());
        assert_eq!(
            turn_rate(actor, unit).to_bits(),
            unit.locomotor.turn_rate.to_bits()
        );
    }

    #[test]
    fn route_arrival_uses_the_position_after_a_noncentral_pivot_turn() {
        let mut game = crate::skirmish::tests::game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        game.catalog[tank].locomotor.turn_pivot_offset = -0.33;
        game.catalog[tank].geometry.shape = crate::collision_geometry::Shape::Box;
        game.catalog[tank].geometry.major = 7.5;
        let id = game.spawn(tank, 0, [300.0, 100.0], true);
        game.entities[id].facing = PI;
        game.entities[id].ai.mood.next_check = u64::MAX;
        game.move_to(id, [450.0, 125.0]).unwrap();
        let Order::Move(goal) = game.entities[id].ai.order else {
            panic!("The ordinary move must retain its destination");
        };
        for _ in 0..600 {
            game.step();
            if matches!(game.entities[id].ai.order, Order::Idle) {
                break;
            }
        }
        assert!(matches!(game.entities[id].ai.order, Order::Idle));
        assert!((game.entities[id].position[0] - goal[0]).abs() < 0.1);
        assert!((game.entities[id].position[1] - goal[1]).abs() < 0.1);
    }
}
