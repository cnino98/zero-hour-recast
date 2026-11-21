//! Native `Object` disable mask and placement's `AI_BUSY`/`IS_USING_ABILITY` test.
use super::{Entity, Skirmish};
mod disabled;

impl Entity {
    pub(in crate::skirmish) const fn reconstructing(&self) -> bool {
        self.object_status & (1 << 21) != 0
    }

    /// Native `Object` `DISABLED_HELD` status, independent of the producer's lifetime.
    #[must_use]
    pub const fn disabled_held(&self) -> bool {
        self.disabled_status & (1 << 3) != 0
    }

    // Source findObjectByID retains dead objects until final destruction.
    #[must_use]
    pub const fn exists(&self) -> bool {
        !self.maintenance.deleted
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn set_disabled_held(&mut self, id: usize) {
        if !self.entities[id].disabled_held() {
            self.entities[id].disabled_status |= 1 << 3;
            self.observe_spy_vision_disabled(id);
        }
    }

    pub(in crate::skirmish) fn clear_disabled_held(&mut self, id: usize) {
        if self.entities[id].disabled_held() {
            self.entities[id].disabled_status &= !(1 << 3);
            self.observe_spy_vision_disabled(id);
        }
    }

    /// `BuildAssistant` rejects either `AI_BUSY` or `OBJECT_STATUS_IS_USING_ABILITY`.
    /// Ordinary AI movement, attack, construction and docking states are not busy.
    pub(in crate::skirmish) fn object_busy(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        actor.object_status & (1 << 24) != 0 || actor.laser_target().is_some() || actor.ai_busy()
    }

    /// Native DisabledTypes.h bit positions for admitted source producers.
    pub(in crate::skirmish) fn object_disabled_mask(&self, id: usize) -> u32 {
        let actor = &self.entities[id];
        actor.disabled_status
            | (u32::from(actor.paralyzed()) << 4)
            | (u32::from(actor.unmanned()) << 5)
            | (u32::from(actor.freefall_disabled()) << 7)
            | (u32::from(actor.subdued()) << 10)
    }

    /// `Object::isDisabled` tests any disabled-mask bit, including `HELD`.
    #[must_use]
    pub fn object_disabled(&self, id: usize) -> bool {
        self.object_disabled_mask(id) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Kind, Order, tests::game};

    #[test]
    fn ordinary_ai_orders_do_not_make_units_busy() {
        let mut game = game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let id = game.spawn(tank, 0, [200.0, 100.0], true);
        for order in [
            Order::Idle,
            Order::Move([300.0, 100.0]),
            Order::Attack(0),
            Order::Build(0),
            Order::Repair(0),
            Order::Enter(0),
        ] {
            game.entities[id].ai.order = order;
            assert!(!game.object_busy(id));
        }
    }

    #[test]
    fn disabled_mask_covers_independent_represented_causes() {
        let mut game = game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let id = game.spawn(tank, 0, [200.0, 100.0], true);
        assert!(!game.object_disabled(id));
        game.set_hacked_until(id, 100);
        assert!(game.object_disabled(id));
        game.clear_hacked_disable(id);
        game.entities[id].battle_bonuses.paralyzed_until = 100;
        assert!(game.object_disabled(id));
    }
    #[test]
    fn held_clear_is_not_reconstructed_from_containment_or_reference_counted() {
        let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.add_to_contain(rider, carrier);
        assert!(game.entities[rider].disabled_held());
        game.clear_disabled_held(rider);
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
        assert_eq!(game.object_disabled_mask(rider) & (1 << 3), 0);
        game.set_disabled_held(rider);
        game.set_disabled_held(rider);
        game.remove_passenger(rider, carrier);
        assert!(!game.entities[rider].disabled_held());
    }
}
