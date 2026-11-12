//! `AIGroup::groupAttackObject` and its passenger command fan-out.
use super::Skirmish;
use crate::skirmish::Order;

impl Skirmish {
    #[must_use]
    pub fn container_can_attack(&self, carrier: usize, target: usize) -> bool {
        self.entities
            .get(target)
            .is_some_and(|victim| victim.alive() && !victim.masked())
            && self
                .passengers(carrier)
                .iter()
                .any(|&id| self.rider_can_attack(id, target, false))
    }

    fn rider_can_attack(&self, id: usize, target: usize, forced: bool) -> bool {
        self.entities[id].alive()
            && self.passenger_can_fire(id)
            && self.weapon_can_target_mode(id, target, forced)
            && self.container_attack_in_range(id, target)
    }

    pub(in crate::skirmish) fn order_container_attack(
        &mut self,
        carrier: usize,
        target: usize,
    ) -> bool {
        if !self.attack_relationship(carrier, target)
            || self
                .entities
                .get(target)
                .is_none_or(|victim| !victim.alive() || victim.masked())
        {
            return false;
        }
        let mut dispatched = false;
        for index in 0..self.passengers(carrier).len() {
            let id = self.passengers(carrier)[index];
            if self.rider_can_attack(id, target, self.entities[carrier].ai.attack_context.forced) {
                self.copy_attack_command(carrier, id);
                self.set_order(id, Order::Attack(target));
                dispatched = true;
            }
        }
        dispatched
    }

    pub(in crate::skirmish) fn stop_garrison(&mut self, carrier: usize) {
        if self.is_garrison(carrier) {
            for index in 0..self.passengers(carrier).len() {
                let id = self.passengers(carrier)[index];
                self.set_order(id, Order::Idle);
                self.clear_ai_goal(id);
            }
        }
    }
}
