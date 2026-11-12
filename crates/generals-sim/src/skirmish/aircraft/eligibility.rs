//! Aircraft `ActionManager` query adapters used by public selection and command admission.
use crate::skirmish::{Skirmish, ai_goals::COMMAND_FROM_PLAYER};
impl Skirmish {
    pub(in crate::skirmish) fn aircraft_can_enter(&self, id: usize, airfield: usize) -> bool {
        self.entities.get(id).is_some()
            && self.entities.get(airfield).is_some()
            && self.attack_kind(id, "AIRCRAFT")
            && self.attack_kind(airfield, "FS_AIRFIELD")
            && self.action_enter_admits(id, airfield, COMMAND_FROM_PLAYER, false)
    }
    pub(in crate::skirmish) fn aircraft_can_repair_at(&self, id: usize, airfield: usize) -> bool {
        self.entities.get(id).is_some()
            && self.attack_kind(id, "AIRCRAFT")
            && self.chinook_repair_admits(id, airfield, COMMAND_FROM_PLAYER)
    }
}
