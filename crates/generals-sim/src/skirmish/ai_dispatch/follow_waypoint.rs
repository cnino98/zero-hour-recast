//! SH AIStates.cpp3789–4268: per-object follow and Team waypoint propagation.
use std::sync::Arc;

use crate::skirmish::{
    CELL_SIZE, OpponentRules, Order, Skirmish,
    ai_dispatch::stored_command::{Command, Kind},
    ai_goals::COMMAND_FROM_AI,
    combat::attack::Goal,
    ground_movement::MoveResult,
    movement::distance_squared,
    path_service,
};
use num_traits::ToPrimitive;

pub(in crate::skirmish) struct State {
    rules: Arc<OpponentRules>,
    current: usize,
    as_team: bool,
    offset: [f32; 2],
    append_goal: bool,
    layer: u8,
    combat: Option<Combat>,
}

struct Combat {
    target: Option<usize>,
}

impl Skirmish {
    pub(in crate::skirmish) fn native_follow_waypoint_from(
        &mut self,
        id: usize,
        rules: Arc<OpponentRules>,
        waypoint: usize,
        as_team: bool,
        source: i32,
    ) {
        self.enter_native_waypoint_follow(id, rules, waypoint, as_team, source, false);
    }

    pub(in crate::skirmish) fn enter_native_waypoint_follow(
        &mut self,
        id: usize,
        rules: Arc<OpponentRules>,
        waypoint: usize,
        as_team: bool,
        source: i32,
        attacking: bool,
    ) {
        if (self.entities[id].aircraft.is_some() || self.entities[id].chinook.is_some())
            && self.native_subclass_command(
                id,
                Command {
                    source,
                    kind: Kind::FollowWaypoint {
                        rules: Arc::clone(&rules),
                        waypoint,
                        as_team,
                        attacking,
                    },
                },
            )
            || !self.native_ai_command_allowed(id)
            || self.catalog[self.entities[id].type_index].immobile()
            || self.object_disabled(id)
        {
            return;
        }
        if self
            .prepare_order_state(id, Order::FollowPath, false, None, Some(source))
            .is_none()
        {
            return;
        }
        let mut speed = f32::MAX;
        let mut offset = [0.0; 2];
        if as_team {
            if let Some(team) = self.team_membership.team_of(id) {
                self.team_membership.set_current_waypoint(team, waypoint);
            }
            if let Some(group) = self.entities[id].ai.command.ai_group {
                let members: Vec<_> = self
                    .team_membership
                    .team_of(id)
                    .into_iter()
                    .flat_map(|team| self.team_membership.team_objects(team))
                    .copied()
                    .filter(|&member| self.entities[member].ai.command.ai_group == Some(group))
                    .collect();
                if let Some(center) = self.native_group_center(&members) {
                    let point = self.entities[id].position;
                    offset = [point[0] - center[0], point[1] - center[1]];
                }
                speed = self.group_travel_speed(&members);
            }
        }
        self.set_ai_ground_goal(id, rules.map.waypoints[waypoint].position);
        if attacking {
            self.reset_nested_attack_idle(id);
        }
        self.entities[id].ai.follow_waypoint = Some(State {
            rules,
            current: waypoint,
            as_team,
            offset,
            append_goal: false,
            layer: 1,
            combat: attacking.then_some(Combat { target: None }),
        });
        self.remove_goal(id);
        self.configure_follow_waypoint(id);
        let distance = self.entities[id].ai.command.path_extra_distance;
        let result = self.enter_primary_ground_move(id);
        self.entities[id].ai.command.desired_speed = speed;
        self.entities[id].ai.command.path_extra_distance = distance;
        let state = self.entities[id].ai.follow_waypoint.as_ref().unwrap();
        let terminal = state.rules.map.waypoints[state.current].links.is_empty();
        self.entities[id].ai.command.move_to.adjusts = terminal && self.attack_ground_movement(id);
        if matches!(result, MoveResult::Failure) || !self.adjust_follow_waypoint_goal(id) {
            self.set_ai_order(id, Order::Idle);
        } else {
            self.claim_follow_waypoint_goal(id);
        }
        let slot = self.entities[id].weapon_slot();
        if attacking && self.allocated_weapon(id, slot).is_some() {
            self.entities[id].weapon_set.weapons[slot].max_shots = Some(i32::MAX.unsigned_abs());
        }
        self.ai_state_changed(id);
    }

    fn configure_follow_waypoint(&mut self, id: usize) {
        let state = self.entities[id].ai.follow_waypoint.as_ref().unwrap();
        let graph = &state.rules.map.waypoints;
        let current = &graph[state.current];
        let original = current.position;
        let mut point = [original[0] + state.offset[0], original[1] + state.offset[1]];
        let terminal = current.links.is_empty();
        let wall = self.movement_terrain.is_point_on_wall(original);
        let height = if wall {
            if !self.movement_terrain.is_point_on_wall(point) {
                point = original;
            }
            self.movement_terrain.wall_height()
        } else {
            self.ground_height(point)
        };
        if self.cell_at(original).is_some() && self.cell_at(point).is_none() {
            let extent = [
                f32::from(self.grid.width()) * CELL_SIZE,
                f32::from(self.grid.height()) * CELL_SIZE,
            ];
            for axis in 0..2 {
                point[axis] = point[axis].clamp(CELL_SIZE, extent[axis] - CELL_SIZE);
            }
        }
        let mut distance = CELL_SIZE / 10.0;
        let mut next = current;
        for _ in 0..5 {
            let Some(link) = next.links.first() else {
                break;
            };
            let Some(waypoint) = graph.iter().find(|node| node.id == *link) else {
                break;
            };
            distance += distance_squared(next.position, waypoint.position).sqrt();
            next = waypoint;
        }
        let off_map = self.cell_at(point).is_none();
        let actor = &mut self.entities[id];
        let state = actor.ai.follow_waypoint.as_mut().unwrap();
        state.append_goal = off_map;
        state.layer = if wall { 15 } else { 1 };
        actor.ai.command.move_to.position = [point[0], point[1], height];
        actor.ai.command.move_to.adjusts = terminal && !off_map;
        actor.ai.command.path_extra_distance = distance;
        if terminal
            && self.catalog[actor.type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "PROJECTILE")
            && let Some(flight) = &mut actor.flight
        {
            flight.set_precise_z(true);
        }
        if off_map {
            actor.ai.motion.set_allow_invalid_position(true);
        }
    }

    fn adjust_follow_waypoint_goal(&mut self, id: usize) -> bool {
        if !self.entities[id].ai.command.move_to.adjusts || !self.attack_ground_movement(id) {
            return true;
        }
        let goal = self.entities[id].ai.command.move_to.position;
        let Some(adjusted) = self.adjusted_destination_world(id, goal) else {
            return false;
        };
        self.entities[id].ai.command.move_to.position = adjusted;
        true
    }

    fn claim_follow_waypoint_goal(&mut self, id: usize) {
        if self.entities[id].ai.command.move_to.adjusts {
            let [x, y, _] = self.entities[id].ai.command.move_to.position;
            let layer = self.entities[id].ai.follow_waypoint.as_ref().unwrap().layer;
            self.reserve_goal_on(id, [x, y], layer);
        }
    }

    fn repath_follow_waypoint(&mut self, id: usize) -> bool {
        self.configure_follow_waypoint(id);
        if !self.adjust_follow_waypoint_goal(id) {
            return false;
        }
        let actor = &mut self.entities[id];
        actor.ai.motion.starting_move();
        actor.ai.blocking.start_move();
        actor.ai.command.move_to.waiting = true;
        self.request_ground_path(
            id,
            path_service::Request {
                goal: self.entities[id].ai.command.move_to.position,
                range: 0.1,
                kind: path_service::Kind::Move,
            },
        );
        self.claim_follow_waypoint_goal(id);
        true
    }

    pub(in crate::skirmish) fn update_native_waypoint_follow(&mut self, id: usize) {
        let state = self.entities[id].ai.follow_waypoint.as_ref().unwrap();
        if state.combat.is_none()
            && self.computer_player(self.entities[id].owner)
            && self.entities[id].ai.mood.attitude >= 1
            && !(self.attack_kind(id, "INFANTRY") && self.attack_kind(id, "IGNORED_IN_GUI"))
        {
            self.enter_native_waypoint_follow(
                id,
                Arc::clone(&state.rules),
                state.current,
                state.as_team,
                COMMAND_FROM_AI,
                true,
            );
            return;
        }
        if state.combat.is_some() && self.update_waypoint_follow_attack(id) {
            return;
        }
        let state = self.entities[id].ai.follow_waypoint.as_ref().unwrap();
        let team = self.team_membership.team_of(id).filter(|_| state.as_team);
        let current = state.current;
        let point = state.rules.map.waypoints[current].position;
        self.set_ai_ground_goal(id, point);
        self.append_off_map_follow_goal(id);
        if let Some(shared) = team.and_then(|team| self.team_membership.current_waypoint(team))
            && shared != current
        {
            self.entities[id]
                .ai
                .follow_waypoint
                .as_mut()
                .unwrap()
                .current = shared;
            if !self.repath_follow_waypoint(id) {
                self.set_ai_order(id, Order::Idle);
                return;
            }
        }
        let result = self.update_internal_move(id);
        if matches!(result, MoveResult::Continue)
            && !team.is_some_and(|team| self.follow_team_close_enough(id, team))
        {
            return;
        }
        let state = self.entities[id].ai.follow_waypoint.as_ref().unwrap();
        let graph = &state.rules.map.waypoints;
        let current = &graph[state.current];
        let completed = current.id;
        let count = current.links.len();
        if count == 0 {
            self.set_ai_order(id, Order::Idle);
            self.entities[id].ai.completed_waypoint = Some(completed);
            return;
        }
        let index =
            self.random_between(u32::try_from(count - 1).expect("native waypoint link count"));
        let state = self.entities[id].ai.follow_waypoint.as_mut().unwrap();
        let graph = &state.rules.map.waypoints;
        let next = graph[state.current].links[usize::try_from(index).expect("native link index")];
        let Some(next) = graph.iter().position(|waypoint| waypoint.id == next) else {
            self.set_ai_order(id, Order::Idle);
            return;
        };
        state.current = next;
        if let Some(team) = team {
            self.team_membership.set_current_waypoint(team, next);
        }
        if !self.repath_follow_waypoint(id) {
            self.set_ai_order(id, Order::Idle);
        }
    }

    fn update_waypoint_follow_attack(&mut self, id: usize) -> bool {
        let target = self.entities[id]
            .ai
            .follow_waypoint
            .as_ref()
            .unwrap()
            .combat
            .as_ref()
            .unwrap()
            .target;
        let repath = if let Some(target) = target {
            self.entities[id].ai.motion.clear_goal();
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(crate::skirmish::ModelConditions::MOVING, false);
            let revision = self.entities[id].ai.command_revision;
            if self.update_attack(id, target) || self.entities[id].ai.command_revision != revision {
                return true;
            }
            self.entities[id]
                .ai
                .follow_waypoint
                .as_mut()
                .unwrap()
                .combat
                .as_mut()
                .unwrap()
                .target = None;
            self.reset_nested_attack_idle(id);
            true
        } else {
            false
        };
        if let Some(target) = self.mood_target(id, false, repath) {
            self.entities[id].ai.attack_context.forced = false;
            if self.enter_native_attack(id, Goal::Object(target)) {
                self.entities[id]
                    .ai
                    .follow_waypoint
                    .as_mut()
                    .unwrap()
                    .combat
                    .as_mut()
                    .unwrap()
                    .target = Some(target);
            } else {
                self.reset_nested_attack_idle(id);
            }
            return true;
        }
        if repath && !self.repath_follow_waypoint(id) {
            self.set_ai_order(id, Order::Idle);
            return true;
        }
        false
    }

    pub(in crate::skirmish) fn exit_native_waypoint_follow(&mut self, id: usize) -> bool {
        let Some(state) = self.entities[id].ai.follow_waypoint.take() else {
            return false;
        };
        if state.combat.is_some() {
            self.exit_native_attack(id);
            self.reset_nested_attack_idle(id);
        }
        true
    }

    fn append_off_map_follow_goal(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        let state = actor.ai.follow_waypoint.as_mut().unwrap();
        if state.append_goal
            && actor.ai.path_request.pending.is_none()
            && let Some(path) = &mut actor.ai.ground_path
        {
            path.append_ground_node(actor.ai.command.move_to.position);
            state.append_goal = false;
        }
    }

    fn follow_team_close_enough(&mut self, id: usize, team: usize) -> bool {
        if !self.computer_player(self.entities[id].owner) {
            return false;
        }
        let members = self.team_membership.team_objects(team).to_vec();
        let members = self.materialize_native_group(&members);
        let Some(center) = self.native_group_center(&members) else {
            return false;
        };
        let actor = &self.entities[id];
        let point = actor.ai.command.move_to.position;
        let fudge = actor
            .ai
            .follow_waypoint
            .as_ref()
            .unwrap()
            .rules
            .skirmish_group_fudge;
        let count = members.len().to_f32().expect("native group count");
        distance_squared([center[0], center[1]], [point[0], point[1]]).sqrt() <= count * fudge
    }
}
