use super::super::{ConstructionState, Order, Skirmish};

impl Skirmish {
    #[must_use]
    pub fn can_sell(&self, owner: usize, id: usize) -> bool {
        self.entities.get(id).is_some_and(|entity| {
            let unit = &self.catalog[entity.type_index];
            entity.owner == owner
                && entity.complete()
                && !entity.masked()
                && unit.kind.is_building()
                && entity.maintenance.instance_flags & super::INSTANCE_UNSELLABLE == 0
                && !unit
                    .maintenance
                    .has_kind(super::MaintenanceRules::UNSELLABLE)
                && unit.maintenance.rebuild.is_empty()
        })
    }

    /// # Errors
    /// Rejects foreign, unfinished, unsellable or already sold structures.
    pub fn sell(&mut self, owner: usize, id: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_sell(owner, id) {
            return Err("This structure cannot be sold");
        }
        self.set_order(id, Order::Idle);
        self.refund_production_on_death(id);
        self.remove_garrison_contents(id);
        self.sell_tunnel(id);
        if self.is_heal_container(id) || self.entities[id].containment.cave.is_some() {
            // OpenContain::onSelling requests ordinary AI exits before sale.
            for rider in self.passengers(id).to_vec() {
                self.set_order(rider, Order::ExitContainer);
            }
        }
        let actor = &mut self.entities[id];
        actor.maintenance.selling = Some(self.tick);
        actor.maintenance.instance_flags |= super::INSTANCE_UNSELECTABLE;
        actor.construction_percent = 99.9;
        actor.build_progress = actor.construction_percent / 100.0;
        actor.drawable.model_conditions.construction = ConstructionState::Active;
        Ok(())
    }

    pub(in crate::skirmish) fn update_sales(&mut self) {
        // BuildAssistant::sellObject prepends entries; newest objects update first.
        let mut sales: Vec<_> = self
            .entities
            .iter()
            .enumerate()
            .filter_map(|(id, entity)| entity.maintenance.selling.map(|frame| (frame, id)))
            .collect();
        sales.sort_unstable_by(|a, b| b.cmp(a));
        for (frame, id) in sales {
            if !self.entities[id].alive() {
                self.entities[id].maintenance.selling = None;
                continue;
            }
            if self.tick - frame < 45 {
                continue;
            }
            let entity = &mut self.entities[id];
            let previous = entity.construction_percent;
            entity.construction_percent -= 100.0 / 90.0;
            entity.build_progress = entity.construction_percent / 100.0;
            if entity.construction_percent <= 0.0 {
                if previous > 0.0 {
                    entity.maintenance.animation_duration = Some((self.tick, 45, false));
                }
                entity.drawable.model_conditions.construction = ConstructionState::Complete;
            }
            if entity.construction_percent > -50.0 {
                continue;
            }
            let owner = entity.owner;
            let type_index = entity.type_index;
            let unit = &self.catalog[type_index];
            let refund = if unit.maintenance.refund == 0 {
                let cost =
                    super::super::progression::integer_real(self.build_cost(owner, type_index));
                super::super::progression::real_unsigned(cost * unit.maintenance.sell_fraction)
            } else {
                unit.maintenance.refund
            };
            self.players[owner].cash = self.players[owner].cash.wrapping_add(refund);
            self.destroy_maintenance_object(id);
            self.entities[id].maintenance.selling = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn authored_unsellable_property_survives_capture_without_changing_selection() {
        let mut game = crate::skirmish::tests::game();
        let building = game.spawn(3, 0, [355.0, 205.0], true);
        assert!(game.can_sell(0, building));
        game.set_map_unsellable(building, true);
        assert!(!game.can_sell(0, building));
        assert!(
            game.entities[building].selectable(&game.catalog[game.entities[building].type_index])
        );
        game.transfer_maintenance_owner(building, 1);
        assert!(!game.can_sell(1, building));
        assert!(
            game.entities[building].selectable(&game.catalog[game.entities[building].type_index])
        );
        game.set_map_unsellable(building, false);
        assert!(game.can_sell(1, building));
        game.sell(1, building).unwrap();
        assert!(
            !game.entities[building].selectable(&game.catalog[game.entities[building].type_index])
        );
    }
}
