//! SH AIUpdate.cpp287–299 / TurretAI.cpp295–337,1283–1307.
use super::Skirmish;

impl Skirmish {
    /// Both turret default-state entries precede NORMAL locomotor construction.
    pub(in crate::skirmish) fn construct_ai_turret(&mut self, id: usize) {
        for which in 0..2 {
            if self.has_native_turret(id, which) {
                self.enter_native_turret_idle(id, which, true);
            }
        }
    }
}
