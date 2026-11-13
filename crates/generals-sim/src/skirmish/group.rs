//! Source human ground-group destination assignment. Shared columns and
//! destination claims follow `AIGroup.cpp` and `AIPathfind.cpp`; source profile
//! and remaining layer/locomotor limits are recorded in docs/PATHFINDING.md.

use super::ai_goals::COMMAND_FROM_PLAYER;
use super::movement::{bounding_radius, distance_squared, native_cell_position};
use super::{CELL_SIZE, Cell, GroupKind, MovementSurface, Order, Skirmish, startup};

mod columns;
#[cfg(test)]
mod destination_tests;
mod formation;

impl Skirmish {
    pub(super) fn adjusted_destination(&mut self, id: usize, point: [f32; 2]) -> Option<[f32; 2]> {
        self.adjust_destination(id, point, None)
    }

    fn adjust_destination(
        &mut self,
        id: usize,
        point: [f32; 2],
        group: Option<[f32; 2]>,
    ) -> Option<[f32; 2]> {
        self.adjust_destination_on(
            id,
            [point[0], point[1], self.ground_height(point)],
            group,
            self.destination_layer(point),
        )
        .map(|point| [point[0], point[1]])
    }

    pub(super) fn adjusted_destination_world(
        &mut self,
        id: usize,
        point: [f32; 3],
    ) -> Option<[f32; 3]> {
        let layer = self
            .movement_terrain
            .get_layer_for_destination(point, self.ground_height([point[0], point[1]]));
        self.adjust_destination_on(id, point, None, layer)
    }

    fn adjust_destination_on(
        &mut self,
        id: usize,
        point: [f32; 3],
        group: Option<[f32; 2]>,
        layer: u8,
    ) -> Option<[f32; 3]> {
        let horizontal = [point[0], point[1]];
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (_, centered) = startup::radius_and_center(&unit.footprint(horizontal, 0.0));
        let offset = if centered { 0.0 } else { CELL_SIZE * 0.5 };
        // adjustDestination adds the half-cell in world units before division;
        // updateGoal/updatePos perform that addition after division instead.
        let cell = self.clipped_path_cell([point[0] + offset, point[1] + offset]);
        if let Some(adjusted) = self.check_group_adjust(id, cell, horizontal, group, layer) {
            // RETAIL_COMPATIBLE_PATHFINDING=0 preserves the exact original
            // coordinate only for a human single-object group with a clear goal.
            return Some(
                if self.single_ai_group(id) && !self.computer_player(self.entities[id].owner) {
                    point
                } else {
                    adjusted
                },
            );
        }
        let mut position = [i32::from(cell.x), i32::from(cell.y)];
        let mut segment = 1;
        let mut remaining = 400;
        // adjustDestination completes the entire four-direction lap even when
        // its counter reaches zero part-way through that lap.
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
                    if let Some(adjusted) =
                        self.check_group_adjust(id, Cell { x, y }, horizontal, group, layer)
                    {
                        return Some(adjusted);
                    }
                }
                if index & 1 == 1 {
                    segment += 1;
                }
            }
        }
        group.and_then(|_| self.adjust_destination_on(id, point, None, layer))
    }

    fn check_group_adjust(
        &mut self,
        id: usize,
        cell: Cell,
        requested: [f32; 2],
        group: Option<[f32; 2]>,
        layer: u8,
    ) -> Option<[f32; 3]> {
        let mut point = self.check_adjust(id, cell, requested, layer)?;
        if let Some(group) = group {
            let from = self.cell_at([point[0], point[1]])?;
            let to = self.cell_at(group)?;
            // Fixed-CRC tightenPathCallback uses orgDestPos and continues even
            // past rejected cells; retain the final accepted line candidate.
            for cell in self
                .navigation_for(id)
                .line_cells(from, to)
                .into_iter()
                .skip(1)
            {
                if let Some(adjusted) = self.check_adjust(id, cell, group, layer) {
                    point = adjusted;
                }
            }
            let units = self.layer_path_info();
            let navigation = self.navigation_for(id);
            let result = navigation.path_cost_counted(
                crate::grid::LayerCell { cell: to, layer },
                crate::grid::LayerCell {
                    cell: self.cell_at([point[0], point[1]])?,
                    layer,
                },
                &units,
            );
            drop(units);
            self.finish_path_search(result.search);
            let cost = result.cost?;
            let dx = (group[0] - point[0]).abs().trunc();
            let dy = (group[1] - point[1]).abs().trunc();
            if f64::from(1.4 * (dx + dy)) < f64::from(cost) {
                return None;
            }
        }
        Some(point)
    }

    fn check_adjust(
        &self,
        id: usize,
        cell: Cell,
        requested: [f32; 2],
        layer: u8,
    ) -> Option<[f32; 3]> {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let (_, centered) = startup::radius_and_center(&unit.footprint(actor.position, 0.0));
        if matches!(
            self.navigation_for(id)
                .source_cell_kind(crate::grid::LayerCell { cell, layer }),
            Some(crate::terrain::CellType::Cliff)
        ) || !self.destination_available(id, crate::grid::LayerCell { cell, layer })
        {
            return None;
        }
        let point = native_cell_position(cell, centered);
        // checkForAdjust preserves the requested destination's reachable zone
        // even if the actor cannot currently reach that zone. Actual movement
        // is computed later; do not silently choose a point on the actor's side.
        (unit.kind_of.iter().any(|kind| kind == "AIRCRAFT")
            || (!self.quick_ground_path_on(id, actor.position, requested, actor.layer, layer)
                && self.quick_ground_path_on(id, requested, point, layer, layer))
            || self.quick_ground_path_on(id, actor.position, point, actor.layer, layer))
        .then(|| {
            let actual_layer = self
                .navigation_for(id)
                .resolve_layer(crate::grid::LayerCell { cell, layer })
                .layer;
            [
                point[0],
                point[1],
                self.movement_layer_height(actual_layer, point),
            ]
        })
    }

    fn quick_ground_path_on(
        &self,
        id: usize,
        from: [f32; 2],
        to: [f32; 2],
        from_layer: u8,
        to_layer: u8,
    ) -> bool {
        let (Some(from), Some(to)) = (self.cell_at(from), self.cell_at(to)) else {
            return false;
        };
        let actor = &self.entities[id];
        let surfaces = actor.locomotor_surfaces(&self.catalog[actor.type_index]);
        if self.navigation_for(id).has_bridge_layers() {
            self.navigation_for(id).quick_path_exists_layers(
                surfaces,
                crate::grid::LayerCell {
                    cell: from,
                    layer: from_layer,
                },
                crate::grid::LayerCell {
                    cell: to,
                    layer: to_layer,
                },
            )
        } else {
            self.navigation_for(id)
                .quick_path_exists(surfaces, from, to)
        }
    }

    /// Applies one original group move/Alt-waypoint command to a selection snapshot.
    ///
    /// # Errors
    /// Rejects unavailable selections and invalid destinations.
    pub fn group_move_to(
        &mut self,
        ids: &[usize],
        position: [f32; 2],
        add_waypoint: bool,
    ) -> Result<(), &'static str> {
        self.group_move_to_world(
            ids,
            [position[0], position[1], self.ground_height(position)],
            add_waypoint,
        )
    }

    /// Original world-space group command including the picked destination Z.
    /// # Errors
    /// Rejects unavailable actors and invalid destinations.
    pub fn group_move_to_world(
        &mut self,
        ids: &[usize],
        position: [f32; 3],
        add_waypoint: bool,
    ) -> Result<(), &'static str> {
        for &id in ids {
            if self
                .entities
                .get(id)
                .is_none_or(|actor| actor.contained_by().is_none())
            {
                self.ready_actor(id)?;
            }
        }
        for &id in ids {
            self.release_group_temporary_lock(id);
        }
        self.move_group_world(ids, position, add_waypoint, COMMAND_FROM_PLAYER)
    }

    /// `AIGroup::groupAttackMoveToPosition` dispatches in member order.
    /// # Errors
    /// Rejects a nonfinite destination or a group with no available member.
    pub fn group_attack_move_world(
        &mut self,
        members: &[usize],
        position: [f32; 3],
    ) -> Result<(), &'static str> {
        if position.iter().any(|value| !value.is_finite()) {
            return Err("Destination is not finite");
        }
        for &id in members {
            if self.entities.get(id).is_some_and(super::Entity::exists) {
                self.release_group_temporary_lock(id);
            }
        }
        let mut issued = false;
        for &id in members {
            if self.ready_actor(id).is_err()
                || self.entities[id].ai.registration.is_none()
                || self.catalog[self.entities[id].type_index].immobile()
            {
                continue;
            }
            self.dozer_player_command(id);
            if self.object_can_attack(id) {
                self.native_attack_move_from(
                    id,
                    position,
                    i32::MAX.unsigned_abs(),
                    COMMAND_FROM_PLAYER,
                );
            } else {
                self.native_move_from(id, position, COMMAND_FROM_PLAYER);
            }
            self.finish_dozer_player_command(id);
            issued = true;
        }
        if issued {
            Ok(())
        } else {
            Err("Selection has no available attack-move members")
        }
    }

    /// `AIGroup::groupGuardPosition` keeps the same requested center for every member.
    /// # Errors
    /// Rejects a nonfinite position or a group with no available guard member.
    pub fn group_guard_world(
        &mut self,
        members: &[usize],
        position: [f32; 3],
        flying_only: bool,
    ) -> Result<(), &'static str> {
        if position.iter().any(|value| !value.is_finite()) {
            return Err("Guard position is not finite");
        }
        let mut issued = false;
        for &id in members {
            issued |= self.guard_world_mode(id, position, flying_only).is_ok();
        }
        if issued {
            Ok(())
        } else {
            Err("Selection has no available guard members")
        }
    }

    pub(super) fn move_group_world(
        &mut self,
        ids: &[usize],
        world: [f32; 3],
        add_waypoint: bool,
        source: i32,
    ) -> Result<(), &'static str> {
        if world.iter().any(|value| !value.is_finite()) {
            return Err("Destination is not finite");
        }
        let position = [world[0], world[1]];
        let mut members = Vec::new();
        for &id in ids {
            if self
                .entities
                .get(id)
                .is_some_and(|actor| actor.disabled_held() || actor.contained_by().is_some())
            {
                // AIGroup excludes DISABLED_HELD passengers while issuing the
                // carrier's move; a held team member must not reject the group.
                continue;
            }
            let actor = self.active_actor(id)?;
            if actor.exists() && !members.contains(&id) {
                members.push(id);
            }
        }
        if members.len() > usize::from(u16::MAX) {
            return Err("Selection exceeds the group size limit");
        }
        let mut movers: Vec<_> = members
            .iter()
            .copied()
            .filter(|&id| self.group_position_mover(id))
            .collect();
        if movers.is_empty() {
            return Err("Selection has no mobile units");
        }
        self.refresh_occupancy();
        let (center, min, max, formation) = self.group_bounds(&members);
        self.update_formation_speed(&members);
        let formation = !add_waypoint && formation;
        let mut did_infantry = false;
        let mut did_vehicles = false;
        if !add_waypoint && !formation {
            for &id in &members {
                self.remove_goal(id);
            }
            if let Some(route) = self.ground_group_route(&members, center, min, max, world) {
                did_infantry = self.move_columns(&movers, center, world, &route, true, source);
                // Infantry deletes a too-short shared path before vehicles see it.
                if columns::valid_infantry_anchors(&route) {
                    did_vehicles = self.move_columns(&movers, center, world, &route, false, source);
                }
            }
        }
        let margin = members.iter().fold(40.0_f32, |margin, &id| {
            let unit = &self.catalog[self.entities[id].type_index];
            margin.max(
                40.0 + match unit.group_kind {
                    GroupKind::Helicopter => unit.geometry.major,
                    GroupKind::Aircraft => 100.0,
                    _ => 0.0,
                },
            )
        });
        let gather = source == COMMAND_FROM_PLAYER
            && !add_waypoint
            && !formation
            && self.gather_group(&members, position, min, max);
        let formation = formation && members.iter().all(|&id| self.allows_group_formation(id));
        let world = self.clamp_group_destination(world, margin.trunc());
        if gather {
            return self.tighten_group(&movers, world);
        }
        if formation {
            let route = self.ground_group_route(&members, center, min, max, world);
            return self.move_formation(&movers, world, route.as_deref(), source);
        }
        movers.retain(|&id| {
            !(did_infantry && self.attack_kind(id, "INFANTRY")
                || did_vehicles
                    && self.attack_kind(id, "VEHICLE")
                    && self.attack_ground_movement(id)
                    && !self.attack_kind(id, "CLIFF_JUMPER"))
        });
        if movers.is_empty() {
            return Ok(());
        }
        self.move_individual_group(movers, world, add_waypoint, source)
    }

    fn group_position_mover(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        actor.ai.registration.is_some() && unit.speed > 0.0 && !unit.immobile()
    }

    fn move_individual_group(
        &mut self,
        mut members: Vec<usize>,
        world: [f32; 3],
        add_waypoint: bool,
        source: i32,
    ) -> Result<(), &'static str> {
        let position = [world[0], world[1]];
        let layer = self
            .movement_terrain
            .get_layer_for_destination(world, self.ground_height(position));
        // SimpleObjectIterator prepends, then sorts stably. Equal distances
        // therefore retain reverse source member-list order.
        members.reverse();
        members.sort_by(|&a, &b| {
            distance_squared(self.entities[a].position, position)
                .total_cmp(&distance_squared(self.entities[b].position, position))
        });
        for &id in &members {
            self.remove_goal(id);
        }
        let center = self.entities[members[0]].position;
        let mut failure = None;
        for id in members {
            self.entities[id].ai.command.formation_id = None;
            self.entities[id].ai.command.formation_speed = None;
            let actor = &self.entities[id];
            let unit = &self.catalog[actor.type_index];
            let delta = [actor.position[0] - center[0], actor.position[1] - center[1]];
            let x_squared = delta[0] * delta[0];
            let y_squared = delta[1] * delta[1];
            let length = (x_squared + y_squared).sqrt();
            let cap = 6.0 * bounding_radius(&unit.footprint(actor.position, actor.facing));
            let displacement = if length > 0.0 {
                [
                    (delta[0] / length) * length.min(cap),
                    (delta[1] / length) * length.min(cap),
                ]
            } else {
                [0.0, 0.0]
            };
            let wanted = [position[0] + displacement[0], position[1] + displacement[1]];
            let mut destination = [
                wanted[0],
                wanted[1],
                self.movement_layer_height(layer, wanted),
            ];
            if self.attack_ground_movement(id) {
                if let Some(adjusted) =
                    self.adjust_destination_on(id, destination, Some(position), layer)
                {
                    destination = adjusted;
                }
                self.reserve_goal_on(id, [destination[0], destination[1]], 1);
            }
            if source == COMMAND_FROM_PLAYER {
                self.delay_group_stealth_acquisition(id);
            }
            let result = if add_waypoint && !self.object_disabled(id) {
                self.queue_move_to_world(id, destination)
            } else {
                self.move_group_member_from(id, destination, source)
            };
            if let Err(error) = result {
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn move_group_member_from(
        &mut self,
        id: usize,
        position: [f32; 3],
        source: i32,
    ) -> Result<(), &'static str> {
        if self.object_disabled(id) {
            return Ok(());
        }
        let actor = &self.entities[id];
        let locomotor = actor.active_locomotor(&self.catalog[actor.type_index]);
        let simplified = locomotor.ground.is_none() && locomotor.flight.is_none();
        self.assault_command_from(
            id,
            Order::Move([position[0], position[1]]),
            Some(position),
            source,
            false,
        );
        if simplified {
            // Catalogs without translated force data use the existing
            // simplified movement adapter.
            self.set_last_command_source(id, source);
            return self.begin_move_world(id, position);
        }
        if source == COMMAND_FROM_PLAYER {
            self.dozer_player_command(id);
        }
        self.native_move_from(id, position, source);
        if source == COMMAND_FROM_PLAYER {
            self.finish_dozer_player_command(id);
        }
        Ok(())
    }

    fn group_bounds(&self, members: &[usize]) -> ([f32; 2], [f32; 2], [f32; 2], bool) {
        let mut center = [0.0; 2];
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        let mut count = 0_u16;
        let mut formation = false;
        for &id in members
            .iter()
            .filter(|&&id| self.entities[id].ai.registration.is_some())
        {
            if count == 0 {
                formation = self.entities[id].ai.command.formation_id.is_some();
            }
            count += 1;
            for axis in 0..2 {
                let value = self.entities[id].position[axis];
                center[axis] += value;
                min[axis] = min[axis].min(value);
                max[axis] = max[axis].max(value);
            }
        }
        let divisor = f32::from(count);
        (
            [center[0] / divisor, center[1] / divisor],
            min,
            max,
            formation && count > 1,
        )
    }

    /// `AIGroup::getCenter` prefers unheld objects with an AI interface.
    pub(in crate::skirmish) fn native_group_center(&self, members: &[usize]) -> Option<[f32; 3]> {
        let mut ai_center = [0.0; 3];
        let mut all_center = [0.0; 3];
        let mut ai_count = 0_u16;
        let mut all_count = 0_u16;
        for &id in members {
            let actor = &self.entities[id];
            if actor.disabled_held() {
                continue;
            }
            let position = self.object_position(id);
            all_count = all_count.checked_add(1)?;
            for axis in 0..3 {
                all_center[axis] += position[axis];
            }
            if actor.ai.registration.is_some() {
                ai_count = ai_count.checked_add(1)?;
                for axis in 0..3 {
                    ai_center[axis] += position[axis];
                }
            }
        }
        if ai_count != 0 {
            Some(ai_center.map(|axis| axis / f32::from(ai_count)))
        } else {
            (all_count != 0).then(|| all_center.map(|axis| axis / f32::from(all_count)))
        }
    }

    fn ground_group_leader(
        &self,
        members: &[usize],
        center: [f32; 2],
        min: [f32; 2],
        max: [f32; 2],
        position: [f32; 2],
    ) -> Option<usize> {
        let mut nearest = None;
        let mut nearest_distance = f32::MAX;
        let mut distance = 4.0 * self.ai_rules.distance_requires_group.powi(2);
        let mut infantry = 0;
        let mut vehicles = 0;
        for &id in members {
            if self.entities[id].ai.registration.is_none() {
                continue;
            }
            if self.attack_kind(id, "INFANTRY") {
                infantry += 1;
            } else if self.attack_kind(id, "VEHICLE") && !self.attack_kind(id, "AIRCRAFT") {
                vehicles += 1;
            } else {
                continue;
            }
            let point = self.entities[id].position;
            distance = distance.min(distance_squared(point, position));
            let from_center = distance_squared(point, center);
            if from_center < nearest_distance {
                nearest = Some(id);
                nearest_distance = from_center;
            }
        }
        let nearest = nearest?;
        let start = self.entities[nearest].position;
        let spread = distance_squared(min, max);
        if spread > self.ai_rules.distance_requires_group.powi(2) {
            distance = spread;
        }
        if distance < self.ai_rules.min_distance_for_group.powi(2) {
            return None;
        }
        let close = distance > self.ai_rules.distance_requires_group.powi(2)
            || infantry > 6
            || vehicles > 4;
        if !close
            && !members.iter().all(|&id| {
                if !self.attack_kind(id, "INFANTRY") {
                    return true;
                }
                self.ground_line_passable_on(
                    id,
                    self.entities[id].position,
                    start,
                    self.entities[id].layer,
                )
            })
        {
            return None;
        }
        Some(nearest)
    }

    fn ground_group_route(
        &mut self,
        members: &[usize],
        center: [f32; 2],
        min: [f32; 2],
        max: [f32; 2],
        world: [f32; 3],
    ) -> Option<Vec<[f32; 3]>> {
        let position = [world[0], world[1]];
        let nearest = self.ground_group_leader(members, center, min, max, position)?;
        let start = self.entities[nearest].position;
        let from = self.object_position(nearest);
        let mut query = self.hierarchy_query(nearest, from, world);
        query.surfaces = 1;
        let hierarchy = self.hierarchical_path(&query);
        let navigation = self.rubble_navigation.as_ref().unwrap_or(&self.navigation);
        let start_node = crate::grid::LayerCell {
            cell: self.cell_at(start)?,
            layer: self
                .movement_terrain
                .get_layer_for_destination(from, self.ground_height(start)),
        };
        let goal = crate::grid::LayerCell {
            cell: self.cell_at(position)?,
            layer: self
                .movement_terrain
                .get_layer_for_destination(world, self.ground_height(position)),
        };
        let (mut units, _) = self.layer_route_context(nearest);
        for context in units.values_mut() {
            context.corridor.clone_from(&hierarchy.blocks);
        }
        let mut result = navigation.group_path_counted(start_node, goal, &units);
        result.cells += hierarchy.search.cells;
        // Path construction reads width blockers, not retained INFO. Release the
        // snapshot before source cleanup while preserving its original ordering.
        for context in units.values_mut() {
            context.shared_info = None;
        }
        let search_route = self.finish_path_search(result).route?;
        let navigation = self.rubble_navigation.as_ref().unwrap_or(&self.navigation);
        let path = navigation.construct_group_path(
            &search_route.raw,
            &search_route.raw_flags,
            from,
            &units.iter().map(|(&layer, units)| (layer, units)).collect(),
            |node| {
                let point = native_cell_position(node.cell, false);
                [
                    point[0],
                    point[1],
                    self.movement_layer_height(node.layer, point),
                ]
            },
        );
        Some(
            path.optimized
                .iter()
                .map(|&index| path.nodes[index].position)
                .collect(),
        )
    }

    fn gather_group(
        &self,
        members: &[usize],
        position: [f32; 2],
        mut min: [f32; 2],
        mut max: [f32; 2],
    ) -> bool {
        if self.ai_rules.group_move_gather_factor <= 0.0
            || members
                .iter()
                .any(|&id| self.group_aircraft_uses_flight(id))
        {
            return false;
        }
        let scale = self.ai_rules.group_move_gather_factor - 1.0;
        for axis in 0..2 {
            let delta = (max[axis] - min[axis]) * scale * 0.5;
            min[axis] -= delta;
            max[axis] += delta;
        }
        let width = ((max[0] - min[0]) / CELL_SIZE).trunc();
        // Original uses X width for both dx and dy here; preserve it.
        width * width < 2000.0
            && (0..2).all(|axis| position[axis] >= min[axis] && position[axis] <= max[axis])
    }

    fn allows_group_formation(&self, id: usize) -> bool {
        !matches!(
            self.catalog[self.entities[id].type_index].group_kind,
            GroupKind::Helicopter
        ) && !self.group_aircraft_uses_flight(id)
    }

    fn group_aircraft_uses_flight(&self, id: usize) -> bool {
        matches!(
            self.catalog[self.entities[id].type_index].group_kind,
            GroupKind::Aircraft
        ) && self.entities[id].ai.registration.is_some()
            && !self.attack_ground_movement(id)
    }

    fn clamp_group_destination(&self, world: [f32; 3], margin: f32) -> [f32; 3] {
        let position = [world[0], world[1]];
        let clamped = self.clamp_group_point(position, margin);
        if position.map(f32::to_bits) == clamped.map(f32::to_bits) {
            world
        } else {
            [clamped[0], clamped[1], self.ground_height(clamped)]
        }
    }

    fn tighten_group(&mut self, members: &[usize], world: [f32; 3]) -> Result<(), &'static str> {
        let position = [world[0], world[1]];
        let mut members = members.to_vec();
        members.reverse();
        members.sort_by(|&a, &b| {
            distance_squared(self.entities[a].position, position)
                .total_cmp(&distance_squared(self.entities[b].position, position))
        });
        let mut helicopter = 0;
        for id in members {
            let goal = if matches!(
                self.catalog[self.entities[id].type_index].group_kind,
                GroupKind::Helicopter
            ) {
                let point = helicopter_offset(position, helicopter);
                helicopter += 1;
                point
            } else {
                position
            };
            self.tighten_member(id, goal, world[2])?;
        }
        Ok(())
    }

    fn tighten_member(
        &mut self,
        id: usize,
        goal: [f32; 2],
        height: f32,
    ) -> Result<(), &'static str> {
        self.remove_goal(id);
        if matches!(
            self.catalog[self.entities[id].type_index].movement_surface,
            MovementSurface::Air
        ) {
            self.begin_move_world(id, [goal[0], goal[1], height])?;
            return Ok(());
        }
        let actor = &self.entities[id];
        if actor
            .active_locomotor(&self.catalog[actor.type_index])
            .ground
            .is_some()
        {
            if !self.native_subclass_command(
                id,
                crate::skirmish::ai_dispatch::stored_command::Command {
                    source: crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
                    kind: crate::skirmish::ai_dispatch::stored_command::Kind::Move([
                        goal[0], goal[1], height,
                    ]),
                },
            ) {
                self.set_order(id, Order::Move(goal));
                self.entities[id].ai.command.goal_position = [goal[0], goal[1], height];
                self.remove_goal(id);
                self.begin_ground_move(id);
                self.request_ground_path(
                    id,
                    super::path_service::Request {
                        goal: [goal[0], goal[1], height],
                        range: 0.1,
                        kind: super::path_service::Kind::Tighten,
                    },
                );
            }
            return Ok(());
        }
        let (radius, centered) = startup::radius_and_center(
            &self.catalog[actor.type_index].footprint(actor.position, 0.0),
        );
        let start = self
            .cell_at(actor.position)
            .ok_or("Actor is outside the map")?;
        let target = self
            .movement_cell(id, goal)
            .ok_or("Gather destination is outside the map")?;
        let result = self.navigation_for(id).closest_path_counted(
            start,
            target,
            radius,
            centered,
            &self.unavailable_goals(id),
            &self.route_units(id),
        );
        let mut path = self
            .finish_path_search(result)
            .route
            .ok_or("Gather position is unreachable")?;
        let cell = *path.optimized.back().ok_or("Gather path is empty")?;
        let goal = native_cell_position(cell.cell, centered);
        path.optimized.pop_front();
        if path.optimized.is_empty() {
            path.optimized.push_back(cell);
        }
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source: crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Move([
                    goal[0],
                    goal[1],
                    self.ground_height(goal),
                ]),
            },
        ) {
            return Ok(());
        }
        self.set_order(id, Order::Move(goal));
        self.entities[id].ai.path = path;
        self.entities[id].ai.path_goal = Some(cell.cell);
        self.entities[id].ai.path_endpoint = Some(goal);
        self.install_ground_path(id);
        self.reserve_goal(id, goal);
        Ok(())
    }

    pub(super) fn clamp_group_point(&self, point: [f32; 2], margin: f32) -> [f32; 2] {
        let width = f32::from(self.grid.width()) * CELL_SIZE;
        let height = f32::from(self.grid.height()) * CELL_SIZE;
        let limits = [width - margin, height - margin];
        std::array::from_fn(|axis| {
            if point[axis] > limits[axis] {
                limits[axis]
            } else if point[axis] < margin {
                margin
            } else {
                point[axis]
            }
        })
    }
}

fn helicopter_offset(point: [f32; 2], index: usize) -> [f32; 2] {
    if index == 0 {
        return point;
    }
    let mut radius = 70.0;
    let mut angle = 0.0_f32;
    let mut spacing = (70.0 / (radius * std::f32::consts::TAU)) * std::f32::consts::TAU;
    for _ in 1..index {
        angle += spacing;
        if angle > std::f32::consts::TAU {
            radius += 70.0;
            spacing = (70.0 / (radius * std::f32::consts::TAU)) * std::f32::consts::TAU;
            angle -= std::f32::consts::TAU;
        }
    }
    let x = angle.sin() * radius;
    let y = angle.cos() * radius;
    [point[0] + x, point[1] + y]
}
