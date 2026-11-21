//! SH `SpecialPowerCompletionDie`, preserving source creator and notification order.
use super::{Skirmish, death_callbacks::Conditions, special_power::Trigger};

pub struct Rules {
    pub power: String,
    pub conditions: Conditions,
}

#[derive(Clone, Default)]
pub(in crate::skirmish) struct State {
    assigned: bool,
    creator: Option<usize>,
}

impl Skirmish {
    pub(in crate::skirmish) fn set_special_power_completion_creator(
        &mut self,
        id: usize,
        creator: Option<usize>,
    ) {
        // Object::findSpecialPowerCompletionDie returns the first matching tag.
        if let Some(state) = self.entities[id].special_power_completions.first_mut()
            && !state.assigned
        {
            state.assigned = true;
            state.creator = creator;
        }
    }

    pub(in crate::skirmish) fn transfer_special_power_completion(
        &mut self,
        source: usize,
        projectile: usize,
    ) {
        let owned_by_source = !self.entities[source].special_power_completions.is_empty();
        if owned_by_source {
            self.notify_special_power_completed(source, 0);
        }
        self.set_special_power_completion_creator(projectile, (!owned_by_source).then_some(source));
    }

    pub(in crate::skirmish) fn special_power_completion_on_die(
        &mut self,
        id: usize,
        ordinal: usize,
    ) {
        let rule = &self.catalog[self.entities[id].type_index]
            .behavior
            .special_power_completions[ordinal];
        let conditions = &rule.conditions;
        if self.death_conditions_match(
            id,
            conditions.death_mask,
            conditions.veterancy_mask,
            conditions.exempt_status,
            conditions.required_status,
        ) {
            self.notify_special_power_completed(id, ordinal);
        }
    }

    fn notify_special_power_completed(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        if let Some(creator) = actor.special_power_completions[ordinal].creator {
            self.completed_special_powers.push(Trigger {
                owner: actor.owner,
                power: self.catalog[actor.type_index]
                    .behavior
                    .special_power_completions[ordinal]
                    .power
                    .clone(),
                object: creator,
            });
        }
    }
}
