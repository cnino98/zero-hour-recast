//! `VeterancyCrateCollide` and its inherited `CrateCollide`, SH 2bfaea60.
use super::Skirmish;

pub struct Restrictions {
    pub required: Vec<String>,
    pub forbidden: Vec<String>,
    pub forbid_owner: bool,
    pub building_pickup: bool,
    pub human_only: bool,
    pub science: Option<String>,
}

pub struct Rules {
    pub restrictions: Restrictions,
    pub adds_owner_veterancy: bool,
    pub is_pilot: bool,
    pub allow_multi_pickup: bool,
    pub execute_fx: String,
}

impl Skirmish {
    /// `ActionManager::canEnterObject` queries the actual collide interface
    /// before requiring the target to have any containment capacity.
    pub(in crate::skirmish) fn veterancy_crate_enterable(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(other)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        let unit = &self.catalog[actor.type_index];
        let victim = &self.catalog[other.type_index];
        id != target
            && actor.exists()
            && other.exists()
            && !actor.under_construction()
            && !other.under_construction()
            && !other.selling()
            && !unit.kind_of.iter().any(|flag| {
                matches!(
                    flag.as_str(),
                    "STRUCTURE" | "IMMOBILE" | "IGNORED_IN_GUI" | "MOB_NEXUS"
                )
            })
            && !victim.kind_of.iter().any(|flag| flag == "IGNORED_IN_GUI")
            && !other.subdued()
            && (self.computer_player(actor.owner)
                || matches!(
                    self.visibility_status(actor.owner, target),
                    super::visibility::Status::Clear | super::visibility::Status::Partial
                ))
            && unit
                .behavior
                .veterancy_crates
                .iter()
                .enumerate()
                .any(|(ordinal, _)| self.veterancy_crate_valid(id, ordinal, target))
    }

    fn veterancy_crate_levels(&self, id: usize, rules: &Rules) -> u8 {
        if rules.adds_owner_veterancy {
            self.entities[id].veterancy
        } else {
            1
        }
    }

    pub(in crate::skirmish) fn veterancy_crate_valid(
        &self,
        id: usize,
        ordinal: usize,
        target: usize,
    ) -> bool {
        let actor = &self.entities[id];
        let other = &self.entities[target];
        let rules = &self.catalog[actor.type_index].behavior.veterancy_crates[ordinal];
        let restriction = &rules.restrictions;
        let unit = &self.catalog[other.type_index];
        let building =
            restriction.building_pickup && unit.kind_of.iter().any(|kind| kind == "STRUCTURE");
        let levels = self.veterancy_crate_levels(id, rules);
        other.owner != self.neutral_owner()
            && (other.ai.registration.is_some() || building)
            && restriction
                .required
                .iter()
                .all(|kind| unit.kind_of.contains(kind))
            && !unit
                .kind_of
                .iter()
                .any(|kind| kind == "PARACHUTE" || restriction.forbidden.contains(kind))
            && !other.is_effectively_dead()
            && (building || self.height_above_ground(id) <= 0.0)
            && (rules.allow_multi_pickup
                || !self.update_objects[actor.update_owner].destroy_requested)
            && (!restriction.forbid_owner || actor.owner != other.owner)
            && (!restriction.human_only || !self.computer_player(other.owner))
            && restriction
                .science
                .as_ref()
                .is_none_or(|science| self.players[other.owner].has_science(science))
            && self.height_above_ground(target) <= -9.0 * self.gravity
            && levels > 0
            && unit.veterancy.trainable
            && other.veterancy < 3
            && (!rules.is_pilot
                || actor.owner == other.owner && !other.is_using_airborne_locomotor(unit))
    }

    pub(in crate::skirmish) fn veterancy_crate_contact(
        &mut self,
        id: usize,
        ordinal: usize,
        target: usize,
    ) {
        if !self.veterancy_crate_valid(id, ordinal, target)
            || self.entities[id].ai.registration.is_none()
            || self.entities[id].ai.command.goal_object != Some(target)
        {
            return;
        }
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .veterancy_crates[ordinal];
        let levels = self.veterancy_crate_levels(id, rules);
        let pilot = rules.is_pilot;
        let effect = rules.execute_fx.clone();
        self.gain_experience_levels(target, levels, !pilot);
        if pilot {
            self.transfer_crew_name(id, target);
        }
        self.object_fx(target, effect);
        // CrateCollide destroys directly: no Die or score callback is involved.
        self.destroy_maintenance_object(id);
    }
}

#[cfg(test)]
mod tests;
