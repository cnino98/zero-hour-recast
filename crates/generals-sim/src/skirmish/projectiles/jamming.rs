//! Native `ProjectileUpdateInterface` jam notification after common body subdual.
use super::{Control, FlightRules};
use crate::skirmish::{Entity, Projectile, Skirmish};

impl Entity {
    #[must_use]
    pub fn projectile_jammed(&self) -> bool {
        self.projectile_flight
            .as_ref()
            .is_some_and(|flight| flight.jammed)
    }
}

impl Projectile {
    #[must_use]
    pub fn jammed(&self, entities: &[Entity]) -> bool {
        self.object
            .is_some_and(|id| entities[id].projectile_jammed())
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn jam_projectile_object(&mut self, id: usize) {
        let Some(flight) = &self.entities[id].projectile_flight else {
            return;
        };
        let Some(Control::Missile(missile)) = flight.controls.first() else {
            return;
        };
        let target = self
            .projectile_component(id)
            .and_then(|index| self.projectiles[index].target)
            .filter(|&target| self.entities[target].exists())
            .map(|target| self.object_position(target));
        let mut destination = missile.jam_destination(target);
        let FlightRules::Missile(rules) =
            &self.catalog[self.entities[id].type_index].projectile_rules[0]
        else {
            unreachable!()
        };
        let scatter = rules.jam_scatter;
        destination[0] +=
            crate::skirmish::startup::random_real(&mut self.random, -scatter, scatter);
        destination[1] +=
            crate::skirmish::startup::random_real(&mut self.random, -scatter, scatter);
        let xy = [destination[0], destination[1]];
        let ground = self.ground_height(xy);
        let layer =
            self.movement_terrain
                .get_highest_layer_for_destination(destination, ground, false);
        destination[2] = self
            .movement_terrain
            .destination_layer_height(layer, xy, ground);
        if let Some(index) = self.projectile_component(id) {
            self.projectiles[index].target = None;
            self.projectiles[index].destination = xy;
        }
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("missile object");
        flight.jammed = true;
        let Control::Missile(missile) = &mut flight.controls[0] else {
            unreachable!()
        };
        missile.jam(destination);
    }
}

#[cfg(test)]
mod tests;
