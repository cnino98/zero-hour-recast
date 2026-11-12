//! Native construction/production cancellation, including partially produced batches.
use super::{Entity, Skirmish};

impl Skirmish {
    /// Cancels an owned foundation and refunds its current source build cost.
    /// # Errors
    /// Rejects ended matches, foreign/dead objects and completed construction.
    pub fn cancel_construction(
        &mut self,
        owner: usize,
        building: usize,
    ) -> Result<(), &'static str> {
        let entity = self.cancellation_target(owner, building)?;
        if entity.build_progress >= 1.0 || !self.catalog[entity.type_index].kind.is_building() {
            return Err("Only an unfinished building can be cancelled");
        }
        // GameLogic::onDozerCancelConstruct refunds calcCostToBuild before kill().
        // RECONSTRUCTING foundations were created for free by their hole.
        let refund = self.build_cost(owner, entity.type_index);
        if !self.entities[building].reconstructing() {
            self.refund(owner, refund);
        }
        self.entities[building].health = 0.0;
        if self.catalog[self.entities[building].type_index]
            .body_kind
            .is_active()
        {
            self.entities[building].refresh_body_health();
        }
        self.finish_death(building, None);
        Ok(())
    }

    /// Cancels the specified production ID, independent of its current queue slot.
    /// # Errors
    /// Rejects ended matches, foreign/dead producers and missing production IDs.
    pub fn cancel_production(
        &mut self,
        owner: usize,
        producer: usize,
        production_id: i32,
    ) -> Result<(), &'static str> {
        let entity = self.cancellation_target(owner, producer)?;
        let index = entity
            .queue
            .iter()
            .position(|entry| {
                entry.id == production_id && matches!(entry.item, super::ProductionItem::Unit(_))
            })
            .ok_or("Production entry no longer exists")?;
        self.cancel_unit_entry(producer, index)
    }

    pub(super) fn cancel_unit_entry(
        &mut self,
        producer: usize,
        index: usize,
    ) -> Result<(), &'static str> {
        self.cancel_unit_entry_at(producer, 0, index)
    }

    pub(in crate::skirmish) fn cancel_unit_entry_at(
        &mut self,
        producer: usize,
        ordinal: usize,
        index: usize,
    ) -> Result<(), &'static str> {
        let entity = &self.entities[producer];
        let owner = entity.owner;
        let refund = self.production_cost(producer, &entity.production_queue_at(ordinal)[index]);
        // SH 2bfaea60, !RETAIL_COMPATIBLE_CRC: a partially produced batch
        // cannot be cancelled for a refund after its first unit has exited.
        if entity.production_queue_at(ordinal)[index].produced > 0 {
            return Err("A batch that has started exiting cannot be cancelled");
        }
        // ProductionUpdate::cancelUnitCreate: refund before removing the entry.
        // No AI callback is issued. Its normal queue polling observes removal.
        self.refund(owner, refund);
        self.remove_production_entry(producer, ordinal, index);
        Ok(())
    }

    pub(in crate::skirmish) fn remove_production_entry(
        &mut self,
        producer: usize,
        ordinal: usize,
        index: usize,
    ) {
        let door = self.entities[producer].production_queue_at(ordinal)[index].parking_door;
        if let Some(parking) = &mut self.entities[producer].parking {
            parking.unreserve_door(door);
        }
        self.entities[producer]
            .production_queue_at_mut(ordinal)
            .remove(index);
        self.entities[producer].production_removed_at(ordinal);
    }

    fn cancellation_target(&self, owner: usize, id: usize) -> Result<&Entity, &'static str> {
        if self.end_game_at.is_some() {
            return Err("Match has ended; restart to play again");
        }
        self.entities
            .get(id)
            .filter(|entity| entity.alive() && entity.owner == owner)
            .ok_or("Object is dead, missing or controlled by another player")
    }

    pub(super) fn refund_production_on_death(&mut self, producer: usize) {
        // Commands (sale/defection/cancellation) use the first native interface.
        self.refund_production_instance(producer, 0);
    }

    pub(in crate::skirmish) fn refund_production_instance(
        &mut self,
        producer: usize,
        ordinal: usize,
    ) {
        // ProductionUpdate::onDie -> cancelAndRefundAllProduction removes an
        // uncancellable partial batch without refunding it under the SH profile.
        for _ in 0..100 {
            let Some(entry) = self.entities[producer].production_queue_at(ordinal).front() else {
                break;
            };
            let cancelled = match entry.item {
                super::ProductionItem::Unit(_) => self.cancel_unit_entry_at(producer, ordinal, 0),
                super::ProductionItem::Upgrade(_) => {
                    self.cancel_upgrade_entry(producer, ordinal, 0)
                }
            };
            if cancelled.is_err() {
                // CRC=0 removes a partial unit batch or player upgrade lacking
                // IN_PRODUCTION status without refunding or changing its owner.
                self.remove_production_entry(producer, ordinal, 0);
            }
        }
    }

    pub(in crate::skirmish) fn destroy_production_instance(
        &mut self,
        producer: usize,
        ordinal: usize,
    ) {
        // ~ProductionUpdate removes each entry (including its reserved door),
        // but does not run cancellation/refund callbacks.
        while !self.entities[producer]
            .production_queue_at(ordinal)
            .is_empty()
        {
            self.remove_production_entry(producer, ordinal, 0);
        }
    }

    fn refund(&mut self, owner: usize, amount: u32) {
        // Money::deposit(amount, TRUE, FALSE) does not track refund as income.
        // Source Money stores unsigned cash; retain its addition semantics.
        self.players[owner].cash = self.players[owner].cash.wrapping_add(amount);
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Kind, Order, tests::game};

    #[test]
    fn stale_queue_click_does_not_cancel_the_next_unit_or_refund_twice() {
        let mut game = game();
        // Energy::getEnergySupplyRatio is zero for a zero-supply fixture.
        // This test's authored 150-frame queue requires normal powered speed.
        game.catalog[0].energy = 1;
        game.update_power();
        let builder = game.type_for(0, Kind::Builder).unwrap();
        let cash = game.players[0].cash;
        let cost = game.catalog[builder].cost;
        game.train(0, builder).unwrap();
        let completed_id = game.entities[0].queue[0].id;
        game.train(0, builder).unwrap();
        let pending_id = game.entities[0].queue[1].id;
        for _ in 0..150 {
            game.step();
        }
        assert_eq!(game.entities[0].queue.len(), 1);
        assert!(game.cancel_production(0, 0, completed_id).is_err());
        assert!(game.cancel_production(1, 0, pending_id).is_err());
        assert_eq!(game.players[0].cash, cash - 2 * cost);
        game.cancel_production(0, 0, pending_id).unwrap();
        assert_eq!(game.players[0].cash, cash - cost);
        assert!(game.cancel_production(0, 0, pending_id).is_err());
        assert_eq!(game.players[0].statistics.money_earned, 0);
        game.train(0, builder).unwrap();
        assert!(game.entities[0].queue[0].id > pending_id);
    }

    #[test]
    fn unfinished_build_refunds_and_its_builder_observes_target_death() {
        let mut game = game();
        // Native Dozer task timestamps use zero as the absent-command sentinel.
        game.step();
        let supply = game.type_for(0, Kind::SupplyCenter).unwrap();
        let cash = game.players[0].cash;
        let spent = game.players[0].statistics.money_spent;
        let site = game.placement_near(supply, [185.0, 215.0]).unwrap();
        let building = game.build(1, supply, site).unwrap();
        for _ in 0..1800 {
            game.step();
            if game.entities[building].build_progress > 0.0 {
                break;
            }
        }
        assert!(game.entities[building].build_progress > 0.0);
        assert!(!game.entities[building].complete());
        assert!(game.cancel_construction(1, building).is_err());
        game.cancel_construction(0, building).unwrap();
        assert_eq!(game.players[0].cash, cash);
        assert_eq!(game.players[0].statistics.money_earned, 0);
        assert_eq!(game.players[0].statistics.money_spent, spent);
        assert_eq!(game.players[0].statistics.buildings_lost, 0);
        assert!(!game.entities[building].alive());
        assert!(game.cancel_construction(0, building).is_err());
        assert!(game.cancel_construction(0, 0).is_err());
        game.step();
        assert!(matches!(game.entities[1].ai.order, Order::Idle));
    }

    #[test]
    fn cancelling_a_later_entry_uses_current_cost_without_restarting_the_head() {
        let mut game = game();
        // Energy::getEnergySupplyRatio is zero for a zero-supply fixture.
        // This test's authored 150-frame queue requires normal powered speed.
        game.catalog[0].energy = 1;
        game.update_power();
        let builder = game.type_for(0, Kind::Builder).unwrap();
        game.train(0, builder).unwrap();
        game.train(0, builder).unwrap();
        for _ in 0..37 {
            game.step();
        }
        let elapsed = game.entities[0].queue[0].elapsed_frames;
        let id = game.entities[0].queue[1].id;
        let cash = game.players[0].cash;
        game.catalog[builder].cost = 250;
        game.cancel_production(0, 0, id).unwrap();
        assert_eq!(game.players[0].cash, cash + 250);
        assert!((game.entities[0].queue[0].elapsed_frames - elapsed).abs() < f32::EPSILON);
        for _ in 37..150 {
            game.step();
        }
        assert!(game.entities[0].queue.is_empty());
        assert_eq!(game.players[0].statistics.units_built, 2);
    }

    #[test]
    fn producer_death_refunds_remaining_queue_without_income_credit() {
        let mut game = game();
        let builder = game.type_for(0, Kind::Builder).unwrap();
        let cash = game.players[0].cash;
        game.train(0, builder).unwrap();
        game.train(0, builder).unwrap();
        let first = game.entities[0].queue[0].id;
        game.apply_hit(0, 2, game.entities[0].health);
        assert!(!game.entities[0].alive());
        assert!(game.entities[0].queue.is_empty());
        assert_eq!(game.players[0].cash, cash);
        assert_eq!(game.players[0].statistics.money_earned, 0);
        assert!(game.cancel_production(0, 0, first).is_err());
    }
}
