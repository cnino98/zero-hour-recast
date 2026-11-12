//! `TechBuildingBehavior` and player kind-of production cost changes.
use super::{ModelConditions, Skirmish};

pub struct ProductionCostModifier {
    pub kind_mask: u8,
    pub percentage: f32,
}

pub struct RepairDock {
    pub full_heal_frames: f32,
    pub approaches: usize,
    pub approach_bones: usize,
}

pub struct TechBuildingRules {
    pub pulse_fx: Option<String>,
    pub pulse_frames: u32,
}

pub struct NeutralTechRules {
    pub tech_buildings: Vec<TechBuildingRules>,
    pub repair_dock: Option<RepairDock>,
    pub cost_modifier: Option<ProductionCostModifier>,
}

impl NeutralTechRules {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            tech_buildings: Vec::new(),
            cost_modifier: None,
            repair_dock: None,
        }
    }
}

pub(super) fn validate(unit: &super::UnitType) -> Result<(), &'static str> {
    if let Some(rules) = &unit.neutral_tech.repair_dock
        && (!rules.full_heal_frames.is_finite()
            || rules.full_heal_frames <= 0.0
            || !(1..=64).contains(&rules.approaches)
            || rules.approach_bones > rules.approaches)
    {
        return Err("Invalid source repair dock timing or approach count");
    }
    Ok(())
}

pub struct ProductionCostChange {
    kind_mask: u8,
    percentage: f32,
    references: u32,
}

impl Skirmish {
    /// Identical kind/percentage providers share the source player reference count.
    pub(super) fn production_cost_multiplier(&self, owner: usize, product: usize) -> f32 {
        let flags = self.catalog[product].maintenance.healing_kind_flags;
        self.players[owner]
            .production_cost_changes
            .iter()
            .filter(|change| flags & change.kind_mask == change.kind_mask)
            .fold(1.0, |multiplier, change| {
                multiplier * (1.0 + change.percentage)
            })
    }

    pub(in crate::skirmish) fn apply_cost_modifier_upgrade(&mut self, id: usize) {
        let actor = &self.entities[id];
        let modifier = self.catalog[actor.type_index]
            .neutral_tech
            .cost_modifier
            .as_ref()
            .expect("CostModifierUpgrade data");
        add_cost_change(
            &mut self.players[actor.owner].production_cost_changes,
            modifier,
        );
    }

    pub(in crate::skirmish) fn capture_cost_modifier(
        &mut self,
        id: usize,
        index: usize,
        previous: usize,
    ) {
        let actor = &self.entities[id];
        let update_owner = actor.update_owner;
        if !self.update_objects[update_owner].upgrade_executed[index] {
            return;
        }
        let modifier = self.catalog[actor.type_index]
            .neutral_tech
            .cost_modifier
            .as_ref()
            .expect("CostModifierUpgrade data");
        remove_cost_change(
            &mut self.players[previous].production_cost_changes,
            modifier,
        );
        self.update_objects[update_owner].upgrade_executed[index] = false;
        add_cost_change(
            &mut self.players[actor.owner].production_cost_changes,
            modifier,
        );
        self.update_objects[update_owner].upgrade_executed[index] = true;
    }

    pub(in crate::skirmish) fn delete_cost_modifier(&mut self, id: usize, index: usize) {
        let actor = &self.entities[id];
        let update_owner = actor.update_owner;
        if !self.update_objects[update_owner].upgrade_executed[index] {
            return;
        }
        let modifier = self.catalog[actor.type_index]
            .neutral_tech
            .cost_modifier
            .as_ref()
            .expect("CostModifierUpgrade data");
        remove_cost_change(
            &mut self.players[actor.owner].production_cost_changes,
            modifier,
        );
        self.update_objects[update_owner].upgrade_executed[index] = false;
    }

    /// TechBuildingBehavior.cpp90: captured is a `PlayerTemplate` property, not
    /// occupancy, match defeat, object health or the `TECH_BUILDING` `KindOf` bit.
    pub(in crate::skirmish) fn update_tech_building(&mut self, id: usize, ordinal: usize) -> i32 {
        let owner = self.entities[id].owner;
        let captured = self.players[owner].playable_side;
        if self.entities[id].drawable.present {
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(ModelConditions::CAPTURED, captured);
        }
        let rules = &self.catalog[self.entities[id].type_index]
            .neutral_tech
            .tech_buildings[ordinal];
        if captured
            && rules.pulse_frames > 0
            && let Some(effect) = &rules.pulse_fx
        {
            let effect = effect.clone();
            let sleep = i32::from_ne_bytes(rules.pulse_frames.to_ne_bytes());
            self.object_fx(id, effect);
            sleep
        } else {
            i32::try_from(super::module_updates::FOREVER).expect("native forever sleep")
        }
    }

    pub(in crate::skirmish) fn tech_building_on_die(&mut self, id: usize) {
        if self.entities[id].drawable.present {
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(ModelConditions::CAPTURED, false);
        }
        self.transfer_maintenance_owner(id, self.neutral_owner());
    }
}

fn remove_cost_change(changes: &mut Vec<ProductionCostChange>, modifier: &ProductionCostModifier) {
    let index = changes
        .iter()
        .position(|change| {
            change.kind_mask == modifier.kind_mask
                && change.percentage.to_bits() == modifier.percentage.to_bits()
        })
        .expect("active refinery player cost modifier");
    changes[index].references -= 1;
    if changes[index].references == 0 {
        changes.remove(index);
    }
}

fn add_cost_change(changes: &mut Vec<ProductionCostChange>, modifier: &ProductionCostModifier) {
    if let Some(change) = changes.iter_mut().find(|change| {
        change.kind_mask == modifier.kind_mask
            && change.percentage.to_bits() == modifier.percentage.to_bits()
    }) {
        change.references += 1;
    } else {
        changes.push(ProductionCostChange {
            kind_mask: modifier.kind_mask,
            percentage: modifier.percentage,
            references: 1,
        });
    }
}

#[cfg(test)]
pub(in crate::skirmish) mod tests;
