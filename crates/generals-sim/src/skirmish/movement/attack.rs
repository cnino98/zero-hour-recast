//! Live ground-target queries from `Pathfinder::findAttackPath`.

use crate::grid::{Attack, SearchResult};
use crate::skirmish::{CELL_SIZE, Cell, GroupKind, Skirmish, startup};

impl Skirmish {
    pub(in crate::skirmish) fn attack_path_satisfied(
        &self,
        id: usize,
        target: usize,
        requested: [f32; 3],
    ) -> bool {
        let victim = self
            .entities
            .get(target)
            .filter(|actor| actor.alive())
            .map(|_| target);
        let from = self.object_position(id);
        let to = victim.map_or(requested, |target| self.object_position(target));
        let in_range = victim.map_or_else(
            || {
                let actor = &self.entities[id];
                let weapon = actor.active_weapon(&self.catalog[actor.type_index]);
                let distance =
                    crate::skirmish::movement::distance_squared([from[0], from[1]], [to[0], to[1]])
                        .sqrt();
                let separation = (distance - self.object_radius(id)).max(0.0);
                separation >= weapon.min_range
                    && separation
                        <= actor.weapon_range(
                            &self.catalog[actor.type_index],
                            self.passenger_bonus_flags(id),
                        )
            },
            |target| self.within_attack_range(id, target),
        );
        let skip_view =
            victim.is_some_and(|target| self.height_above_ground(target) > -9.0 * self.gravity);
        in_range
            && (!self.attack_ground_movement(id)
                || skip_view
                || !self.attack_view_blocked(id, victim, from, to))
    }

    /// `AIUpdateInterface::doPathfind` falls back to computePath after the
    /// bounded attack search fails. Failure does not cancel the attack order.
    pub(in crate::skirmish) fn attack_approach_path_counted(
        &mut self,
        id: usize,
        target: usize,
        snapshot: [f32; 3],
    ) -> SearchResult {
        let victim = self
            .entities
            .get(target)
            .filter(|actor| actor.alive())
            .map(|_| target);
        let contact = self.entities[id]
            .active_weapon(&self.catalog[self.entities[id].type_index])
            .contact();
        let mut result = if contact {
            self.entities[id].ai.motion.set_no_slow_down(true);
            let mut result = self.object_approach_path_world(id, snapshot);
            self.finish_contact_route(id, snapshot, &mut result);
            result
        } else {
            self.attack_path_for(id, victim, snapshot)
        };
        if result.route.is_some() {
            self.entities[id].ai.path_request.attack_path = true;
            return result;
        }
        self.entities[id].ai.path_request.attack_path = false;
        let requested = victim.map_or(snapshot, |target| self.object_position(target));
        let layer = self
            .movement_terrain
            .get_layer_for_destination(requested, self.ground_height([requested[0], requested[1]]));
        let point = self.possible_attack_destination(id, [requested[0], requested[1]], layer);
        let goal = [point[0], point[1], requested[2]];
        if let Some(target) = victim {
            self.entities[id].ai.ignored_producer = Some(target);
        }
        let position = self.entities[id].position;
        let next = self.movement_path_world(id, position, goal);
        result.cells += next.cells;
        result.route = next.route;
        if result.route.is_some() {
            return result;
        }
        // computePath's findClosestPath fallback uses pathCostFactor zero.
        let next = self.closest_queued_path(id, goal, 0.0);
        result.cells += next.cells;
        result.route = next.route;
        result
    }

    fn finish_contact_route(&self, id: usize, requested: [f32; 3], result: &mut SearchResult) {
        let Some(route) = result.route.as_mut() else {
            return;
        };
        let Some(end) = route.optimized.back().copied() else {
            return;
        };
        let point = self.path_position(id, end.cell);
        let target = [requested[0], requested[1]];
        let close = crate::skirmish::movement::distance_squared(point, target) < 900.0;
        let endpoint = if close {
            route.exact_endpoint = Some(requested);
            let layer = self
                .movement_terrain
                .get_layer_for_destination(requested, self.ground_height(target));
            if let Some(end) = route.optimized.back_mut() {
                end.layer = layer;
            }
            if let Some(end) = route.raw.last_mut() {
                end.layer = layer;
            }
            target
        } else {
            point
        };
        let from = self.entities[id].position;
        if crate::skirmish::movement::distance_squared(from, endpoint) < 100.0 {
            result.route = None;
        }
    }

    /// Ground `adjustToPossibleDestination` / `checkForPossible`. The caller
    /// retains the last mutated coordinate even when adjustment returns false.
    fn possible_attack_destination(&self, id: usize, mut point: [f32; 2], layer: u8) -> [f32; 2] {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let offset = if centered { 0.0 } else { CELL_SIZE * 0.5 };
        let (Some(start), Some(goal)) = (
            self.cell_at(actor.position),
            self.cell_at([point[0] + offset, point[1] + offset]),
        ) else {
            return point;
        };
        let clear = |cell| {
            self.navigation_for(id).layer_destination_clear(
                crate::grid::LayerCell { cell, layer },
                radius,
                centered,
            ) && self.destination_unclaimed_on(id, crate::grid::LayerCell { cell, layer })
        };
        if self.navigation_for(id).quick_path_exists_layers(
            actor.locomotor_surfaces(unit),
            crate::grid::LayerCell {
                cell: start,
                layer: actor.layer,
            },
            crate::grid::LayerCell { cell: goal, layer },
        ) && clear(goal)
        {
            return point;
        }
        let mut position = [i32::from(goal.x), i32::from(goal.y)];
        let mut segment = 1;
        let mut remaining = 400;
        while remaining > 0 {
            for (index, direction) in [[1, 0], [0, 1], [-1, 0], [0, -1]].into_iter().enumerate() {
                for _ in 0..segment {
                    position[0] += direction[0];
                    position[1] += direction[1];
                    remaining -= 1;
                    let (Ok(x), Ok(y)) = (u16::try_from(position[0]), u16::try_from(position[1]))
                    else {
                        continue;
                    };
                    let cell = Cell { x, y };
                    if self.navigation_for(id).quick_path_exists_layers(
                        actor.locomotor_surfaces(unit),
                        crate::grid::LayerCell {
                            cell: start,
                            layer: actor.layer,
                        },
                        crate::grid::LayerCell { cell, layer },
                    ) {
                        point = self.path_position(id, cell);
                        if clear(cell) {
                            return point;
                        }
                    }
                }
                if index & 1 == 1 {
                    segment += 1;
                }
            }
        }
        point
    }

    fn point_sphere_distance_squared(&self, id: usize, point: [f32; 3]) -> f32 {
        let center = self.target_center(id);
        let squares = [0, 1, 2].map(|axis| (point[axis] - center[axis]).powi(2));
        let xy = squares[0] + squares[1];
        let distance = xy + squares[2];
        let radius = self.catalog[self.entities[id].type_index]
            .geometry_bounds
            .sphere_radius;
        if radius > 0.0 {
            (distance.sqrt() - radius).max(0.0).powi(2)
        } else {
            distance
        }
    }

    fn attack_path_for(
        &mut self,
        id: usize,
        victim: Option<usize>,
        requested: [f32; 3],
    ) -> SearchResult {
        let quick = self.search_attack_path_for(id, victim, requested, None, true);
        if quick.route.is_some() {
            return quick;
        }
        let (mut destination, _) = self.attack_destination(id, victim, requested);
        let layer = victim.map_or(1, |target| self.entities[target].layer);
        destination[2] = self.movement_layer_height(layer, [destination[0], destination[1]]);
        let mut query = self.hierarchy_query(id, self.object_position(id), destination);
        query.closest = true;
        query.crusher = self.catalog[self.entities[id].type_index]
            .crush
            .crusher_level
            > 0;
        let hierarchy = self.hierarchical_path(&query);
        let mut result =
            self.search_attack_path_for(id, victim, requested, hierarchy.blocks.as_ref(), false);
        result.cells += hierarchy.search.cells;
        self.finish_path_search(result)
    }

    fn attack_destination(
        &self,
        id: usize,
        victim: Option<usize>,
        requested: [f32; 3],
    ) -> ([f32; 3], Option<[[f32; 3]; 2]>) {
        let bridge = victim.and_then(|target| {
            self.catalog[self.entities[target].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "BRIDGE")
                .then(|| {
                    self.bridge_attack_points(target)
                        .expect("live bridge victim")
                })
        });
        let position = bridge.map_or_else(
            || victim.map_or(requested, |target| self.object_position(target)),
            |points| {
                let distances = points.map(|point| self.point_sphere_distance_squared(id, point));
                if distances[1] < distances[0] {
                    points[1]
                } else {
                    points[0]
                }
            },
        );
        (position, bridge)
    }

    fn search_attack_path_for(
        &self,
        id: usize,
        victim: Option<usize>,
        requested: [f32; 3],
        corridor: Option<&std::collections::BTreeSet<Cell>>,
        quick: bool,
    ) -> SearchResult {
        let (position, bridge) = self.attack_destination(id, victim, requested);
        let target_position = [position[0], position[1]];
        let heuristic_position =
            victim.map_or(target_position, |target| self.entities[target].position);
        let target_radius = if bridge.is_some() {
            0.0
        } else {
            victim.map_or(0.0, |target| self.object_radius(target))
        };
        let target_layer = victim.map_or(1, |target| self.entities[target].layer);
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (radius, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        let unavailable = self.unavailable_goals(id);
        let range = actor.weapon_range(unit, self.passenger_bonus_flags(id));
        let query = Attack {
            from: self.object_position(id),
            target: heuristic_position,
            range_targets: bridge.map(|points| points.map(|point| [point[0], point[1]])),
            radius,
            centered,
            weapon_range: [actor.active_weapon(unit).min_range, range],
            geometry_radius: self.object_radius(id) + target_radius,
            distance: range + self.object_radius(id) + target_radius,
            vehicle: matches!(unit.group_kind, GroupKind::Vehicle),
            unavailable: &unavailable,
        };
        let ignored = self.ignored_navigation(id);
        let navigation = ignored.as_ref().unwrap_or_else(|| self.navigation_for(id));
        let source_height = self.object_position(id)[2];
        let victim_position = [
            position[0],
            position[1],
            self.movement_layer_height(target_layer, target_position),
        ];
        let mut view = |point| {
            self.attack_position_visible(id, victim, victim_position, source_height, &point)
        };
        if navigation.has_bridge_layers() {
            let (Some(start), Some(goal)) = (
                self.cell_at(actor.position),
                self.cell_at(heuristic_position),
            ) else {
                return SearchResult::empty();
            };
            let (mut units, unavailable) = self.layer_route_context(id);
            for context in units.values_mut() {
                context.corridor = corridor.cloned();
            }
            let layer_query = crate::grid::LayerQuery {
                start: crate::grid::LayerCell {
                    cell: start,
                    layer: actor.layer,
                },
                goal: crate::grid::LayerCell {
                    cell: goal,
                    layer: target_layer,
                },
                radius,
                centered,
                closest_cost: None,
            };
            if quick {
                let route =
                    navigation.quick_attack_layers(&query, &layer_query, &unavailable, &mut view);
                return SearchResult {
                    route,
                    cells: 0,
                    info_cleanup: Vec::new(),
                    retained_infos: Vec::new(),
                };
            }
            return navigation.attack_search_layers_counted(
                &query,
                &layer_query,
                &units,
                &unavailable,
                view,
            );
        }
        if quick {
            let route = navigation.quick_attack_path(&query, view);
            return SearchResult {
                route,
                cells: 0,
                info_cleanup: Vec::new(),
                retained_infos: Vec::new(),
            };
        }
        let mut units = self.route_units(id);
        units.corridor = corridor.cloned();
        navigation.attack_search_counted(&query, &units, view)
    }
    fn attack_position_visible(
        &self,
        id: usize,
        victim: Option<usize>,
        victim_position: [f32; 3],
        source_height: f32,
        point: &crate::grid::AttackPoint,
    ) -> bool {
        if point.on_grid
            && victim.is_some_and(|target| self.height_above_ground(target) > -9.0 * self.gravity)
        {
            return true;
        }
        let height = if point.on_grid {
            self.movement_layer_height(point.layer, point.position)
        } else {
            source_height
        };
        !self.attack_view_blocked(
            id,
            victim,
            [point.position[0], point.position[1], height],
            victim_position,
        )
    }
}
