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
        self.deselect_dead_object(id);
        let actor = &mut self.entities[id];
        actor.maintenance.selling = Some(self.tick);
        actor.maintenance.instance_flags |= super::INSTANCE_UNSELECTABLE;
        actor.construction_percent = 99.9;
        actor.build_progress = actor.construction_percent / 100.0;
        actor.drawable.model_conditions.construction = ConstructionState::Active;
        actor.maintenance.animation_duration = Some((self.tick, 45, false));
        self.refund_production_on_death(id);
        self.remove_garrison_contents(id);
        self.sell_tunnel(id);
        if self.is_heal_container(id) || self.entities[id].containment.cave.is_some() {
            // OpenContain::onSelling requests ordinary AI exits before sale.
            for rider in self.passengers(id).to_vec() {
                self.set_order(rider, Order::ExitContainer);
            }
        }
        if self.entities[id].ai.registration.is_some() {
            self.ai_idle_from_ai(id);
        }
        if self.entities[id].parking.is_some() {
            self.parking_death(id);
        }
        // GameLogic's object list visits the newest mines first.
        for mine in (0..self.entities.len()).rev() {
            if self.entities[mine].exists()
                && self.entities[mine].producer == Some(id)
                && self.attack_kind(mine, "MINE")
            {
                self.destroy_maintenance_object(mine);
            }
        }
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
            if !self.entities[id].exists() {
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
            self.deposit_sale_refund(owner, refund);
            self.refund_production_on_death(id);
            self.destroy_maintenance_object(id);
            self.entities[id].maintenance.selling = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn selling_retires_its_own_mines_without_triggering_death() {
        use crate::skirmish::{Faction, Kind, tests};

        let mut game = tests::game();
        let building = game.spawn(3, 0, [355.0, 205.0], true);
        let other = game.spawn(3, 0, [455.0, 205.0], true);
        let mut unit = tests::fixture_type(Faction::China, Kind::Vehicle);
        unit.kind_of.push("MINE".into());
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let mine = game.spawn(kind, 0, [355.0, 255.0], true);
        let foreign_mine = game.spawn(kind, 0, [455.0, 255.0], true);
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let non_mine = game.spawn(tank, 0, [355.0, 300.0], true);
        game.entities[mine].producer = Some(building);
        game.entities[foreign_mine].producer = Some(other);
        game.entities[non_mine].producer = Some(building);

        game.sell(0, building).unwrap();
        game.process_update_destruction();

        assert!(!game.entities[mine].exists());
        assert!(!game.entities[mine].is_effectively_dead());
        assert!(game.entities[foreign_mine].exists());
        assert!(game.entities[non_mine].exists());
    }

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
