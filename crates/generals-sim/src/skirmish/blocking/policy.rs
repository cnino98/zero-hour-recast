//! Live grounded branches of `AIUpdate.cpp` priority, blocking and speed limits.
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use crate::skirmish::{Entity, GroupKind, UnitType};

pub(super) fn has_higher_priority(
    actor: &Entity,
    unit: &UnitType,
    other: &Entity,
    other_unit: &UnitType,
    ids: (usize, usize),
) -> bool {
    let dozer = unit
        .locomotor
        .ground
        .as_ref()
        .is_some_and(|rules| rules.is_dozer);
    let other_dozer = other_unit
        .locomotor
        .ground
        .as_ref()
        .is_some_and(|rules| rules.is_dozer);
    if dozer != other_dozer {
        return dozer;
    }
    if matches!(unit.group_kind, GroupKind::Vehicle)
        && matches!(other_unit.group_kind, GroupKind::Infantry)
    {
        return true;
    }
    if matches!(unit.group_kind, GroupKind::Infantry)
        && matches!(other_unit.group_kind, GroupKind::Vehicle)
    {
        return false;
    }
    let ours = direction(actor);
    let other_direction = direction(other);
    if dot(ours, other_direction) <= 0.0 {
        return ids.0 < ids.1;
    }
    let combined = [ours[0] + other_direction[0], ours[1] + other_direction[1]];
    let relative = [
        other.position[0] - actor.position[0],
        other.position[1] - actor.position[1],
    ];
    let ahead = dot(combined, relative);
    if ahead > 0.0 {
        false
    } else if ahead < 0.0 {
        true
    } else {
        ids.0 < ids.1
    }
}

pub(super) fn blocked_by(
    actor: &Entity,
    unit: &UnitType,
    other: &Entity,
    other_unit: &UnitType,
    can_crush: bool,
    higher_priority: bool,
) -> bool {
    if let (Some(cell), Some(goal)) = (
        actor.ai.path_goal,
        actor.ai.blocking.command_goal(actor.ai.path_endpoint),
    ) && cell.x > 0
        && cell.y > 0
        && (goal[0] - actor.position[0]).abs() < 10.0
        && (goal[1] - actor.position[1]).abs() < 10.0
    {
        return false;
    }
    if can_crush || other_unit.locomotor.ground.is_none() || actor.ai.motion.moving_backwards() {
        return false;
    }
    // AIPathfind.h enables INFANTRY_MOVES_THROUGH_INFANTRY.
    if matches!(unit.group_kind, GroupKind::Infantry)
        && matches!(other_unit.group_kind, GroupKind::Infantry)
    {
        return false;
    }
    let relative = [
        actor.position[0] - other.position[0],
        actor.position[1] - other.position[1],
    ];
    let squared = dot(relative, relative);
    if squared < 10.0 * 10.0 * 0.0001 {
        return higher_priority;
    }
    let ours = direction(actor);
    let theirs = direction(other);
    let aligned = dot(ours, theirs);
    if actor.ai.blocking.frames > 30 && aligned <= 0.0 {
        return false;
    }
    let angle = relative_angle(actor.position, ours, other.position);
    let other_angle = relative_angle(other.position, theirs, actor.position);
    if !(-FRAC_PI_2..=FRAC_PI_2).contains(&angle) {
        return false;
    }
    let other_moving = other.ai.motion.has_goal();
    let limit = if other_moving {
        FRAC_PI_4
    } else {
        FRAC_PI_4 * 0.75
    };
    if angle > limit || angle < -limit {
        if aligned <= 0.0 {
            return false;
        }
        if other_moving && (other_angle > limit || other_angle < -limit) {
            let next = [
                relative[0] + (ours[0] - theirs[0]),
                relative[1] + (ours[1] - theirs[1]),
            ];
            if squared > dot(next, next) {
                if higher_priority {
                    return false;
                }
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
    other.alive()
}

pub(super) fn max_blocked_speed(actor: &Entity, other: &Entity) -> f32 {
    let mut relative = [
        other.position[0] - actor.position[0],
        other.position[1] - actor.position[1],
    ];
    let length = dot(relative, relative).sqrt();
    if length != 0.0 {
        relative[0] /= length;
        relative[1] /= length;
    }
    let away = dot(relative, direction(other));
    if away < 0.0 {
        return 0.0;
    }
    let away_speed = dot(other.ai.motion.velocity, other.ai.motion.velocity).sqrt() * away;
    let towards = dot(relative, direction(actor));
    if towards <= 0.0 {
        return actor.ai.blocking.max_speed;
    }
    let mut maximum = away_speed / towards;
    if other.ai.command.formation_id.is_some()
        && actor.ai.command.formation_id == other.ai.command.formation_id
    {
        maximum *= 0.55;
    }
    maximum.min(actor.ai.blocking.max_speed)
}

pub(super) fn relative_angle(position: [f32; 2], facing: [f32; 2], goal: [f32; 2]) -> f32 {
    let mut vector = [goal[0] - position[0], goal[1] - position[1]];
    let length = dot(vector, vector).sqrt();
    if length == 0.0 {
        return 0.0;
    }
    let reciprocal = 1.0 / length;
    vector[0] *= reciprocal;
    vector[1] *= reciprocal;
    let cosine = dot(facing, vector).clamp(-1.0, 1.0);
    // PartitionManager::getRelativeAngle2D / ACos use acosf, not an atan2 delta.
    let angle = cosine.acos();
    let positive = facing[0] * vector[1];
    let negative = facing[1] * vector[0];
    if positive - negative < 0.0 {
        -angle
    } else {
        angle
    }
}

pub(super) fn direction(actor: &Entity) -> [f32; 2] {
    [actor.facing.cos(), actor.facing.sin()]
}

pub(super) fn dot(left: [f32; 2], right: [f32; 2]) -> f32 {
    let x = left[0] * right[0];
    let y = left[1] * right[1];
    x + y
}
