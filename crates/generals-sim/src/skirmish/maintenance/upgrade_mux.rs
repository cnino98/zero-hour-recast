//! `Object::updateUpgradeModules` and `UpgradeMux`, SH 2bfaea60.
use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn give_self_upgrade_instance(
        &mut self,
        id: usize,
        family: &str,
        ordinal: usize,
    ) {
        let index = self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.family == family)
            .nth(ordinal)
            .map(|(index, _)| index)
            .expect("authored UpgradeMux instance");
        self.give_self_upgrade(id, index);
    }

    pub(in crate::skirmish) fn give_self_upgrade(&mut self, id: usize, index: usize) {
        let modules = &self.catalog[self.entities[id].type_index].behavior_modules;
        let module = &modules[index];
        let effect = module.upgrade.fx.clone();
        let removals = module.upgrade.removals.clone();
        let family = module.family.clone();
        let ordinal = modules[..index]
            .iter()
            .filter(|module| module.family == family)
            .count();
        self.object_fx(id, effect);
        for name in removals {
            self.remove_native_object_upgrade(id, &name);
        }
        self.apply_native_upgrade(id, index, ordinal);
        self.update_objects[self.entities[id].update_owner].upgrade_executed[index] = true;
    }

    pub(in crate::skirmish) fn native_upgrade_executed(
        &self,
        id: usize,
        family: &str,
        ordinal: usize,
    ) -> bool {
        self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.family == family)
            .nth(ordinal)
            .is_some_and(|(index, _)| {
                self.update_objects[self.entities[id].update_owner].upgrade_executed[index]
            })
    }

    pub(in crate::skirmish) fn activate_upgrade_modules(&mut self, id: usize) {
        let actor = &self.entities[id];
        if actor.under_construction()
            || !actor.exists()
            || self.update_objects[actor.update_owner].destroy_requested
        {
            return;
        }
        let mut key_mask = self.players[actor.owner].upgrades.clone();
        key_mask.extend(actor.maintenance.upgrades.iter().cloned());
        for (level, name) in [
            (1, "Upgrade_Veterancy_VETERAN"),
            (2, "Upgrade_Veterancy_ELITE"),
            (3, "Upgrade_Veterancy_HEROIC"),
        ] {
            if actor.veterancy_upgrades & (1 << level) != 0 {
                key_mask.push(name.into());
            }
        }
        let kind = actor.type_index;
        for index in 0..self.catalog[kind].behavior_modules.len() {
            let module = &self.catalog[kind].behavior_modules[index];
            if module.interfaces & 0x80 == 0
                || self.update_objects[self.entities[id].update_owner].upgrade_executed[index]
            {
                continue;
            }
            let data = &module.upgrade;
            let has = |name: &String| key_mask.contains(name);
            if !data.triggers.is_empty()
                && !data.conflicts.iter().any(has)
                && (if data.requires_all {
                    data.triggers.iter().all(has)
                } else {
                    data.triggers.iter().any(has)
                })
            {
                self.give_self_upgrade(id, index);
            }
        }
    }

    fn apply_native_upgrade(&mut self, id: usize, index: usize, ordinal: usize) {
        let kind = self.entities[id].type_index;
        match self.catalog[kind].behavior_modules[index].family.as_str() {
            "AutoHealBehavior" => self.apply_auto_heal_upgrade(id, ordinal),
            "GenerateMinefieldBehavior" => self.generate_minefield(id, ordinal),
            "FireWeaponWhenDamagedBehavior" => self.apply_damage_weapon_upgrade(id, ordinal),
            "FireWeaponWhenDeadBehavior" => self.apply_death_weapon_upgrade(id, ordinal),
            "FXListDie" => self.apply_death_fx_upgrade(id, ordinal),
            "CountermeasuresBehavior" => self.apply_countermeasure_upgrade(id, ordinal),
            "RadarUpgrade" => self.apply_radar_upgrade(id, ordinal),
            "CostModifierUpgrade" => self.apply_cost_modifier_upgrade(id),
            "SpyVisionUpdate" => self.apply_spy_vision_upgrade(id, ordinal),
            "PassengersFireUpgrade" => self.apply_passenger_fire_upgrade(id),
            "SubObjectsUpgrade" => self.apply_sub_object_upgrade(id, index, ordinal),
            "WeaponSetUpgrade" => self.apply_weapon_set_upgrade(id),
            "GrantScienceUpgrade" => {
                if let Some(science) = self.catalog[kind].behavior.granted_sciences[ordinal].clone()
                {
                    self.grant_native_science(self.entities[id].owner, &science);
                }
            }
            "UnpauseSpecialPowerUpgrade" => self.apply_unpause_special_power_upgrade(id, ordinal),
            _ => self.apply_maintenance_upgrade(id, index, ordinal),
        }
    }
}

pub(super) fn validate(unit: &crate::skirmish::UnitType) -> Result<(), String> {
    for module in &unit.behavior_modules {
        if module.interfaces & 0x80 == 0 {
            continue;
        }
        let count = unit
            .behavior_modules
            .iter()
            .filter(|other| other.family == module.family)
            .count();
        let expected = match module.family.as_str() {
            "AutoHealBehavior" => Some(unit.maintenance.auto_heal.len()),
            "GenerateMinefieldBehavior" => Some(unit.behavior.mine_generators.len()),
            "FireWeaponWhenDamagedBehavior" => Some(unit.behavior.damage_weapons.len()),
            "FireWeaponWhenDeadBehavior" => Some(unit.behavior.death_weapons.len()),
            "FXListDie" => Some(unit.behavior.death_fx.len()),
            "CountermeasuresBehavior" => Some(unit.behavior.countermeasures.len()),
            "RadarUpgrade" => Some(unit.behavior.radar_upgrades.len()),
            "CostModifierUpgrade" => Some(usize::from(unit.neutral_tech.cost_modifier.is_some())),
            "SpyVisionUpdate" => Some(unit.behavior.spy_vision.len()),
            "CommandSetUpgrade" => Some(unit.maintenance.command_sets.len()),
            "ExperienceScalarUpgrade" => Some(unit.maintenance.experience_upgrades.len()),
            "MaxHealthUpgrade" => Some(unit.maintenance.health_upgrades.len()),
            "ModelConditionUpgrade" => Some(unit.maintenance.condition_upgrades.len()),
            "SubObjectsUpgrade" => Some(unit.behavior.sub_object_upgrades.len()),
            "ObjectCreationUpgrade" => Some(unit.behavior.object_creation.len()),
            "GrantScienceUpgrade" => Some(unit.behavior.granted_sciences.len()),
            "ReplaceObjectUpgrade" => Some(unit.behavior.replacement_names.len()),
            "UnpauseSpecialPowerUpgrade" => Some(unit.unpause_special_powers.len()),
            "PowerPlantUpgrade" if unit.maintenance.power_upgrade.is_some() => Some(count),
            "LocomotorSetUpgrade" if unit.maintenance.movement_upgrade.is_some() => Some(count),
            "StealthUpgrade" if unit.behavior.stealth.is_some() => Some(count),
            "ArmorUpgrade" if unit.maintenance.armor_upgrade.is_some() => Some(count),
            "WeaponBonusUpgrade" | "WeaponSetUpgrade" | "PassengersFireUpgrade" => Some(count),
            _ => None,
        };
        if expected != Some(count) {
            return Err(format!(
                "{}: unbound or mismatched UPGRADE implementation {} {} at {}:{} ({})",
                unit.name,
                module.family,
                module.tag,
                module.source.path,
                module.source.line,
                module.source.origin
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{ExperienceUpgrade, Faction, Kind, tests};

    #[test]
    fn science_upgrade_grants_without_purchase_and_notifies_scripts_only_for_new_science() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::Gla, Kind::CommandCenter);
        unit.behavior.granted_sciences = vec![Some("SCIENCE_MOAB".into()), None];
        for _ in 0..2 {
            tests::register_behavior(&mut unit, "GrantScienceUpgrade", 0x80);
            unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Unlock".into()];
        }
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let object = game.spawn(kind, 0, [350.0, 250.0], true);
        let cash = game.players[0].cash;
        let points = game.players[0].science_points;
        game.grant_native_object_upgrade(object, "Unlock");
        assert!(game.players[0].has_science("SCIENCE_MOAB"));
        assert_eq!(game.players[0].acquired_sciences, ["SCIENCE_MOAB"]);
        assert_eq!(game.players[0].cash, cash);
        assert_eq!(game.players[0].science_points, points);
        assert!(game.native_upgrade_executed(object, "GrantScienceUpgrade", 1));
        game.remove_native_object_upgrade(object, "Unlock");
        game.grant_native_object_upgrade(object, "Unlock");
        assert_eq!(game.players[0].acquired_sciences, ["SCIENCE_MOAB"]);
    }

    #[test]
    fn authored_mux_keeps_one_key_snapshot_then_reset_preserves_effects() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::Usa, Kind::Barracks);
        for (scalar, effect) in [(1.0, "FirstFx"), (2.0, "SecondFx")] {
            unit.maintenance
                .experience_upgrades
                .push(ExperienceUpgrade {
                    trigger: "Shared".into(),
                    scalar,
                });
            tests::register_behavior(&mut unit, "ExperienceScalarUpgrade", 0x80);
            let data = &mut unit.behavior_modules.last_mut().unwrap().upgrade;
            data.triggers = vec!["Shared".into()];
            data.fx = effect.into();
        }
        unit.behavior_modules
            .iter_mut()
            .find(|module| module.family == "ExperienceScalarUpgrade")
            .unwrap()
            .upgrade
            .removals = vec!["Shared".into()];
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [350.0, 250.0], true);
        let first_fx = game.presentation.object_fx_events.len();
        game.grant_native_object_upgrade(id, "Shared");
        // The first callback removes the key, but the second callback still sees
        // Object::updateUpgradeModules' copied mask and executes in authored order.
        assert!(!game.object_has_upgrade(id, "Shared"));
        assert!(game.native_upgrade_executed(id, "ExperienceScalarUpgrade", 0));
        assert!(game.native_upgrade_executed(id, "ExperienceScalarUpgrade", 1));
        assert_eq!(
            game.entities[id].experience_scalar().to_bits(),
            4.0_f32.to_bits()
        );
        let effects: Vec<_> = game.presentation.object_fx_events[first_fx..]
            .iter()
            .map(|event| event.effect.as_str())
            .collect();
        assert_eq!(effects, ["FirstFx", "SecondFx"]);
        game.remove_native_object_upgrade(id, "Shared");
        assert!(!game.native_upgrade_executed(id, "ExperienceScalarUpgrade", 0));
        assert!(!game.native_upgrade_executed(id, "ExperienceScalarUpgrade", 1));
        assert_eq!(
            game.entities[id].experience_scalar().to_bits(),
            4.0_f32.to_bits()
        );
        game.grant_native_object_upgrade(id, "Shared");
        assert_eq!(
            game.entities[id].experience_scalar().to_bits(),
            7.0_f32.to_bits()
        );
        assert!(game.native_upgrade_executed(id, "ExperienceScalarUpgrade", 0));
        assert!(game.native_upgrade_executed(id, "ExperienceScalarUpgrade", 1));
    }
}
