//! Native `BattlePlanUpdate` transitions and the fixed-CRC player callbacks.
use super::{Entity, Order, Skirmish, UnitType};

mod bonuses;
mod projectiles;
pub(super) use projectiles::Damage;
#[cfg(test)]
mod tests;
pub(super) use bonuses::{Army, Bonuses};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    Bombardment,
    HoldTheLine,
    SearchAndDestroy,
}

impl Plan {
    pub(super) const fn index(self) -> usize {
        match self {
            Self::Bombardment => 0,
            Self::HoldTheLine => 1,
            Self::SearchAndDestroy => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Unpacking,
    Active,
    Packing,
}

pub struct Rules {
    pub animation_frames: [u32; 3],
    pub transition_idle_frames: u32,
    pub paralyze_frames: u32,
    pub valid_kind_of: Vec<String>,
    pub invalid_kind_of: Vec<String>,
    pub armor_scalar: f32,
    pub sight_scalar: f32,
    pub center_sight_scalar: f32,
    pub center_health_scalar: f32,
    pub center_detects_stealth: bool,
}

pub(super) struct State {
    current: Option<Plan>,
    desired: Option<Plan>,
    affected: Option<Plan>,
    status: Status,
    next_ready: u64,
    centering: bool,
    animation: Option<(u64, u32, bool)>,
}

impl State {
    pub(super) fn for_unit(unit: &UnitType) -> Option<Box<Self>> {
        unit.battle_plan.as_ref().map(|_| Box::new(Self::new()))
    }

    pub(super) const fn new() -> Self {
        Self {
            current: None,
            desired: None,
            affected: None,
            status: Status::Idle,
            next_ready: 0,
            centering: false,
            animation: None,
        }
    }
}

pub struct Event {
    pub frame: u64,
    pub object: usize,
    pub plan: Plan,
    pub old_status: Status,
    pub status: Status,
}

impl Entity {
    #[must_use]
    pub fn battle_plan(&self) -> Option<Plan> {
        self.battle_plan.as_ref().and_then(|state| {
            (state.status == Status::Active)
                .then_some(state.affected)
                .flatten()
        })
    }

    #[must_use]
    pub fn battle_plan_desired(&self) -> Option<Plan> {
        self.battle_plan.as_ref().and_then(|state| state.desired)
    }

    #[must_use]
    pub fn battle_plan_condition(&self) -> Option<&'static str> {
        let state = self.battle_plan.as_ref()?;
        let index = state.current?.index();
        match state.status {
            Status::Idle => None,
            Status::Unpacking => {
                Some(["DOOR_1_OPENING", "DOOR_2_OPENING", "DOOR_3_OPENING"][index])
            }
            Status::Active => Some(
                [
                    "DOOR_1_WAITING_TO_CLOSE",
                    "DOOR_2_WAITING_TO_CLOSE",
                    "DOOR_3_WAITING_TO_CLOSE",
                ][index],
            ),
            Status::Packing => Some(["DOOR_1_CLOSING", "DOOR_2_CLOSING", "DOOR_3_CLOSING"][index]),
        }
    }

    #[must_use]
    pub const fn paralyzed(&self) -> bool {
        self.battle_bonuses.paralyzed_until != 0
    }

    pub(super) fn battle_plan_animation(&self) -> Option<(u64, u32, bool)> {
        self.battle_plan.as_ref().and_then(|state| state.animation)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn created_battle_plan(&mut self, id: usize) {
        // BattlePlanUpdate::onObjectCreated sets a weapon condition, not rank.
        self.entities[id].weapon_set.forced_veteran = true;
        self.replace_weapon_set(id);
        if !matches!(
            self.catalog[self.entities[id].type_index].ai.update,
            super::UnitAiUpdate::Absent
        ) {
            if self.weapon_slot_present(id, 0)
                && !matches!(
                    self.entities[id].weapon_set.lock,
                    super::weapons::Lock::Permanent
                )
            {
                self.entities[id].weapon_set.current = 0;
                self.entities[id].weapon_set.lock = super::weapons::Lock::Temporary;
            }
            if let Some(which) = self.current_weapon_turret(id) {
                self.set_native_turret_enabled(id, which, false);
            }
        }
    }

    #[must_use]
    pub fn can_choose_battle_plan(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            actor.complete() && !actor.hacked() && !actor.paralyzed() && actor.battle_plan.is_some()
        }) && self.has_operational_power(id)
    }

    /// Native command options select a desired plan; an in-flight transition finishes first.
    pub fn choose_battle_plan(&mut self, id: usize, plan: Plan) -> bool {
        if !self.can_choose_battle_plan(id) {
            return false;
        }
        if let Some(state) = &mut self.entities[id].battle_plan {
            state.desired = Some(plan);
        }
        true
    }

    pub(super) fn update_battle_plan_module(&mut self, id: usize) -> i32 {
        let state = self.entities[id]
            .battle_plan
            .as_ref()
            .expect("bound BattlePlanUpdate");
        if u64::from(self.update_frame()) < state.next_ready {
            return 1;
        }
        match state.status {
            Status::Idle if state.desired.is_some() => {
                let state = self.entities[id].battle_plan.as_mut().expect("battle plan");
                state.current = state.desired;
                self.set_battle_plan_status(id, Status::Unpacking);
            }
            Status::Unpacking => {
                self.set_battle_plan_status(id, Status::Active);
                if self.entities[id].battle_plan() == Some(Plan::Bombardment) {
                    self.entities[id].ai.turret.enabled = true;
                }
            }
            Status::Active if state.current != state.desired => self.pack_battle_plan(id),
            Status::Packing => self.set_battle_plan_status(id, Status::Idle),
            Status::Idle | Status::Active => {}
        }
        1
    }

    fn pack_battle_plan(&mut self, id: usize) {
        if self.entities[id].battle_plan() == Some(Plan::Bombardment) {
            let actor = &self.entities[id];
            let Some(rules) = &self.catalog[actor.type_index].turret else {
                return;
            };
            if !actor.ai.turret.natural(rules) {
                if !actor
                    .battle_plan
                    .as_ref()
                    .is_some_and(|state| state.centering)
                {
                    self.set_ai_order(id, Order::Idle);
                    let actor = &mut self.entities[id];
                    // SH 19/02/2026: an attack must not interrupt this recenter.
                    actor.ai.turret.enabled = false;
                    actor.ai.turret.recenter();
                    actor.battle_plan.as_mut().expect("battle plan").centering = true;
                }
                return;
            }
            self.entities[id].ai.turret.enabled = false;
            self.entities[id]
                .battle_plan
                .as_mut()
                .expect("battle plan")
                .centering = false;
        }
        self.set_battle_plan_status(id, Status::Packing);
    }

    fn set_battle_plan_status(&mut self, id: usize, status: Status) {
        let actor = &self.entities[id];
        let state = actor.battle_plan.as_ref().expect("battle plan");
        let plan = state.current.expect("selected battle plan");
        let old_status = state.status;
        let rules = self.catalog[actor.type_index]
            .battle_plan
            .as_ref()
            .expect("battle rules");
        let frames = match status {
            Status::Idle => rules.transition_idle_frames,
            Status::Unpacking | Status::Packing => rules.animation_frames[plan.index()],
            Status::Active => 0,
        };
        if status == Status::Active {
            self.set_army_battle_plan(id, Some(plan));
        } else if status == Status::Packing {
            self.set_army_battle_plan(id, None);
        }
        let state = self.entities[id].battle_plan.as_mut().expect("battle plan");
        state.status = status;
        if status != Status::Active {
            state.next_ready = self.tick + u64::from(frames);
        }
        state.animation = match status {
            Status::Unpacking | Status::Packing => Some((self.tick, frames, true)),
            _ => None,
        };
        if status == Status::Idle {
            state.current = None;
        }
        self.presentation.battle_plan_events.push(Event {
            frame: self.tick,
            object: id,
            plan,
            old_status,
            status,
        });
    }

    pub(super) fn expire_paralysis(&mut self) {
        for id in 0..self.entities.len() {
            if self.tick >= self.entities[id].battle_bonuses.paralyzed_until {
                self.entities[id].battle_bonuses.paralyzed_until = 0;
                self.observe_spy_vision_disabled(id);
            }
        }
    }
}

pub(super) fn validate(unit: &UnitType) -> Result<(), &'static str> {
    if unit.battle_plan.as_ref().is_some_and(|rules| {
        [
            rules.armor_scalar,
            rules.sight_scalar,
            rules.center_sight_scalar,
            rules.center_health_scalar,
        ]
        .into_iter()
        .any(|scalar| !scalar.is_finite() || scalar <= 0.0)
            || rules.valid_kind_of.is_empty()
    }) {
        return Err("Invalid battle plan source rules");
    }
    Ok(())
}
