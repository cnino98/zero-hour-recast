mod money;
pub use money::Event as MoneyDepositEvent;
pub(super) use money::State as MoneyState;
mod dock;
mod docking;
mod warehouse;
pub(super) use dock::State as DockState;
#[cfg(test)]
mod limits;
mod production_conditions;
pub(super) mod production_instances;
#[cfg(test)]
mod production_scheduler_tests;
#[cfg(test)]
mod resource_selection;
pub(super) use docking::{DockTarget, Docking};

use super::{ConstructionState, Faction, Kind, Order, Skirmish};
use num_traits::ToPrimitive;

pub(super) use super::ai_dispatch::supply::State as SupplyAi;

// Player::m_energy owns production, consumption and sabotage timing.
pub(super) struct Energy {
    pub(super) sabotaged_until: u64,
    pub(super) brownout: bool,
    pub(super) unsabotaged: [f32; 2],
    pub(super) native: [i32; 2],
    pub(super) bonus: i32,
    pub(super) production: f32,
    pub(super) consumption: f32,
}

impl Energy {
    pub(super) const fn new() -> Self {
        Self {
            sabotaged_until: 0,
            brownout: false,
            unsabotaged: [0.0; 2],
            native: [0; 2],
            bonus: 0,
            production: 0.0,
            consumption: 0.0,
        }
    }
}

impl super::Player {
    #[must_use]
    pub const fn energy(&self) -> (f32, f32) {
        (self.energy.production, self.energy.consumption)
    }
}

impl Skirmish {
    /// Installs the effective `ControlBar` command-set products before logic starts.
    ///
    /// # Errors
    /// Rejects replacing command definitions after the first logic frame.
    pub fn configure_command_sets(
        &mut self,
        sets: std::collections::BTreeMap<String, Vec<String>>,
    ) -> Result<(), &'static str> {
        if self.tick != 0 {
            return Err("Command sets must be configured before the match starts");
        }
        self.command_set_products = sets;
        Ok(())
    }

    /// Checks ordinary builder production before a placement message is emitted.
    ///
    /// # Errors
    /// Rejects an unavailable builder, an unavailable construction command,
    /// a full production queue or insufficient money.
    pub fn construction_admission(
        &self,
        builder: usize,
        product: usize,
    ) -> Result<(), &'static str> {
        let actor = self
            .entities
            .get(builder)
            .filter(|actor| actor.exists())
            .ok_or("Builder does not exist")?;
        let unit = self.catalog.get(product).ok_or("Unknown building type")?;
        if !unit.kind.is_building() {
            return Err("Unknown building type");
        }
        self.check_production_prerequisite(builder, product)?;
        if self.catalog[actor.type_index].production.is_some()
            && !self.production_queue_available(builder)
        {
            return Err("Production queue is full");
        }
        if self.players[actor.owner].cash < self.build_cost(actor.owner, product) {
            return Err("Not enough money");
        }
        Ok(())
    }

    pub(super) fn production_queue_available(&self, producer: usize) -> bool {
        let actor = &self.entities[producer];
        let limit = self.catalog[actor.type_index].production_queue_limit;
        usize::try_from(limit).is_ok_and(|limit| actor.queue.len() < limit)
    }

    #[must_use]
    pub fn prerequisites_met(&self, owner: usize, type_index: usize) -> bool {
        let (Some(player), Some(unit)) = (self.players.get(owner), self.catalog.get(type_index))
        else {
            return false;
        };
        // Player::allowedToBuild separates STRUCTURE from every other KindOf.
        if !self.map_build_allowed(owner, !unit.kind_of.iter().any(|kind| kind == "STRUCTURE")) {
            return false;
        }
        match unit.build_rules.status {
            super::Buildable::No => return false,
            super::Buildable::IgnorePrerequisites => return true,
            super::Buildable::OnlyByAi if !self.computer_player(owner) => return false,
            super::Buildable::Yes | super::Buildable::OnlyByAi => {}
        }
        // Player::canBuild and ProductionPrerequisite::isSatisfied: AND rows,
        // with OR alternatives within each Object row; unfinished units do not count.
        unit.build_rules
            .sciences
            .iter()
            .all(|name| player.has_science(name))
            && unit.build_rules.objects.iter().all(|alternatives| {
                alternatives.iter().any(|name| {
                    self.catalog
                        .iter()
                        .find(|unit| &unit.name == name)
                        .is_some_and(|required| {
                            self.entities.iter().any(|entity| {
                                entity.owner == owner
                                    && entity.complete()
                                    && self.catalog[entity.type_index].equivalent_to(required)
                            })
                        })
                })
            })
            && self.can_build_more(owner, type_index)
    }

    // Player::canBuildMoreOfType includes unfinished objects, and also queued
    // units for non-structures. Dead objects and their queues do not count.
    fn can_build_more(&self, owner: usize, type_index: usize) -> bool {
        let unit = &self.catalog[type_index];
        let limit = match unit.build_rules.limit {
            super::TypeLimit::Superweapon => self.superweapon_limit,
            super::TypeLimit::Count(limit) => u32::from(limit),
        };
        if limit == 0 {
            return true;
        }
        let mut count = 0_u32;
        for actor in &self.entities {
            if actor.owner != owner || !actor.alive() {
                continue;
            }
            let existing = &self.catalog[actor.type_index];
            count += u32::from(
                unit.equivalent_to(existing)
                    || unit.build_rules.linked_limit.as_ref().is_some_and(|link| {
                        existing.build_rules.linked_limit.as_ref() == Some(link)
                    }),
            );
            if !unit.kind_of.iter().any(|flag| flag == "STRUCTURE") {
                for entry in &actor.queue {
                    if matches!(entry.item, super::ProductionItem::Unit(index) if index == type_index)
                    {
                        count += 1;
                    }
                }
            }
        }
        count < limit
    }

    pub(super) fn check_prerequisite(
        &self,
        owner: usize,
        type_index: usize,
    ) -> Result<(), &'static str> {
        if self.prerequisites_met(owner, type_index) {
            Ok(())
        } else {
            Err("Required building, General's promotion or type limit is unavailable")
        }
    }

    pub(super) fn check_production_prerequisite(
        &self,
        producer: usize,
        product: usize,
    ) -> Result<(), &'static str> {
        let actor = &self.entities[producer];
        if !self.can_build_more(actor.owner, product) {
            return Err("Unit type limit reached");
        }
        if self.production_possible(producer, product) {
            Ok(())
        } else {
            Err("Required production command, prerequisite or type limit is unavailable")
        }
    }

    pub(super) fn production_possible(&self, producer: usize, product: usize) -> bool {
        let actor = &self.entities[producer];
        // BuildAssistant keeps the last equivalent command and checks its
        // template's prerequisites, which may differ from a requested reskin.
        let products = if let Some(name) = actor.command_set_override() {
            let Some(products) = self.command_set_products.get(name) else {
                return false;
            };
            products
        } else {
            &self.catalog[actor.type_index].buildable_objects
        };
        let command = products.iter().rev().find_map(|name| {
            self.catalog
                .iter()
                .position(|unit| &unit.name == name && unit.equivalent_to(&self.catalog[product]))
        });
        command.is_some_and(|command| self.prerequisites_met(actor.owner, command))
    }

    pub(super) fn update_power(&mut self) {
        self.update_power_excluding(None, None);
    }

    // During team transfer, native removes old influence before adding new influence.
    pub(in crate::skirmish) fn update_power_excluding(
        &mut self,
        excluded: Option<usize>,
        influence: Option<usize>,
    ) {
        self.update_power_influence(excluded, influence, None);
    }

    pub(in crate::skirmish) fn sabotage_power(&mut self, owner: usize) {
        self.update_power_influence(None, None, Some(owner));
    }

    fn update_power_influence(
        &mut self,
        excluded: Option<usize>,
        influence: Option<usize>,
        sabotage: Option<usize>,
    ) {
        // Energy keeps production/consumption even during sabotage. Its queries
        // recover at the deadline, but Player::update clears the persistent
        // brownout only on the following frame. A real energy adjustment invokes
        // onPowerBrownOutChange immediately, including at the deadline.
        let previous_energy: Vec<_> = self
            .players
            .iter()
            .map(|player| player.energy.native)
            .collect();
        for player in &mut self.players {
            player.energy.native = [0; 2];
            player.power = 0;
            player.energy.production = 0.0;
            player.energy.consumption = 0.0;
        }
        for (id, entity) in self.entities.iter().enumerate() {
            if Some(id) == excluded || !entity.alive() || entity.under_construction() {
                continue;
            }
            let unit = &self.catalog[entity.type_index];
            let energy = if self.object_disabled(id) && unit.energy > 0 {
                0
            } else {
                unit.energy
            };
            let player = &mut self.players[entity.owner];
            if energy > 0 {
                player.energy.native[0] = player.energy.native[0]
                    .checked_add(energy)
                    .expect("native Energy production overflow");
            } else {
                player.energy.native[1] = player.energy.native[1]
                    .checked_sub(energy)
                    .expect("native Energy consumption overflow");
            }
        }
        for player in &mut self.players {
            player.energy.native[0] = player.energy.native[0]
                .checked_add(player.energy.bonus)
                .expect("native Energy bonus overflow");
            player.energy.production = player.energy.native[0]
                .to_f32()
                .expect("native Energy Real conversion");
            player.energy.consumption = player.energy.native[1]
                .to_f32()
                .expect("native Energy Real conversion");
            player.power = if self.tick < player.energy.sabotaged_until {
                player.energy.native[1]
                    .checked_neg()
                    .expect("native Energy consumption overflow")
            } else {
                player.energy.native[0]
                    .checked_sub(player.energy.native[1])
                    .expect("native Energy balance overflow")
            };
        }
        let mut energy_callbacks = Vec::with_capacity(self.players.len());
        for (owner, player) in self.players.iter_mut().enumerate() {
            let energy = [player.energy.production, player.energy.consumption];
            let adjusted = player.energy.native != previous_energy[owner];
            player.energy.unsabotaged = energy;
            energy_callbacks.push(adjusted);
            if sabotage == Some(owner) {
                // The source sabotage callback passes literal TRUE, including zero duration.
                player.energy.brownout = true;
            } else if player.energy.sabotaged_until != 0
                && self.tick > player.energy.sabotaged_until
            {
                player.energy.sabotaged_until = 0;
                player.energy.brownout = false;
            } else if self.tick >= player.energy.sabotaged_until && adjusted {
                player.energy.brownout = false;
            }
            if self.tick < player.energy.sabotaged_until {
                player.energy.production = 0.0;
            }
        }
        for (owner, adjusted) in energy_callbacks.into_iter().enumerate() {
            let player = &self.players[owner];
            let brownout =
                player.energy.brownout || player.energy.production < player.energy.consumption;
            if sabotage != Some(owner)
                && influence != Some(owner)
                && !adjusted
                && self.players[owner].radar.disabled == brownout
            {
                continue;
            }
            self.power_brownout_change(owner, brownout);
        }
    }

    pub(in crate::skirmish) fn add_native_power_bonus(
        &mut self,
        id: usize,
        owner: usize,
        amount: i32,
    ) {
        debug_assert!(
            !self.object_disabled(id),
            "native addPowerBonus requires enabled Object"
        );
        self.adjust_native_power_bonus(owner, amount);
    }

    pub(in crate::skirmish) fn remove_native_power_bonus(
        &mut self,
        id: usize,
        owner: usize,
        amount: i32,
    ) {
        // Energy::removePowerBonus fixed branch, RETAIL_COMPATIBLE_CRC=0.
        if self.object_disabled(id) {
            return;
        }
        self.adjust_native_power_bonus(
            owner,
            amount
                .checked_neg()
                .expect("native power bonus negation overflow"),
        );
    }

    fn adjust_native_power_bonus(&mut self, owner: usize, amount: i32) {
        let player = &mut self.players[owner];
        player.energy.bonus = player
            .energy
            .bonus
            .checked_add(amount)
            .expect("native power bonus overflow");
        player.energy.native[0] = player.energy.native[0]
            .checked_add(amount)
            .expect("native Energy production overflow");
        player.energy.unsabotaged[0] = player.energy.native[0]
            .to_f32()
            .expect("native Energy Real conversion");
        player.energy.production = if self.tick < player.energy.sabotaged_until {
            0.0
        } else {
            player.energy.unsabotaged[0]
        };
        player.power = if self.tick < player.energy.sabotaged_until {
            player.energy.native[1]
                .checked_neg()
                .expect("native consumption overflow")
        } else {
            player.energy.native[0]
                .checked_sub(player.energy.native[1])
                .expect("native Energy balance overflow")
        };
        debug_assert!(
            player.energy.native[0] >= 0 && player.energy.native[1] >= 0,
            "native Energy negative production/consumption"
        );
        let brownout = self.tick < player.energy.sabotaged_until
            || player.energy.native[0] < player.energy.native[1];
        player.energy.brownout = self.tick < player.energy.sabotaged_until;
        self.power_brownout_change(owner, brownout);
    }

    /// `Object::onDisabledEdge` changes bonus production before the common energy callback.
    pub(in crate::skirmish) fn on_disabled_economy_power(
        &mut self,
        id: usize,
        becoming_disabled: bool,
        was_disabled: bool,
    ) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if unit.energy <= 0 || actor.under_construction() {
            return;
        }
        let base = unit.energy;
        let mut amount = base;
        if self.native_upgrade_executed(id, "PowerPlantUpgrade", 0) {
            amount = amount
                .checked_add(
                    unit.maintenance
                        .power_upgrade
                        .as_ref()
                        .expect("PowerPlantUpgrade data")
                        .bonus,
                )
                .expect("native power overflow");
        }
        if actor.first_overcharge_active() {
            amount = amount
                .checked_add(unit.maintenance.overcharges[0].energy_bonus)
                .expect("native power overflow");
        }
        let baseline = if actor.alive() { base } else { 0 };
        let current_base = if self.object_disabled(id) {
            0
        } else {
            baseline
        };
        let previous_base = if was_disabled { 0 } else { baseline };
        let baseline_change = current_base
            .checked_sub(previous_base)
            .expect("native influence delta");
        let native_change = if becoming_disabled {
            amount.checked_neg().expect("native negative production")
        } else {
            amount
        };
        // The baseline traversal accounts for actual mask changes. Retain the
        // remaining source delta, including the native zero-duration edge bug.
        let adjustment = native_change
            .checked_sub(baseline_change)
            .expect("native energy delta");
        let owner = actor.owner;
        self.players[owner].energy.bonus = self.players[owner]
            .energy
            .bonus
            .checked_add(adjustment)
            .expect("native power overflow");
        self.update_power_excluding(None, Some(owner));
    }

    fn power_brownout_change(&mut self, owner: usize, brownout: bool) {
        self.set_player_radar_disabled(owner, brownout);
        for id in self.team_membership.player_objects(owner) {
            if self.entities[id].exists()
                && self.catalog[self.entities[id].type_index]
                    .kind_of
                    .iter()
                    .any(|flag| flag == "POWERED")
            {
                if brownout {
                    self.entities[id].disabled_status |= 1 << 6;
                } else {
                    self.entities[id].disabled_status &= !(1 << 6);
                }
                self.observe_spy_vision_disabled(id);
            }
        }
    }

    fn production_frames(&self, owner: usize, type_index: usize) -> f32 {
        let player = &self.players[owner];
        let ratio = if self.tick < player.energy.sabotaged_until {
            0.0
        } else if player.energy.consumption > 0.0 {
            (player.energy.production / player.energy.consumption).min(1.0)
        } else {
            player.energy.production.min(1.0)
        };
        // Core/.../ThingTemplate.cpp:1579 calcTimeToBuild, excluding handicap,
        // upgrade and multiple-factory modifiers not represented in this catalog.
        let shortage = 1.0 - ratio;
        let penalty = shortage * self.rules.low_energy_penalty_modifier;
        let mut speed = (1.0 - penalty).max(self.rules.min_low_energy_production_speed);
        if ratio < 1.0 {
            speed = speed.min(self.rules.max_low_energy_production_speed);
        }
        ((self.catalog[type_index].build_seconds * 30.0).trunc() / speed.max(0.01))
            .trunc()
            .max(1.0)
    }

    pub(super) fn update_construction(&mut self, worker: usize, building: usize) {
        let Some(target) = self.entities.get(building) else {
            self.set_order(worker, Order::Idle);
            return;
        };
        if !target.alive()
            || target.build_progress >= 1.0
            || target.owner != self.entities[worker].owner
        {
            self.set_order(worker, Order::Idle);
            return;
        }
        let goal = if let Some(point) = self.entities[worker].ai.construction_dock {
            point
        } else {
            let Some(point) = self.construction_approach(worker, building) else {
                return;
            };
            // DozerAIUpdate::newTask retains the same native start/action dock
            // point while approaching; it is not recomputed every logic tick.
            self.entities[worker].ai.construction_dock = Some(point);
            point
        };
        if matches!(
            self.entities[worker].ai.construction_phase,
            super::ConstructionPhase::Approach
        ) {
            self.set_ai_ground_goal(worker, goal);
            let arrived = self.travel_to_exact(worker, goal);
            if !arrived && !matches!(self.entities[worker].ai.order, Order::Idle) {
                return;
            }
            // DozerActionDoActionState::update observes the nested mover's
            // isIdle(), including failure. The build task itself is retained;
            // unlike an ordinary failed move, it proceeds to DO_BUILD_AT_DOCK.
            self.entities[worker].ai.order = Order::Build(building);
            self.entities[worker].ai.construction_dock = Some(goal);
            self.entities[worker].ai.construction_phase = super::ConstructionPhase::Build;
        }
        self.set_mine_clearing(worker, false);
        if self.construct_building_piece(worker, building) {
            self.set_order(worker, Order::Idle);
        }
    }

    pub(in crate::skirmish) fn construct_building_piece(
        &mut self,
        worker: usize,
        building: usize,
    ) -> bool {
        let type_index = self.entities[building].type_index;
        // DozerActionDoActionState at DOZER_DO_BUILD_AT_DOCK, paired with the
        // approach state's AWAITING -> PARTIALLY/ACTIVELY transition. Cancellation
        // deliberately retains the building flag in the pinned original source.
        self.entities[worker].drawable.model_conditions.constructing = true;
        self.entities[building]
            .drawable
            .model_conditions
            .construction = ConstructionState::Active;
        let frames = self.production_frames(self.entities[worker].owner, type_index);
        let health = self.entities[building].max_health;
        // DozerActionDoActionState::update increments progress and HP at the dock.
        let target = &mut self.entities[building];
        target.construction_percent = (target.construction_percent + 100.0 / frames).min(100.0);
        target.build_progress = target.construction_percent / 100.0;
        let completed = target.build_progress >= 1.0;
        if !matches!(
            self.catalog[type_index].body_kind,
            crate::skirmish::BodyKind::Inactive
        ) {
            self.change_body_health(building, health / frames);
        }
        self.entities[building].producer = Some(worker);
        self.entities[building].maintenance.builder = Some(worker);
        if completed {
            self.entities[building].object_status &= !((1 << 3) | (1 << 21));
            self.finish_dozer_building_sound(worker);
            self.entities[building]
                .drawable
                .model_conditions
                .construction = ConstructionState::Complete;
            self.entities[building].collector_pending =
                self.catalog[type_index].kind == Kind::SupplyCenter;
            self.unregister_object_obstacle(building);
            self.restore_object_obstacle(building);
            if !self.dozer_is_rebuilding(worker) {
                self.record_built(building);
                self.players[self.entities[building].owner]
                    .statistics
                    .money_spent += self.build_cost(self.entities[building].owner, type_index);
            }
            // Player::onStructureConstructionComplete adjusts power before notifying AI.
            self.update_power();
            self.opponent_structure_produced(building);
            self.map_structure_produced(building);
            self.native_build_complete(building);
            self.observe_ground_partition(building);
            self.observe_visibility(building);
            self.activate_upgrade_modules(building);
        }
        completed
    }

    #[cfg(test)]
    pub(super) fn update_production(&mut self, producer: usize) {
        self.update_production_instance(producer, 0);
    }

    pub(in crate::skirmish) fn update_production_instance(
        &mut self,
        producer: usize,
        ordinal: usize,
    ) {
        self.update_production_doors(producer, ordinal);
        self.entities[producer].update_production_condition(ordinal);
        let actor = &self.entities[producer];
        let Some(production) = actor.production_queue_at(ordinal).front() else {
            return;
        };
        let item = production.item;
        let owner = actor.owner;
        if actor.selling() {
            return;
        }
        if let super::ProductionItem::Unit(type_index) = item {
            let unit = &self.catalog[type_index];
            let structure = unit.kind_of.iter().any(|flag| flag == "STRUCTURE");
            if !self.map_build_allowed(owner, !structure)
                && !unit.kind_of.iter().any(|flag| flag == "DOZER")
                && self.cancel_unit_entry_at(producer, ordinal, 0).is_ok()
            {
                return;
            }
        }
        let frames = match item {
            super::ProductionItem::Unit(type_index) => self.production_frames(owner, type_index),
            super::ProductionItem::Upgrade(index) => super::progression::integer_real(
                self.catalog[self.entities[producer].type_index]
                    .maintenance
                    .research[index]
                    .frames,
            ),
        };
        let production = self.entities[producer]
            .production_queue_at_mut(ordinal)
            .front_mut()
            .expect("Queue was inspected above");
        // ProductionUpdate::update counts logic frames, then compares with calcTimeToBuild.
        production.elapsed_frames += 1.0;
        production.total_seconds = frames / 30.0;
        production.remaining_seconds = (frames - production.elapsed_frames).max(0.0) / 30.0;
        if production.elapsed_frames < frames {
            return;
        }
        match item {
            super::ProductionItem::Unit(type_index) => {
                let production = &self.entities[producer].production_queue_at(ordinal)[0];
                let remaining = production.quantity - production.produced;
                for _ in 0..remaining {
                    if !self.production_exit_ready(producer, ordinal)
                        || self
                            .spawn_produced_unit_at(producer, type_index, false, ordinal)
                            .is_none()
                    {
                        continue;
                    }
                    self.entities[producer].production_queue_at_mut(ordinal)[0].produced += 1;
                    self.entities[producer].production_queue_at_mut(ordinal)[0].parking_door = None;
                }
                let production = &self.entities[producer].production_queue_at(ordinal)[0];
                if production.produced < production.quantity {
                    return;
                }
            }
            super::ProductionItem::Upgrade(index) => {
                // Native destruction is deferred beyond this callback. Remove
                // completed research before a replacement's immediate Rust deletion,
                // so only remaining queue entries receive cancellation refunds.
                self.entities[producer]
                    .production_queue_at_mut(ordinal)
                    .pop_front();
                self.entities[producer].production_removed_at(ordinal);
                self.finish_research(producer, index);
                return;
            }
        }
        self.entities[producer]
            .production_queue_at_mut(ordinal)
            .pop_front();
        self.entities[producer].production_removed_at(ordinal);
    }

    pub(super) fn update_free_collector(&mut self, producer: usize) {
        let actor = &self.entities[producer];
        if !actor.collector_pending
            || !actor.complete()
            || actor.fire_physics_asleep(self.tick)
            || actor.paralyzed()
            || actor.hacked()
            || actor.unmanned()
            || actor.subdued()
            || actor.contained_by().is_some()
        {
            return;
        }
        let faction = self.catalog[actor.type_index].faction;
        let kind = if faction == Faction::Gla {
            Kind::Builder
        } else {
            Kind::Harvester
        };
        let Some(type_index) = self
            .catalog
            .iter()
            .position(|unit| unit.faction == faction && unit.kind == kind)
        else {
            return;
        };
        if self
            .spawn_produced_unit(producer, type_index, true)
            .is_none()
        {
            return;
        }
        // Selected SupplyCenter SpawnBehavior data: SpawnNumber=1, OneShot=Yes,
        // SlavesHaveFreeWill=Yes. The client loader verifies each expected template.
        // SpawnBehavior.cpp spawnOneSlave (608/728) reserves and uses the parent's
        // production exit. Free grants take the same native exit as paid units.
        self.entities[producer].collector_pending = false;
    }

    pub(super) fn closest_supply_center(&mut self, id: usize) -> Option<usize> {
        let actor = &self.entities[id];
        let owner = actor.owner;
        if !actor.complete() || actor.cargo == 0 {
            return None;
        }
        if let Some(DockTarget::Center(center)) = actor.ai.supply.preferred_dock
            && self.supply_center_cost(id, center) < f32::MAX
        {
            return Some(center);
        }
        let mut best = None;
        let mut best_cost = f32::MAX;
        let mut cursor = 0;
        // ResourceGatheringManager preserves registration order, including ties,
        // and erases destroyed IDs only when this player's list is searched.
        while cursor < self.players[owner].supply_centers.len() {
            let center = self.players[owner].supply_centers[cursor];
            if !self.entities[center].exists() {
                self.players[owner].supply_centers.remove(cursor);
                continue;
            }
            let cost = self.supply_center_cost(id, center);
            if cost < best_cost {
                best = Some(center);
                best_cost = cost;
            }
            cursor += 1;
        }
        best
    }

    fn supply_center_cost(&self, id: usize, center: usize) -> f32 {
        let Some(_) = self.entities.get(center) else {
            return f32::MAX;
        };
        if !self.can_transfer_supplies(id, center)
            || self.dock_slot(id, DockTarget::Center(center)).is_none()
        {
            return f32::MAX;
        }
        supply_distance_squared(self.object_position(id), self.object_position(center))
    }

    pub(super) fn best_supply(&mut self, id: usize) -> Option<usize> {
        let actor = &self.entities[id];
        let owner = actor.owner;
        if let Some(DockTarget::Warehouse(index)) = actor.ai.supply.preferred_dock
            && self
                .supplies
                .get(index)
                .is_some_and(|source| self.can_transfer_supplies(id, source.object))
            && self.dock_slot(id, DockTarget::Warehouse(index)).is_some()
        {
            return Some(index);
        }
        let range = self.supply_scan_distance(id);
        let position = self.object_position(id);
        let mut best = None;
        let mut best_cost = f32::MAX;
        let mut cursor = 0;
        while cursor < self.players[owner].supply_warehouses.len() {
            let object = self.players[owner].supply_warehouses[cursor];
            if !self.entities[object].exists() {
                self.players[owner].supply_warehouses.remove(cursor);
                continue;
            }
            if let Some(index) = self.supply_index(object)
                && self.can_transfer_supplies(id, object)
                && self.native_dock_clear_to_approach(object, id)
            {
                let cost = supply_distance_squared(position, self.object_position(object));
                if cost < best_cost && cost < range * range {
                    best = Some(index);
                    best_cost = cost;
                }
            }
            cursor += 1;
        }
        best
    }

    pub(in crate::skirmish) fn can_transfer_supplies(&self, id: usize, object: usize) -> bool {
        let actor = &self.entities[id];
        let target = &self.entities[object];
        if !target.exists()
            || target.is_effectively_dead()
            || target.under_construction()
            || actor.under_construction()
            || target.selling()
            || !actor.ai.supply.constructed()
            || !self.supply_available(id)
        {
            return false;
        }
        if let Some(index) = self.supply_index(object) {
            if self.supplies[index].boxes == 0 || self.object_relationship(object, id) == 0 {
                return false;
            }
        } else if self.catalog[target.type_index].supply.dock.is_some()
            && self.catalog[target.type_index]
                .neutral_tech
                .repair_dock
                .is_none()
        {
            if actor.cargo == 0 || actor.owner != target.owner {
                return false;
            }
        } else {
            return false;
        }
        self.computer_player(actor.owner)
            || !matches!(
                self.visibility_status(actor.owner, object),
                super::visibility::Status::Shrouded
            )
    }

    pub(super) fn resume_supply(&mut self, id: usize) {
        self.entities[id].ai.supply.force_wanting = true;
    }

    /// Original player docking command directed at a friendly supply center.
    ///
    /// # Errors
    /// Rejects unavailable collectors and unfinished, hostile or missing centers.
    pub fn deliver_supplies(&mut self, id: usize, center: usize) -> Result<(), &'static str> {
        let actor = self.ready_actor(id)?;
        if self.catalog[actor.type_index].supply.capacity == 0 {
            return Err("This unit cannot collect supplies");
        }
        if actor.cargo == 0 {
            return Err("Collector is not carrying supplies");
        }
        self.entities
            .get(center)
            .filter(|target| {
                target.owner == actor.owner
                    && target.complete()
                    && self.catalog[target.type_index].supply.dock.is_some()
                    && self.catalog[target.type_index]
                        .neutral_tech
                        .repair_dock
                        .is_none()
            })
            .ok_or("Supply center is unavailable")?;
        if !self.can_transfer_supplies(id, center) {
            return Err("Supply center is unavailable");
        }
        self.ai_dock(
            id,
            DockTarget::Center(center),
            super::ai_goals::COMMAND_FROM_PLAYER,
        );
        Ok(())
    }

    pub(super) fn update_harvest(&mut self, id: usize, source: usize) {
        self.update_docking(id, DockTarget::Warehouse(source));
    }

    pub(super) fn update_dropoff(&mut self, id: usize) {
        let target = self.entities[id]
            .ai
            .docking
            .as_ref()
            .expect("Dock eager entry")
            .target;
        self.update_docking(id, target);
    }
}

// PartitionManager FROM_CENTER_3D uses object origins, without geometry offsets.
fn supply_distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    // Separate squared terms retain source rounding; fused multiply-add changes ties.
    let xx = dx * dx;
    let yy = dy * dy;
    let zz = dz * dz;
    xx + yy + zz
}
