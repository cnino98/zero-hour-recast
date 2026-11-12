//! Native fixture projection of the production registrations.
use super::{TemplateMetadata, UPDATE};
pub(super) use generals_sim::skirmish::module_updates::Helper;
pub(super) fn helper_name(helper: Helper) -> &'static str {
    match helper {
        Helper::StateMachine => "ObjectSMCHelper",
        Helper::StatusDamage => "StatusDamageHelper",
        Helper::SubdualDamage => "SubdualDamageHelper",
        Helper::Repulsor => "ObjectRepulsorHelper",
        Helper::Defection => "ObjectDefectionHelper",
        Helper::WeaponStatus => "ObjectWeaponStatusHelper",
        Helper::FiringTracker => "FiringTracker",
        Helper::TemporaryWeaponBonus => "TempWeaponBonusHelper",
    }
}

impl TemplateMetadata {
    /// Readable native fixture output, never parsed back into runtime registrations.
    pub fn update_registrations(&self, ai_present: bool, repulsors_enabled: bool) -> Vec<String> {
        let mut result: Vec<String> = self
            .helper_prefix(ai_present, repulsors_enabled)
            .iter()
            .map(|helper| helper_name(*helper).into())
            .collect();
        for entry in &self.modules[0] {
            if entry.interface_mask & UPDATE != 0 {
                result.push(format!("{}:{}", entry.family, entry.tag));
            }
        }
        result
    }
}
