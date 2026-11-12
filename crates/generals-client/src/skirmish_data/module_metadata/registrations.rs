//! `Object::init` helper prefix followed by actual ordered UPDATE declarations.
use super::{TemplateMetadata, UPDATE};
use generals_sim::skirmish::module_updates::{Helper, ModuleRegistration, ModuleSource};

impl TemplateMetadata {
    pub(in crate::skirmish_data) fn runtime_behaviors(
        &self,
        files: &[generals_assets::content::ContentFile],
    ) -> Result<Vec<generals_sim::skirmish::module_updates::BehaviorRegistration>, String> {
        self.modules[0]
            .iter()
            .map(|entry| {
                let mut upgrade = entry.upgrade.clone();
                if entry.interface_mask & 0x80 != 0 {
                    for name in upgrade
                        .triggers
                        .iter()
                        .chain(&upgrade.conflicts)
                        .chain(&upgrade.removals)
                    {
                        // UpgradeCenter::init creates these three OBJECT
                        // templates before INI loading; REGULAR has no upgrade.
                        if !matches!(
                            name.as_str(),
                            "Upgrade_Veterancy_VETERAN"
                                | "Upgrade_Veterancy_ELITE"
                                | "Upgrade_Veterancy_HEROIC"
                        ) {
                            super::super::definition(files, "Upgrade", name)
                                .map_err(|error| error.to_string())?;
                        }
                    }
                    // INI::parseFXList stores a null pointer for unresolved names.
                    if super::super::fx_definition(files, &upgrade.fx)
                        .map_err(|error| error.to_string())?
                        .is_none()
                    {
                        upgrade.fx.clear();
                    }
                }
                Ok(
                    generals_sim::skirmish::module_updates::BehaviorRegistration {
                        family: entry.family.clone(),
                        tag: entry.tag.clone(),
                        interfaces: entry.interface_mask,
                        upgrade,
                        source: ModuleSource {
                            path: entry.source.path.clone(),
                            origin: entry.source.origin.clone(),
                            line: entry.source.line,
                        },
                    },
                )
            })
            .collect()
    }

    pub(super) fn can_have_any_weapon(&self) -> bool {
        self.weapon_sets
            .iter()
            .any(|slots| slots.iter().any(|slot| *slot))
    }
    /// Native Object.cpp helper order. Constructor sleep and phases are separate contracts.
    pub(super) fn helper_prefix(&self, ai_present: bool, repulsors_enabled: bool) -> Vec<Helper> {
        let mut result = vec![Helper::StateMachine];
        if !self.modules[0]
            .iter()
            .any(|entry| entry.family == "InactiveBody")
        {
            result.extend([Helper::StatusDamage, Helper::SubdualDamage]);
        }
        if ai_present && repulsors_enabled && self.kind_of.contains("CAN_BE_REPULSED") {
            result.push(Helper::Repulsor);
        }
        if !self.kind_of.contains("SHRUBBERY") {
            result.push(Helper::Defection);
        }
        if self.can_have_any_weapon() {
            result.extend([
                Helper::WeaponStatus,
                Helper::FiringTracker,
                Helper::TemporaryWeaponBonus,
            ]);
        }
        result
    }

    pub(in crate::skirmish_data) fn runtime_registrations(
        &self,
        repulsors_enabled: bool,
    ) -> Vec<ModuleRegistration> {
        // Skirmish owns the native-equivalent global AI subsystem, irrespective
        // of whether this object's template has an AIUpdate module.
        let mut result: Vec<_> = self
            .helper_prefix(true, repulsors_enabled)
            .into_iter()
            .map(ModuleRegistration::Helper)
            .collect();
        result.extend(
            self.modules[0]
                .iter()
                .filter(|entry| entry.interface_mask & UPDATE != 0)
                .map(|entry| ModuleRegistration::Behavior {
                    family: entry.family.clone(),
                    tag: entry.tag.clone(),
                    source: ModuleSource {
                        path: entry.source.path.clone(),
                        origin: entry.source.origin.clone(),
                        line: entry.source.line,
                    },
                }),
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish_data::module_metadata;
    use generals_assets::{content, virtual_assets::Asset};

    fn behaviors(
        trigger: &str,
    ) -> Result<Vec<generals_sim::skirmish::module_updates::BehaviorRegistration>, String> {
        let text = format!(
            "Object DefaultThingTemplate\nEnd\nObject Unit\nKindOf=VEHICLE\nBehavior=AutoHealBehavior Heal\nTriggeredBy={trigger}\nEnd\nEnd\n"
        );
        let file = content::scan(Asset {
            path: "native-veterancy-upgrades.ini".into(),
            origin: "source fixture".into(),
            bytes: text.into_bytes(),
        })
        .unwrap();
        let metadata = module_metadata::load(std::slice::from_ref(&file), &[], &["Unit"]).unwrap();
        metadata["Unit"].runtime_behaviors(&[file])
    }

    #[test]
    fn source_upgrade_center_builtins_resolve_without_authored_upgrade_declarations() {
        let modules =
            behaviors("Upgrade_Veterancy_VETERAN Upgrade_Veterancy_ELITE Upgrade_Veterancy_HEROIC")
                .unwrap();
        let module = modules.iter().find(|module| module.tag == "Heal").unwrap();
        assert_eq!(
            module.upgrade.triggers,
            [
                "Upgrade_Veterancy_VETERAN",
                "Upgrade_Veterancy_ELITE",
                "Upgrade_Veterancy_HEROIC"
            ]
        );
        // Native init deliberately does not create the regular-level name.
        assert!(
            behaviors("Upgrade_Veterancy_REGULAR")
                .err()
                .expect("missing regular upgrade")
                .contains("missing Upgrade")
        );
        assert!(
            behaviors("Upgrade_Veterancy_ELITE_Missing")
                .err()
                .expect("unknown upgrade")
                .contains("missing Upgrade")
        );
    }
}
