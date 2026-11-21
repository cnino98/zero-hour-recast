//! SH `SlowDeathBehavior.cpp`: each authored module owns its activation and wake.
use super::{Derived, Entity, Skirmish, SlowDeath, probability};
use crate::skirmish::{ModelConditions, UnitAiUpdate};
use num_traits::ToPrimitive;

#[cfg(test)]
mod tests;

const ACTIVE: u8 = 1;
const MIDPOINT: u8 = 2;
const FLUNG: u8 = 4;
const BOUNCED: u8 = 8;

pub(in crate::skirmish) enum Extension {
    None,
    BattleBus(crate::skirmish::battle_bus::NativeState),
    Jet(crate::skirmish::aircraft_death::NativeState),
    Helicopter(crate::skirmish::helicopter_death::NativeState),
}

pub(in crate::skirmish) struct State {
    pub(in crate::skirmish) extension: Extension,
    sink: u32,
    midpoint: u32,
    destruction: u32,
    scale: f32,
    flags: u8,
}

impl State {
    pub(in crate::skirmish) const fn active(&self) -> bool {
        self.flags & ACTIVE != 0
    }
}

impl Entity {
    pub(super) fn native_slow_death_condition(&self) -> Option<&'static str> {
        if !self
            .death_lifecycle
            .as_ref()?
            .native
            .iter()
            .any(State::active)
        {
            return None;
        }
        let conditions = &self.drawable.model_conditions;
        Some(if conditions.has_flag(ModelConditions::PARACHUTING) {
            "PARACHUTING"
        } else if conditions.has_flag(ModelConditions::EXPLODED_BOUNCING) {
            "EXPLODED_BOUNCING"
        } else if conditions.has_flag(ModelConditions::EXPLODED_FLAILING) {
            "EXPLODED_FLAILING"
        } else {
            "DYING"
        })
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn registered_slow_death(&self, id: usize) -> bool {
        self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .any(|module| {
                matches!(
                    module.family.as_str(),
                    "SlowDeathBehavior"
                        | "BattleBusSlowDeathBehavior"
                        | "JetSlowDeathBehavior"
                        | "HelicopterSlowDeathBehavior"
                        | "NeutronMissileSlowDeathBehavior"
                )
            })
    }

    pub(in crate::skirmish) fn slow_death_index(
        &self,
        id: usize,
        family: &str,
        ordinal: usize,
    ) -> usize {
        let unit = &self.catalog[self.entities[id].type_index];
        let tag = &unit
            .behavior_modules
            .iter()
            .filter(|module| module.family == family)
            .nth(ordinal)
            .expect("authored SlowDeath instance")
            .tag;
        unit.death_lifecycle
            .as_ref()
            .expect("SlowDeath rules")
            .slow
            .iter()
            .position(|rule| &rule.tag == tag)
            .expect("bound SlowDeath tag")
    }

    /// Audio handles belong to the authored module, across all behavior families.
    pub(in crate::skirmish) fn slow_death_module_index(&self, id: usize, index: usize) -> usize {
        let rule = self.native_slow_rule(id, index);
        self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .position(|module| module.family == rule.family() && module.tag == rule.tag)
            .expect("authored SlowDeath module identity")
    }

    pub(in crate::skirmish) fn native_slow_rule(&self, id: usize, index: usize) -> &SlowDeath {
        &self.catalog[self.entities[id].type_index]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath rules")
            .slow[index]
    }

    pub(in crate::skirmish) fn construct_slow_death(&mut self, id: usize, index: usize) {
        self.initialize_death_lifecycle(id);
        let rule = self.native_slow_rule(id, index);
        assert!(rule.probability >= 1);
        let extension = match &rule.derived {
            Derived::None | Derived::Neutron(_) => Extension::None,
            Derived::BattleBus(_) => {
                Extension::BattleBus(crate::skirmish::battle_bus::NativeState::new())
            }
            Derived::Jet(_) => Extension::Jet(crate::skirmish::aircraft_death::NativeState::new()),
            Derived::Helicopter(_) => {
                Extension::Helicopter(crate::skirmish::helicopter_death::NativeState::new())
            }
        };
        let instances = &mut self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("SlowDeath state")
            .native;
        assert_eq!(instances.len(), index);
        instances.push(State {
            extension,
            sink: 0,
            midpoint: 0,
            destruction: 0,
            scale: 1.0,
            flags: 0,
        });
    }

    fn slow_applicable(
        &self,
        id: usize,
        ordinal: usize,
        death: crate::skirmish::DeathType,
    ) -> bool {
        let conditions = &self.native_slow_rule(id, ordinal).conditions;
        self.death_conditions_match_type(
            id,
            death,
            conditions.death_mask,
            conditions.veterancy_mask,
            conditions.exempt_status,
            conditions.required_status,
        )
    }

    pub(in crate::skirmish) fn slow_death_on_die(&mut self, id: usize, ordinal: usize) {
        let index = self.slow_death_index(id, "SlowDeathBehavior", ordinal);
        self.native_slow_death_on_die(id, index);
    }

    pub(in crate::skirmish) fn native_slow_death_on_die(&mut self, id: usize, ordinal: usize) {
        let death = self.entities[id].damage_state.death_type;
        if !self.slow_applicable(id, ordinal, death) {
            return;
        }
        if !matches!(
            self.catalog[self.entities[id].type_index].ai.update,
            UnitAiUpdate::Absent
        ) {
            if self.entities[id].is_ai_dead() {
                return;
            }
            self.mark_ai_dead(id);
        }
        self.deselect_dead_object(id);
        self.select_native_slow_death(id, death);
    }

    pub(in crate::skirmish) fn select_native_slow_death(
        &mut self,
        id: usize,
        death: crate::skirmish::DeathType,
    ) {
        let count = self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native
            .len();
        let overkill = self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .overkill;
        let maximum = self.entities[id].max_health;
        let total: u32 = (0..count)
            .filter(|&index| self.slow_applicable(id, index, death))
            .map(|index| probability(self.native_slow_rule(id, index), overkill, maximum))
            .sum();
        // UndeadBody can enter its second life with no applicable SlowDeath
        // (the installed Battle Bus has no CRUSHED/SPLATTED SlowDeath).
        // CRC0 GameLogicRandomValue(1, 0) returns 0 without consuming RNG;
        // the empty selection then falls through without activating a module.
        if total == 0 {
            return;
        }
        let mut roll = self.random_between(total - 1) + 1;
        for index in 0..count {
            if !self.slow_applicable(id, index, death) {
                continue;
            }
            let weight = probability(self.native_slow_rule(id, index), overkill, maximum);
            if roll <= weight {
                match &self.native_slow_rule(id, index).derived {
                    Derived::None | Derived::Neutron(_) => self.activate_slow_death(id, index),
                    Derived::BattleBus(_) => self.begin_native_battle_bus_death(id, index),
                    Derived::Jet(_) => self.begin_native_jet_death(id, index),
                    Derived::Helicopter(_) => self.begin_native_helicopter_death(id, index),
                }
                return;
            }
            roll -= weight;
        }
        unreachable!("positive applicable SlowDeath selector");
    }

    pub(in crate::skirmish) fn activate_slow_death(&mut self, id: usize, ordinal: usize) {
        if self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native[ordinal]
            .active()
        {
            return;
        }
        let rule = self.native_slow_rule(id, ordinal);
        let scale = self.slow_death_scale;
        let has_fling_force = rule
            .fling
            .as_ref()
            .is_some_and(|fling| fling.force[0] > 0.0);
        let sink_range = rule.sink_delay;
        let death_range = rule.destruction_delay;
        let infantry_sink = rule.sink_rate != 0.0
            && self.catalog[self.entities[id].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "INFANTRY");
        let non_lod = rule.has_non_lod_effects();
        if infantry_sink {
            self.set_slow_death_shadow(id);
        }
        if scale == 0.0 && !non_lod {
            self.destroy_maintenance_object(id);
            return;
        }

        let (sink, midpoint, destruction, accelerated) =
            self.slow_death_times(id, sink_range, death_range, scale);
        let index = ordinal;
        let flung = if has_fling_force {
            if self.entities[id].disabled_held() {
                self.slow_death_release_slave(id);
            }
            if self.object_physics_mass(id).is_some() {
                self.fling_slow_death(id, index);
                self.entities[id]
                    .drawable
                    .model_conditions
                    .set_flag(ModelConditions::EXPLODED_FLAILING, true);
                true
            } else {
                false
            }
        } else {
            false
        };
        let delay = if has_fling_force {
            1
        } else {
            sink.min(midpoint).min(destruction)
        };
        let family = self.native_slow_rule(id, ordinal).family();
        let module_ordinal = self.catalog[self.entities[id].type_index]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath rules")
            .slow[..ordinal]
            .iter()
            .filter(|rule| rule.family() == family)
            .count();
        self.wake_family_update(id, family, module_ordinal, delay);
        let now = self.update_frame();
        let state = &mut self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("SlowDeath state")
            .native[ordinal];
        state.sink = sink.wrapping_add(now);
        state.midpoint = midpoint.wrapping_add(now);
        state.destruction = destruction.wrapping_add(now);
        state.scale = accelerated;
        state.flags = ACTIVE | if flung { FLUNG } else { 0 };
        self.slow_death_phase(id, index, 0);
    }

    fn slow_death_times(
        &mut self,
        id: usize,
        sink_range: [u32; 2],
        death_range: [u32; 2],
        scale: f32,
    ) -> (u32, u32, u32, f32) {
        if self.hulk_lifetime_override.is_some()
            && self.catalog[self.entities[id].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "HULK")
        {
            (1, 16, 31, 1.0)
        } else {
            let sink = sink_range[0].wrapping_add(self.slow_death_variance(sink_range[1]));
            let sink = scaled_frame(sink, scale);
            let destruction = death_range[0].wrapping_add(self.slow_death_variance(death_range[1]));
            let destruction = scaled_frame(destruction, scale);
            let duration = destruction.to_f32().expect("native destruction duration");
            let low = (0.35 * duration).to_u32().expect("native midpoint low");
            let high = (0.65 * duration).to_u32().expect("native midpoint high");
            let midpoint = low.wrapping_add(self.random_between(high - low));
            (sink, midpoint, destruction, scale)
        }
    }

    fn slow_death_variance(&mut self, maximum: u32) -> u32 {
        // GetGameLogicRandomValue takes signed Int bounds; CRC=0 returns hi
        // without consuming RNG when the unsigned variance converts below zero.
        if i32::from_ne_bytes(maximum.to_ne_bytes()) <= 0 {
            maximum
        } else {
            self.random_between(maximum)
        }
    }

    pub(in crate::skirmish) fn update_slow_death(&mut self, id: usize, ordinal: usize) -> i32 {
        let index = self.slow_death_index(id, "SlowDeathBehavior", ordinal);
        self.update_slow_death_base(id, index)
    }

    pub(in crate::skirmish) fn update_slow_death_base(&mut self, id: usize, ordinal: usize) -> i32 {
        // Native checks activation only with DEBUG_ASSERTCRASH; release callbacks
        // retain their constructor deadlines if an inactive instance is awakened.
        let scale = self.slow_death_scale;
        let non_lod = self.native_slow_rule(id, ordinal).has_non_lod_effects();
        let state = &mut self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("SlowDeath state")
            .native[ordinal];
        if scale.to_bits() != 1.0_f32.to_bits()
            && state.scale.to_bits() == 1.0_f32.to_bits()
            && !non_lod
        {
            if scale == 0.0 {
                self.destroy_maintenance_object(id);
                return 1;
            }
            state.sink = scaled_frame(state.sink, scale);
            state.midpoint = scaled_frame(state.midpoint, scale);
            state.destruction = scaled_frame(state.destruction, scale);
            state.scale = scale;
        }
        let index = ordinal;
        let sink_rate = self.native_slow_rule(id, ordinal).sink_rate;
        self.advance_flung_death(id, ordinal, sink_rate);
        let now = self.update_frame();
        let state = &self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native[ordinal];
        if now >= state.sink && sink_rate > 0.0 {
            let amount = sink_rate / state.scale;
            self.set_disabled_held(id);
            let mut position = self.object_position(id);
            position[2] -= amount;
            self.set_object_position(id, position);
        }
        let state = &self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native[ordinal];
        if now >= state.midpoint && state.flags & MIDPOINT == 0 {
            self.slow_death_phase(id, index, 1);
            self.entities[id]
                .death_lifecycle
                .as_mut()
                .expect("SlowDeath state")
                .native[ordinal]
                .flags |= MIDPOINT;
        }
        if now
            >= self.entities[id]
                .death_lifecycle
                .as_ref()
                .expect("SlowDeath state")
                .native[ordinal]
                .destruction
        {
            self.slow_death_phase(id, index, 2);
            self.destroy_maintenance_object(id);
        }
        1
    }

    fn advance_flung_death(&mut self, id: usize, ordinal: usize, sink_rate: f32) {
        let above = self.height_above_ground(id) > 0.0;
        let state = &mut self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("SlowDeath state")
            .native[ordinal];
        if state.flags & FLUNG == 0 || state.flags & BOUNCED != 0 {
            return;
        }
        state.sink = state.sink.wrapping_add(1);
        state.midpoint = state.midpoint.wrapping_add(1);
        state.destruction = state.destruction.wrapping_add(1);
        if !above {
            state.flags |= BOUNCED;
            let conditions = &mut self.entities[id].drawable.model_conditions;
            conditions.set_flag(ModelConditions::EXPLODED_FLAILING, false);
            conditions.set_flag(ModelConditions::EXPLODED_BOUNCING, true);
        }
        let tree = self.physics_collided_with_tree(id);
        if tree {
            self.set_disabled_held(id);
            let conditions = &mut self.entities[id].drawable.model_conditions;
            conditions.set_flag(ModelConditions::EXPLODED_FLAILING, false);
            conditions.set_flag(ModelConditions::EXPLODED_BOUNCING, false);
            conditions.set_flag(ModelConditions::PARACHUTING, true);
            let mut position = self.object_position(id);
            let snag_drop = sink_rate * 50.0;
            position[2] -= snag_drop;
            self.set_object_position(id, position);
            if self.height_above_ground(id) <= 0.0 {
                self.destroy_maintenance_object(id);
            }
        }
    }
}

fn scaled_frame(frame: u32, scale: f32) -> u32 {
    (frame.to_f32().expect("native frame") * scale)
        .to_u32()
        .expect("native scaled frame")
}

impl SlowDeath {
    pub(in crate::skirmish) const fn family(&self) -> &'static str {
        match self.derived {
            Derived::None => "SlowDeathBehavior",
            Derived::BattleBus(_) => "BattleBusSlowDeathBehavior",
            Derived::Jet(_) => "JetSlowDeathBehavior",
            Derived::Helicopter(_) => "HelicopterSlowDeathBehavior",
            Derived::Neutron(_) => "NeutronMissileSlowDeathBehavior",
        }
    }
}
