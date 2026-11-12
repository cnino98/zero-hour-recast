//! Projectile detonation ORs its live conditions with the launcher's saved flags.
use super::super::{Entity, WeaponRules};

pub(in crate::skirmish) struct Damage {
    launch_flags: u32,
}

impl Entity {
    pub(in crate::skirmish) fn damage_snapshot(&self, extra: u32) -> Damage {
        let flags = self.weapon_bonus_flags() | extra;
        Damage {
            launch_flags: flags,
        }
    }
}

impl Damage {
    pub(in crate::skirmish) fn factor(
        &self,
        weapon: &WeaponRules,
        extra: u32,
        field: usize,
    ) -> f32 {
        let flags = self.launch_flags | extra;
        weapon.bonuses.compute(flags, field)
    }
}
