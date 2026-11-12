//! Ground-unit collision response and temporary yielding from `AIUpdate`/`AIStates`.
use super::{Entity, GroupKind, Kind, Order, Skirmish, startup};
use crate::grid::{Cell, Route, move_away};
use std::f32::consts::PI;

mod construction_move;
mod policy;
#[cfg(test)]
mod tests;

const FAST_AS_POSSIBLE: f32 = 999_999.0;

pub(super) struct Blocking {
    pub frames: u32,
    pub max_speed: f32,
    bump_speed: f32,
    pub blocked: bool,
    pub stuck: bool,
    pub path_through_units: bool,
    pub ignore_until: u64,
    pub retry_at: Option<u64>,
    pub path_frame: u64,
    blockers: [Option<usize>; 2],
    temporary: Option<TemporaryState>,
}

enum TemporaryState {
    Movement(MoveAside),
    Busy { until: u32 },
}

struct MoveAside {
    parent_goal: Option<[f32; 2]>,
    goal: [f32; 2],
    until: u64,
    kind: TemporaryKind,
}

enum TemporaryKind {
    OutOfWay,
    MoveTo(construction_move::MoveTo),
}

impl Blocking {
    pub const fn new() -> Self {
        Self {
            frames: 0,
            max_speed: 0.0,
            bump_speed: FAST_AS_POSSIBLE,
            blocked: false,
            stuck: false,
            path_through_units: false,
            ignore_until: 0,
            retry_at: None,
            path_frame: 0,
            blockers: [None; 2],
            temporary: None,
        }
    }

    pub const fn start_move(&mut self) {
        self.frames = 0;
        self.stuck = false;
    }

    /// Consumes contacts from the preceding physics update, after AI state work.
    pub const fn begin_locomotor(&mut self) -> bool {
        self.frames = if self.blocked { self.frames + 1 } else { 0 };
        self.blocked = false;
        self.frames > 0
    }

    pub fn limit_speed(&mut self, mut speed: f32) -> (f32, bool) {
        let mut blocked = self.frames > 0;
        if blocked && speed > self.max_speed {
            speed = self.max_speed;
            if self.bump_speed > speed {
                self.bump_speed = speed;
            }
            self.bump_speed *= 0.95;
            speed = self.bump_speed;
        } else {
            blocked = false;
            if self.bump_speed < FAST_AS_POSSIBLE {
                if self.bump_speed < speed * 0.2 {
                    self.bump_speed = speed * 0.2;
                }
                self.bump_speed *= 1.05;
            }
            if speed > self.bump_speed {
                speed = self.bump_speed;
            }
        }
        (speed, blocked)
    }

    pub const fn finish_locomotor(&mut self, blocked: bool) {
        if !blocked && self.frames > 1 {
            self.frames = 1;
        }
        self.max_speed = FAST_AS_POSSIBLE;
    }

    pub const fn temporary_goal(&self) -> Option<[f32; 2]> {
        match self.temporary {
            Some(TemporaryState::Movement(ref state)) => Some(state.goal),
            _ => None,
        }
    }

    pub const fn command_goal(&self, fallback: Option<[f32; 2]>) -> Option<[f32; 2]> {
        match self.temporary {
            Some(TemporaryState::Movement(ref state)) => state.parent_goal,
            _ => fallback,
        }
    }

    pub(super) const fn temporary_world_goal(&self) -> Option<[f32; 3]> {
        match &self.temporary {
            Some(TemporaryState::Movement(state)) => match &state.kind {
                TemporaryKind::MoveTo(movement) => Some(movement.goal),
                TemporaryKind::OutOfWay => None,
            },
            _ => None,
        }
    }

    pub(super) const fn temporary_adjusts(&self) -> Option<bool> {
        match &self.temporary {
            Some(TemporaryState::Movement(state)) => match &state.kind {
                TemporaryKind::MoveTo(movement) => Some(movement.adjusts),
                TemporaryKind::OutOfWay => None,
            },
            _ => None,
        }
    }

    pub(super) const fn temporary_move_to(&self) -> bool {
        matches!(
            &self.temporary,
            Some(TemporaryState::Movement(MoveAside {
                kind: TemporaryKind::MoveTo(_),
                ..
            }))
        )
    }

    fn moving_away_from(&self, other: usize) -> bool {
        self.temporary.is_some() && self.blockers.contains(&Some(other))
    }
}

impl Skirmish {
    /// Called in the source collision callback order. Living represented AI
    /// objects normally suppress the physical collision impulse.
    pub(super) fn process_ground_contact(&mut self, id: usize, other: usize) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let target = &self.entities[other];
        let target_type = &self.catalog[target.type_index];
        if actor.ai.blocking.ignore_until > self.tick
            || actor.ai.blocking.path_through_units
            || target.ai.registration.is_none()
            || !self.attack_ground_movement(id)
            || !self.attack_ground_movement(other)
        {
            return false;
        }
        if moving(actor) {
            let priority =
                policy::has_higher_priority(actor, unit, target, target_type, (id, other));
            if policy::blocked_by(
                actor,
                unit,
                target,
                target_type,
                self.can_crush(id, other),
                priority,
            ) {
                if matches!(
                    actor.ai.command.main_state,
                    super::ai_goals::MainState::Panic
                ) && self.attack_kind(id, "INFANTRY")
                {
                    return true;
                }
                self.handle_blocked_contact(id, other, priority);
            }
        } else {
            if actor.is_ai_dead()
                && self.attack_kind(id, "INFANTRY")
                && self.can_squish_only(other, id)
            {
                return true;
            }
            let delta = [
                actor.position[0] - target.position[0],
                actor.position[1] - target.position[1],
            ];
            if !moving(target) && policy::dot(delta, delta) < 25.0 {
                if self.entities[id].object_status & (1 << 24) != 0 || self.object_busy(id) {
                    return false;
                }
                self.separate_idle(id);
                self.separate_idle(other);
            }
        }
        false
    }

    fn handle_blocked_contact(&mut self, id: usize, other: usize, priority: bool) {
        self.entities[id].ai.blocking.blocked = true;
        let target_moving = moving(&self.entities[other]);
        if target_moving && self.entities[other].ai.path_request.pending.is_some() {
            return;
        }
        let limit = policy::max_blocked_speed(&self.entities[id], &self.entities[other]);
        if limit < self.entities[id].ai.blocking.max_speed {
            self.entities[id].ai.blocking.max_speed = limit;
        }
        let infantry = |unit: &Entity| {
            matches!(
                self.catalog[unit.type_index].group_kind,
                GroupKind::Infantry
            )
        };
        if !self.entities[other].ai.blocking.moving_away_from(id)
            && infantry(&self.entities[other])
            && !infantry(&self.entities[id])
        {
            if !self.object_busy(other) {
                self.move_away_from(other, id, 0);
            }
            return;
        }
        if self.entities[id].ai.blocking.frames == 0 {
            self.entities[id].ai.blocking.frames = 1;
        }
        if self.need_to_rotate(id) {
            self.entities[id].ai.blocking.frames = 1;
        } else if !target_moving {
            self.entities[id].ai.blocking.stuck = true;
        } else {
            let actor = &self.entities[id];
            let target = &self.entities[other];
            let target_priority = policy::has_higher_priority(
                target,
                &self.catalog[target.type_index],
                actor,
                &self.catalog[actor.type_index],
                (other, id),
            );
            if policy::blocked_by(
                target,
                &self.catalog[target.type_index],
                actor,
                &self.catalog[actor.type_index],
                self.can_crush(other, id),
                target_priority,
            ) && !self.need_to_rotate(other)
                && !priority
            {
                self.move_away_from(id, other, 0);
            }
        }
    }

    fn separate_idle(&mut self, id: usize) {
        if !self.entities[id].ai_idle() {
            return;
        }
        let position = self.object_position(id);
        if let Some(goal) = self.adjusted_destination_world(id, position) {
            self.construction_move(id, goal);
        }
    }

    fn need_to_rotate(&mut self, id: usize) -> bool {
        if self.entities[id].ai.blocking.retry_at.is_some() {
            return true;
        }
        let position = self.entities[id].position;
        let facing = policy::direction(&self.entities[id]);
        // Sampling preserves the native Path cache/probe order. The same line
        // predicate is exposed by the ground integration, not approximated here.
        self.blocking_path_point(id)
            .is_some_and(|point| policy::relative_angle(position, facing, point).abs() > PI / 30.0)
    }

    /// Factory `computeQuickPath` uses positional occupants along the exact
    /// segment, not the ordinary raw-path blocked-by-ally callback.
    pub(super) fn move_allies_from_exit(&mut self, id: usize, destination: [f32; 2]) {
        let (Some(start), Some(goal)) = (
            self.cell_at(self.entities[id].position),
            self.cell_at(destination),
        ) else {
            return;
        };
        let layer = if self.entities[id].layer == 1 {
            self.movement_goal_layer(id, destination)
        } else {
            self.entities[id].layer
        };
        for cell in self.navigation_for(id).line_cells(start, goal) {
            let node = self
                .navigation_for(id)
                .resolve_layer(crate::grid::LayerCell { cell, layer });
            let Some(other) = self.occupancy.position_owner_on(node) else {
                continue;
            };
            let actor = &self.entities[id];
            let target = &self.entities[other];
            if other == id
                || actor.ai.ignored_producer == Some(other)
                || !self.allied(actor.owner, target.owner)
                || moving(target)
                || self.object_busy(other)
            {
                continue;
            }
            self.move_away_from(other, id, 1);
        }
    }

    pub(super) fn move_allies(&mut self, id: usize) {
        self.move_allies_depth(id, 0);
    }

    fn move_allies_depth(&mut self, id: usize, depth: u8) {
        if depth >= 2 || self.catalog[self.entities[id].type_index].no_collide() {
            return;
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let blocked = actor.ai.path.blocked_by_ally;
        let dozer = unit
            .locomotor
            .ground
            .as_ref()
            .is_some_and(|rules| rules.is_dozer);
        if !dozer && !matches!(unit.kind, Kind::Harvester) && !blocked {
            return;
        }
        // Native iterates the unsmoothed path backwards, excluding its first node.
        let mut cells = Vec::new();
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        for &node in actor.ai.path.raw.iter().skip(1).rev() {
            for x in i32::from(node.cell.x) - radius
                ..i32::from(node.cell.x) + radius + i32::from(centered)
            {
                for y in i32::from(node.cell.y) - radius
                    ..i32::from(node.cell.y) + radius + i32::from(centered)
                {
                    if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) {
                        cells.push(
                            self.navigation_for(id)
                                .resolve_layer(crate::grid::LayerCell {
                                    cell: Cell { x, y },
                                    layer: node.layer,
                                }),
                        );
                    }
                }
            }
        }
        for cell in cells {
            let Some(other) = self.occupancy.position_owner_on(cell) else {
                continue;
            };
            if self.can_ask_ally_to_move(id, other, blocked) {
                self.move_away_from(other, id, depth + 1);
            }
        }
    }

    fn can_ask_ally_to_move(&self, id: usize, other: usize, blocked: bool) -> bool {
        let actor = &self.entities[id];
        let target = &self.entities[other];
        if id == other
            || actor.ai.ignored_producer == Some(other)
            || actor.owner != target.owner
            || !target.complete()
            || moving(target)
            || attacking(target)
        {
            return false;
        }
        let infantry = matches!(
            self.catalog[actor.type_index].group_kind,
            GroupKind::Infantry
        );
        let target_infantry = matches!(
            self.catalog[target.type_index].group_kind,
            GroupKind::Infantry
        );
        !infantry || (!target_infantry && blocked)
    }

    pub(super) fn move_away_from(&mut self, id: usize, other: usize, depth: u8) {
        if !self.dozer_accepts_move_aside(id, other)
            || !self.native_ai_command_allowed(id)
            || !self.entities[id].jet_allows_move_aside()
        {
            return;
        }
        if self.entities.get(other).is_none()
            || !self.entities[id].complete()
            || self.entities[id]
                .active_locomotor(&self.catalog[self.entities[id].type_index])
                .ground
                .is_none()
        {
            return;
        }
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source: crate::skirmish::ai_goals::COMMAND_FROM_AI,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::MoveAside {
                    other,
                    depth,
                },
            },
        ) {
            return;
        }
        if self.entities[id].ai.blocking.moving_away_from(other) {
            if self.entities[id].ai.blocking.blocked {
                self.entities[id].ai.blocking.ignore_until = self.tick + 60;
            }
            return;
        }
        let blocking = &mut self.entities[id].ai.blocking;
        blocking.blockers[1] = blocking.blockers[0];
        blocking.blockers[0] = Some(other);
        let Some(mut route) = self.find_move_aside(id, other).or_else(|| {
            if self.entities[id].ai.blocking.path_through_units {
                return None;
            }
            self.entities[id].ai.blocking.path_through_units = true;
            self.find_move_aside(id, other)
        }) else {
            return;
        };
        let Some(&last) = route.optimized.back() else {
            return;
        };
        let goal = self.path_position(id, last.cell);
        route.optimized.pop_front();
        // privateMoveAwayFromUnit assigns newPath before setTemporaryState.
        // An old temporary state's onExit destroys that newly assigned path;
        // the next onEnter then fails. Preserve the pinned source ordering.
        if self.entities[id].ai.blocking.temporary.is_some() {
            self.entities[id].ai.path = route;
            self.clear_temporary(id);
            return;
        }
        let parent_goal = self.entities[id].ai.path_endpoint;
        let actor = &mut self.entities[id];
        actor.ai.path = route;
        actor.ai.path_goal = Some(last.cell);
        actor.ai.path_endpoint = Some(goal);
        actor.ai.ground_path = None;
        actor.ai.motion.clear_goal();
        actor.ai.motion.movement_active = false;
        actor.ai.blocking.temporary = Some(TemporaryState::Movement(MoveAside {
            parent_goal,
            goal,
            until: self.tick + 300,
            kind: TemporaryKind::OutOfWay,
        }));
        self.install_ground_path(id);
        self.entities[id].ai.blocking.start_move();
        self.reserve_goal(id, goal);
        self.move_allies_depth(id, depth);
    }

    fn find_move_aside(&mut self, id: usize, other: usize) -> Option<Route> {
        let actor = &self.entities[id];
        let target = &self.entities[other];
        let first: Vec<_> = target.ai.ground_path.as_ref()?.positions().collect();
        let second: Vec<_> = actor.ai.blocking.blockers[1]
            .and_then(|id| self.entities.get(id))
            .and_then(|actor| actor.ai.ground_path.as_ref())
            .into_iter()
            .flat_map(super::path_following::Path::positions)
            .collect();
        let unit = &self.catalog[actor.type_index];
        let other_unit = &self.catalog[target.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let (other_radius, other_centered) =
            startup::radius_and_center(&other_unit.footprint(target.position, 0.0));
        let as_real = |radius| f32::from(i16::try_from(radius).expect("bounded source footprint"));
        let width = as_real(radius) * 10.0;
        let mut half_width = width - 2.5;
        if centered {
            half_width += 5.0;
        }
        let other_width = as_real(other_radius) * 10.0;
        half_width += other_width;
        if other_centered {
            half_width += 5.0;
        }
        let start = self.cell_at(actor.position)?;
        // The pinned function adds its two half-cell corrections to X; preserve
        // that source arithmetic instead of silently changing the second to Y.
        let mut indexed = actor.position;
        if !centered {
            indexed[0] += 5.0;
            indexed[0] += 5.0;
        }
        let indexed_start = self.cell_at(indexed)?;
        let unavailable = self.unavailable_goals(id);
        let query = move_away::Avoidance {
            start,
            indexed_start,
            radius,
            centered,
            half_width,
            paths: [&first, &second],
            unavailable: &unavailable,
        };
        let result =
            move_away::find_counted(self.navigation_for(id), &self.route_units(id), &query);
        let result = self.finish_path_search(result);
        self.path_service.cumulative_cells += result.cells;
        result.route
    }

    pub(in crate::skirmish) fn set_temporary_busy(&mut self, id: usize) {
        self.clear_temporary(id);
        self.entities[id].ai.blocking.temporary = Some(TemporaryState::Busy {
            until: self.update_frame().wrapping_add(1),
        });
    }

    /// A temporary state resumes its unchanged parent in the same AI update.
    pub(super) fn update_move_aside(&mut self, id: usize) -> bool {
        if let Some(TemporaryState::Busy { until }) = self.entities[id].ai.blocking.temporary {
            if until >= self.update_frame() {
                return true;
            }
            self.clear_temporary(id);
            return false;
        }
        let Some(TemporaryState::Movement(state)) = &self.entities[id].ai.blocking.temporary else {
            return false;
        };
        let expired = state.until < self.tick;
        let arrived = if self.entities[id].ai.blocking.temporary_move_to() {
            !self.update_temporary_move(id)
        } else {
            if self.entities[id].ai.blocking.stuck {
                self.entities[id].ai.blocking.path_through_units = true;
            }
            self.advance_ground(id)
        };
        if arrived || expired || !self.entities[id].alive() {
            self.clear_temporary(id);
            return false;
        }
        true
    }

    /// `AIUpdate::update` consumes `friend_endingMove` after all state transitions,
    /// before doLocomotor. A new friend's startingMove can cancel completion.
    pub(super) fn finish_completed_movement(&mut self, id: usize) {
        if !self.entities[id].ai.motion.take_completion() {
            return;
        }
        let final_goal = self
            .occupancy
            .goal_cell(id)
            .map(|cell| self.path_position(id, cell));
        let actor = &mut self.entities[id];
        actor.ai.path_request.cancel();
        actor.ai.blocking.retry_at = None;
        actor.ai.path.clear();
        actor.ai.ground_path = None;
        actor.ai.motion.clear_goal();
        actor
            .drawable
            .model_conditions
            .set_flag(crate::skirmish::ModelConditions::MOVING, false);
        actor.ai.ignored_producer = None;
        if let Some(mut goal) = final_goal {
            if super::movement::distance_squared(goal, actor.position) >= 100.0
                && let Some(cell) = self.movement_cell(id, self.entities[id].position)
            {
                goal = self.path_position(id, cell);
            }
            // Pinned setFinalPosition stores a coordinate but sets its apply
            // flag false. The observable work here is the native goal claim.
            self.reserve_goal(id, goal);
        }
    }

    pub(super) fn clear_temporary(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        let Some(TemporaryState::Movement(state)) = actor.ai.blocking.temporary.take() else {
            return;
        };
        actor.ai.motion.end_move();
        if matches!(state.kind, TemporaryKind::MoveTo(_)) {
            return;
        }
        actor.ai.path_request.cancel();
        actor.ai.path.clear();
        actor.ai.path_goal = None;
        actor.ai.path_endpoint = None;
        actor.ai.ground_path = None;
        actor.ai.motion.clear_goal();
        actor.ai.motion.end_move();
        actor
            .drawable
            .model_conditions
            .set_flag(crate::skirmish::ModelConditions::MOVING, false);
        actor.ai.blocking.path_through_units = false;
        actor.ai.blocking.blockers = [None; 2];
    }
}

fn moving(actor: &Entity) -> bool {
    !actor.ai_idle() && (actor.ai.motion.has_goal() || actor.ai.motion.is_moving())
}

const fn attacking(actor: &Entity) -> bool {
    matches!(actor.ai.order, Order::Attack(_)) || actor.ai.ai_target.is_some()
}
