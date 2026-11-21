//! Bounded native Terrorist/ToxicInfantry `SlowDeath` and `LifetimeUpdate` callbacks.
use super::{Entity, Skirmish, WeaponRules, startup};
use num_traits::ToPrimitive;

pub(in crate::skirmish) mod native;
mod physics;

#[cfg(test)]
mod tests;

pub struct Rules {
    pub on_die: Vec<DieAction>,
    pub slow: Vec<SlowDeath>,
    pub physics: Option<Physics>,
    pub weapons: Vec<WeaponRules>,
}

pub enum DieAction {
    Slow,
}

pub struct Physics {
    pub gravity: f32,
    pub mass: f32,
    pub stiffness: f32,
    pub forward: f32,
    pub lateral: f32,
    pub aerodynamic: f32,
    pub stick_to_ground: bool,
    pub airborne_friction: bool,
}

pub enum Derived {
    None,
    BattleBus(Box<super::battle_bus::DeathRules>),
    Jet(Box<super::aircraft_death::Jet>),
    Helicopter(Box<super::helicopter_death::Rules>),
    Neutron(Box<super::neutron_missile::DeathRules>),
}

pub struct SlowDeath {
    pub derived: Derived,
    pub tag: String,
    pub conditions: super::death_callbacks::Conditions,
    pub probability: u32,
    pub overkill_bonus: f32,
    pub sink_delay: [u32; 2],
    pub destruction_delay: [u32; 2],
    pub sink_rate: f32,
    pub fling: Option<Fling>,
    pub fx: [Vec<String>; 3],
    pub objects: [Vec<Spawn>; 3],
    pub weapons: [Vec<usize>; 3],
}

pub struct Fling {
    pub force: [f32; 2],
    pub pitch: [f32; 2],
}

pub enum Spawn {
    None,
    Object { name: String, skip_airborne: bool },
    Physical(super::physical_ocl::List),
}

pub(super) struct State {
    overkill: i32,
    body: Option<physics::Body>,
    slow: Option<Timing>,
    ai_frame: Option<u64>,
    update_frame: Option<u64>,
    pub(in crate::skirmish) native: Vec<native::State>,
}

struct Timing {
    index: usize,
    sink: u64,
    midpoint: u64,
    destruction: u64,
    wake: u64,
    flags: u8,
}

const FLUNG: u8 = 1;
const BOUNCED: u8 = 2;
const MIDPOINT: u8 = 4;

impl Entity {
    pub(in crate::skirmish) fn set_corpse_height(&mut self, height: f32) {
        if let Some(body) = self
            .death_lifecycle
            .as_mut()
            .and_then(|state| state.body.as_mut())
        {
            body.height = height;
        }
    }

    #[must_use]
    pub fn corpse_height(&self) -> Option<f32> {
        self.death_lifecycle
            .as_ref()?
            .body
            .as_ref()
            .map(|body| body.height)
    }

    #[must_use]
    pub fn corpse_present(&self) -> bool {
        self.jet_corpse()
            || self.helicopter_corpse()
            || (!self.alive()
                && !self.masked()
                && self.death_lifecycle.as_ref().is_some_and(|state| {
                    state.slow.is_some() || state.native.iter().any(native::State::active)
                }))
    }

    #[must_use]
    pub fn death_condition(&self) -> Option<&'static str> {
        if self.helicopter_hit_ground() {
            return Some("SPECIAL_DAMAGED");
        }
        if self.jet_corpse() || self.helicopter_corpse() {
            return Some("DYING");
        }
        if let Some(condition) = self.native_slow_death_condition() {
            return Some(condition);
        }
        let timing = self.death_lifecycle.as_ref()?.slow.as_ref()?;
        Some(if timing.flags & BOUNCED != 0 {
            "EXPLODED_BOUNCING"
        } else if timing.flags & FLUNG != 0 {
            "EXPLODED_FLAILING"
        } else {
            "DYING"
        })
    }
}

impl Skirmish {
    pub(super) fn initialize_death_lifecycle(&mut self, id: usize) {
        let actor = &self.entities[id];
        if actor.death_lifecycle.is_some()
            || self.catalog[actor.type_index].death_lifecycle.is_none()
        {
            return;
        }
        self.entities[id].death_lifecycle = Some(Box::new(State {
            overkill: 0,
            body: None,
            slow: None,
            ai_frame: None,
            update_frame: None,
            native: Vec::new(),
        }));
    }

    pub(super) fn begin_slow_death(&mut self, id: usize) {
        let registered = self.registered_slow_death(id);
        let Some(rules) = &self.catalog[self.entities[id].type_index].death_lifecycle else {
            return;
        };
        if !rules.on_die.is_empty() {
            for index in 0..rules.on_die.len() {
                let rules = self.catalog[self.entities[id].type_index]
                    .death_lifecycle
                    .as_ref()
                    .expect("death rules");
                match &rules.on_die[index] {
                    DieAction::Slow => {
                        if !registered {
                            self.choose_slow_death(id);
                        }
                    }
                }
            }
            return;
        }
        if !registered {
            self.choose_slow_death(id);
        }
    }

    pub(super) fn record_slow_death_damage(&mut self, id: usize, actual: f32, clipped: f32) {
        if let Some(state) = &mut self.entities[id].death_lifecycle {
            state.overkill = (actual - clipped).to_i32().unwrap_or(i32::MIN);
        }
    }

    fn choose_slow_death(&mut self, id: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let Some(rules) = &unit.death_lifecycle else {
            return;
        };
        if actor
            .death_lifecycle
            .as_ref()
            .is_some_and(|state| state.slow.is_some())
        {
            return;
        }
        let bit = 1 << actor.damage_state.death_type as u32;
        let overkill = actor
            .death_lifecycle
            .as_ref()
            .expect("death lifecycle state")
            .overkill;
        let maximum_health = actor.max_health;
        let total: u32 = rules
            .slow
            .iter()
            .filter(|rule| {
                rule.conditions.death_mask & bit != 0
                    && !((rule.conditions.exempt_status & (1 << 3) != 0)
                        && actor.under_construction())
            })
            .map(|rule| probability(rule, overkill, maximum_health))
            .sum();
        if total > 0 {
            let mut roll = self.random_between(total - 1) + 1;
            let rules = self.catalog[self.entities[id].type_index]
                .death_lifecycle
                .as_ref()
                .expect("death rules");
            let index = rules
                .slow
                .iter()
                .position(|rule| {
                    if rule.conditions.death_mask & bit == 0
                        || ((rule.conditions.exempt_status & (1 << 3) != 0)
                            && self.entities[id].under_construction())
                    {
                        return false;
                    }
                    let weight = probability(rule, overkill, maximum_health);
                    if roll <= weight {
                        true
                    } else {
                        roll -= weight;
                        false
                    }
                })
                .expect("weighted applicable death");
            self.start_slow_death(id, index);
        }
    }

    fn start_slow_death(&mut self, id: usize, index: usize) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index]
            .death_lifecycle
            .as_ref()
            .expect("death rules")
            .slow[index];
        let sink =
            rules.sink_delay[0] + startup::random_between(&mut self.random, rules.sink_delay[1]);
        let destruction = rules.destruction_delay[0]
            + startup::random_between(&mut self.random, rules.destruction_delay[1]);
        let duration = destruction.to_f32().expect("native frame duration");
        let low = (0.35 * duration).to_u32().expect("midpoint low");
        let high = (0.65 * duration).to_u32().expect("midpoint high");
        let midpoint = low + startup::random_between(&mut self.random, high - low);
        let fling = rules.fling.is_some();
        self.initialize_death_body(id);
        if fling {
            self.fling_corpse(id, index);
        }
        let wake = if fling {
            self.tick + 1
        } else {
            self.tick + u64::from(sink.min(midpoint).min(destruction))
        };
        self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("lifecycle")
            .slow = Some(Timing {
            index,
            sink: self.tick + u64::from(sink),
            midpoint: self.tick + u64::from(midpoint),
            destruction: self.tick + u64::from(destruction),
            wake,
            flags: if fling { FLUNG } else { 0 },
        });
        self.death_phase(id, 0);
    }

    fn death_phase(&mut self, id: usize, phase: usize) {
        let actor = &self.entities[id];
        let index = actor
            .death_lifecycle
            .as_ref()
            .expect("lifecycle")
            .slow
            .as_ref()
            .expect("slow death")
            .index;
        self.slow_death_phase(id, index, phase);
    }

    fn slow_death_phase(&mut self, id: usize, index: usize, phase: usize) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index]
            .death_lifecycle
            .as_ref()
            .expect("rules")
            .slow[index];
        if !rules.has_effects() {
            return;
        }
        let effect = pick(&rules.fx[phase], &mut self.random)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        if let Some(effect) = effect {
            self.object_fx(id, effect);
        }
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index]
            .death_lifecycle
            .as_ref()
            .expect("rules");
        let choices = &rules.slow[index].objects[phase];
        if !choices.is_empty() {
            let choice = usize::try_from(
                self.random_between(u32::try_from(choices.len() - 1).expect("OCL choices")),
            )
            .expect("OCL index");
            self.spawn_death_object(id, index, phase, choice);
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let choices = &unit.death_lifecycle.as_ref().expect("rules").slow[index].weapons[phase];
        if !choices.is_empty() {
            let choice = usize::try_from(startup::random_between(
                &mut self.random,
                u32::try_from(choices.len() - 1).expect("weapon choices"),
            ))
            .expect("weapon index");
            if choices[choice] != usize::MAX {
                let weapon = unit.slow_weapon_start() + choices[choice];
                self.fire_temporary_weapon(id, weapon);
            }
        }
    }

    fn spawn_death_object(&mut self, id: usize, index: usize, phase: usize, choice: usize) {
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .death_lifecycle
            .as_ref()
            .expect("death rules");
        if matches!(rules.slow[index].objects[phase][choice], Spawn::None) {
            return;
        }
        let Spawn::Object {
            name,
            skip_airborne,
        } = &rules.slow[index].objects[phase][choice]
        else {
            let Spawn::Physical(list) = &rules.slow[index].objects[phase][choice] else {
                unreachable!("source OCL form");
            };
            let list = list.clone();
            self.create_physical_ocl(id, &list)
                .expect("validated SlowDeath physical object templates");
            return;
        };
        if *skip_airborne
            && rules
                .physics
                .as_ref()
                .is_some_and(|physics| self.height_above_ground(id) > -9.0 * physics.gravity)
        {
            return;
        }
        let type_index = self
            .catalog
            .iter()
            .position(|unit| &unit.name == name)
            .expect("validated death OCL template");
        let (position, basis) = self.object_transform(id);
        let owner = actor.owner;
        let child = self.new_object(type_index, owner, true);
        self.set_object_transform(child, position, basis);
    }

    pub(super) fn update_deaths(&mut self, physics_count: usize) {
        // The live heap admits newborn NORMAL modules now. PhysicsBehavior's
        // constructor schedules its first update for the next logic frame.
        let mut id = 0;
        while id < self.entities.len() {
            self.update_dead_ground(id);
            if id >= physics_count && !self.registered_physics(id) {
                self.integrate_death_body(id);
                self.integrate_physical_object(id);
            }
            self.update_death_lifecycle(id);
            self.update_physical_slow_death(id);
            id += 1;
        }
    }

    fn update_death_lifecycle(&mut self, id: usize) {
        if self.registered_slow_death(id) {
            return;
        }
        let Some(timing) = self.entities[id]
            .death_lifecycle
            .as_ref()
            .and_then(|state| state.slow.as_ref())
        else {
            return;
        };
        if timing.wake > self.tick
            || self.entities[id].masked()
            || self.entities[id]
                .death_lifecycle
                .as_ref()
                .expect("lifecycle")
                .update_frame
                == Some(self.tick)
        {
            return;
        }
        self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("lifecycle")
            .update_frame = Some(self.tick);
        self.advance_slow_death(id);
    }

    fn advance_slow_death(&mut self, id: usize) {
        let ground = self.ground_height(self.entities[id].position);
        let type_index = self.entities[id].type_index;
        let state = self.entities[id]
            .death_lifecycle
            .as_mut()
            .expect("lifecycle");
        let timing = state.slow.as_mut().expect("slow death");
        let rules = &self.catalog[type_index]
            .death_lifecycle
            .as_ref()
            .expect("rules")
            .slow[timing.index];
        if timing.flags & FLUNG != 0 && timing.flags & BOUNCED == 0 {
            timing.sink += 1;
            timing.midpoint += 1;
            timing.destruction += 1;
            if state
                .body
                .as_ref()
                .is_some_and(|body| body.height <= ground)
            {
                timing.flags |= BOUNCED;
            }
        }
        if self.tick >= timing.sink
            && rules.sink_rate > 0.0
            && let Some(body) = &mut state.body
        {
            body.height -= rules.sink_rate;
        }
        let sinking = self.tick >= timing.sink && rules.sink_rate > 0.0;
        let midpoint = self.tick >= timing.midpoint && timing.flags & MIDPOINT == 0;
        if midpoint {
            timing.flags |= MIDPOINT;
        }
        let destroy = self.tick >= timing.destruction;
        if sinking {
            self.set_disabled_held(id);
        }
        if midpoint {
            self.death_phase(id, 1);
        }
        if destroy {
            self.death_phase(id, 2);
            self.destroy_maintenance_object(id);
            self.entities[id].maintenance.masked = true;
            self.observe_visibility(id);
        }
    }
}

fn pick<'a>(values: &'a [String], random: &mut [u32; 6]) -> Option<&'a str> {
    if values.is_empty() {
        return None;
    }
    let max = u32::try_from(values.len() - 1).expect("source FX list length");
    let index = usize::try_from(startup::random_between(random, max)).expect("FX index");
    Some(&values[index])
}

fn probability(rule: &SlowDeath, overkill: i32, maximum_health: f32) -> u32 {
    let percent = overkill.to_f32().expect("native overkill integer") / maximum_health;
    // Windows x86 conversion yields integer-indefinite for the native
    // InactiveBody 0/0 case, as well as out-of-range finite products.
    let modifier = (percent * rule.overkill_bonus).to_i32().unwrap_or(i32::MIN);
    let probability = i32::try_from(rule.probability).expect("native SlowDeath probability");
    u32::try_from(probability.wrapping_add(modifier).max(1))
        .expect("positive SlowDeath probability")
}

impl SlowDeath {
    fn has_non_lod_effects(&self) -> bool {
        self.objects
            .iter()
            .flatten()
            .any(|spawn| !matches!(spawn, Spawn::None))
            || self
                .weapons
                .iter()
                .flatten()
                .any(|&weapon| weapon != usize::MAX)
    }

    fn has_effects(&self) -> bool {
        self.has_non_lod_effects() || self.fx.iter().flatten().any(|name| !name.is_empty())
    }
}

impl super::UnitType {
    pub fn jet_death_rules(&self) -> impl Iterator<Item = &super::aircraft_death::Jet> {
        self.death_lifecycle
            .iter()
            .flat_map(|rules| &rules.slow)
            .filter_map(|rule| match &rule.derived {
                Derived::Jet(jet) => Some(jet.as_ref()),
                _ => None,
            })
    }

    pub fn helicopter_death_rules(&self) -> impl Iterator<Item = &super::helicopter_death::Rules> {
        self.death_lifecycle
            .iter()
            .flat_map(|rules| &rules.slow)
            .filter_map(|rule| match &rule.derived {
                Derived::Helicopter(helicopter) => Some(helicopter.as_ref()),
                _ => None,
            })
    }
}
