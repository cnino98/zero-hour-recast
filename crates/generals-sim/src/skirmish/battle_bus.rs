//! SH `UndeadBody.cpp` and per-instance `BattleBusSlowDeathBehavior.cpp`.
use super::death_lifecycle::{Derived, native::Extension};
use super::{
    DAMAGE_TYPE_COUNT, DamageType, DeathType, Entity, ModelConditions, Skirmish, UnitType,
    physical_ocl,
};

pub struct Rules {
    pub second_health: f32,
    pub first_armor: [[f32; DAMAGE_TYPE_COUNT]; 2],
    pub second_armor: [[f32; DAMAGE_TYPE_COUNT]; 3],
}

pub struct DeathRules {
    pub throw_force: f32,
    pub passenger_damage: f32,
    pub empty_delay: u32,
    pub start_fx: String,
    pub ground_fx: String,
    pub start_ocl: Option<physical_ocl::List>,
    pub ground_ocl: Option<physical_ocl::List>,
}

pub(super) struct State {
    pub(super) second_life: bool,
}
impl State {
    pub(super) const fn new() -> Self {
        Self { second_life: false }
    }
}

pub(in crate::skirmish) struct NativeState {
    real_death: bool,
    first_death: bool,
    ground_check: u32,
    penalty: u32,
}
impl NativeState {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            real_death: false,
            first_death: false,
            ground_check: 0,
            penalty: 0,
        }
    }
}

impl Entity {
    #[must_use]
    pub fn battle_bus_second_life(&self) -> bool {
        self.battle_bus
            .as_ref()
            .is_some_and(|state| state.second_life)
    }
    #[must_use]
    pub const fn battle_bus_grounded(&self) -> bool {
        self.drawable
            .model_conditions
            .has_flag(ModelConditions::SECOND_LIFE)
    }
    pub(in crate::skirmish) fn battle_bus_armor<'a>(
        &self,
        unit: &'a UnitType,
    ) -> Option<&'a [f32; DAMAGE_TYPE_COUNT]> {
        let rules = unit.battle_bus.as_ref()?;
        let level = usize::from(self.salvage_level().min(2));
        if self.battle_bus_second_life() {
            Some(&rules.second_armor[level])
        } else if level > 0 {
            Some(&rules.first_armor[level - 1])
        } else {
            None
        }
    }
}

impl Skirmish {
    /// `UndeadBody` estimates armored damage before bounding the raw input.
    pub(super) fn battle_bus_damage_raw(
        &self,
        id: usize,
        damage: DamageType,
        raw: f32,
    ) -> Option<f32> {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        unit.battle_bus.as_ref()?;
        // ActiveBody::estimateDamage returns armor adjustment without the
        // separate incoming damage scalar. Its under-construction Sniper
        // exception applies before UndeadBody compares against current health.
        let estimate = if matches!(damage, DamageType::Sniper)
            && actor.under_construction()
            && unit.kind_of.iter().any(|kind| kind == "STRUCTURE")
        {
            0.0
        } else {
            actor.armor_damage(unit, damage, raw)
        };
        if matches!(
            damage,
            DamageType::Unresistable
                | DamageType::Status
                | DamageType::KillPilot
                | DamageType::KillGarrisoned
                | DamageType::SubdualMissile
                | DamageType::SubdualVehicle
                | DamageType::SubdualBuilding
                | DamageType::SubdualUnresistable
        ) || actor.battle_bus_second_life()
            || estimate < actor.health
        {
            return None;
        }
        Some(raw.min(actor.health - 1.0))
    }

    pub(super) fn start_battle_bus_second_life(&mut self, id: usize, death: DeathType) {
        let health = self.catalog[self.entities[id].type_index]
            .battle_bus
            .as_ref()
            .expect("UndeadBody data")
            .second_health;
        self.entities[id]
            .battle_bus
            .as_mut()
            .expect("UndeadBody state")
            .second_life = true;
        self.entities[id].max_health = health;
        self.entities[id].damage_state.initial_health = health;
        self.change_body_health(id, health - self.entities[id].health);
        self.select_native_slow_death(id, death);
    }

    fn battle_bus_death_state(&self, id: usize, index: usize) -> &NativeState {
        let Extension::BattleBus(state) = &self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native[index]
            .extension
        else {
            unreachable!("BattleBus instance")
        };
        state
    }
    fn battle_bus_death_state_mut(&mut self, id: usize, index: usize) -> &mut NativeState {
        let Extension::BattleBus(state) = &mut self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("SlowDeath state")
            .native[index]
            .extension
        else {
            unreachable!("BattleBus instance")
        };
        state
    }
    fn battle_bus_death_rule(&self, id: usize, index: usize) -> &DeathRules {
        let Derived::BattleBus(rule) = &self.native_slow_rule(id, index).derived else {
            unreachable!("BattleBus data")
        };
        rule
    }

    pub(in crate::skirmish) fn battle_bus_slow_death_on_die(&mut self, id: usize, ordinal: usize) {
        let index = self.slow_death_index(id, "BattleBusSlowDeathBehavior", ordinal);
        let state = self.battle_bus_death_state_mut(id, index);
        state.real_death = true;
        state.first_death = false;
        self.native_slow_death_on_die(id, index);
    }

    pub(in crate::skirmish) fn begin_native_battle_bus_death(&mut self, id: usize, index: usize) {
        if self.battle_bus_death_state(id, index).real_death {
            self.activate_slow_death(id, index);
            return;
        }
        let frame = self.update_frame().wrapping_add(10);
        let state = self.battle_bus_death_state_mut(id, index);
        state.first_death = true;
        state.ground_check = frame;
        self.battle_bus_effects(id, index, false);
        if !matches!(
            self.catalog[self.entities[id].type_index].ai.update,
            super::UnitAiUpdate::Absent
        ) {
            self.ai_idle_from_ai(id);
        }
        if self.object_physics_mass(id).is_some() {
            self.clear_live_physics(id);
            let force = self.battle_bus_death_rule(id, index).throw_force;
            self.apply_live_shock(id, [0.0, 0.0, force]);
        }
        if self.container_rules(id).is_some() {
            let damage = self.battle_bus_death_rule(id, index).passenger_damage;
            self.process_damage_to_contained(id, damage);
        }
        let ordinal = self.catalog[self.entities[id].type_index]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath data")
            .slow[..index]
            .iter()
            .filter(|rule| matches!(rule.derived, Derived::BattleBus(_)))
            .count();
        self.wake_family_update(id, "BattleBusSlowDeathBehavior", ordinal, 1);
    }

    fn battle_bus_effects(&mut self, id: usize, index: usize, grounded: bool) {
        let rule = self.battle_bus_death_rule(id, index);
        let (effect, objects) = if grounded {
            (&rule.ground_fx, &rule.ground_ocl)
        } else {
            (&rule.start_fx, &rule.start_ocl)
        };
        let effect = effect.clone();
        let objects = objects.clone();
        self.object_fx(id, effect);
        if let Some(objects) = objects {
            self.create_physical_ocl(id, &objects)
                .expect("admitted BattleBus OCL");
        }
    }

    pub(in crate::skirmish) fn update_battle_bus_slow_death(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        let index = self.slow_death_index(id, "BattleBusSlowDeathBehavior", ordinal);
        let state = self.battle_bus_death_state(id, index);
        if state.first_death {
            if state.ground_check < self.update_frame() && self.height_above_ground(id) <= 0.0 {
                self.battle_bus_death_state_mut(id, index).first_death = false;
                self.battle_bus_effects(id, index, true);
                self.entities[id]
                    .drawable
                    .model_conditions
                    .set_flag(ModelConditions::SECOND_LIFE, true);
                if !matches!(
                    self.catalog[self.entities[id].type_index].ai.update,
                    super::UnitAiUpdate::Absent
                ) {
                    self.ai_idle_from_ai(id);
                }
                if self.object_physics_mass(id).is_some() {
                    self.clear_live_physics(id);
                }
                self.set_disabled_held(id);
                if self.battle_bus_death_rule(id, index).empty_delay == 0 {
                    return i32::try_from(super::module_updates::FOREVER)
                        .expect("native forever sleep");
                }
            }
            return 1;
        }
        if state.real_death {
            return self.update_slow_death_base(id, index);
        }
        if self.container_rules(id).is_none() {
            return i32::try_from(super::module_updates::FOREVER).expect("native forever sleep");
        }
        let filled = !self.entities[id].containment.passengers.is_empty();
        let penalty = state.penalty;
        if penalty != 0 {
            if filled {
                self.battle_bus_death_state_mut(id, index).penalty = 0;
            } else if self.update_frame() > penalty {
                self.kill_object_with_type(id, DamageType::Penalty, DeathType::Extra4);
                return i32::try_from(super::module_updates::FOREVER)
                    .expect("native forever sleep");
            }
        } else if !filled {
            let deadline = self
                .update_frame()
                .wrapping_add(self.battle_bus_death_rule(id, index).empty_delay);
            self.battle_bus_death_state_mut(id, index).penalty = deadline;
        }
        15
    }
}

#[cfg(test)]
pub(in crate::skirmish) mod tests;
