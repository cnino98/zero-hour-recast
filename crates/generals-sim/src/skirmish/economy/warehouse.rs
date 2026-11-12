//! Native `SupplyWarehouseDockUpdate` stock and `SupplyCenterDockUpdate` transactions.
use super::supply_distance_squared;
use crate::skirmish::{Skirmish, Supply, object_sounds};

impl Skirmish {
    pub(in crate::skirmish) fn construct_warehouse_module(&mut self, id: usize) {
        self.construct_dock_module(id);
        let rules = self.catalog[self.entities[id].type_index]
            .supply
            .warehouse
            .as_ref()
            .expect("Warehouse module data");
        self.supplies.push(Supply {
            object: id,
            boxes: rules.starting_boxes,
        });
    }

    pub(in crate::skirmish) fn created_supply_warehouse(&mut self, id: usize) {
        for player in self.players.iter_mut().rev() {
            player.supply_warehouses.push(id);
        }
    }

    /// Stable warehouse-module index used by map scripts and selection.
    #[must_use]
    pub fn supply_index(&self, id: usize) -> Option<usize> {
        self.supplies.iter().position(|source| source.object == id)
    }

    #[must_use]
    pub fn supply_position(&self, source: usize) -> [f32; 2] {
        self.entities[self.supplies[source].object].position
    }

    pub(in crate::skirmish) fn native_warehouse_action(
        &mut self,
        source: usize,
        id: usize,
    ) -> bool {
        if self.supplies[source].boxes == 0 {
            return false;
        }
        let object = self.supplies[source].object;
        let radius = self.object_radius(id);
        let radii = radius + self.object_radius(object);
        let delta = [
            self.entities[id].position[0] - self.entities[object].position[0],
            self.entities[id].position[1] - self.entities[object].position[1],
        ];
        let squared = delta.map(|coordinate| coordinate * coordinate);
        let separation = ((squared[0] + squared[1]).sqrt() - radii).max(0.0);
        let diameter = radius * 2.0;
        if separation * separation > diameter * diameter {
            let mut position = self.object_position(id);
            position[0] += crate::skirmish::startup::random_real(&mut self.random, -4.0, 4.0);
            position[1] += crate::skirmish::startup::random_real(&mut self.random, -4.0, 4.0);
            self.set_object_position(id, position);
            return false;
        }
        self.supplies[source].boxes -= 1;
        if !self.gain_supply_box(id, self.supplies[source].boxes) {
            self.supplies[source].boxes += 1;
            return false;
        }
        if self.supplies[source].boxes == 0
            && self.catalog[self.entities[object].type_index]
                .supply
                .warehouse
                .as_ref()
                .expect("Warehouse data")
                .delete_when_empty
        {
            self.destroy_maintenance_object(object);
            return false;
        }
        true
    }

    fn gain_supply_box(&mut self, id: usize, remaining: u32) -> bool {
        if !self.entities[id].ai.supply.constructed()
            || self.entities[id].cargo >= self.catalog[self.entities[id].type_index].supply.capacity
        {
            return false;
        }
        self.entities[id].cargo += 1;
        if remaining == 0 {
            let depleted = self.best_supply(id).is_none_or(|source| {
                let range = self.supply_scan_distance(id) / 4.0;
                supply_distance_squared(
                    self.object_position(id),
                    self.object_position(self.supplies[source].object),
                )
                .sqrt()
                    > range
            });
            if depleted {
                let sound = self.catalog[self.entities[id].type_index]
                    .supply
                    .depleted_voice
                    .clone();
                self.object_sound(id, sound, object_sounds::Action::Play);
            }
        }
        true
    }

    pub(in crate::skirmish) fn supply_scan_distance(&self, id: usize) -> f32 {
        let actor = &self.entities[id];
        let distance = self.catalog[actor.type_index].supply.scan_distance;
        if self.computer_player(actor.owner) {
            distance * 2.0
        } else {
            distance
        }
    }

    fn supply_income_feedback(&mut self, center: usize, id: usize, value: u32) {
        let visible_players = self
            .players
            .iter()
            .enumerate()
            .fold(0, |mask, (viewer, _)| {
                if self.logically_visible(viewer, center) {
                    mask | (1 << viewer)
                } else {
                    mask
                }
            });
        let [x, y] = self.entities[id].position;
        self.presentation
            .deposit_events
            .push(crate::skirmish::DepositEvent {
                object: center,
                owner: self.entities[center].owner,
                frame: self.tick,
                amount: value.cast_signed(),
                position: [x, y, self.ground_height([x, y])],
                radii: None,
                visible_players,
            });
    }

    pub(in crate::skirmish) fn native_supply_center_action(
        &mut self,
        center: usize,
        id: usize,
    ) -> bool {
        if !self.entities[id].ai.supply.constructed() {
            return false;
        }
        let cargo = self.entities[id].cargo;
        let capacity = self.catalog[self.entities[id].type_index].supply.capacity;
        let boost = (self.upgraded_collector_boost(id).wrapping_mul(cargo))
            .checked_div(capacity)
            .unwrap_or(0);
        let owner = self.entities[center].owner;
        let value = cargo
            .wrapping_mul(self.rules.supply_value)
            .wrapping_add(boost);
        self.entities[id].cargo = 0;
        if value > 0 {
            self.deposit_native_money(owner, value);
            self.players[owner].statistics.money_earned = self.players[owner]
                .statistics
                .money_earned
                .wrapping_add(value);
            let frames = self.catalog[self.entities[center].type_index]
                .supply
                .dock
                .as_ref()
                .expect("Center data")
                .temporary_stealth_frames;
            if frames > 0 && self.entities[center].stealthed() {
                self.receive_temporary_stealth(id, frames);
            }
            self.supply_income_feedback(center, id, value);
        }
        false
    }
}
