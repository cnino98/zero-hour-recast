//! AIUpdate.cpp3358–3423: native path commands and retained XYZ arguments.
use crate::skirmish::{
    Order, Skirmish,
    ai_dispatch::stored_command::{Command, Kind},
    ground_movement::MoveResult,
};

impl Skirmish {
    pub(in crate::skirmish) fn native_follow_path_from(
        &mut self,
        id: usize,
        points: Vec<[f32; 3]>,
        ignore: Option<usize>,
        source: i32,
        exit_production: bool,
    ) {
        // AICommandParms::storeCommand copies the complete authored path when
        // the Chinook retains a command during a vertical transition.
        if (self.entities[id].chinook.is_some() || self.entities[id].aircraft.is_some())
            && self.native_subclass_command(
                id,
                Command {
                    source,
                    kind: Kind::FollowPath {
                        points: points.clone(),
                        ignore,
                        exit_production,
                    },
                },
            )
        {
            return;
        }
        if !self.native_ai_command_allowed(id)
            || self.catalog[self.entities[id].type_index].immobile()
            || self.object_disabled(id)
        {
            return;
        }
        if exit_production && self.entities[id].aircraft.is_some() {
            self.jet_exit_production_from(id, ignore, source);
            return;
        }
        let first = points.first().copied();
        let order = if exit_production {
            Order::Exit(first.unwrap_or([0.0; 3]))
        } else {
            Order::FollowPath
        };
        if self
            .prepare_order_state(id, order, false, None, Some(source))
            .is_none()
        {
            return;
        }
        self.clear_ai_goal(id);
        if let Some(&last) = points.last() {
            self.set_ai_goal_position(id, last);
        }
        let actor = &mut self.entities[id];
        actor.ai.waypoints.points = points;
        actor.ai.ignored_producer = ignore;
        actor.ai.waypoints.exit_quick_pending = exit_production;
        actor.ai.blocking.path_through_units = exit_production;
        self.ai_state_changed(id);
        self.entities[id].ai.command.move_to.adjusts = !exit_production;
        let result = self.enter_follow_path_state(id, first);
        if matches!(result, MoveResult::Failure) {
            self.set_ai_order(id, Order::Idle);
        }
    }

    pub(in crate::skirmish) fn enter_follow_path_state(
        &mut self,
        id: usize,
        first: Option<[f32; 3]>,
    ) -> MoveResult {
        let Some(first) = first else {
            return MoveResult::Failure;
        };
        self.entities[id].ai.waypoints.index = 0;
        self.entities[id].ai.command.move_to.position = first;
        let result = self.enter_primary_ground_move(id);
        if let Some(speed) = self.entities[id].ai.command.formation_speed {
            self.entities[id].ai.command.desired_speed = speed;
        }
        self.follow_path_segment_flags(id);
        result
    }

    pub(in crate::skirmish) fn native_append_path_from(
        &mut self,
        id: usize,
        position: [f32; 3],
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            Command {
                source,
                kind: Kind::AppendPath(position),
            },
        ) || !self.native_ai_command_allowed(id)
        {
            return;
        }
        if matches!(self.entities[id].ai.order, Order::FollowPath)
            && self.entities[id].ai.follow_waypoint.is_none()
        {
            self.entities[id].ai.waypoints.points.push(position);
            return;
        }
        let actor = &self.entities[id];
        let mut points = Vec::new();
        if actor.ai.ground_path.is_some() || actor.ai.path_request.pending.is_some() {
            points.push(actor.ai_goal_position());
        }
        points.push(position);
        self.native_follow_path_from(id, points, None, source, false);
    }

    fn follow_path_segment_flags(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        let index = actor.ai.waypoints.index;
        let points = &actor.ai.waypoints.points;
        if let Some(next) = points.get(index + 1) {
            let current = points[index];
            let dx = next[0] - current[0];
            let dy = next[1] - current[1];
            let squared_x = dx * dx;
            let squared_y = dy * dy;
            let mut extra = (squared_x + squared_y).sqrt();
            if points.get(index + 2).is_some() {
                let lookahead = 4.0 * crate::skirmish::CELL_SIZE;
                extra += lookahead;
            }
            actor.ai.command.path_extra_distance = extra;
            actor.ai.command.move_to.adjusts = false;
        } else {
            actor.ai.command.move_to.adjusts = true;
            if index == 0 {
                actor.ai.command.path_extra_distance = 0.0;
            }
            if self.attack_kind(id, "PROJECTILE")
                && let Some(flight) = &mut self.entities[id].flight
            {
                flight.set_precise_z(true);
            }
        }
    }

    pub(in crate::skirmish) fn update_native_follow_path(&mut self, id: usize) {
        let result = self.update_follow_path_state(id);
        if !matches!(result, MoveResult::Continue) {
            let supply = self.entities[id].ai.supply.force_wanting;
            self.set_ai_order(id, Order::Idle);
            if supply && self.catalog[self.entities[id].type_index].supply.capacity > 0 {
                self.resume_supply(id);
            }
        }
    }

    pub(in crate::skirmish) fn update_follow_path_state(&mut self, id: usize) -> MoveResult {
        let position = self.entities[id].ai.command.move_to.position;
        self.set_ai_goal_position(id, position);
        let result = self.update_internal_move(id);
        if matches!(result, MoveResult::Continue) {
            return MoveResult::Continue;
        }
        let which = self
            .jet_follow_path_index(id)
            .unwrap_or_else(|| usize::from(matches!(self.entities[id].ai.order, Order::Exit(_))));
        let actor = &mut self.entities[id];
        if matches!(result, MoveResult::Failure) && actor.ai.waypoints.retries[which] > 0 {
            actor.ai.waypoints.retries[which] -= 1;
        } else {
            actor.ai.waypoints.index += 1;
        }
        while let Some(next) = actor.ai.waypoints.points.get(actor.ai.waypoints.index) {
            let dx = next[0] - actor.position[0];
            let dy = next[1] - actor.position[1];
            let squared_x = dx * dx;
            let squared_y = dy * dy;
            let cell_squared = crate::skirmish::CELL_SIZE * crate::skirmish::CELL_SIZE;
            if squared_x + squared_y >= cell_squared {
                break;
            }
            actor.ai.waypoints.index += 1;
        }
        actor.ai.ignored_producer = None;
        let Some(&next) = actor.ai.waypoints.points.get(actor.ai.waypoints.index) else {
            return MoveResult::Success;
        };
        actor.ai.motion.starting_move();
        actor.ai.blocking.start_move();
        actor.ai.command.move_to.position = next;
        if matches!(actor.ai.order, Order::Exit(_)) {
            actor.ai.order = Order::Exit(next);
        }
        self.follow_path_segment_flags(id);
        let final_segment =
            self.entities[id].ai.waypoints.index + 1 == self.entities[id].ai.waypoints.points.len();
        if final_segment {
            self.entities[id].ai.command.move_to.adjusts = self.attack_ground_movement(id);
            if self.entities[id].ai.command.move_to.adjusts {
                let Some(adjusted) = self.adjusted_destination_world(id, next) else {
                    return MoveResult::Failure;
                };
                self.entities[id].ai.command.move_to.position = adjusted;
                if matches!(self.entities[id].ai.order, Order::Exit(_)) {
                    self.entities[id].ai.order = Order::Exit(adjusted);
                }
                let layer = self.movement_terrain.get_layer_for_destination(
                    adjusted,
                    self.ground_height([adjusted[0], adjusted[1]]),
                );
                self.reserve_goal_on(id, [adjusted[0], adjusted[1]], layer);
            }
        }
        self.entities[id].ai.command.move_to.waiting = true;
        self.request_ground_path(
            id,
            crate::skirmish::path_service::Request {
                goal: self.entities[id].ai.command.move_to.position,
                range: 0.1,
                kind: crate::skirmish::path_service::Kind::Move,
            },
        );
        self.entities[id].ai.motion.starting_move();
        self.entities[id].ai.blocking.start_move();
        MoveResult::Continue
    }

    pub(in crate::skirmish) fn exit_native_follow_path(&mut self, id: usize) {
        if !matches!(
            self.entities[id].ai.order,
            Order::FollowPath | Order::Exit(_)
        ) && self.jet_follow_path_index(id).is_none()
        {
            return;
        }
        self.entities[id].ai.motion.end_move();
        if self.exit_native_waypoint_follow(id) {
            self.entities[id].ai.motion.set_ultra_accurate(false);
        }
        self.entities[id].ai.blocking.path_through_units = false;
        if let Some(flight) = &mut self.entities[id].flight {
            flight.set_precise_z(false);
        }
    }
}
