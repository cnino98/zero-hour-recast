//! SH `AIWanderState` / `AIPanicState`, with `ALLOW_BACKTRACK` waypoint links.
use crate::skirmish::{
    AiWaypoint, CELL_SIZE, Order, Skirmish, ai_goals::MainState, ground_movement::MoveResult,
    path_service,
};
use num_traits::ToPrimitive;

pub(in crate::skirmish) struct State {
    graph: Vec<AiWaypoint>,
    current: Option<usize>,
    prior: Option<usize>,
    offset: [f32; 2],
    timer: i32,
    wait_frames: i32,
}

impl State {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            graph: Vec::new(),
            current: None,
            prior: None,
            offset: [0.0; 2],
            timer: 0,
            wait_frames: 0,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn start_waypoint_wander(
        &mut self,
        id: usize,
        graph: &[AiWaypoint],
        current: usize,
        panic: bool,
    ) {
        if self.attack_kind(id, "IMMOBILE") || self.object_disabled(id) {
            return;
        }
        self.exit_waypoint_state(id);
        self.exit_repulsor_state(id);
        self.exit_attack_move(id);
        self.exit_native_attack(id);
        self.clear_ai_goal(id);
        self.entities[id].ai.waypoints.points.clear();
        self.clear_temporary(id);
        self.entities[id].ai.command_revision = self.entities[id]
            .ai
            .command_revision
            .checked_add(1)
            .expect("AI command revision overflow");
        self.set_last_command_source(id, 1);
        let which = usize::from(panic);
        let actor = &mut self.entities[id];
        actor.ai.order = Order::Idle;
        actor.ai.command.main_state = if panic {
            MainState::Panic
        } else {
            MainState::Wander
        };
        actor.ai.primary.sleep_until = 0;
        let state = &mut actor.ai.command.wander[which];
        state.graph = graph
            .iter()
            .map(|waypoint| AiWaypoint {
                id: waypoint.id,
                position: waypoint.position,
                labels: waypoint.labels.clone(),
                links: waypoint.links.clone(),
                bidirectional: waypoint.bidirectional,
            })
            .collect();
        state.current = Some(current);
        if !panic {
            state.prior = None;
            state.offset = [0.0; 2];
        }
        self.randomize_waypoint_offset(id, which);
        if !panic {
            self.reset_waypoint_timer(id, which);
        }
        self.compute_wander_goal(id, which);
        let result = self.enter_primary_ground_move(id);
        if panic {
            self.reset_waypoint_timer(id, which);
        }
        self.entities[id].ai.command.path_extra_distance = self.wander_extra_distance(id, which);
        if panic {
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(crate::skirmish::ModelConditions::PANICKING, true);
        }
        self.ai_state_changed(id);
        if matches!(result, MoveResult::Failure) {
            self.enter_repulsor_state(id);
        }
    }

    fn reset_waypoint_timer(&mut self, id: usize, which: usize) {
        let state = &mut self.entities[id].ai.command.wander[which];
        state.timer = 0;
        state.wait_frames = 10 + i32::try_from(id & 7).expect("native object id low bits");
    }

    fn randomize_waypoint_offset(&mut self, id: usize, which: usize) {
        let actor = &self.entities[id];
        let width = actor
            .active_locomotor(&self.catalog[actor.type_index])
            .wander_width;
        if width <= 0.0 {
            return;
        }
        let delta = (width + 0.5)
            .floor()
            .max(1.0)
            .to_i32()
            .expect("native wander width");
        let span = u32::try_from(i64::from(delta) * 2).expect("native wander width span");
        for axis in 0..2 {
            let offset = i64::from(self.random_between(span)) - i64::from(delta);
            self.entities[id].ai.command.wander[which].offset[axis] =
                offset.to_f32().expect("native wander offset") * CELL_SIZE;
        }
    }

    fn wander_extra_distance(&self, id: usize, which: usize) -> f32 {
        let state = &self.entities[id].ai.command.wander[which];
        let mut current = state.current;
        let mut distance = CELL_SIZE / 10.0;
        for _ in 0..5 {
            let Some(index) = current else {
                break;
            };
            let waypoint = &state.graph[index];
            let Some(next) = waypoint
                .links
                .first()
                .and_then(|link| state.graph.iter().position(|node| node.id == *link))
            else {
                break;
            };
            let point = state.graph[next].position;
            let squared = [
                point[0] - waypoint.position[0],
                point[1] - waypoint.position[1],
            ]
            .map(|value| value * value);
            distance += (squared[0] + squared[1]).sqrt();
            current = Some(next);
        }
        distance
    }

    fn compute_wander_goal(&mut self, id: usize, which: usize) {
        let state = &self.entities[id].ai.command.wander[which];
        let Some(current) = state.current else {
            return;
        };
        let waypoint = &state.graph[current];
        let original = waypoint.position;
        let mut point = [original[0] + state.offset[0], original[1] + state.offset[1]];
        let last = waypoint.links.is_empty();
        let height = if self.movement_terrain.is_point_on_wall(original) {
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
        self.entities[id].ai.command.move_to.position = [point[0], point[1], height];
        self.entities[id].ai.command.move_to.adjusts = last;
        if last
            && self.attack_kind(id, "PROJECTILE")
            && let Some(flight) = &mut self.entities[id].flight
        {
            flight.set_precise_z(true);
        }
        if self.cell_at(point).is_none() {
            self.entities[id].ai.command.move_to.adjusts = false;
            self.entities[id].ai.motion.set_allow_invalid_position(true);
        }
        self.entities[id].ai.command.path_extra_distance = self.wander_extra_distance(id, which);
    }

    pub(in crate::skirmish) fn update_waypoint_wander(&mut self, id: usize) {
        let panic = matches!(self.entities[id].ai.command.main_state, MainState::Panic);
        let which = usize::from(panic);
        let result = self.update_internal_move(id);
        if self.attack_kind(id, "CAN_BE_REPULSED") {
            let state = &mut self.entities[id].ai.command.wander[which];
            state.timer -= 1;
            if state.timer < 0 {
                state.timer = state.wait_frames;
                if self.closest_repulsor(id).is_some() {
                    self.enter_repulsor_state(id);
                    return;
                }
            }
        }
        let advance = if panic {
            matches!(result, MoveResult::Success)
        } else {
            !matches!(result, MoveResult::Continue)
        };
        if !advance {
            return;
        }
        let state = &self.entities[id].ai.command.wander[which];
        let current = state.current.expect("active waypoint state");
        let links = state.graph[current].links.len();
        let link = if links == 0 {
            None
        } else {
            Some(
                usize::try_from(
                    self.random_between(u32::try_from(links - 1).expect("native waypoint links")),
                )
                .expect("native link index"),
            )
        };
        let state = &mut self.entities[id].ai.command.wander[which];
        let point = state.graph[current].position;
        state.prior = Some(current);
        state.current = link.and_then(|link| {
            state
                .graph
                .iter()
                .position(|node| node.id == state.graph[current].links[link])
        });
        let completed = state
            .current
            .is_none()
            .then(|| state.graph[state.prior.expect("prior waypoint")].id);
        self.set_ai_ground_goal(id, point);
        if let Some(waypoint) = completed {
            self.entities[id].ai.completed_waypoint = Some(waypoint);
            self.exit_waypoint_state(id);
            self.entities[id].ai.command.main_state = MainState::Active;
            self.enter_primary_idle(id);
            return;
        }
        self.randomize_waypoint_offset(id, which);
        self.compute_wander_goal(id, which);
        self.entities[id].ai.command.move_to.waiting = true;
        self.request_ground_path(
            id,
            path_service::Request {
                goal: self.entities[id].ai.command.move_to.position,
                range: 0.1,
                kind: path_service::Kind::Move,
            },
        );
    }

    pub(in crate::skirmish) fn exit_waypoint_state(&mut self, id: usize) {
        let panic = matches!(self.entities[id].ai.command.main_state, MainState::Panic);
        if !panic && !matches!(self.entities[id].ai.command.main_state, MainState::Wander) {
            return;
        }
        let actor = &mut self.entities[id];
        actor.ai.motion.end_move();
        actor.ai.motion.movement_active = false;
        if panic {
            actor
                .drawable
                .model_conditions
                .set_flag(crate::skirmish::ModelConditions::PANICKING, false);
        } else {
            actor.ai.motion.clear_ultra_accuracy();
            self.set_flight_precision(id, false, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> Vec<AiWaypoint> {
        vec![
            AiWaypoint {
                id: 40,
                position: [330.0, 150.0],
                labels: vec!["road".into()],
                links: vec![41],
                bidirectional: false,
            },
            AiWaypoint {
                id: 41,
                position: [370.0, 150.0],
                labels: vec!["road".into()],
                links: vec![],
                bidirectional: false,
            },
        ]
    }

    #[test]
    fn waypoint_wander_owns_initial_updates_and_completes_without_map_script_polling() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let graph = route();
        game.tick = 10;
        game.start_waypoint_wander(actor, &graph, 0, false);
        assert!(matches!(
            game.entities[actor].ai.command.main_state,
            MainState::Wander
        ));
        assert_eq!(game.entities[actor].ai.command.last_command_source, 1);
        assert!(game.entities[actor].ai.path_request.pending.is_some());
        assert_eq!(
            game.entities[actor]
                .ai
                .command
                .path_extra_distance
                .to_bits(),
            41.0_f32.to_bits()
        );
        let mut completed = false;
        for _ in 0..600 {
            game.step();
            if game.entities[actor].ai.completed_waypoint == Some(41) {
                completed = true;
                break;
            }
        }
        assert!(completed);
        assert!(game.entities[actor].ai_idle());
        assert!(!game.entities[actor].ai.motion.is_moving());
    }

    #[test]
    fn panic_does_not_advance_a_failed_path_but_wander_does() {
        for panic in [false, true] {
            let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
            game.tick = 10;
            game.start_waypoint_wander(actor, &route(), 0, panic);
            game.entities[actor].ai.path_request.cancel();
            game.entities[actor].ai.ground_path = None;
            game.update_waypoint_wander(actor);
            let which = usize::from(panic);
            assert_eq!(
                game.entities[actor].ai.command.wander[which].current,
                Some(usize::from(!panic))
            );
            game.stop(actor).unwrap();
            assert!(
                !game
                    .model_flags(&game.entities[actor], [0.5, 0.1])
                    .contains("PANICKING")
            );
        }
    }
    #[test]
    fn wall_waypoint_retains_height_and_discards_offset_that_leaves_walkable_cells() {
        use crate::collision_geometry::Shape;
        use crate::skirmish::Footprint;
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let wall = Footprint {
            position: [330.0, 150.0],
            major: 30.0,
            minor: 20.0,
            angle: 0.0,
            shape: Shape::Box,
        };
        game.movement_terrain.add_wall_piece(actor);
        game.movement_terrain.allocate_wall_layer(80.0, &[wall]);
        let state = &mut game.entities[actor].ai.command.wander[0];
        state.graph = route();
        state.current = Some(0);
        state.offset = [100.0, 0.0];
        game.compute_wander_goal(actor, 0);
        assert_eq!(
            game.entities[actor]
                .ai
                .command
                .move_to
                .position
                .map(f32::to_bits),
            [330.0_f32, 150.0, 80.0].map(f32::to_bits)
        );
        game.entities[actor].ai.command.wander[0].offset = [10.0, 0.0];
        game.compute_wander_goal(actor, 0);
        assert_eq!(
            game.entities[actor]
                .ai
                .command
                .move_to
                .position
                .map(f32::to_bits),
            [340.0_f32, 150.0, 80.0].map(f32::to_bits)
        );
    }
}
