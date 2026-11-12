//! Builder repairs, sale reversal and GLA reconstruction from the pinned source.
mod auto_deposit;
mod capture;
mod overcharge;
mod power_plant;
pub use auto_deposit::{
    AutoDepositUpgradeLookup, Event as DepositEvent, Rules as DepositRules,
    UpgradeBoost as DepositBoost,
};
pub use overcharge::{Event as OverchargeEvent, Rules as OverchargeRules};
mod healing_pad;
mod idle_repair;
pub use idle_repair::IdleRepair;
mod rebuilding;
mod research;
pub use capture::{CaptureEffect, CaptureEvent};
mod regeneration;
mod selling;
pub use regeneration::Regeneration;
mod sub_objects;
mod upgrade_mux;
mod upgrades;
pub use research::{Research, ResearchEvent};
pub use sub_objects::{SubObjectOverride, SubObjectUpgrade};
pub use upgrades::{
    ArmorUpgrade, ConditionUpgrade, ExperienceUpgrade, HealthUpgrade, MaxHealthChange,
    MovementUpgrade, PowerUpgrade, SupplyBoost, WeaponUpgrade,
};
#[cfg(test)]
mod tests;

use super::{ConstructionPhase, Entity, Kind, Order, Skirmish};

pub struct MaintenanceRules {
    pub subdual: Option<super::microwave::SubdualRules>,
    pub auto_heal: Vec<super::AutoHeal>,
    pub healing_kind_flags: u8,
    pub rubble_height: Option<f32>,
    pub regeneration: Option<Regeneration>,
    pub research: Vec<Research>,
    pub experience_upgrades: Vec<ExperienceUpgrade>,
    pub health_upgrades: Vec<HealthUpgrade>,
    pub power_upgrade: Option<PowerUpgrade>,
    pub armor_upgrade: Option<ArmorUpgrade>,
    pub weapon_upgrade: Option<WeaponUpgrade>,
    pub movement_upgrade: Option<MovementUpgrade>,
    pub collector_boost: Option<SupplyBoost>,
    pub condition_upgrades: Vec<ConditionUpgrade>,
    pub replacement: Option<super::fake_buildings::Replacement>,
    pub command_sets: Vec<super::fake_buildings::CommandSet>,
    pub deposits: Vec<DepositRules>,
    pub power_plants: Vec<u32>,
    pub overcharges: Vec<OverchargeRules>,
    pub kind_flags: u8,
    pub repair_per_second: f32,
    pub construction_sound: String,
    pub idle_repair: Option<IdleRepair>,
    pub refund: u32,
    pub sell_fraction: f32,
    pub expose_hole: Vec<HoleExposure>,
    pub rebuild: Vec<RebuildRules>,
}

pub struct HoleExposure {
    pub conditions: super::death_callbacks::Conditions,
    pub template: String,
    pub health: f32,
    pub transfer_attackers: bool,
}

pub struct RebuildRules {
    pub worker: String,
    pub respawn_frames: f32,
    pub regeneration: f32,
}

impl MaintenanceRules {
    pub const CAPTURABLE: u8 = 1;
    pub const IMMUNE_TO_CAPTURE: u8 = 2;
    pub const UNSELLABLE: u8 = 4;
    pub const TECH_BUILDING: u8 = 8;

    pub(super) const fn has_kind(&self, flag: u8) -> bool {
        self.kind_flags & flag != 0
    }

    #[must_use]
    pub const fn none() -> Self {
        Self {
            subdual: None,
            auto_heal: Vec::new(),
            healing_kind_flags: 0,
            rubble_height: None,
            regeneration: None,
            research: Vec::new(),
            experience_upgrades: Vec::new(),
            health_upgrades: Vec::new(),
            power_upgrade: None,
            armor_upgrade: None,
            weapon_upgrade: None,
            movement_upgrade: None,
            collector_boost: None,
            condition_upgrades: Vec::new(),
            replacement: None,
            command_sets: Vec::new(),
            deposits: Vec::new(),
            power_plants: Vec::new(),
            overcharges: Vec::new(),
            kind_flags: 0,
            repair_per_second: 0.0,
            construction_sound: String::new(),
            idle_repair: None,
            refund: 0,
            sell_fraction: 0.0,
            expose_hole: Vec::new(),
            rebuild: Vec::new(),
        }
    }
}

pub(in crate::skirmish) const INSTANCE_UNSELECTABLE: u8 = 1;
const INSTANCE_UNSELLABLE: u8 = 2;
const INSTANCE_SCRIPT_TARGETABLE: u8 = 4;
const INSTANCE_INDESTRUCTIBLE: u8 = 8;

pub(super) struct MaintenanceState {
    pub(super) microwave: super::microwave::State,
    pub(super) healing_benefactor_until: u64,
    pub(super) healing_benefactor: Option<usize>,
    pub(super) auto_heal: Vec<super::healing::State>,
    upgrades: Vec<String>,
    effects: upgrades::UpgradeState,
    power_plants: Vec<power_plant::State>,
    power_conditions: u8,
    overcharges: Vec<overcharge::State>,
    deposits: Vec<auto_deposit::State>,
    animation_duration: Option<(u64, u32, bool)>,
    selling: Option<u64>,
    pub(super) masked: bool,
    pub(in crate::skirmish) deleted: bool,
    pub(in crate::skirmish) instance_flags: u8,
    holes: Vec<rebuilding::Hole>,
    pub(in crate::skirmish) builder: Option<usize>,
    idle_since: u64,
    repair_from_ai: bool,
    pub(super) repair_dock_last: Option<usize>,
    pub(super) repair_dock_health_per_frame: f32,
}

impl MaintenanceState {
    pub(super) const fn new(frame: u64) -> Self {
        Self {
            microwave: super::microwave::State::new(),
            healing_benefactor_until: 0,
            healing_benefactor: None,
            auto_heal: Vec::new(),
            upgrades: Vec::new(),
            effects: upgrades::UpgradeState::new(),
            power_plants: Vec::new(),
            power_conditions: 0,
            overcharges: Vec::new(),
            deposits: Vec::new(),
            animation_duration: None,
            selling: None,
            masked: false,
            deleted: false,
            instance_flags: 0,
            holes: Vec::new(),
            builder: None,
            idle_since: frame,
            repair_from_ai: false,
            repair_dock_last: None,
            repair_dock_health_per_frame: 0.0,
        }
    }

    pub(super) const fn ordered(&mut self, frame: u64, previous: Order, next: Order) {
        self.repair_from_ai = false;
        if matches!(previous, Order::Build(_) | Order::Repair(_)) && matches!(next, Order::Idle) {
            self.idle_since = frame;
        }
    }
}

pub(super) fn validate(
    unit: &super::UnitType,
    catalog: &[super::UnitType],
) -> Result<(), &'static str> {
    super::neutral_tech::validate(unit)?;
    upgrades::validate(unit)?;
    regeneration::validate(unit)?;
    idle_repair::validate(unit)?;
    let rules = &unit.maintenance;
    if !rules.repair_per_second.is_finite()
        || rules.repair_per_second < 0.0
        || !rules.sell_fraction.is_finite()
        || !(0.0..=1.0).contains(&rules.sell_fraction)
    {
        return Err("Invalid source repair or selling values");
    }
    if rules.research.iter().any(|upgrade| {
        (upgrade.frames == 0
            && !rules
                .command_sets
                .iter()
                .any(|command| command.upgrade == upgrade.name))
            || upgrade.frames > u32::from(u16::MAX)
            || upgrade.name.is_empty()
    }) {
        return Err("Invalid source upgrade duration or name");
    }
    if rules.expose_hole.iter().any(|expose| {
        !expose.health.is_finite() || !catalog.iter().any(|unit| unit.name == expose.template)
    }) {
        return Err("Invalid or missing rebuild-hole template");
    }
    if rules.rebuild.iter().any(|rebuild| {
        !rebuild.regeneration.is_finite()
            || !rebuild.respawn_frames.is_finite()
            || !(0.0..4_294_967_296.0).contains(&rebuild.respawn_frames)
            || (rebuild.respawn_frames >= 1.0
                && !catalog.iter().any(|unit| {
                    unit.name == rebuild.worker
                        && matches!(
                            unit.ai.update,
                            super::UnitAiUpdate::Absent
                                | super::UnitAiUpdate::Dozer
                                | super::UnitAiUpdate::Worker
                        )
                }))
    }) {
        return Err("Invalid rebuild duration, regeneration or worker construct interface");
    }
    Ok(())
}

impl Entity {
    #[must_use]
    pub const fn animation_frame(&self) -> Option<(u64, i32)> {
        self.drawable.animation_frame
    }

    #[must_use]
    pub fn animation_duration(&self) -> Option<(u64, u32, bool)> {
        self.drawable
            .animation_completion
            .or_else(|| self.battle_plan_animation())
            .or_else(|| self.pre_attack_animation())
            .or(self.hacking.animation)
            .or(self.maintenance.animation_duration)
    }

    #[must_use]
    pub fn under_construction(&self) -> bool {
        self.build_progress < 1.0 && !self.selling()
    }

    #[must_use]
    pub const fn masked(&self) -> bool {
        self.maintenance.masked
    }

    #[must_use]
    pub const fn unselectable(&self) -> bool {
        self.maintenance.instance_flags & INSTANCE_UNSELECTABLE != 0
    }

    #[must_use]
    pub fn selectable(&self, unit: &super::UnitType) -> bool {
        self.exists()
            && (unit.kind_of.iter().any(|kind| kind == "ALWAYS_SELECTABLE")
                || (self.selectable
                    && !self.unselectable()
                    && !self.is_effectively_dead()
                    && !unit.kind_of.iter().any(|kind| kind == "NO_SELECT")))
    }

    #[must_use]
    pub const fn script_targetable(&self) -> bool {
        self.maintenance.instance_flags & INSTANCE_SCRIPT_TARGETABLE != 0
    }

    #[must_use]
    pub const fn indestructible(&self) -> bool {
        self.maintenance.instance_flags & INSTANCE_INDESTRUCTIBLE != 0
    }

    #[must_use]
    pub const fn selling(&self) -> bool {
        self.maintenance.selling.is_some()
    }

    #[must_use]
    pub fn sold_model(&self) -> bool {
        self.selling() && self.construction_percent <= 0.0
    }
}

impl Skirmish {
    // Object::setSelectable notifies its Drawable; this UI-only operation must
    // not rebuild the player's logic Squad or change retained AI groups.
    pub(in crate::skirmish) fn set_object_selectable(&mut self, id: usize, selectable: bool) {
        let actor = &mut self.entities[id];
        actor.selectable = selectable;
        if !selectable && actor.has_drawable() && actor.drawable.selected != 0 {
            actor.drawable.selected = 0;
            self.deselected_objects.push(id);
        }
    }

    pub(in crate::skirmish) fn set_map_body_controls(
        &mut self,
        id: usize,
        targetable: bool,
        indestructible: bool,
    ) {
        let flags = &mut self.entities[id].maintenance.instance_flags;
        *flags &= !(INSTANCE_SCRIPT_TARGETABLE | INSTANCE_INDESTRUCTIBLE);
        if targetable {
            *flags |= INSTANCE_SCRIPT_TARGETABLE;
        }
        if indestructible {
            *flags |= INSTANCE_INDESTRUCTIBLE;
        }
    }

    pub(in crate::skirmish) fn set_map_unsellable(&mut self, id: usize, unsellable: bool) {
        let flags = &mut self.entities[id].maintenance.instance_flags;
        if unsellable {
            *flags |= INSTANCE_UNSELLABLE;
        } else {
            *flags &= !INSTANCE_UNSELLABLE;
        }
    }
}

impl Skirmish {
    /// Source `ActionManager::canRepairObject`, for represented ground structures.
    #[must_use]
    pub fn can_repair(&self, worker: usize, building: usize) -> bool {
        (self
            .entities
            .get(worker)
            .is_some_and(|actor| actor.chinook.is_some())
            && self.chinook_repair_admits(
                worker,
                building,
                crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
            ))
            || self.aircraft_can_repair_at(worker, building)
            || self.repair_pad_admits(worker, building)
            || self.repair_target(worker, building, false)
    }

    pub(in crate::skirmish) fn repair_target(
        &self,
        worker: usize,
        building: usize,
        from_ai: bool,
    ) -> bool {
        let (Some(actor), Some(target)) = (self.entities.get(worker), self.entities.get(building))
        else {
            return false;
        };
        let unit = &self.catalog[actor.type_index];
        let structure = &self.catalog[target.type_index];
        actor.complete()
            && (from_ai || self.maintenance_target_visible(actor.owner, building))
            && actor.contained_by().is_none()
            && target.alive()
            && !target.under_construction()
            && !target.masked()
            && self.object_relationship(worker, building) != 0
            && unit.kind == Kind::Builder
            && unit.maintenance.repair_per_second > 0.0
            && structure.kind_of.iter().any(|flag| flag == "STRUCTURE")
            && !structure
                .kind_of
                .iter()
                .any(|flag| matches!(flag.as_str(), "BRIDGE" | "BRIDGE_TOWER" | "REBUILD_HOLE"))
            && structure.maintenance.rebuild.is_empty()
            && target.health < target.max_health
    }

    pub(in crate::skirmish) fn maintenance_target_visible(&self, owner: usize, id: usize) -> bool {
        // ActionManager::isObjectShroudedForAction applies only to human orders.
        owner != 0
            || matches!(
                self.visibility_status(owner, id),
                super::visibility::Status::Clear | super::visibility::Status::Partial
            )
    }

    /// # Errors
    /// Rejects an invalid repair target or another active healing benefactor.
    pub fn repair(&mut self, worker: usize, building: usize) -> Result<(), &'static str> {
        if self
            .entities
            .get(worker)
            .is_some_and(|actor| actor.chinook.is_some())
        {
            self.ready_actor(worker)?;
            let source = crate::skirmish::ai_goals::COMMAND_FROM_PLAYER;
            if !self.chinook_repair_admits(worker, building, source) {
                return Err("Chinook cannot repair at this airfield");
            }
            self.native_chinook_repair_from(worker, building, source);
            return Ok(());
        }
        if self.aircraft_can_repair_at(worker, building) {
            let source = crate::skirmish::ai_goals::COMMAND_FROM_PLAYER;
            if !self.native_subclass_command(
                worker,
                crate::skirmish::ai_dispatch::stored_command::Command {
                    source,
                    kind: crate::skirmish::ai_dispatch::stored_command::Kind::Repair(building),
                },
            ) {
                self.native_jet_landing_from(worker, building, source, true);
            }
            return Ok(());
        }
        if self.repair_pad_admits(worker, building) {
            return self.repair_at_pad(worker, building);
        }
        self.begin_repair(worker, building, false)
    }

    pub(in crate::skirmish) fn begin_repair(
        &mut self,
        worker: usize,
        building: usize,
        from_ai: bool,
    ) -> Result<(), &'static str> {
        self.ready_actor(worker)?;
        if self.entities[worker].ai.dozer.is_some() {
            return self.dozer_repair_command(worker, building, from_ai);
        }
        if matches!(self.entities[worker].ai.order, Order::Repair(id) if id == building) {
            return Ok(());
        }
        if !self.repair_target(worker, building, from_ai)
            || !self.repair_benefactor(worker, building)
        {
            return Err("Builder cannot repair this object");
        }
        self.set_order(worker, Order::Repair(building));
        self.entities[worker].maintenance.repair_from_ai = from_ai;
        self.entities[worker].ai.construction_dock = self.construction_approach(worker, building);
        Ok(())
    }

    pub(in crate::skirmish) fn repair_benefactor(&self, worker: usize, building: usize) -> bool {
        let target = &self.entities[building];
        target.maintenance.healing_benefactor.is_none()
            || self.tick > target.maintenance.healing_benefactor_until
            || target.maintenance.healing_benefactor == Some(worker)
    }

    pub(super) fn update_repair(&mut self, worker: usize, building: usize) {
        let from_ai = self.entities[worker].maintenance.repair_from_ai;
        if !self.repair_target(worker, building, from_ai)
            || !self.repair_benefactor(worker, building)
        {
            self.set_order(worker, Order::Idle);
            return;
        }
        let Some(goal) = self.entities[worker].ai.construction_dock else {
            self.set_order(worker, Order::Idle);
            return;
        };
        self.set_ai_ground_goal(worker, goal);
        if matches!(
            self.entities[worker].ai.construction_phase,
            ConstructionPhase::Approach
        ) {
            // DozerActionMoveToActionPosState admits the action from its
            // FROM_BOUNDINGSPHERE_2D distance; the nested mover then finishes.
            let actor = &self.entities[worker];
            let radius = self.catalog[actor.type_index].geometry_bounds.sphere_radius;
            let distance = (super::movement::distance_squared(actor.position, goal).sqrt()
                - self.object_radius(worker))
            .max(0.0);
            if distance > (radius + 15.0).max(70.0) {
                self.travel_to_exact(worker, goal);
                if matches!(self.entities[worker].ai.order, Order::Idle) {
                    self.begin_repair(worker, building, from_ai).ok();
                }
                return;
            }
            self.entities[worker].ai.construction_phase = ConstructionPhase::Build;
        }
        // Unlike DOZER_TASK_BUILD, the repair action heals while its nested
        // mover finishes approaching the dock.
        self.travel_to_exact(worker, goal);
        if matches!(self.entities[worker].ai.order, Order::Idle) {
            self.entities[worker].ai.order = Order::Repair(building);
            self.entities[worker].maintenance.repair_from_ai = from_ai;
        }
        self.entities[worker].drawable.model_conditions.constructing = true;
        self.set_mine_clearing(worker, false);
        let fraction = self.catalog[self.entities[worker].type_index]
            .maintenance
            .repair_per_second;
        let target = &mut self.entities[building];
        let health = target.max_health * fraction / 30.0;
        target.maintenance.healing_benefactor = Some(worker);
        target.maintenance.healing_benefactor_until = self.tick + 2;
        self.apply_healing(building, Some(worker), health);
    }

    pub(super) fn destroy_maintenance_object(&mut self, id: usize) {
        let object = self.entities[id].update_owner;
        if self.update_objects[object].destroy_requested || self.entities[id].maintenance.deleted {
            return;
        }
        self.update_objects[object].destroy_requested = true;
        self.exit_rappel(id);
        self.exit_chinook_phase(id);
        self.exit_native_attack(id);
        self.entities[id].ai.command_revision = self.entities[id]
            .ai
            .command_revision
            .checked_add(1)
            .expect("AI command revision overflow");
        self.set_attack_current_victim(id, None);
        for which in 0..2 {
            if self.has_native_turret(id, which) {
                self.set_native_turret_target(id, which, None, false, false);
            }
        }
        let actor = &mut self.entities[id];
        actor.ai.motion.clear_goal();
        actor.ai.path.clear();
        actor.ai.ground_path = None;
        actor.ai.path_request.cancel();
        self.queue_update_destruction(id);
        // Object::onDestroy removes from its parent before module onDelete.
        if let Some(carrier) = self.entities[id].contained_by() {
            self.remove_passenger(id, carrier);
        }
        // Object::onDestroy invokes onDelete after appending to the destruction list.
        for index in 0..self.catalog[self.entities[id].type_index]
            .behavior_modules
            .len()
        {
            let modules = &self.catalog[self.entities[id].type_index].behavior_modules;
            let module = &modules[index];
            let ordinal = modules[..index]
                .iter()
                .filter(|prior| prior.family == module.family)
                .count();
            match module.family.as_str() {
                "TunnelContain" => self.unregister_tunnel(id),
                "OpenContain"
                | "CaveContain"
                | "ParachuteContain"
                | "HealContain"
                | "TransportContain"
                | "GarrisonContain"
                | "InternetHackContain" => self.destroy_container_contents(id),
                "HelixContain" => self.delete_helix_containment(id),
                "OverlordContain" => self.delete_overlord_containment(id),
                "DozerAIUpdate" => self.delete_dozer(id),
                "JetAIUpdate" => self.delete_jet_module(id),
                "PowerPlantUpgrade" => self.on_delete_power_upgrade(id, index),
                "CostModifierUpgrade" => self.delete_cost_modifier(id, index),
                "SpectreGunshipUpdate" => self.delete_spectre(id),
                "SpawnBehavior" => self.spawn_on_delete(id, ordinal),
                "StickyBombUpdate" => self.delete_sticky_bomb(id, ordinal),
                "OverchargeBehavior" => self.on_delete_overcharge(id, ordinal),
                _ => {}
            }
        }
        self.radar_on_delete(id);
        self.delete_battle_plan(id);
        self.delete_spy_vision(id);
        if self.attack_kind(id, "WALK_ON_TOP_OF_WALL") {
            self.movement_terrain.remove_wall_piece(id);
        }
    }

    pub(super) fn finalize_object_destruction(&mut self, id: usize) {
        self.remove_wall_footprint(id);
        let actor = &self.entities[id];
        let changes_navigation = (actor.alive()
            && self.catalog[actor.type_index].kind.is_building())
            || self.retained_rubble(id)
            || self.attack_kind(id, "WALK_ON_TOP_OF_WALL");
        self.discard_destroyed_selection(id);
        self.entities[id].maintenance.masked = true;
        self.entities[id].maintenance.deleted = true;
        self.entities[id].ai.command.ai_group = None;
        self.update_position_claim(id);
        self.observe_ground_partition(id);
        self.observe_visibility(id);
        if changes_navigation {
            self.rebuild_navigation();
        }
        // Object::~Object destroys its Drawable before team and behavior teardown.
        if self.entities[id].drawable.present {
            self.entities[id].drawable.present = false;
            self.presentation
                .weapon_fx
                .push(super::WeaponFxEvent::DrawableDestroyed(id));
        }
        self.team_membership.set_team(id, None);
        self.destroy_rebuild_workers(id);
        self.destroy_object_update_instances(id);
        if let Some(hazard) = self.entities[id].native_hazard.take()
            && let Some(index) = self.hazard_index(hazard)
        {
            self.retire_hazard(index);
        }
        if let Some(projectile) = self.entities[id].native_projectile.take()
            && let Some(index) = self.projectile_index(projectile)
        {
            self.projectiles.remove(index);
            self.rebuild_projectile_indices();
        }
        self.entities[id].projectile_flight = None;
    }
}

/// Reject matching reset targets whose executed latch has not been translated.
pub(in crate::skirmish) fn validate_upgrade_resets(unit: &super::UnitType) -> Result<(), String> {
    upgrade_mux::validate(unit)
}
