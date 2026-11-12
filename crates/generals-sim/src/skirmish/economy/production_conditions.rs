//! `ProductionUpdate` owns pending flags; all instances update the same Drawable.
use crate::skirmish::Entity;

impl Entity {
    pub(in crate::skirmish) fn production_added(&mut self) {
        self.production_added_at(0);
    }

    pub(in crate::skirmish) fn production_added_at(&mut self, ordinal: usize) {
        if !self.drawable.model_conditions.production {
            *self.production_pending_at(ordinal) = Some(true);
        }
    }

    pub(in crate::skirmish) fn production_removed_at(&mut self, ordinal: usize) {
        if self.production_queue_at(ordinal).is_empty() && self.drawable.model_conditions.production
        {
            *self.production_pending_at(ordinal) = Some(false);
        }
    }

    pub(in crate::skirmish) fn update_production_condition(&mut self, ordinal: usize) {
        // Deliver this module's flags before advancing its own queue.
        if let Some(active) = self.production_pending_at(ordinal).take() {
            self.drawable.model_conditions.production = active;
        }
    }
}

#[cfg(test)]
mod tests;
