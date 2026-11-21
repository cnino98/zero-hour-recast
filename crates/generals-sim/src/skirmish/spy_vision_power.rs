//! Native `SpyVisionSpecialPower` extends the base no-target callback.
use super::Skirmish;

pub struct Rules {
    pub base_duration: u32,
    pub captured_bonus: u32,
    pub maximum: u32,
}

impl Skirmish {
    pub(in crate::skirmish) fn use_spy_vision_power(&mut self, source: usize, ordinal: usize) {
        let unit = &self.catalog[self.entities[source].type_index];
        if unit.behavior.spy_vision.is_empty() {
            return;
        }
        let rules = &unit.behavior.spy_vision_powers[ordinal];
        let actor = &self.entities[source];
        let mut duration = rules.base_duration;
        if unit.contain.is_some() {
            let passengers = u32::try_from(actor.containment.passengers.len())
                .expect("native containment count");
            duration = duration
                .wrapping_add(passengers.wrapping_mul(rules.captured_bonus))
                .min(rules.maximum);
        }
        self.activate_spy_vision(source, 0, duration);
    }
}
