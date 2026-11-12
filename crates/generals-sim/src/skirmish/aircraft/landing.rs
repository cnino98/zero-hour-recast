//! `JetAIUpdate::doLandingCommand` uses the partition's default search before reservation.
use crate::skirmish::Skirmish;
impl Skirmish {
    pub(in crate::skirmish::aircraft) fn aircraft_landing_position(
        &mut self,
        id: usize,
        airfield: usize,
    ) -> [f32; 3] {
        let origin = self.object_position(airfield);
        if self.attack_kind(id, "PRODUCED_AT_HELIPAD") {
            self.find_default_position_around(origin, self.object_radius(airfield) * 10.0, None)
                .unwrap_or(origin)
        } else {
            origin
        }
    }
}
