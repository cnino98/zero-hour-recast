//! Ground order states from the pinned AIUpdate.cpp, AIStates.cpp and AIGuard.cpp.
//! Stealth, terrain LOS, containers and repulsor/crate branches remain unported.

use super::movement::{bounding_radius, distance_squared};
use super::{AiRules, Order, Skirmish, UnitType, startup};

mod attack_area;
pub(super) use attack_area::State as AttackAreaState;

/// `AIUpdate` mood attitude and next scheduled scan, including its first RNG offset.
pub(super) struct Mood {
    pub(super) next_check: u64,
    pub(super) offset_check: bool,
    pub(super) attitude: i32,
}

impl Mood {
    pub(super) const fn new(next_check: u64) -> Self {
        Self {
            next_check,
            offset_check: true,
            attitude: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum GuardPhase {
    Return,
    Idle,
    Inner,
    Outer,
    Aggressor,
}

pub(super) struct GuardState {
    pub(super) center: [f32; 3],
    pub(super) phase: GuardPhase,
    pub(super) area: Option<usize>,
    pub(super) object: Option<usize>,
    pub(super) flying_only: bool,
    guardee_position: [f32; 2],
    pub(super) return_position: [f32; 3],
    pub(super) deadline: u64,
    pub(super) next_scan: u64,
    pub(super) retaliating: bool,
    pub(super) home_height: f32,
}

impl GuardState {
    pub(super) const fn new(position: [f32; 2]) -> Self {
        Self {
            center: [position[0], position[1], 0.0],
            phase: GuardPhase::Return,
            area: None,
            object: None,
            flying_only: false,
            guardee_position: [0.0; 2],
            return_position: [position[0], position[1], 0.0],
            deadline: 0,
            next_scan: 0,
            retaliating: false,
            home_height: 0.0,
        }
    }
}

pub(super) fn validate_rules(catalog: &[UnitType], rules: &AiRules) -> Result<(), &'static str> {
    if [
        rules.max_retaliation_distance,
        rules.retaliation_friends_radius,
        rules.guard_inner_human,
        rules.guard_outer_human,
        rules.guard_inner_ai,
        rules.guard_outer_ai,
        rules.min_distance_for_group,
        rules.distance_requires_group,
        rules.group_move_gather_factor,
        rules.dozer_bored_radius_modifier,
    ]
    .into_iter()
    .any(|value| !value.is_finite() || value < 0.0)
        || catalog.iter().any(|unit| {
            !unit.ai.vision_range.is_finite()
                || unit.ai.vision_range < 0.0
                || [
                    rules.guard_inner_human,
                    rules.guard_outer_human,
                    rules.guard_inner_ai,
                    rules.guard_outer_ai,
                ]
                .into_iter()
                .any(|modifier| !(unit.ai.vision_range * modifier).is_finite())
        })
    {
        return Err("Invalid AI source ranges");
    }
    Ok(())
}

impl Skirmish {
    /// Follows the requested Alt-waypoint goals (`AI_FOLLOW_PATH`).
    ///
    /// # Errors
    /// Rejects immobile objects and unreachable/outside destinations.
    pub fn queue_move_to(&mut self, id: usize, position: [f32; 2]) -> Result<(), &'static str> {
        self.queue_move_to_world(id, [position[0], position[1], self.ground_height(position)])
    }

    /// Append the original XYZ goal without losing its terrain layer.
    /// # Errors
    /// Rejects unavailable actors and nonfinite/outside destinations.
    pub fn queue_move_to_world(
        &mut self,
        id: usize,
        position: [f32; 3],
    ) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if position.iter().any(|value| !value.is_finite())
            || self.cell_at([position[0], position[1]]).is_none()
        {
            return Err("Waypoint is nonfinite or outside the map");
        }
        let actor = self.ready_actor(id)?;
        if self.catalog[actor.type_index].speed <= 0.0 || actor.disabled_held() {
            return Err("This object cannot move");
        }
        self.dozer_player_command(id);
        self.native_append_path_from(id, position, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
        self.assault_command(id, Order::FollowPath);
        self.finish_dozer_player_command(id);
        Ok(())
    }

    /// Sets the producer's user rally point, after its model-defined exit path.
    ///
    /// # Errors
    /// Rejects nonproducers, unfinished/dead objects and points outside the map.
    pub fn set_rally_point(&mut self, id: usize, position: [f32; 3]) -> Result<(), &'static str> {
        let actor = self.ready_actor(id)?;
        if self.catalog[actor.type_index]
            .behavior
            .production_exit
            .is_none()
            && self.catalog[actor.type_index].parking.is_none()
            && !self.is_tunnel(id)
        {
            return Err("This object has no production exit");
        }
        if self.cell_at([position[0], position[1]]).is_none() {
            return Err("Rally point is outside the map");
        }
        self.entities[id].rally_point = Some(position);
        Ok(())
    }

    /// Moves while checking for enemies, retaining the destination during combat.
    ///
    /// # Errors
    /// Rejects immobile objects or destinations that cannot be reached.
    pub fn attack_move(&mut self, id: usize, position: [f32; 2]) -> Result<(), &'static str> {
        self.attack_move_world(id, [position[0], position[1], self.ground_height(position)])
    }

    /// Attack-move to the original picked XYZ destination.
    /// # Errors
    /// Rejects unavailable actors and invalid destinations.
    pub fn attack_move_world(&mut self, id: usize, world: [f32; 3]) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if world.iter().any(|value| !value.is_finite()) {
            return Err("Destination is not finite");
        }
        self.dozer_player_command(id);
        // privateAttackMoveToPosition permits every mobile object; mixed groups
        // retain their dozers. An unarmed member moves but cannot acquire targets.
        self.native_attack_move_from(
            id,
            world,
            i32::MAX.unsigned_abs(),
            super::ai_goals::COMMAND_FROM_PLAYER,
        );
        self.finish_dozer_player_command(id);
        Ok(())
    }

    /// Guards a location with the source inner/outer pursuit ranges and timers.
    ///
    /// # Errors
    /// Rejects unavailable/immobile objects, projectiles and nonfinite positions.
    pub fn guard(&mut self, id: usize, position: [f32; 2]) -> Result<(), &'static str> {
        self.guard_world(id, [position[0], position[1], self.ground_height(position)])
    }

    /// Guard the original XYZ center; Return retains its terrain layer.
    /// # Errors
    /// Rejects unavailable actors and invalid destinations.
    pub fn guard_world(&mut self, id: usize, world: [f32; 3]) -> Result<(), &'static str> {
        self.guard_world_mode(id, world, false)
    }

    pub(super) fn guard_world_mode(
        &mut self,
        id: usize,
        world: [f32; 3],
        flying_only: bool,
    ) -> Result<(), &'static str> {
        let actor = self.ready_actor(id)?;
        let unit = &self.catalog[actor.type_index];
        if unit.speed <= 0.0
            || actor.disabled_held()
            || unit.kind_of.iter().any(|kind| kind == "PROJECTILE")
        {
            return Err("This object cannot guard a position");
        }
        if world.iter().any(|value| !value.is_finite()) {
            return Err("Guard position is not finite");
        }
        self.dozer_player_command(id);
        self.native_guard_mode_from(id, world, super::ai_goals::COMMAND_FROM_PLAYER, flying_only);
        self.finish_dozer_player_command(id);
        Ok(())
    }

    pub(in crate::skirmish) fn native_guard_from(
        &mut self,
        id: usize,
        world: [f32; 3],
        source: i32,
    ) {
        self.native_guard_mode_from(id, world, source, false);
    }

    pub(in crate::skirmish) fn native_guard_mode_from(
        &mut self,
        id: usize,
        world: [f32; 3],
        source: i32,
        flying_only: bool,
    ) {
        if self.native_subclass_command(
            id,
            super::ai_dispatch::stored_command::Command {
                source,
                kind: if flying_only {
                    super::ai_dispatch::stored_command::Kind::GuardFlying(world)
                } else {
                    super::ai_dispatch::stored_command::Kind::Guard(world)
                },
            },
        ) || !self.native_ai_command_allowed(id)
        {
            return;
        }
        if self.catalog[self.entities[id].type_index].immobile() || self.object_disabled(id) {
            return;
        }
        self.clear_ai_goal(id);
        let center = if source == super::ai_goals::COMMAND_FROM_PLAYER {
            self.player_guard_position([world[0], world[1]])
        } else {
            [world[0], world[1]]
        };
        self.set_last_command_source(id, source);
        self.refresh_occupancy();
        self.assault_command_from(id, Order::Guard(center), None, source, false);
        self.set_order(id, Order::Guard(center));
        self.entities[id].ai.guard.center = [center[0], center[1], world[2]];
        self.entities[id].ai.guard.flying_only = flying_only;
        self.enter_guard_phase(id, GuardPhase::Return);
    }

    /// `AIUpdateInterface::privateGuardObject` retains the guarded `ObjectID`.
    pub(in crate::skirmish) fn native_guard_object_from(
        &mut self,
        id: usize,
        target: usize,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            super::ai_dispatch::stored_command::Command {
                source,
                kind: super::ai_dispatch::stored_command::Kind::GuardObject(target),
            },
        ) || !self.native_ai_command_allowed(id)
            || self.catalog[self.entities[id].type_index].immobile()
            || self.attack_kind(id, "PROJECTILE")
            || self.object_disabled(id)
        {
            return;
        }
        let center = self.guard_object_position(target);
        let point = [center[0], center[1]];
        self.clear_ai_goal(id);
        self.set_last_command_source(id, source);
        self.assault_command_from(id, Order::Guard(point), None, source, false);
        self.set_order(id, Order::Guard(point));
        let guard = &mut self.entities[id].ai.guard;
        guard.object = Some(target);
        guard.center = center;
        // AIGuardIdleState retains this sample across Return/Idle transitions.
        guard.guardee_position = [0.0; 2];
        self.refresh_occupancy();
        self.enter_guard_phase(id, GuardPhase::Return);
    }

    fn guard_object_position(&self, target: usize) -> [f32; 3] {
        // The object guard has a distinct, zero-initialized position fallback.
        self.entities
            .get(target)
            .filter(|object| object.exists())
            .map_or([0.0; 3], |_| self.object_position(target))
    }

    fn player_guard_position(&self, mut point: [f32; 2]) -> [f32; 2] {
        let width = f32::from(self.grid.width()) * super::CELL_SIZE;
        let height = f32::from(self.grid.height()) * super::CELL_SIZE;
        if (0.0..=width).contains(&point[0]) && (0.0..=height).contains(&point[1]) {
            return point;
        }
        // TerrainLogic::findClosestEdgePoint chooses top/right/bottom/left,
        // with strict comparison preserving that tie order. It changes one
        // coordinate only, even when a corner starts outside both axes.
        let distances = [
            point[1].abs(),
            (point[0] - width).abs(),
            (point[1] - height).abs(),
            point[0].abs(),
        ];
        let mut closest = 0;
        for edge in 1..4 {
            if distances[edge] < distances[closest] {
                closest = edge;
            }
        }
        match closest {
            0 => point[1] = 0.0,
            1 => point[0] = width,
            2 => point[1] = height,
            _ => point[0] = 0.0,
        }
        point
    }

    pub(super) fn guard_area(&mut self, id: usize, area: usize) -> Result<(), &'static str> {
        let actor = self.active_actor(id)?;
        if self.catalog[actor.type_index].speed <= 0.0 {
            return Err("This object cannot guard an area");
        }
        if actor.ai.mood.attitude == -2 && self.computer_player(actor.owner) {
            return Ok(());
        }
        if self
            .opponent_rules
            .as_ref()
            .and_then(|rules| rules.map.areas.get(area))
            .is_none()
        {
            return Err("Guard area is missing");
        }
        self.native_guard_area_from(id, area, 1);
        Ok(())
    }

    pub(in crate::skirmish) fn native_guard_area_from(
        &mut self,
        id: usize,
        area: usize,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::GuardArea(area),
            },
        ) || !self.native_ai_command_allowed(id)
            || self.catalog[self.entities[id].type_index].speed <= 0.0
            || self.attack_kind(id, "PROJECTILE")
        {
            return;
        }
        let Some(region) = self
            .opponent_rules
            .as_ref()
            .and_then(|rules| rules.map.areas.get(area))
        else {
            return;
        };
        let (center, _) = super::opponent::map::bounds(region);
        self.clear_ai_goal(id);
        self.set_last_command_source(id, source);
        self.assault_command(id, Order::Guard(center));
        self.set_order(id, Order::Guard(center));
        self.entities[id].ai.guard.area = Some(area);
        self.entities[id].ai.guard.center = [center[0], center[1], self.ground_height(center)];
        self.enter_guard_phase(id, GuardPhase::Return);
        self.finish_dozer_player_command(id);
    }

    /// Original radius cursor uses `AIGuardMachine::getStdGuardRange`.
    #[must_use]
    pub fn guard_radius(&self, id: usize) -> f32 {
        self.guard_range(id, true)
    }

    fn guard_range(&self, id: usize, inner: bool) -> f32 {
        let actor = &self.entities[id];
        let computer = self.computer_player(actor.owner);
        let factor = match (computer, inner) {
            (true, true) => self.ai_rules.guard_inner_ai,
            (true, false) => self.ai_rules.guard_outer_ai,
            (false, true) => self.ai_rules.guard_inner_human,
            (false, false) => self.ai_rules.guard_outer_human,
        };
        let mut range = actor.vision_range(&self.catalog[actor.type_index]) * factor;
        if computer && let Some(rules) = &self.opponent_rules {
            match actor.ai.mood.attitude {
                -2 => return 0.0,
                1 => range *= rules.alert_range_modifier,
                2 => range *= rules.aggressive_range_modifier,
                _ => {}
            }
        }
        range
    }

    pub(super) fn object_radius(&self, id: usize) -> f32 {
        bounding_radius(&self.object_footprint(id))
    }

    fn enemy(&self, id: usize, target: usize) -> bool {
        self.entities.get(target).is_some_and(|other| {
            other.alive()
                && self.partition.registered(target)
                && !other.masked()
                && self.object_relationship(id, target) == 0
                && self.weapon_target_reachable(id, target)
        })
    }

    pub(in crate::skirmish) fn range_distance(&self, id: usize, target: usize) -> f32 {
        let distance =
            distance_squared(self.entities[id].position, self.entities[target].position).sqrt();
        (distance - (self.object_radius(id) + self.object_radius(target))).max(0.0)
    }

    // PartitionFilterRejectBuildings::allow (PartitionManager.cpp:5020).
    pub(in crate::skirmish) fn building_target_allowed(&self, id: usize, target: usize) -> bool {
        if !self.attack_kind(target, "STRUCTURE") {
            return true;
        }
        let owner = self.entities[id].owner;
        let apparent = self
            .container_apparent_owner(target, owner)
            .unwrap_or(self.entities[target].owner);
        self.enemies(owner, apparent)
            && (self.computer_player(owner)
                || self.attack_kind(target, "FS_BASE_DEFENSE")
                || self.catalog[self.entities[target].type_index]
                    .contain
                    .is_some()
                    && self.object_can_attack(target))
    }

    // AIUpdateInterface::getNextMoodTarget: idle flag, check cadence/jitter,
    // human outer vision and within-weapon-range restriction, building filter.
    pub(super) fn mood_target(
        &mut self,
        id: usize,
        during_idle: bool,
        force: bool,
    ) -> Option<usize> {
        let actor = &self.entities[id];
        if actor.is_effectively_dead() || actor.object_status & (1 << 24) != 0 {
            return None;
        }
        let unit = &self.catalog[actor.type_index];
        if !(0..3).any(|slot| self.weapon_available(id, slot))
            || (during_idle && !unit.ai.auto_acquire())
            || (during_idle
                && actor.stealthed()
                && !unit.ai.acquire_stealthed()
                && unit
                    .behavior
                    .stealth
                    .as_ref()
                    .is_none_or(|rules| !matches!(rules.wake, super::stealth::Wake::SpecialPower))
                && actor.contained_by().is_none_or(|carrier| {
                    self.catalog[self.entities[carrier].type_index]
                        .contain
                        .as_ref()
                        .is_none_or(|rules| !rules.weapons.fire)
                }))
            || (unit.ai.not_while_attacking() && actor.ai.ai_target.is_some())
        {
            return None;
        }
        let attitude = actor.ai.mood.attitude;
        if !force
            && self.team_attacks_common_target(id)
            && let Some(target) = self.team_attack_target(id)
            && self.weapon_target_reachable_mode(id, target, false)
            && attitude >= 0
        {
            return Some(target);
        }
        if !force {
            if self.tick < self.entities[id].ai.mood.next_check {
                return None;
            }
            let unit = &self.catalog[self.entities[id].type_index];
            let rate = u64::from(unit.ai.mood_check_frames);
            let offset = if self.entities[id].ai.mood.offset_check {
                let half = unit.ai.mood_check_frames >> 1;
                i64::from(self.random_between(half.saturating_mul(2))) - i64::from(half)
            } else {
                0
            };
            let actor = &mut self.entities[id];
            actor.ai.mood.next_check = (self.tick + rate).saturating_add_signed(offset);
            actor.ai.mood.offset_check = false;
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let range = self.guard_range(id, false)
            + actor
                .contained_by()
                .map_or(0.0, |carrier| self.object_radius(carrier));
        if range <= 0.0 {
            return None;
        }
        let computer = self.computer_player(actor.owner);
        if computer && actor.ai.mood.attitude == -1 {
            return actor
                .damage_state
                .last_attacker
                .filter(|&target| self.enemy(id, target));
        }
        let candidates = self
            .entities
            .iter()
            .enumerate()
            .filter(|(target, _)| {
                self.enemy(id, *target)
                    && self.container_attack_in_range(id, *target)
                    && (computer || self.visible_to(actor.owner, *target))
                    && (unit.ai.auto_acquire_buildings()
                        || self.building_target_allowed(id, *target))
                    && self.range_distance(id, *target) <= range
                    && (force || computer || self.any_weapon_in_range(id, *target))
            })
            .map(|(target, _)| target);
        self.prioritized_target(id, candidates)
    }

    pub(super) fn acquire_target(&mut self, id: usize) {
        if let Some(target) = self.mood_target(id, true, false) {
            self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
            self.set_order(id, Order::Attack(target));
            self.entities[id].ai.attack_context.source =
                crate::skirmish::builder_disarm::Source::Ai;
            self.entities[id].ai.command.from_ai = true;
        }
    }

    fn guard_target(&mut self, id: usize, center: [f32; 2]) -> Option<usize> {
        let hijacker = self.catalog[self.entities[id].type_index]
            .hijacker
            .is_some();
        if !hijacker && !self.object_can_attack(id) {
            return None;
        }
        if !self.entities[id].ai.guard.retaliating
            && self.team_attacks_common_target(id)
            && let Some(target) = self.team_attack_target(id)
        {
            return Some(target);
        }
        let area = self.entities[id].ai.guard.area;
        if area.is_some()
            && self.tick
                > self.trigger_state.changed_at + u64::from(self.ai_rules.guard_scan_frames)
        {
            return None;
        }
        let range = area
            .and_then(|area| self.opponent_rules.as_ref()?.map.areas.get(area))
            .map_or_else(
                || self.guard_range(id, true),
                |area| super::opponent::map::bounds(area).1,
            );
        let mut closest = None;
        let mut best = range * range;
        let mut last_ring = f64::INFINITY;
        for (ring, target) in self.partition.outward_range_rings(center, range) {
            if ring > last_ring {
                break;
            }
            let actor = &self.entities[id];
            let victim = &self.entities[target];
            let distance = distance_squared(victim.position, center);
            if distance >= best
                || actor.is_off_map() != victim.is_off_map()
                || self.object_relationship(id, target) != 0
                || actor.ai.guard.flying_only
                    && (victim.ai.registration.is_none()
                        || !victim.is_using_airborne_locomotor(&self.catalog[victim.type_index]))
                || area
                    .and_then(|area| self.opponent_rules.as_ref()?.map.areas.get(area))
                    .is_some_and(|region| !super::opponent::map::inside(region, victim.position))
            {
                continue;
            }
            let eligible = if hijacker {
                self.can_hijack(id, target)
            } else if actor.ai.guard.retaliating {
                self.enemy(id, target)
                    && self.retaliation_building_target(id, target)
                    && self.maintenance_target_visible(actor.owner, target)
            } else {
                matches!(
                    self.native_weapon_attack_result(
                        id,
                        Some(target),
                        self.object_position(target),
                        false,
                        super::ai_goals::COMMAND_FROM_AI,
                    ),
                    super::CanAttackResult::Possible | super::CanAttackResult::PossibleAfterMoving
                )
            };
            if eligible {
                closest = Some(target);
                best = distance;
                last_ring = ring;
            }
        }
        closest
    }

    pub(super) fn enter_guard_phase(&mut self, id: usize, phase: GuardPhase) {
        // Each Guard child owns its Attack/AIInternalMoveTo state and exits it.
        self.exit_native_attack(id);
        if matches!(self.entities[id].ai.guard.phase, GuardPhase::Return) {
            self.entities[id].ai.motion.end_move();
        }
        if matches!(phase, GuardPhase::Return)
            && matches!(self.entities[id].ai.order, Order::Guard(_))
        {
            if let Some(object) = self.entities[id].ai.guard.object {
                let center = self.guard_object_position(object);
                self.entities[id].ai.guard.center = center;
                self.entities[id].ai.order = Order::Guard([center[0], center[1]]);
            }
            self.entities[id].ai.guard.return_position = self
                .adjusted_destination_world(id, self.entities[id].ai.guard.center)
                .unwrap_or(self.entities[id].ai.guard.center);
        }
        let scan = match phase {
            GuardPhase::Return => Some(self.ai_rules.guard_return_scan_frames),
            GuardPhase::Idle => Some(self.ai_rules.guard_scan_frames),
            GuardPhase::Inner | GuardPhase::Outer | GuardPhase::Aggressor => None,
        };
        let delay = scan.map_or(0, |maximum| self.random_between(maximum));
        let actor = &mut self.entities[id];
        actor.ai.guard.phase = phase;
        actor.ai.motion.movement_active = false;
        actor.ai.path.clear();
        actor.ai.ground_path = None;
        actor.ai.path_goal = None;
        actor.ai.guard.next_scan = self.tick + u64::from(delay);
        actor.ai.guard.deadline = self.tick + u64::from(self.ai_rules.guard_chase_frames);
        if matches!(phase, GuardPhase::Return) {
            self.entities[id].ai.command.move_to.position =
                self.entities[id].ai.guard.return_position;
            self.entities[id].ai.command.move_to.adjusts = true;
            if matches!(
                self.enter_primary_ground_move(id),
                super::ground_movement::MoveResult::Failure
            ) {
                self.enter_guard_phase(id, GuardPhase::Inner);
            }
        } else if matches!(phase, GuardPhase::Inner | GuardPhase::Aggressor)
            && self.catalog[self.entities[id].type_index]
                .hijacker
                .is_some()
            && let Some(target) = self.entities[id].ai.ai_target
        {
            // AIGuardInnerState owns an AIEnterState and invokes its entry before update.
            self.set_ai_goal_object(id, target);
            if !self.enter_native_enter(id, target) {
                self.enter_guard_phase(id, GuardPhase::Outer);
            }
        }
    }

    pub(super) fn update_guard(&mut self, id: usize, center: [f32; 2]) {
        let phase = self.entities[id].ai.guard.phase;
        match phase {
            GuardPhase::Return | GuardPhase::Idle => {
                // RETAIL_COMPATIBLE_CRC=0 removes aggressor hooks during Return
                // and Inner, retaining the hook only during GuardIdle.
                if matches!(phase, GuardPhase::Idle)
                    && let Some(attacker) = self.entities[id].damage_state.last_attacker.take()
                    && self.enemy(id, attacker)
                    && self.entities[id]
                        .active_weapon(&self.catalog[self.entities[id].type_index])
                        .damage
                        > 0.0
                {
                    self.entities[id].ai.ai_target = Some(attacker);
                    self.enter_guard_phase(id, GuardPhase::Aggressor);
                    return;
                }
                if self.tick >= self.entities[id].ai.guard.next_scan {
                    let interval = if matches!(phase, GuardPhase::Return) {
                        self.ai_rules.guard_return_scan_frames
                    } else {
                        self.ai_rules.guard_scan_frames
                    };
                    self.entities[id].ai.guard.next_scan = self.tick + u64::from(interval);
                    let scan_center = self.entities[id].ai.guard.object.map_or(center, |target| {
                        let position = self.guard_object_position(target);
                        [position[0], position[1]]
                    });
                    if let Some(target) = self.guard_target(id, scan_center) {
                        let guard = &mut self.entities[id].ai.guard;
                        guard.center[0] = scan_center[0];
                        guard.center[1] = scan_center[1];
                        self.entities[id].ai.order = Order::Guard(scan_center);
                        self.entities[id].ai.ai_target = Some(target);
                        self.enter_guard_phase(id, GuardPhase::Inner);
                        return;
                    }
                    if matches!(phase, GuardPhase::Idle)
                        && let Some(target) = self.entities[id].ai.guard.object
                        && self.entities.get(target).is_some_and(super::Entity::exists)
                    {
                        let guard = &mut self.entities[id].ai.guard;
                        let [x, y] = guard.guardee_position;
                        let threshold = 2.0 * super::CELL_SIZE;
                        if (scan_center[0] - x).abs() > threshold
                            || (scan_center[1] - y).abs() > threshold
                        {
                            guard.guardee_position = scan_center;
                            self.enter_guard_phase(id, GuardPhase::Return);
                            return;
                        }
                    }
                    if matches!(phase, GuardPhase::Idle) && self.entities[id].ai.guard.retaliating {
                        self.set_order(id, Order::Idle);
                        return;
                    }
                }
                if matches!(phase, GuardPhase::Return) {
                    match self.update_internal_move(id) {
                        super::ground_movement::MoveResult::Success => {
                            self.enter_guard_phase(id, GuardPhase::Idle);
                        }
                        super::ground_movement::MoveResult::Failure => {
                            self.enter_guard_phase(id, GuardPhase::Inner);
                        }
                        super::ground_movement::MoveResult::Continue => {}
                    }
                }
            }
            GuardPhase::Inner | GuardPhase::Outer | GuardPhase::Aggressor => {
                let center = if let Some(object) = self.entities[id].ai.guard.object
                    && self.entities.get(object).is_some_and(super::Entity::exists)
                {
                    let center = self.object_position(object);
                    let point = [center[0], center[1]];
                    self.entities[id].ai.guard.center = center;
                    self.entities[id].ai.order = Order::Guard(point);
                    point
                } else {
                    center
                };
                self.update_guard_attack(id, center, phase);
            }
        }
    }

    fn update_guard_attack(&mut self, id: usize, center: [f32; 2], phase: GuardPhase) {
        let area = self.entities[id].ai.guard.area;
        // AIGuardInner/RetaliateInner own an AIEnterState for EnterGuard units.
        // Its explicit object goal survives under the outer guard command.
        if self.catalog[self.entities[id].type_index]
            .hijacker
            .is_some()
            && matches!(phase, GuardPhase::Inner | GuardPhase::Aggressor)
            && let Some(target) = self.entities[id].ai.ai_target
            && self.can_hijack(id, target)
        {
            self.entities[id].ai.command.ability_goal = Some(target);
            self.entities[id].ai.ignored_producer = Some(target);
            self.entities[id].ai.order = Order::Enter(target);
            self.update_enter(id, target);
            if self.entities[id].masked() || !self.entities[id].alive() {
                return;
            }
            if matches!(self.entities[id].ai.order, Order::Enter(_)) {
                self.entities[id].ai.order = Order::Guard(center);
                self.entities[id].ai.guard.area = area;
                return;
            }
        }
        let inner = self.guard_range(id, true);
        let retaliating = self.entities[id].ai.guard.retaliating;
        let range = if retaliating {
            let vision = self.guard_range(id, false);
            match phase {
                GuardPhase::Inner => 1.5 * inner,
                GuardPhase::Outer => 0.67 * (vision + inner),
                _ => vision + inner,
            }
        } else if matches!(phase, GuardPhase::Outer) {
            let outer = self.guard_range(id, false);
            area.and_then(|area| self.opponent_rules.as_ref()?.map.areas.get(area))
                .map_or(outer, |area| {
                    outer.max(super::opponent::map::bounds(area).1)
                })
        } else {
            inner
        };
        let target = self.entities[id].ai.ai_target;
        if matches!(phase, GuardPhase::Outer)
            && target.is_some_and(|target| {
                let guard = &self.entities[id].ai.guard;
                let height = if retaliating {
                    guard.home_height
                } else {
                    guard.center[2]
                };
                let horizontal = distance_squared(center, self.entities[target].position);
                let delta_z = height - self.object_position(target)[2];
                let height_squared = delta_z * delta_z;
                let squared = horizontal + height_squared;
                self.enemy(id, target) && squared <= inner * inner
            })
        {
            self.entities[id].ai.guard.deadline =
                self.tick + u64::from(self.ai_rules.guard_chase_frames);
        }
        let active = target.is_some_and(|target| {
            self.enemy(id, target)
                && distance_squared(center, self.entities[target].position) <= range * range
                && (!retaliating
                    || distance_squared(center, self.entities[id].position) <= inner * inner)
                && (matches!(phase, GuardPhase::Inner)
                    || self.tick < self.entities[id].ai.guard.deadline)
        });
        if active && self.update_attack(id, target.expect("Active guard has a target")) {
            return;
        }
        self.entities[id].ai.order = Order::Guard(center);
        self.entities[id].ai.guard.area = area;
        self.entities[id].ai.guard.retaliating = retaliating;
        let next = match phase {
            GuardPhase::Inner => GuardPhase::Outer,
            GuardPhase::Aggressor if retaliating => GuardPhase::Return,
            GuardPhase::Aggressor => GuardPhase::Inner,
            _ => {
                self.entities[id].ai.ai_target = None;
                GuardPhase::Return
            }
        };
        self.enter_guard_phase(id, next);
    }

    pub(super) fn random_between(&mut self, maximum: u32) -> u32 {
        startup::random_between(&mut self.random, maximum)
    }
}

/// Retained `AIFollowPathState` route and destination-adjustment state.
pub(super) struct Waypoints {
    // Current command group's cardinality for fixed-profile adjustDestination.
    pub points: Vec<[f32; 3]>,
    pub index: usize,
    pub adjusts_destination: bool,
    pub exit_quick_pending: bool,
    // Each registered FollowPath state retains its constructor retry budget.
    pub retries: [u8; 4],
}

impl Waypoints {
    pub(super) const fn new() -> Self {
        Self {
            points: Vec::new(),
            index: 0,
            adjusts_destination: true,
            exit_quick_pending: false,
            retries: [10; 4],
        }
    }
}
