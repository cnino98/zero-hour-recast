//! Each native `ProductionUpdate` owns its queue, IDs, doors and pending conditions.
use crate::skirmish::{Entity, Production, Skirmish, UnitType, parking};
use std::collections::VecDeque;

pub struct Rules {
    pub quantities: Vec<(String, u32)>,
    pub queue_limit: i32,
    pub disabled_types: u32,
    pub doors: parking::DoorRules,
}

pub(in crate::skirmish) struct State {
    queue: VecDeque<Production>,
    next_id: i32,
    doors: parking::doors::State,
    pending: Option<bool>,
}

impl State {
    const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            next_id: 1,
            doors: parking::doors::State::new(),
            pending: None,
        }
    }
}

impl UnitType {
    pub(in crate::skirmish) fn production_disabled_at(&self, ordinal: usize) -> u32 {
        if ordinal == 0 {
            self.production_disabled_types
        } else {
            self.additional_production[ordinal - 1].disabled_types
        }
    }

    pub(in crate::skirmish) fn production_doors_at(&self, ordinal: usize) -> &parking::DoorRules {
        if ordinal == 0 {
            &self.production_doors
        } else {
            &self.additional_production[ordinal - 1].doors
        }
    }
}

impl Entity {
    // Object::getProductionUpdateInterface returns the first authored module.
    // The existing public queue remains its backing storage; later modules have
    // independent concrete state and use the same ordinal callback paths.
    pub(in crate::skirmish) fn production_queue_at(&self, ordinal: usize) -> &VecDeque<Production> {
        if ordinal == 0 {
            &self.queue
        } else {
            &self.additional_production[ordinal - 1].queue
        }
    }

    pub(in crate::skirmish) fn production_queue_at_mut(
        &mut self,
        ordinal: usize,
    ) -> &mut VecDeque<Production> {
        if ordinal == 0 {
            &mut self.queue
        } else {
            &mut self.additional_production[ordinal - 1].queue
        }
    }

    pub(in crate::skirmish) fn production_pending_at(
        &mut self,
        ordinal: usize,
    ) -> &mut Option<bool> {
        if ordinal == 0 {
            &mut self.drawable.model_conditions.production_pending
        } else {
            &mut self.additional_production[ordinal - 1].pending
        }
    }

    pub(in crate::skirmish) fn production_doors_at_mut(
        &mut self,
        ordinal: usize,
    ) -> &mut parking::doors::State {
        if ordinal == 0 {
            &mut self.production_doors
        } else {
            &mut self.additional_production[ordinal - 1].doors
        }
    }

    pub(in crate::skirmish) fn production_next_id(&self, ordinal: usize) -> i32 {
        if ordinal == 0 {
            self.next_production_id
        } else {
            self.additional_production[ordinal - 1].next_id
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_production_instance(&mut self, id: usize, ordinal: usize) {
        if ordinal != 0 {
            assert_eq!(self.entities[id].additional_production.len(), ordinal - 1);
            self.entities[id].additional_production.push(State::new());
        }
    }
}

#[cfg(test)]
mod tests;
