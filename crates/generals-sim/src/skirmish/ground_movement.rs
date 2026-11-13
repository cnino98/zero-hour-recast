//! Retained ground routes connected to the translated ground locomotors.

use crate::grid::Grid;
use crate::grid::construction::{CellNode, LineCheck, prepend_cells};

use super::movement::{distance_squared, grid_coordinate};
use super::path_following::{Node, Path};
use super::{CELL_SIZE, Cell, Order, STEP_SECONDS, Skirmish, locomotor, path_service, startup};

const FOLLOWING_SEGMENT_DISTANCE: f32 = 4.0 * CELL_SIZE;
mod attack;
mod move_state;
mod recovery;
pub(super) use attack::Approach;
pub(super) use move_state::MoveResult;

impl Skirmish {
    pub(super) fn approaches_ignored_object(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        matches!(actor.ai.order, Order::Enter(_))
            || (actor.ai.ignored_producer.is_some()
                && actor
                    .active_weapon(&self.catalog[actor.type_index])
                    .contact())
            || self.entities[id].ability_approaching()
    }
    /// `AIInternalMoveToState::onEnter` resets the donut timer once. A new
    /// route within the same state, including following goals, does not.
    pub(super) fn begin_ground_move(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        if actor
            .active_locomotor(&self.catalog[actor.type_index])
            .ground
            .is_some()
            && !actor.ai.motion.movement_active
        {
            actor.ai.motion.movement_active = true;
            actor.ai.motion.start_move(self.tick);
            actor.ai.blocking.start_move();
        }
    }

    pub(super) fn travel_ground(
        &mut self,
        id: usize,
        goal: [f32; 2],
        stop_range: f32,
        attack_target: Option<usize>,
    ) -> bool {
        // Object::isMobile rejects DISABLED_HELD before attack approach/pursuit
        // computes a path; passenger attacks remain enabled in the CRC0 profile.
        if self.entities[id].disabled_held() {
            return false;
        }
        if self.entities[id].ai.path_request.pending.is_some() {
            if self.entities[id].ai.ground_path.is_some() && self.entities[id].ai.motion.has_goal()
            {
                self.follow_ground_path(id, true);
            }
            return false;
        }
        if self.entities[id].ai.path_request.failed {
            self.entities[id].ai.path_request.failed = false;
            if self.entities[id].ability_approaching() {
                self.finish_ground_path(id);
                self.entities[id].ai.motion.clear_goal();
            } else if self.entities[id].cleaning_area().is_some() {
                // AIInternalMoveToState failure leaves the last command's
                // CMD_FROM_AI source intact for CleanupHazardUpdate.
                self.cleanup_order(id, Order::Idle);
            } else {
                self.set_order(id, Order::Idle);
            }
            return false;
        }
        if self.entities[id].ai.blocking.retry_at.is_some() && !self.recover_ground_path(id) {
            return false;
        }
        let reached = attack_target.map_or_else(
            || {
                stop_range >= CELL_SIZE * 0.5
                    && distance_squared(self.entities[id].position, goal) <= stop_range * stop_range
            },
            |target| self.within_attack_range(id, target),
        );
        if reached {
            self.finish_ground_path(id);
            return true;
        }
        self.begin_ground_move(id);
        let reusable = if let Some(target) = attack_target {
            self.reuse_attack_path(id, target)
        } else {
            self.reuse_ground_path(id, goal, stop_range)
        };
        let actor = &self.entities[id];
        if !reusable {
            if let Some(target) = attack_target
                && actor
                    .active_weapon(&self.catalog[actor.type_index])
                    .contact()
            {
                // AIAttackApproachTargetState lets contact weapons run into their target.
                self.entities[id].ai.ignored_producer = Some(target);
            }
            self.set_ground_path_extra_distance(id);
            self.request_ground_path(
                id,
                path_service::Request {
                    goal: self.movement_goal_position(id, goal),
                    range: stop_range,
                    kind: attack_target
                        .map_or(path_service::Kind::Move, path_service::Kind::Attack),
                },
            );
            if self.entities[id].ai.ground_path.is_some() && self.entities[id].ai.motion.has_goal()
            {
                self.follow_ground_path(id, true);
            }
            return false;
        }
        self.advance_ground(id)
    }

    fn reuse_ground_path(&self, id: usize, goal: [f32; 2], stop_range: f32) -> bool {
        let actor = &self.entities[id];
        actor.ai.path_goal.is_some_and(|cell| {
            actor.ai.path_endpoint == Some(goal)
                || distance_squared(self.path_position(id, cell), goal) <= stop_range * stop_range
        }) && (!actor.ai.path.optimized.is_empty() || actor.ai.ground_path.is_some())
    }

    /// Rebuild only when the native route is replaced, never from a moving
    /// actor's current position on each frame.
    pub(super) fn install_ground_path(&mut self, id: usize) {
        self.begin_ground_move(id);
        let actor = &self.entities[id];
        let locomotor = actor.active_locomotor(&self.catalog[actor.type_index]);
        if locomotor.ground.is_none() && locomotor.flight.is_none() {
            return;
        }
        let Some(goal_cell) = actor.ai.path_goal else {
            return;
        };
        let temporary = actor.ai.blocking.temporary_goal();
        let requested = temporary
            .or_else(|| {
                actor
                    .ai
                    .path
                    .exact_endpoint
                    .map(|point| [point[0], point[1]])
            })
            .or(actor.ai.path_endpoint)
            .unwrap_or_else(|| self.path_position(id, goal_cell));
        let initial_exit = temporary.is_none()
            && matches!(actor.ai.order, Order::Exit(_))
            && actor.ai.waypoints.exit_quick_pending;
        let intermediate = temporary.is_none() && self.intermediate_waypoint(id);
        // Route construction decides computeQuickPath. Reinstallation of an
        // ordinary recovery path must not replace its detour with a new line.
        let quick = actor.ai.path.quick_start.is_some() || initial_exit;
        let mut nodes = if quick {
            // computeQuickPath's nodes are not marked optimizable.
            let mut start = node(actor.position, false);
            start.layer = actor.layer;
            start.position = self.object_position(id);
            let mut end = node(requested, false);
            end.layer = actor.ai.path.optimized.back().map_or_else(
                || self.movement_goal_layer(id, requested),
                |node| node.layer,
            );
            end.previous_layer = start.layer;
            end.position[2] = self.movement_layer_height(end.layer, requested);
            vec![start, end]
        } else {
            self.ordinary_ground_nodes(id)
        };
        let actor = &self.entities[id];
        if let Some(endpoint) = actor.ai.path.exact_endpoint {
            if quick {
                nodes[0].position = actor.ai.path.quick_start.unwrap_or([
                    actor.position[0],
                    actor.position[1],
                    endpoint[2],
                ]);
            }
            if let Some(last) = nodes.last_mut() {
                last.position = endpoint;
            }
        }
        if quick {
            self.retain_quick_route_cells(id, &nodes);
        }
        let actor = &mut self.entities[id];
        actor.ai.ground_path = Some(Path::new(nodes));
        if initial_exit {
            // AIUpdate::computePath consumes canPathThroughUnits after the first
            // successful factory quick path, while retaining the ignored factory.
            actor.ai.waypoints.exit_quick_pending = false;
        }
        if temporary.is_none()
            && actor.ai.follow_waypoint.is_none()
            && matches!(actor.ai.order, Order::FollowPath)
        {
            actor.ai.waypoints.adjusts_destination = !intermediate;
        }
        self.set_ground_path_extra_distance(id);
    }

    fn retain_quick_route_cells(&mut self, id: usize, nodes: &[Node]) {
        let (Some(start), Some(end)) = (nodes.first(), nodes.last()) else {
            return;
        };
        let (Some(start_cell), Some(end_cell)) = (
            self.cell_at([start.position[0], start.position[1]]),
            self.cell_at([end.position[0], end.position[1]]),
        ) else {
            return;
        };
        self.entities[id].ai.path.raw = vec![
            crate::grid::LayerCell {
                cell: start_cell,
                layer: start.layer,
            },
            crate::grid::LayerCell {
                cell: end_cell,
                layer: end.layer,
            },
        ];
        self.entities[id].ai.path.raw_flags.clear();
        self.entities[id].ai.path.optimization_blocked = false;
        self.entities[id].ai.path.blocked_by_ally = false;
    }

    fn raw_ground_nodes(&self, id: usize, navigation: &crate::grid::Grid) -> Vec<CellNode> {
        let actor = &self.entities[id];
        actor
            .ai
            .path
            .raw
            .iter()
            .enumerate()
            .map(|(index, &cell)| {
                let current = navigation.path_cell_flags(cell, false);
                let flags = actor.ai.path.raw_flags.get(index).unwrap_or(&current);
                let point = self.path_position(id, cell.cell);
                CellNode {
                    cell: [cell.cell.x, cell.cell.y],
                    position: [
                        point[0],
                        point[1],
                        self.movement_layer_height(cell.layer, point),
                    ],
                    layer: cell.layer,
                    cliff: flags.cliff,
                    pinched: flags.pinched,
                    blocked_by_ally: flags.blocked_by_ally,
                }
            })
            .collect()
    }

    fn ordinary_ground_nodes(&mut self, id: usize) -> Vec<Node> {
        let actor = &self.entities[id];
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let raw = self.raw_ground_nodes(id, navigation);
        let mut path = prepend_cells(&raw, self.object_position(id))
            .expect("ordinary path retains its nonempty raw search chain");
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let blocked = actor.ai.path.optimization_blocked;
        let units = blocked.then(|| self.layer_route_context(id).0);
        path.optimize(|layer, from, to| {
            let Some(destination) = world_cell(navigation, to) else {
                return LineCheck {
                    passable: false,
                    unpinched_cliff: false,
                };
            };
            let flags = navigation.path_cell_flags(
                crate::grid::LayerCell {
                    cell: destination,
                    layer,
                },
                false,
            );
            let pinched_clear = world_cell(navigation, from).is_some_and(|start| {
                navigation.unpinched_line(
                    crate::grid::LayerCell { cell: start, layer },
                    crate::grid::LayerCell {
                        cell: destination,
                        layer,
                    },
                )
            });
            let passable = pinched_clear
                && line_passable_on(
                    navigation,
                    |cell| {
                        self.fixed_unit_blocks_on(id, cell)
                            || units
                                .as_ref()
                                .and_then(|layers| layers.get(&cell.layer))
                                .is_some_and(|units| {
                                    units.transient_allies.contains(&cell.cell)
                                        || units.transient_enemies.contains(&cell.cell)
                                })
                    },
                    radius,
                    centered,
                    from,
                    to,
                    layer,
                );
            LineCheck {
                passable,
                unpinched_cliff: flags.cliff && !flags.pinched,
            }
        });
        let nodes = path
            .optimized
            .iter()
            .map(|&index| {
                let node = &path.nodes[index];
                Node {
                    position: node.position,
                    layer: node.layer,
                    can_optimize: node.can_optimize,
                    previous_layer: node.previous_layer,
                }
            })
            .collect();
        let mut optimized = path.optimized_cells();
        let destination = optimized
            .back()
            .expect("canonical path has a destination")
            .cell;
        if optimized.len() > 1 {
            optimized.pop_front();
        }
        let actor = &mut self.entities[id];
        actor.ai.path.optimized = optimized;
        actor.ai.path.blocked_by_ally = path.blocked_by_ally;
        actor.ai.path_goal = Some(destination);
        nodes
    }

    /// One initial-phase locomotor request; physics displacement is performed
    /// once by the caller's later native physics phase.
    pub(super) fn advance_ground(&mut self, id: usize) -> bool {
        if !self.recover_ground_path(id) {
            if self.entities[id].ai.ground_path.is_some() && self.entities[id].ai.motion.has_goal()
            {
                self.follow_ground_path(id, true);
            }
            return false;
        }
        self.follow_ground_path(id, false)
    }

    pub(super) fn follow_ground_path(&mut self, id: usize, waiting: bool) -> bool {
        if self.entities[id].ai.ground_path.is_none() {
            self.install_ground_path(id);
        }
        if self.entities[id].ai.ground_path.is_none() {
            return true;
        }
        let moving = self.entities[id].ai.blocking.frames <= 7;
        self.entities[id]
            .drawable
            .model_conditions
            .set_flag(crate::skirmish::ModelConditions::MOVING, moving);
        if !waiting
            && self.entities[id].ai.blocking.retry_at.is_none()
            && self.ground_path_arrived(id)
        {
            if self.entities[id].ai.blocking.temporary_move_to()
                && self.move_destination_adjusts(id)
            {
                self.entities[id].ai.motion.clear_goal();
            }
            if !self.entities[id].ai.blocking.temporary_move_to()
                && !matches!(
                    self.entities[id].ai.order,
                    Order::FollowPath | Order::Exit(_)
                )
            {
                self.finish_ground_path(id);
            }
            return true;
        }
        self.entities[id].ai.motion.follow_path();
        false
    }

    /// `AIInternalMoveToState` checks arrival before the base callback's cleanup.
    pub(in crate::skirmish) fn ground_path_arrived(&mut self, id: usize) -> bool {
        let Some(mut path) = self.entities[id].ai.ground_path.take() else {
            return false;
        };
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = actor
            .active_locomotor(unit)
            .ground
            .as_ref()
            .expect("Ground rules selected");
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let position = self.object_position(id);
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let current_layer = actor.layer;
        let mut passable = |layer, from, to| {
            line_passable_on(
                navigation,
                |node| self.fixed_unit_blocks_on(id, node),
                radius,
                centered,
                from,
                to,
                layer,
            )
        };
        let waypoint_arrival = actor.ai.command.path_extra_distance > 1.0;
        let distance = if waypoint_arrival {
            path.waypoint_distance_to_goal(position)
        } else {
            let arrival = path.sample_layer(position, current_layer, &mut passable);
            path.distance_to_goal(position, &arrival)
        };
        if path.arrived(position, distance, rules.close_enough) {
            self.entities[id].ai.ground_path = Some(path);
            return true;
        }
        if distance < rules.close_enough {
            // The ground sanity failure repeats getLocomotorDistanceToGoal
            // before returning STATE_CONTINUE to the locomotor update.
            if waypoint_arrival {
                path.waypoint_distance_to_goal(position);
            } else {
                path.sample_layer(position, current_layer, &mut passable);
            }
        }
        self.entities[id].ai.ground_path = Some(path);
        false
    }

    /// `AIUpdate::doLocomotor` samples after state transitions and completion cleanup.
    pub(in crate::skirmish) fn sample_ground_path_force(&mut self, id: usize) {
        let Some(mut path) = self.entities[id].ai.ground_path.take() else {
            return;
        };
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let position = self.object_position(id);
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let current_layer = actor.layer;
        let mut passable = |layer, from, to| {
            line_passable_on(
                navigation,
                |node| self.fixed_unit_blocks_on(id, node),
                radius,
                centered,
                from,
                to,
                layer,
            )
        };
        let sample = path.sample_layer(position, current_layer, &mut passable);
        self.update_path_layer(id, sample.layer);
        let actor = &mut self.entities[id];
        actor.ai.ground_path = Some(path);
        let goal = locomotor::GroundGoal {
            height: sample.position[2],
            position: [sample.position[0], sample.position[1]],
            distance: sample.distance + actor.ai.command.path_extra_distance,
            // POSITION_ON_PATH clamps FAST_AS_POSSIBLE after selecting the
            // current surface locomotor in doLocomotor, not before a switch.
            desired_speed: actor
                .ai
                .command
                .formation_speed
                .map_or(f32::MAX, |speed| speed * STEP_SECONDS)
                .min(actor.ai.command.desired_speed),
            no_slow_down: actor.ai.motion.no_slow_down(),
            ultra_accurate: actor.ai.motion.ultra_accurate(),
        };
        actor.ai.motion.path_force(goal);
    }

    /// `Pathfinder::updateLayer` validates bridge contact before `Object::setLayer`.
    fn update_path_layer(&mut self, id: usize, candidate: u8) {
        let actor = &self.entities[id];
        let layer = if candidate != 1
            && !(candidate == 15 && actor.layer == 15)
            && !self.movement_terrain.bridge_interacts(
                candidate,
                self.object_position(id),
                self.body_geometry(id).minor,
                true,
            ) {
            1
        } else {
            candidate
        };
        if actor.layer != layer {
            self.entities[id].layer = layer;
            self.update_position_claim(id);
            self.observe_ground_partition(id);
        }
    }

    pub(super) fn finish_ground_path(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        if actor.ai.blocking.temporary_goal().is_none() {
            actor.ai.command.attack_approach = None;
        }
        if actor.ai.motion.movement_active
            && actor.ai.blocking.temporary_goal().is_none()
            && !matches!(actor.ai.order, Order::FollowPath | Order::Exit(_))
        {
            actor.ai.motion.end_move();
            actor.ai.motion.movement_active = false;
        }
        actor.ai.path.clear();
        actor.ai.path_request.attack_path = false;
        actor.ai.path_endpoint = None;
        actor.ai.ground_path = None;
        actor
            .drawable
            .model_conditions
            .set_flag(crate::skirmish::ModelConditions::MOVING, false);
    }

    fn set_ground_path_extra_distance(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        if actor.ai.follow_waypoint.is_some()
            || matches!(
                actor.ai.command.main_state,
                super::ai_goals::MainState::Jet
                    | super::ai_goals::MainState::Wander
                    | super::ai_goals::MainState::Panic
            )
        {
            return;
        }
        if actor.ai.blocking.temporary_goal().is_some() {
            actor.ai.command.path_extra_distance = 0.0;
            return;
        }
        if actor.ai.ignored_producer.is_some()
            && actor
                .active_weapon(&self.catalog[actor.type_index])
                .contact()
            && !matches!(actor.ai.order, Order::Enter(_))
        {
            actor.ai.command.path_extra_distance = 10.0 * CELL_SIZE;
            return;
        }
        match actor.ai.order {
            Order::FollowPath | Order::Exit(_) => {
                if let Some(next) = actor.ai.waypoints.points.get(actor.ai.waypoints.index + 1) {
                    let current = actor.ai.waypoints.points[actor.ai.waypoints.index];
                    let mut distance =
                        distance_squared([current[0], current[1]], [next[0], next[1]]).sqrt();
                    if actor.ai.waypoints.index + 2 < actor.ai.waypoints.points.len() {
                        distance += FOLLOWING_SEGMENT_DISTANCE;
                    }
                    actor.ai.command.path_extra_distance = distance;
                } else if actor.ai.waypoints.index == 0 {
                    actor.ai.command.path_extra_distance = 0.0;
                }
                // AIFollowPathState::update does not clear the previous extra
                // distance on its final segment; onEnter alone resets it.
            }
            _ => actor.ai.command.path_extra_distance = 0.0,
        }
    }

    pub(super) fn ground_line_passable_on(
        &self,
        id: usize,
        from: [f32; 2],
        to: [f32; 2],
        target_layer: u8,
    ) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(from, 0.0));
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        if navigation.has_bridge_layers() {
            let (Some(from), Some(to)) = (self.cell_at(from), self.cell_at(to)) else {
                return false;
            };
            return navigation.layer_line_passable(
                crate::grid::LayerCell {
                    cell: from,
                    layer: actor.layer,
                },
                crate::grid::LayerCell {
                    cell: to,
                    layer: target_layer,
                },
                radius,
                centered,
                |node| self.fixed_unit_blocks_on(id, node),
            );
        }
        line_passable(
            navigation,
            |cell| self.fixed_unit_blocks(id, cell),
            radius,
            centered,
            [from[0], from[1], 0.0],
            [to[0], to[1], 0.0],
        )
    }

    pub(super) fn ground_line_details(
        &self,
        id: usize,
        from: [f32; 3],
        to: [f32; 3],
        layer: u8,
    ) -> String {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let ignored = self.ignored_navigation(id);
        let grid = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let (Some(start), Some(goal)) = (world_cell(grid, from), world_cell(grid, to)) else {
            return "line outside map".into();
        };
        for cell in grid.line_cells(start, goal) {
            let node = grid.resolve_layer(crate::grid::LayerCell { cell, layer });
            if grid.layer_blocked(node) {
                let owner = if node.layer == 1 {
                    self.navigation_obstacles.owner_details(cell)
                } else {
                    "upper-layer terrain or structure".into()
                };
                let terrain = self.grid.layer_blocked(node);
                return format!(
                    "static layer={} cell={:?} terrain={terrain} owner={owner}",
                    node.layer,
                    [cell.x, cell.y]
                );
            }
            for occupied in self.body_cells(id, cell) {
                let node = grid.resolve_layer(crate::grid::LayerCell {
                    cell: occupied,
                    layer,
                });
                if self.fixed_unit_blocks_on(id, node) {
                    let Some(other) = self.occupancy.position_owner_on(node) else {
                        continue;
                    };
                    let target = &self.entities[other];
                    return format!(
                        "fixed cell={:?} owner={other} position={:?} goal={:?} radius={radius} centered={centered}",
                        [occupied.x, occupied.y],
                        target.position,
                        target.ai.path_endpoint
                    );
                }
            }
        }
        "no represented blocker".into()
    }

    pub(super) fn blocking_path_point(&mut self, id: usize) -> Option<[f32; 2]> {
        let mut path = self.entities[id].ai.ground_path.take()?;
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let position = self.object_position(id);
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let sample = path.sample_layer(position, actor.layer, |layer, from, to| {
            let Some(start) = world_cell(navigation, from) else {
                return false;
            };
            let Some(goal) = world_cell(navigation, to) else {
                return false;
            };
            navigation.layer_line_passable(
                crate::grid::LayerCell { cell: start, layer },
                crate::grid::LayerCell { cell: goal, layer },
                radius,
                centered,
                |node| self.fixed_unit_blocks_on(id, node),
            )
        });
        self.entities[id].ai.ground_path = Some(path);
        Some([sample.position[0], sample.position[1]])
    }
}

const fn node(point: [f32; 2], can_optimize: bool) -> Node {
    Node {
        position: [point[0], point[1], 0.0],
        layer: 1,
        can_optimize,
        previous_layer: 1,
    }
}

pub(super) fn line_passable(
    grid: &Grid,
    fixed_unit: impl FnMut(Cell) -> bool,
    radius: i32,
    centered: bool,
    from: [f32; 3],
    to: [f32; 3],
) -> bool {
    let Some(start) = world_cell(grid, from) else {
        return false;
    };
    let Some(goal) = world_cell(grid, to) else {
        return false;
    };
    grid.line_passable(start, goal, radius, centered, fixed_unit)
}

pub(super) fn line_passable_on(
    grid: &Grid,
    mut fixed_unit: impl FnMut(crate::grid::LayerCell) -> bool,
    radius: i32,
    centered: bool,
    from: [f32; 3],
    to: [f32; 3],
    layer: u8,
) -> bool {
    if !grid.has_bridge_layers() {
        return line_passable(
            grid,
            |cell| fixed_unit(crate::grid::LayerCell { cell, layer: 1 }),
            radius,
            centered,
            from,
            to,
        );
    }
    let (Some(start), Some(goal)) = (world_cell(grid, from), world_cell(grid, to)) else {
        return false;
    };
    grid.layer_line_passable(
        crate::grid::LayerCell { cell: start, layer },
        crate::grid::LayerCell { cell: goal, layer },
        radius,
        centered,
        fixed_unit,
    )
}

fn world_cell(grid: &Grid, position: [f32; 3]) -> Option<Cell> {
    Some(Cell {
        x: grid_coordinate(position[0] / CELL_SIZE, grid.width())?,
        y: grid_coordinate(position[1] / CELL_SIZE, grid.height())?,
    })
}

#[cfg(test)]
pub(super) mod tests;
