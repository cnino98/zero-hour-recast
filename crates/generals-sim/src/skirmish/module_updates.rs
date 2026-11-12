//! Ordered object update instances, SH 2bfaea60 `Object::init` / `GameLogic`.
use super::Skirmish;
mod declared;
mod scheduler;
#[cfg(test)]
mod tests;
pub(super) use scheduler::Scheduler;

/// Validate actual template instances before creating authoritative objects.
/// # Errors
/// Reports missing callback/data bindings with authored instance provenance.
pub fn validate_registrations(unit: &super::UnitType) -> Result<(), String> {
    super::combat::body_callbacks::validate(unit)?;
    super::crushing::validate(unit)?;
    super::death_callbacks::validate(unit)?;
    super::maintenance::validate_upgrade_resets(unit)?;
    declared::validate(unit)
}

pub(super) const FOREVER: u32 = 0x3fff_ffff;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Helper {
    StateMachine,
    StatusDamage,
    SubdualDamage,
    Repulsor,
    Defection,
    WeaponStatus,
    FiringTracker,
    TemporaryWeaponBonus,
}

pub struct ModuleSource {
    pub path: String,
    pub origin: String,
    pub line: u32,
}

/// Shared native `UpgradeMuxData`; cloned by actual template inheritance.
#[derive(Clone)]
pub struct UpgradeData {
    pub triggers: Vec<String>,
    pub conflicts: Vec<String>,
    pub requires_all: bool,
    pub removals: Vec<String>,
    pub fx: String,
}

impl UpgradeData {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            triggers: Vec::new(),
            conflicts: Vec::new(),
            requires_all: false,
            removals: Vec::new(),
            fx: String::new(),
        }
    }
}

pub struct BehaviorRegistration {
    pub family: String,
    pub tag: String,
    pub interfaces: u32,
    pub upgrade: UpgradeData,
    pub source: ModuleSource,
}

pub enum ModuleRegistration {
    Helper(Helper),
    Behavior {
        family: String,
        tag: String,
        source: ModuleSource,
    },
}

// Common native Object identities outlive their motion/render components.
pub(super) struct Object {
    pub(super) entity: usize,
    pub(super) handles: Vec<usize>,
    pub(super) constructed_behaviors: usize,
    pub(super) upgrade_executed: Vec<bool>,
    pub(super) create_completed: Vec<bool>,
    pub(super) destroy_requested: bool,
}

impl Object {
    pub(super) const fn new(entity: usize) -> Self {
        Self {
            entity,
            handles: Vec::new(),
            constructed_behaviors: 0,
            upgrade_executed: Vec::new(),
            create_completed: Vec::new(),
            destroy_requested: false,
        }
    }
}

pub(super) struct Descriptor {
    pub(super) phase: u32,
    pub(super) disabled_types: u32,
    pub(super) constructor_wakes: &'static [u32],
}

pub(super) const fn helper_descriptor(helper: Helper) -> Descriptor {
    let (phase, disabled_types, constructor_wakes): (_, _, &'static [u32]) = match helper {
        Helper::StateMachine | Helper::Repulsor => (2, 0, &[FOREVER]),
        Helper::Defection => (2, 0x1fff, &[FOREVER]),
        Helper::StatusDamage | Helper::SubdualDamage | Helper::TemporaryWeaponBonus => {
            (2, 0x1fff, &[FOREVER, FOREVER])
        }
        Helper::WeaponStatus => (3, 0, &[FOREVER, 1]),
        Helper::FiringTracker => (3, 0x1fff, &[FOREVER]),
    };
    Descriptor {
        phase,
        disabled_types,
        constructor_wakes,
    }
}

impl Skirmish {
    // GetGameLogicRandomValue(1, UnsignedInt delay): native converts the upper
    // bound to Int; CRC=0 returns it without a random draw when lo >= hi.
    pub(super) fn random_update_phase(&mut self, delay: u32) -> u32 {
        if i32::from_ne_bytes(delay.to_ne_bytes()) <= 1 {
            delay
        } else {
            1 + self.random_between(delay - 1)
        }
    }

    pub(super) fn allocate_object_update(
        &mut self,
        owner: usize,
        phase: u32,
        accepted_disabled: u32,
    ) -> usize {
        let object = self.entities[owner].update_owner;
        let handle = self
            .update_scheduler
            .create(object, phase, accepted_disabled);
        self.update_objects[object].handles.push(handle);
        handle
    }

    pub(super) fn finish_object_update_registration(&mut self, owner: usize) {
        let now = self.update_frame();
        let object = self.entities[owner].update_owner;
        self.update_scheduler
            .register_object(object, &self.update_objects[object].handles, now);
    }

    pub(in crate::skirmish) fn registered_physics(&self, owner: usize) -> bool {
        let actor = &self.entities[owner];
        self.catalog[actor.type_index].update_modules.iter().enumerate().any(|(index, entry)| {
            index < self.update_objects[actor.update_owner].handles.len()
                && matches!(entry, ModuleRegistration::Behavior { family, .. } if family == "PhysicsBehavior")
        })
    }

    pub(super) fn wake_helper(&mut self, owner: usize, helper: Helper, delay: u32) {
        let actor = &self.entities[owner];
        let index = self.catalog[actor.type_index]
            .update_modules
            .iter()
            .position(
                |entry| matches!(entry, ModuleRegistration::Helper(found) if *found == helper),
            );
        if let Some(index) =
            index.filter(|&index| index < self.update_objects[actor.update_owner].handles.len())
        {
            self.wake_update(owner, index, delay);
        }
    }

    pub(super) fn wake_family_update(
        &mut self,
        owner: usize,
        family: &str,
        ordinal: usize,
        delay: u32,
    ) {
        let actor = &self.entities[owner];
        let index = self.catalog[actor.type_index]
            .update_modules
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                matches!(entry,
                ModuleRegistration::Behavior { family: found, .. } if found == family)
            })
            .nth(ordinal)
            .map(|(index, _)| index);
        if let Some(index) =
            index.filter(|&index| index < self.update_objects[actor.update_owner].handles.len())
        {
            self.wake_update(owner, index, delay);
        }
    }

    pub(super) fn run_object_updates(&mut self) {
        let now = self.update_frame();
        while let Some(handle) = self.update_scheduler.next_due(now) {
            let object = self.update_scheduler.owner(handle);
            let owner = self.update_objects[object].entity;
            let disabled = self.object_disabled_mask(owner);
            if !self.update_scheduler.begin_due(handle, now, disabled) {
                continue;
            }
            let registration = self.update_objects[object]
                .handles
                .iter()
                .position(|&found| found == handle)
                .expect("module belongs to its native object");
            let actor = &self.entities[owner];
            let sleep = match &self.catalog[actor.type_index].update_modules[registration] {
                ModuleRegistration::Helper(helper) => self.update_builtin_helper(owner, *helper),
                ModuleRegistration::Behavior { .. } => {
                    self.update_declared_module(owner, registration)
                }
            };
            self.update_scheduler.finish(handle, now, sleep);
        }
    }

    pub(super) fn wake_update(&mut self, owner: usize, registration: usize, delay: u32) {
        let handle = self.update_objects[self.entities[owner].update_owner].handles[registration];
        self.update_scheduler
            .set_wake(handle, self.update_frame(), delay);
    }

    pub(super) fn update_remaining_sleep(&self, owner: usize, registration: usize) -> u32 {
        self.update_scheduler.remaining_sleep(
            self.update_objects[self.entities[owner].update_owner].handles[registration],
            self.update_frame(),
        )
    }

    pub(super) fn process_update_destruction(&mut self) {
        while let Some(object) = self.update_scheduler.next_destruction() {
            self.finalize_object_destruction(self.update_objects[object].entity);
            self.update_objects[object].handles.clear();
        }
    }

    pub(super) fn destroy_object_update_instances(&mut self, owner: usize) {
        let count = self.catalog[self.entities[owner].type_index]
            .update_modules
            .len();
        for index in 0..count {
            if let ModuleRegistration::Helper(helper) =
                &self.catalog[self.entities[owner].type_index].update_modules[index]
            {
                self.destroy_builtin_helper(owner, *helper);
            }
        }
        let mut flammable = 0;
        let mut production = 0;
        let mut ability = 0;
        let mut bone_fx = 0;
        for index in 0..self.catalog[self.entities[owner].type_index]
            .behavior_modules
            .len()
        {
            match self.catalog[self.entities[owner].type_index].behavior_modules[index]
                .family
                .as_str()
            {
                "BoneFXUpdate" => {
                    if bone_fx < self.entities[owner].behaviors.bone_fx.len() {
                        self.destroy_bone_fx(owner, bone_fx);
                    }
                    bone_fx += 1;
                }
                "ActiveBody" | "ImmortalBody" | "HighlanderBody" | "StructureBody"
                | "HiveStructureBody" | "UndeadBody" => self
                    .presentation
                    .weapon_fx
                    .push(super::WeaponFxEvent::BodyParticlesClear(owner)),
                "JetAIUpdate" => {
                    self.destroy_jet_module(owner);
                    self.cancel_docking(owner);
                }
                // AIUpdateInterface's base destructor owns its nested dock machine.
                "AIUpdateInterface"
                | "DozerAIUpdate"
                | "SupplyTruckAIUpdate"
                | "ChinookAIUpdate"
                | "WorkerAIUpdate"
                | "WanderAIUpdate"
                | "HackInternetAIUpdate"
                | "DeployStyleAIUpdate"
                | "TransportAIUpdate"
                | "AssaultTransportAIUpdate"
                | "DeliverPayloadAIUpdate"
                | "MissileAIUpdate" => self.cancel_docking(owner),
                "ProductionUpdate" => {
                    self.destroy_production_instance(owner, production);
                    production += 1;
                }
                "FlammableUpdate" => {
                    self.destroy_flammable_module(owner, flammable);
                    flammable += 1;
                }
                "SpecialAbilityUpdate" => {
                    self.exit_native_ability(owner, ability, true);
                    ability += 1;
                }
                _ => {}
            }
        }
        self.update_objects[self.entities[owner].update_owner]
            .handles
            .clear();
    }

    pub(super) fn queue_update_destruction(&mut self, owner: usize) {
        self.update_scheduler
            .queue_destroy(self.entities[owner].update_owner);
    }

    pub(super) fn update_frame(&self) -> u32 {
        // Native world frame is unsigned 32-bit; retain its low word.
        u32::try_from(self.tick & u64::from(u32::MAX)).expect("masked world frame")
    }
}
