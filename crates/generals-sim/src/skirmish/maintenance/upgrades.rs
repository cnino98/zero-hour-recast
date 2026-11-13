//! Concrete `UpgradeMux` modules used by represented research, without new gameplay rules.
use super::super::{DamageType, Entity, Locomotor, Skirmish, TerrainDecal, UnitType};

pub struct SupplyBoost {
    pub trigger: String,
    pub amount: u32,
}

pub struct PowerUpgrade {
    pub trigger: String,
    pub bonus: i32,
}

pub struct ExperienceUpgrade {
    pub trigger: String,
    pub scalar: f32,
}

pub enum MaxHealthChange {
    PreserveRatio,
    AddCurrentHealthToo,
    SameCurrentHealth,
    FullyHeal,
}

pub struct HealthUpgrade {
    pub trigger: String,
    pub amount: f32,
    pub change_type: MaxHealthChange,
}

pub struct ArmorUpgrade {
    pub trigger: String,
    pub coefficients: [f32; crate::skirmish::DAMAGE_TYPE_COUNT],
}

pub struct WeaponUpgrade {
    pub trigger: String,
}

pub struct MovementUpgrade {
    pub trigger: String,
    pub speed: f32,
    pub locomotor: Locomotor,
}

pub struct ConditionUpgrade {
    pub trigger: String,
    pub condition: String,
}

const ARMOR: u8 = 1;
const WEAPON: u8 = 2;
const MOVEMENT: u8 = 4;

pub(super) struct UpgradeState {
    active: u8,
    chemical_suit: bool,
    conditions: Vec<String>,
    experience_scalar: f32,
    command_set: Option<String>,
}

impl UpgradeState {
    pub(super) const fn new() -> Self {
        Self {
            active: 0,
            chemical_suit: false,
            conditions: Vec::new(),
            experience_scalar: 1.0,
            command_set: None,
        }
    }
}

impl Entity {
    #[must_use]
    pub fn command_set_override(&self) -> Option<&str> {
        self.maintenance.effects.command_set.as_deref()
    }

    pub(in crate::skirmish) fn set_bike_command_set(&mut self, commands: String) {
        self.maintenance.effects.command_set = Some(commands);
    }

    pub(in crate::skirmish) const fn experience_scalar(&self) -> f32 {
        self.maintenance.effects.experience_scalar
    }

    #[must_use]
    pub fn upgrade_conditions(&self, _tick: u64) -> Vec<&str> {
        let state = &self.maintenance.effects;
        let mut flags: Vec<&str> = state.conditions.iter().map(String::as_str).collect();
        for condition in self.power_plant_conditions() {
            flags.push(condition);
        }
        flags
    }

    /// `ArmorUpgrade`'s chemical-suit terrain decal persists with the applied module.
    #[must_use]
    pub const fn chemical_suit(&self) -> bool {
        self.maintenance.effects.chemical_suit
    }

    pub(in crate::skirmish) const fn armor_upgrade_active(&self) -> bool {
        self.maintenance.effects.active & ARMOR != 0
    }

    pub(in crate::skirmish) fn active_armor<'a>(
        &self,
        unit: &'a UnitType,
    ) -> &'a [f32; crate::skirmish::DAMAGE_TYPE_COUNT] {
        if let Some(armor) = self.battle_bus_armor(unit) {
            return armor;
        }
        unit.maintenance
            .armor_upgrade
            .as_ref()
            .filter(|_| self.maintenance.effects.active & ARMOR != 0)
            .map_or(&unit.armor, |upgrade| &upgrade.coefficients)
    }

    pub(in crate::skirmish) fn armor_damage(
        &self,
        unit: &UnitType,
        damage: DamageType,
        amount: f32,
    ) -> f32 {
        if matches!(
            damage,
            DamageType::Unresistable | DamageType::SubdualUnresistable
        ) {
            return amount;
        }
        let amount = amount * self.active_armor(unit)[damage as usize];
        if amount < 0.0 { 0.0 } else { amount }
    }

    pub(in crate::skirmish) fn adjust_damage(
        &self,
        unit: &UnitType,
        damage: DamageType,
        amount: f32,
    ) -> f32 {
        // ActiveBody::attemptDamage applies ArmorTemplate first, then its
        // damage scalar. Combining coefficients changes native float rounding.
        let armored = self.armor_damage(unit, damage, amount);
        if matches!(
            damage,
            DamageType::Unresistable | DamageType::SubdualUnresistable
        ) {
            armored
        } else {
            armored * self.battle_bonuses.armor
        }
    }

    pub(in crate::skirmish) const fn weapon_upgrade_active(&self) -> bool {
        self.maintenance.effects.active & WEAPON != 0
    }

    /// Native `CommandXlat` tests the active locomotor surface, not current altitude.
    #[must_use]
    pub fn is_using_airborne_locomotor(&self, unit: &UnitType) -> bool {
        self.active_locomotor(unit).legal_surfaces & 8 != 0
    }

    pub(in crate::skirmish) fn upgraded_locomotor<'a>(
        &self,
        unit: &'a UnitType,
    ) -> Option<&'a Locomotor> {
        unit.maintenance
            .movement_upgrade
            .as_ref()
            .filter(|_| self.maintenance.effects.active & MOVEMENT != 0)
            .map(|upgrade| &upgrade.locomotor)
    }

    pub(in crate::skirmish) fn active_locomotor<'a>(&self, unit: &'a UnitType) -> &'a Locomotor {
        unit.locomotor_in_set(self.ai.locomotor_set, self.ai.surface_locomotor)
            .expect("selected locomotor template")
    }

    pub(in crate::skirmish) fn upgraded_speed(&self, unit: &UnitType) -> f32 {
        if self.ai.locomotor_set == super::super::locomotor::set::SLUGGISH
            && let Some(bike) = &unit.combat_bike
        {
            return bike.sluggish[self.ai.surface_locomotor].speed;
        }
        if self.ai.locomotor_set == super::super::locomotor::set::WANDER
            && !unit.locomotor_sets.wander.is_empty()
        {
            return unit.locomotor_sets.wander[self.ai.surface_locomotor].speed;
        }
        if self.ai.locomotor_set == super::super::locomotor::set::PANIC
            && !unit.locomotor_sets.panic.is_empty()
        {
            return unit.locomotor_sets.panic[self.ai.surface_locomotor].speed;
        }
        if let Some(speed) = self.mob_speed(unit) {
            return speed;
        }
        if let Some(speed) = self.aircraft_speed(unit) {
            return speed;
        }
        if self.ai.locomotor_set == super::super::locomotor::set::PANIC
            && let Some(drone) = unit.behavior.slaves.first()
        {
            return drone.panic_speed;
        }
        if self.ai.locomotor_set == super::super::locomotor::set::TAXI {
            return unit.chinook.as_ref().expect("Chinook rules").taxi_speed;
        }
        if self.ai.locomotor_set == super::super::locomotor::set::NORMAL
            && self.ai.surface_locomotor > 0
        {
            return unit.locomotor_sets.normal[self.ai.surface_locomotor - 1].speed;
        }
        unit.maintenance
            .movement_upgrade
            .as_ref()
            .filter(|_| self.maintenance.effects.active & MOVEMENT != 0)
            .map_or(unit.speed, |upgrade| upgrade.speed)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn on_delete_power_upgrade(&mut self, id: usize, index: usize) {
        let update_owner = self.entities[id].update_owner;
        if !self.update_objects[update_owner].upgrade_executed[index] {
            return;
        }
        let bonus = self.catalog[self.entities[id].type_index]
            .maintenance
            .power_upgrade
            .as_ref()
            .expect("PowerPlantUpgrade data")
            .bonus;
        self.remove_native_power_bonus(id, self.entities[id].owner, bonus);
        self.update_objects[update_owner].upgrade_executed[index] = false;
    }

    pub(in crate::skirmish) fn on_capture_power_upgrade(
        &mut self,
        id: usize,
        index: usize,
        old: usize,
        new: usize,
    ) {
        let update_owner = self.entities[id].update_owner;
        if !self.update_objects[update_owner].upgrade_executed[index] || self.object_disabled(id) {
            return;
        }
        let bonus = self.catalog[self.entities[id].type_index]
            .maintenance
            .power_upgrade
            .as_ref()
            .expect("PowerPlantUpgrade data")
            .bonus;
        self.remove_native_power_bonus(id, old, bonus);
        self.update_objects[update_owner].upgrade_executed[index] = false;
        self.add_native_power_bonus(id, new, bonus);
        self.update_objects[update_owner].upgrade_executed[index] = true;
    }

    pub(in crate::skirmish) fn upgraded_collector_boost(&self, id: usize) -> u32 {
        let actor = &self.entities[id];
        // WorkerAIUpdate asks the current player's completed upgrade on each delivery.
        let rules = &self.catalog[actor.type_index].maintenance;
        if let Some(boost) = &rules.collector_boost {
            return if self.has_upgrade(actor.owner, &boost.trigger) {
                boost.amount
            } else {
                0
            };
        }
        0
    }
}

pub(super) fn validate(unit: &UnitType) -> Result<(), &'static str> {
    let rules = &unit.maintenance;
    if rules.collector_boost.as_ref().is_some_and(|boost| {
        boost.trigger.is_empty() || boost.amount > u32::from(u16::MAX) || unit.supply.capacity == 0
    }) {
        return Err("Invalid collector supply boost");
    }
    if rules.power_upgrade.as_ref().is_some_and(|upgrade| {
        upgrade.trigger.is_empty()
            || upgrade.bonus <= 0
            || i16::try_from(unit.energy + upgrade.bonus).is_err()
    }) || rules.armor_upgrade.as_ref().is_some_and(|upgrade| {
        upgrade.trigger.is_empty()
            || upgrade
                .coefficients
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
    }) || rules
        .weapon_upgrade
        .as_ref()
        .is_some_and(|upgrade| upgrade.trigger.is_empty())
        || rules
            .condition_upgrades
            .iter()
            .any(|upgrade| upgrade.trigger.is_empty() || upgrade.condition.is_empty())
    {
        return Err("Invalid represented upgrade module");
    }
    if rules.experience_upgrades.iter().any(|upgrade| {
        upgrade.trigger.is_empty() || !upgrade.scalar.is_finite() || upgrade.scalar < 0.0
    }) || rules
        .health_upgrades
        .iter()
        .any(|upgrade| upgrade.trigger.is_empty() || !upgrade.amount.is_finite())
    {
        return Err("Invalid experience or health upgrade");
    }
    if let Some(upgrade) = &rules.movement_upgrade {
        if upgrade.trigger.is_empty()
            || !upgrade.speed.is_finite()
            || upgrade.speed <= 0.0
            || upgrade.locomotor.legal_surfaces != unit.locomotor.legal_surfaces
        {
            return Err("Invalid movement upgrade");
        }
        super::super::locomotor::validate(&upgrade.locomotor)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

impl Skirmish {
    pub(in crate::skirmish) fn apply_maintenance_upgrade(
        &mut self,
        id: usize,
        index: usize,
        ordinal: usize,
    ) {
        let kind = self.entities[id].type_index;
        match self.catalog[kind].behavior_modules[index].family.as_str() {
            "ArmorUpgrade" => {
                self.entities[id].maintenance.effects.active |= ARMOR;
                let chemical_suit = self.catalog[kind].behavior_modules[index]
                    .upgrade
                    .triggers
                    .iter()
                    .any(|name| name == "Upgrade_AmericaChemicalSuits");
                self.entities[id].maintenance.effects.chemical_suit |= chemical_suit;
                if chemical_suit {
                    self.entities[id].drawable.terrain_decal = TerrainDecal::ChemicalSuit;
                }
            }
            "WeaponBonusUpgrade" => {
                let extra = self.passenger_bonus_flags(id);
                let actor = &mut self.entities[id];
                if actor.maintenance.effects.active & WEAPON == 0 {
                    actor.maintenance.effects.active |= WEAPON;
                    actor.weapon_bonus_changed(&self.catalog[kind], self.tick, extra);
                }
            }
            "LocomotorSetUpgrade" => {
                if self.entities[id].ai.registration.is_some() {
                    let changed = self.entities[id].maintenance.effects.active & MOVEMENT == 0;
                    self.entities[id].maintenance.effects.active |= MOVEMENT;
                    if changed
                        && self.entities[id].ai.locomotor_set
                            == super::super::locomotor::set::NORMAL
                    {
                        self.choose_locomotor_set(id, super::super::locomotor::set::NORMAL);
                    }
                }
            }
            "PowerPlantUpgrade" => {
                let bonus = self.catalog[kind]
                    .maintenance
                    .power_upgrade
                    .as_ref()
                    .expect("PowerPlantUpgrade data")
                    .bonus;
                self.add_native_power_bonus(id, self.entities[id].owner, bonus);
                self.extend_all_power_rods(id, true);
            }
            "ExperienceScalarUpgrade" => {
                self.entities[id].maintenance.effects.experience_scalar +=
                    self.catalog[kind].maintenance.experience_upgrades[ordinal].scalar;
            }
            "MaxHealthUpgrade" => self.apply_max_health_upgrade(id, ordinal),
            "ModelConditionUpgrade" => {
                let condition = self.catalog[kind].maintenance.condition_upgrades[ordinal]
                    .condition
                    .clone();
                let conditions = &mut self.entities[id].maintenance.effects.conditions;
                if !conditions.contains(&condition) {
                    conditions.push(condition);
                }
            }
            "CommandSetUpgrade" => self.apply_command_set_upgrade(id, ordinal),
            "StealthUpgrade" => self.apply_stealth_upgrade(id),
            "ObjectCreationUpgrade" => self.apply_object_creation_upgrade(id, ordinal),
            "ReplaceObjectUpgrade" => self.apply_replacement_upgrade(id, ordinal),
            _ => unreachable!("unbound UpgradeMux implementation passed admission"),
        }
    }

    fn apply_command_set_upgrade(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let rule = &self.catalog[actor.type_index].maintenance.command_sets[ordinal];
        let command = rule
            .alternate
            .as_ref()
            .filter(|(name, _)| {
                self.has_upgrade(actor.owner, name) || self.object_has_upgrade(id, name)
            })
            .map_or_else(|| rule.command_set.clone(), |(_, command)| command.clone());
        self.entities[id].maintenance.effects.command_set = Some(command);
    }

    fn apply_max_health_upgrade(&mut self, id: usize, ordinal: usize) {
        let kind = self.entities[id].type_index;
        if matches!(
            self.catalog[kind].body_kind,
            crate::skirmish::BodyKind::Inactive
        ) {
            return;
        }
        let rule = &self.catalog[kind].maintenance.health_upgrades[ordinal];
        let actor = &mut self.entities[id];
        let previous_max = actor.max_health;
        actor.max_health += rule.amount;
        actor.damage_state.initial_health = actor.max_health;
        let delta = match rule.change_type {
            MaxHealthChange::PreserveRatio => {
                let ratio = actor.health / previous_max;
                let desired = actor.max_health * ratio;
                Some(desired - actor.health)
            }
            MaxHealthChange::AddCurrentHealthToo => Some(actor.max_health - previous_max),
            MaxHealthChange::SameCurrentHealth => None,
            MaxHealthChange::FullyHeal => Some(actor.max_health - actor.health),
        };
        if let Some(delta) = delta {
            self.change_body_health(id, delta);
        }
        let actor = &self.entities[id];
        if actor.health > actor.max_health {
            self.change_body_health(id, actor.max_health - actor.health);
        }
    }
}
