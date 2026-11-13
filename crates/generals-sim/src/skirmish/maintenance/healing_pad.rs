//! SH 2bfaea60 `ActionManager::canGetRepairedAt` and `RepairDockUpdate::action`.
use crate::skirmish::{Order, Skirmish, economy::DockTarget};

impl Skirmish {
    pub(in crate::skirmish) fn repair_pad_admits(&self, id: usize, pad: usize) -> bool {
        let (Some(actor), Some(target)) = (self.entities.get(id), self.entities.get(pad)) else {
            return false;
        };
        let unit = &self.catalog[actor.type_index];
        let structure = &self.catalog[target.type_index];
        self.allied(actor.owner, target.owner)
            && actor.alive()
            && unit.speed > 0.0
            && !actor.under_construction()
            && !target.under_construction()
            && !target.selling()
            && unit.maintenance.healing_kind_flags & 2 != 0
            && unit.maintenance.healing_kind_flags & 4 == 0
            && structure.kind_of.iter().any(|kind| kind == "REPAIR_PAD")
            && structure.neutral_tech.repair_dock.is_some()
            && actor.health.to_bits() != actor.max_health.to_bits()
            && self.maintenance_target_visible(actor.owner, pad)
    }

    pub(in crate::skirmish) fn repair_at_pad(
        &mut self,
        id: usize,
        pad: usize,
    ) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.repair_pad_admits(id, pad) {
            return Err("Vehicle cannot get repaired at this pad");
        }
        self.set_order(id, Order::RepairAt(pad));
        Ok(())
    }

    pub(in crate::skirmish) fn update_repair_pad(&mut self, id: usize, pad: usize) {
        let actor = &self.entities[id];
        let valid = self.entities.get(pad).is_some_and(|target| {
            target.complete()
                && self.allied(actor.owner, target.owner)
                && self.catalog[target.type_index]
                    .neutral_tech
                    .repair_dock
                    .is_some()
        });
        if !valid {
            self.set_order(id, Order::Idle);
            return;
        }
        self.update_docking(id, DockTarget::Repair(pad));
    }

    pub(in crate::skirmish) fn repair_dock_action(&mut self, id: usize, pad: usize) -> bool {
        let rules = self.catalog[self.entities[pad].type_index]
            .neutral_tech
            .repair_dock
            .as_ref()
            .expect("repair dock source module");
        if self.entities[pad].maintenance.repair_dock_last.is_none() {
            self.entities[pad].maintenance.repair_dock_last = Some(id);
            self.entities[pad].maintenance.repair_dock_health_per_frame =
                (self.entities[id].max_health - self.entities[id].health) / rules.full_heal_frames;
        }
        if self.entities[id].health >= self.entities[id].max_health {
            self.entities[pad].maintenance.repair_dock_last = None;
            return false;
        }
        let amount = self.entities[pad].maintenance.repair_dock_health_per_frame;
        self.apply_healing(id, Some(pad), amount);
        let drone = self.entities[id]
            .ai
            .docking
            .as_ref()
            .and_then(|dock| dock.repair_drone)
            .filter(|&drone| self.entities[drone].exists());
        if let Some(drone) = drone {
            // RepairDockUpdate reuses its healing source and submits maxHealth,
            // not the missing-health amount; the body applies HEALING armor.
            self.apply_healing(drone, Some(pad), self.entities[drone].max_health);
        }
        true
    }
}

#[cfg(test)]
mod tests;
