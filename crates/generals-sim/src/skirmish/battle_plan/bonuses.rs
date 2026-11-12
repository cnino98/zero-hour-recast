use super::{Entity, Plan, Rules, Skirmish, UnitType};

pub(in crate::skirmish) struct Army {
    counts: [i32; 3],
    pub(super) mask: u8,
    armor: f32,
    sight: f32,
    pub(super) valid: Vec<String>,
    pub(super) invalid: Vec<String>,
}

impl Army {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            counts: [0; 3],
            mask: 0,
            armor: 1.0,
            sight: 1.0,
            valid: Vec::new(),
            invalid: Vec::new(),
        }
    }
}

pub(in crate::skirmish) struct Bonuses {
    pub(in crate::skirmish) mask: u8,
    pub(in crate::skirmish) armor: f32,
    pub(in crate::skirmish) sight: f32,
    pub(in crate::skirmish) paralyzed_until: u64,
}

impl Bonuses {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            mask: 0,
            armor: 1.0,
            sight: 1.0,
            paralyzed_until: 0,
        }
    }
}

pub(super) fn member(unit: &UnitType, valid: &[String], invalid: &[String]) -> bool {
    unit.kind_of.iter().any(|kind| valid.contains(kind))
        && !unit.kind_of.iter().any(|kind| invalid.contains(kind))
}

impl Entity {
    fn set_battle_plan_mask(&mut self, unit: &UnitType, mask: u8, frame: u64, extra: u32) {
        // Player's callback sets/clears each source condition in this order.
        for flag in [1, 2, 4] {
            if (self.battle_bonuses.mask ^ mask) & flag != 0 {
                self.battle_bonuses.mask ^= flag;
                self.weapon_bonus_changed(unit, frame, extra);
            }
        }
    }

    #[must_use]
    pub fn vision_range(&self, unit: &UnitType) -> f32 {
        if self.car_bomb.is_none()
            && let Some(sight) = self.crew.sight
        {
            return sight[0];
        }
        self.car_bomb
            .as_ref()
            .map_or(unit.ai.vision_range * self.battle_bonuses.sight, |state| {
                state.vision
            })
    }

    #[must_use]
    pub fn shroud_range(&self, unit: &UnitType) -> f32 {
        if let Some(range) = self.shroud_range_override {
            return range;
        }
        if self.car_bomb.is_none()
            && let Some(sight) = self.crew.sight
        {
            return sight[1];
        }
        self.car_bomb.as_ref().map_or(
            unit.sight.clearing_range * self.battle_bonuses.sight,
            |state| state.shroud,
        )
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn initialize_battle_bonuses(&mut self, id: usize) {
        let extra = self.passenger_bonus_flags(id);
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let army = &self.players[actor.owner].battle_plans;
        if member(unit, &army.valid, &army.invalid) {
            actor.battle_bonuses.armor *= army.armor;
            actor.battle_bonuses.sight *= army.sight;
            actor.set_battle_plan_mask(unit, army.mask, self.tick, extra);
        }
    }

    pub(in crate::skirmish) fn capture_battle_plan(&mut self, id: usize, old_owner: usize) {
        let extra = self.passenger_bonus_flags(id);
        let actor = &self.entities[id];
        let owner = actor.owner;
        let plan = actor.battle_plan.as_ref().and_then(|state| state.affected);
        // Player team membership removes old bonuses before adding new ones.
        let army = &self.players[old_owner].battle_plans;
        let unit = &self.catalog[actor.type_index];
        if member(unit, &army.valid, &army.invalid) {
            let actor = &mut self.entities[id];
            actor.battle_bonuses.armor *= 1.0 / army.armor.max(0.01);
            actor.battle_bonuses.sight *= 1.0 / army.sight.max(0.01);
            actor.set_battle_plan_mask(unit, 0, self.tick, extra);
        }
        self.initialize_battle_bonuses(id);
        if let Some(plan) = plan {
            self.change_battle_plan(id, old_owner, plan, -1);
            self.change_battle_plan(id, owner, plan, 1);
        }
    }

    pub(in crate::skirmish) fn delete_battle_plan(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        let owner = actor.owner;
        if let Some(plan) = actor
            .battle_plan
            .as_mut()
            .and_then(|state| state.affected.take())
        {
            self.change_battle_plan(id, owner, plan, -1);
        }
    }

    pub(super) fn set_army_battle_plan(&mut self, id: usize, plan: Option<Plan>) {
        let actor = &self.entities[id];
        let owner = actor.owner;
        let previous = actor.battle_plan.as_ref().and_then(|state| state.affected);
        if let Some(previous) = previous {
            self.change_battle_plan(id, owner, previous, -1);
            self.center_battle_bonus(id, previous, false);
        }
        if let Some(plan) = plan {
            self.center_battle_bonus(id, plan, true);
            self.change_battle_plan(id, owner, plan, 1);
        } else {
            let rules = self.catalog[self.entities[id].type_index]
                .battle_plan
                .as_ref()
                .expect("battle rules");
            let mut disabled_edges = Vec::new();
            for (other, actor) in self.entities.iter_mut().enumerate() {
                if actor.owner == owner
                    && member(
                        &self.catalog[actor.type_index],
                        &rules.valid_kind_of,
                        &rules.invalid_kind_of,
                    )
                {
                    actor.battle_bonuses.paralyzed_until = if rules.paralyze_frames == 0 {
                        0
                    } else {
                        self.tick + u64::from(rules.paralyze_frames)
                    };
                    disabled_edges.push(other);
                }
            }
            for other in disabled_edges {
                self.observe_spy_vision_disabled(other);
            }
        }
        self.entities[id]
            .battle_plan
            .as_mut()
            .expect("battle plan")
            .affected = plan;
    }

    fn center_battle_bonus(&mut self, id: usize, plan: Plan, enable: bool) {
        let actor = &mut self.entities[id];
        let rules = self.catalog[actor.type_index]
            .battle_plan
            .as_ref()
            .expect("battle rules");
        match plan {
            Plan::Bombardment => {}
            Plan::HoldTheLine => {
                let scalar = if enable {
                    rules.center_health_scalar
                } else {
                    1.0 / rules.center_health_scalar
                };
                // ActiveBody::setMaxHealth(PRESERVE_RATIO).
                let fraction = actor.health / actor.max_health;
                actor.max_health *= scalar;
                actor.health = actor.max_health * fraction;
                if self.catalog[actor.type_index].body_kind.is_active() {
                    actor.refresh_body_health();
                }
            }
            Plan::SearchAndDestroy => {
                actor.battle_bonuses.sight *= if enable {
                    rules.center_sight_scalar
                } else {
                    1.0 / rules.center_sight_scalar
                };
            }
        }
        if matches!(plan, Plan::SearchAndDestroy) && rules.center_detects_stealth {
            self.set_detection_enabled(id, enable);
        }
    }

    fn change_battle_plan(&mut self, center: usize, owner: usize, plan: Plan, delta: i32) {
        let rules = self.catalog[self.entities[center].type_index]
            .battle_plan
            .as_ref()
            .expect("battle rules");
        let army = &mut self.players[owner].battle_plans;
        let index = plan.index();
        army.counts[index] += delta;
        let add = army.counts[index] == 1 && delta == 1;
        let remove = army.counts[index] == 0 && delta == -1;
        if !add && !remove {
            return;
        }
        let (mut armor, mut sight) = scalars(rules, plan);
        if remove {
            armor = 1.0 / armor.max(0.01);
            sight = 1.0 / sight.max(0.01);
            army.mask &= !(1 << index);
        } else {
            army.mask |= 1 << index;
        }
        army.armor *= armor;
        army.sight *= sight;
        army.valid.clone_from(&rules.valid_kind_of);
        army.invalid.clone_from(&rules.invalid_kind_of);
        let mask = army.mask;
        for id in 0..self.entities.len() {
            let extra = self.passenger_bonus_flags(id);
            let actor = &mut self.entities[id];
            if actor.owner == owner
                && member(
                    &self.catalog[actor.type_index],
                    &rules.valid_kind_of,
                    &rules.invalid_kind_of,
                )
            {
                actor.battle_bonuses.armor *= armor;
                actor.battle_bonuses.sight *= sight;
                // SH 05/02/2026: preserve the other simultaneous plan flags.
                actor.set_battle_plan_mask(&self.catalog[actor.type_index], mask, self.tick, extra);
            }
        }
    }
}

const fn scalars(rules: &Rules, plan: Plan) -> (f32, f32) {
    match plan {
        Plan::Bombardment => (1.0, 1.0),
        Plan::HoldTheLine => (rules.armor_scalar, 1.0),
        Plan::SearchAndDestroy => (1.0, rules.sight_scalar),
    }
}
