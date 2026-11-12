//! `AIUpdate::privateMoveToPosition` AI-source temporary state and shared goal.
use super::{MoveAside, Skirmish, TemporaryKind, TemporaryState};
use crate::skirmish::{Order, path_service};

pub(super) struct MoveTo {
    pub goal: [f32; 3],
    pub adjusts: bool,
    path_goal: [f32; 3],
    path_timestamp: u64,
    waiting: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn construction_move(&mut self, id: usize, position: [f32; 3]) {
        self.construction_move_command(id, position, false);
    }

    pub(in crate::skirmish) fn move_even_if_sleeping(&mut self, id: usize, position: [f32; 3]) {
        self.construction_move_command(id, position, true);
    }

    fn construction_move_command(&mut self, id: usize, position: [f32; 3], even_if_sleeping: bool) {
        use crate::skirmish::ai_dispatch::stored_command::Kind;
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source: crate::skirmish::ai_goals::COMMAND_FROM_AI,
                kind: if even_if_sleeping {
                    Kind::MoveEvenIfSleeping(position)
                } else {
                    Kind::Move(position)
                },
            },
        ) {
            return;
        }
        if self.entities[id].ai.dozer.is_some() {
            self.entities[id].drawable.model_conditions.constructing = false;
        }
        if !self.native_ai_command_allowed_for_sleep(id, even_if_sleeping) {
            return;
        }
        if self.catalog[self.entities[id].type_index].immobile() || self.object_disabled(id) {
            return;
        }
        let idle = self.entities[id].ai_idle();
        if idle {
            self.clear_ai_goal(id);
        }
        // CMD_FROM_AI deliberately does not clip to the player's playable area.
        // StateMachine::setGoalPosition does not clear an existing goal object.
        self.set_ai_goal_position(id, position);
        self.entities[id].ai.blocking.frames = 0;
        self.entities[id].ai.blocking.blocked = false;
        self.entities[id].ai.blocking.stuck = false;
        self.clear_temporary(id);
        let (goal, adjusts, apply_adjustment) = self.construction_move_destination(id, position);
        if idle {
            self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
            self.set_order_state(id, Order::Move([goal[0], goal[1]]), false, Some(position));
            self.entities[id].ai.command.from_ai = true;
            self.entities[id].ai.command.move_to.position = goal;
            self.entities[id].ai.command.move_to.adjusts = adjusts;
            self.set_ai_goal_position(id, position);
            self.begin_ground_move(id);
            if apply_adjustment {
                self.reserve_goal_on(
                    id,
                    [goal[0], goal[1]],
                    self.movement_goal_layer(id, [goal[0], goal[1]]),
                );
            }
            self.request_temporary_path(id, goal, false);
            return;
        }
        let parent_goal = Some([position[0], position[1]]);
        // internalGetState(AI_MOVE_TO) returns the SAME stored state if it is
        // already current: onEnter replaces that state's internal destination.
        if matches!(self.entities[id].ai.order, Order::Move(_)) {
            self.entities[id].ai.order = Order::Move([goal[0], goal[1]]);
            self.entities[id].ai.command.move_to.position = goal;
            self.entities[id].ai.command.move_to.adjusts = adjusts;
        }
        let actor = &mut self.entities[id];
        actor.ai.blocking.temporary = Some(TemporaryState::Movement(MoveAside {
            parent_goal,
            goal: [goal[0], goal[1]],
            until: self.tick + 600,
            kind: TemporaryKind::MoveTo(MoveTo {
                goal,
                adjusts,
                path_goal: goal,
                path_timestamp: self.tick,
                waiting: true,
            }),
        }));
        actor.ai.command.path_extra_distance = 0.0;
        actor.ai.motion.movement_active = false;
        self.begin_ground_move(id);
        if apply_adjustment {
            self.reserve_goal_on(
                id,
                [goal[0], goal[1]],
                self.movement_goal_layer(id, [goal[0], goal[1]]),
            );
        }
        self.request_temporary_path(id, goal, false);
    }

    // AIInternalMoveToState resolves the retained object goal before adjusting
    // its destination; the source machine goal remains the commanded XYZ.
    fn construction_move_destination(
        &mut self,
        id: usize,
        position: [f32; 3],
    ) -> ([f32; 3], bool, bool) {
        let requested = self.entities[id]
            .ai
            .command
            .goal_object
            .filter(|&target| {
                self.entities
                    .get(target)
                    .is_some_and(crate::skirmish::Entity::exists)
            })
            .map_or(position, |target| self.object_position(target));
        let actor = &self.entities[id];
        let adjusts = !(actor.ai.motion.ultra_accurate()
            || actor.flight_ultra_accurate()
            || actor
                .ai
                .command
                .goal_object
                .is_some_and(|target| actor.ai.ignored_producer == Some(target)));
        let apply_adjustment = adjusts && !actor.parachute_rider() && !actor.landed();
        let goal = if apply_adjustment {
            self.adjusted_destination_world(id, requested)
                .unwrap_or_else(|| self.snap_closest_goal(id, requested))
        } else {
            requested
        };
        (goal, adjusts, apply_adjustment)
    }

    fn request_temporary_path(&mut self, id: usize, goal: [f32; 3], blocked: bool) {
        self.request_ground_path(
            id,
            path_service::Request {
                goal,
                range: 0.1,
                kind: if blocked {
                    path_service::Kind::Recovery
                } else {
                    path_service::Kind::Move
                },
            },
        );
        if let Some(TemporaryState::Movement(state)) = &mut self.entities[id].ai.blocking.temporary
            && let TemporaryKind::MoveTo(movement) = &mut state.kind
        {
            movement.waiting = true;
        }
    }

    /// Returns whether the temporary state continues; its owner handles timeout
    /// and onExit before running the retained main state in this same callback.
    pub(super) fn update_temporary_move(&mut self, id: usize) -> bool {
        let target = self.entities[id].ai.command.goal_object.filter(|&target| {
            self.entities
                .get(target)
                .is_some_and(crate::skirmish::Entity::exists)
        });
        let target_position = target.map(|target| self.object_position(target));
        let actor = &mut self.entities[id];
        let Some(TemporaryState::Movement(state)) = &mut actor.ai.blocking.temporary else {
            return false;
        };
        let TemporaryKind::MoveTo(movement) = &mut state.kind else {
            return false;
        };
        if let Some(position) = target_position {
            movement.goal = position;
            state.goal = [position[0], position[1]];
        }
        if movement.waiting {
            movement.path_timestamp = self.tick;
            if actor.ai.path_request.pending.is_some() {
                return true;
            }
            if actor.ai.path_request.failed || actor.ai.ground_path.is_none() {
                return false;
            }
            movement.waiting = false;
            movement.path_goal = movement.goal;
        }
        let goal = movement.goal;
        let delta = [
            movement.path_goal[0] - goal[0],
            movement.path_goal[1] - goal[1],
        ];
        let to_actor = [goal[0] - actor.position[0], goal[1] - actor.position[1]];
        let displacement_squared = delta.map(|component| component * component);
        let distance_squared = to_actor.map(|component| component * component);
        let changed = displacement_squared[0] + displacement_squared[1]
            > (distance_squared[0] + distance_squared[1]) * 0.01;
        let blocked = actor.ai.blocking.stuck || actor.ai.blocking.frames > 60;
        let force = actor.ai.ground_path.is_none() || blocked;
        if force || (self.tick - movement.path_timestamp > 10 && changed) {
            self.request_temporary_path(id, goal, blocked);
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if actor.active_locomotor(unit).ground.is_some() {
            if actor.ai.path.raw.is_empty() {
                return true;
            }
            !self.follow_ground_path(id, actor.ai.path_request.pending.is_some())
        } else if actor.active_locomotor(unit).flight.is_some() && actor.flight.is_some() {
            !self.travel_flight(id, [goal[0], goal[1]], 0.1)
        } else {
            true
        }
    }
}
