//! SH 2bfaea60 `DozerPrimaryIdleState::update` / `findObjectToRepair`.

use super::super::{Kind, Order, Skirmish, UnitType, movement::distance_squared};
use num_traits::ToPrimitive;

pub struct IdleRepair {
    pub frames: f32,
    pub range: f32,
}

pub(super) fn validate(unit: &UnitType) -> Result<(), &'static str> {
    if let Some(rules) = &unit.maintenance.idle_repair
        && (unit.kind != Kind::Builder
            || [rules.frames, rules.range]
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0))
    {
        return Err("Invalid original idle builder repair rules");
    }
    Ok(())
}

impl Skirmish {
    pub(in crate::skirmish) fn update_idle_repair(&mut self, id: usize) {
        let actor = &self.entities[id];
        let Some(rules) = &self.catalog[actor.type_index].maintenance.idle_repair else {
            return;
        };
        if !matches!(actor.ai.order, Order::Idle) || actor.ai.blocking.temporary_goal().is_some() {
            self.entities[id].maintenance.idle_since = self.tick;
            return;
        }
        let elapsed = (self.tick - actor.maintenance.idle_since)
            .to_f32()
            .expect("bounded simulation frame count");
        if elapsed <= rules.frames {
            return;
        }
        self.entities[id].maintenance.idle_since = self.tick;
        if let Some(target) = self.idle_repair_target(id) {
            // privateRepair rejects an already active benefactor after the
            // closest-object scan; it does not fall back to the next building.
            self.begin_repair(id, target, true).ok();
        } else {
            // This fallback uses the authored bored range without the computer
            // player's repair-only radius multiplier.
            let range = self.catalog[self.entities[id].type_index]
                .maintenance
                .idle_repair
                .as_ref()
                .expect("builder idle state")
                .range;
            self.bored_disarm(id, range);
        }
    }

    pub(in crate::skirmish) fn idle_repair_target(&self, id: usize) -> Option<usize> {
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .maintenance
            .idle_repair
            .as_ref()
            .expect("builder idle state");
        let mut range = rules.range;
        if self.computer_player(actor.owner) {
            range *= self.ai_rules.dozer_bored_radius_modifier;
        }
        let mut best = None;
        let mut closest = range * range;
        // getClosestObjects uses outward COI order; SimpleObjectIterator::insert
        // prepends it. Equal distances retain its first ITER_FASTEST candidate.
        for target in self
            .partition
            .outward_range(actor.position, range)
            .into_iter()
            .rev()
        {
            let candidate = &self.entities[target];
            if candidate.owner != actor.owner || !self.repair_target(id, target, true) {
                continue;
            }
            let distance = distance_squared(actor.position, candidate.position);
            if distance < range * range && (best.is_none() || distance < closest) {
                best = Some(target);
                closest = distance;
            }
        }
        best
    }
}

#[cfg(test)]
mod tests;
