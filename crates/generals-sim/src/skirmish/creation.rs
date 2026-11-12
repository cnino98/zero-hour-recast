//! SH 2bfaea60 ordered `CreateModule::onCreate` and `onBuildComplete` dispatch.
use super::{Skirmish, weapons};

impl Skirmish {
    // ThingFactory::newObject runs this phase only after Object::init has
    // finished every onObjectCreated callback, before partition registration.
    pub(in crate::skirmish) fn create_object_callbacks(&mut self, id: usize) {
        let kind = self.entities[id].type_index;
        for index in 0..self.catalog[kind].behavior_modules.len() {
            let modules = &self.catalog[kind].behavior_modules;
            let module = &modules[index];
            if module.interfaces & 8 == 0 {
                continue;
            }
            let ordinal = modules[..index]
                .iter()
                .filter(|other| other.family == module.family)
                .count();
            match module.family.as_str() {
                "CaveContain" => self.created_cave(id),
                "SupplyWarehouseCreate" => self.created_supply_warehouse(id),
                "VeterancyGainCreate" => self.created_veterancy_gain(id, ordinal),
                "GrantUpgradeCreate" => self.grant_native_upgrade_create(id, ordinal, true),
                // Other admitted CREATE families inherit or override with no action.
                _ => {}
            }
        }
    }

    pub(in crate::skirmish) fn native_build_complete(&mut self, id: usize) {
        let kind = self.entities[id].type_index;
        for index in 0..self.catalog[kind].behavior_modules.len() {
            let modules = &self.catalog[kind].behavior_modules;
            let module = &modules[index];
            if module.interfaces & 8 == 0 {
                continue;
            }
            let ordinal = modules[..index]
                .iter()
                .filter(|other| other.family == module.family)
                .count();
            let object = self.entities[id].update_owner;
            match module.family.as_str() {
                // These overrides deliberately do not guard on the base latch.
                "PreorderCreate" => {
                    self.entities[id].drawable.model_conditions.preordered =
                        self.players[self.entities[id].owner].preordered;
                }
                "LockWeaponCreate" => {
                    self.update_objects[object].create_completed[index] = true;
                    let slot = self.catalog[kind].behavior.lock_weapons[ordinal];
                    if self.allocated_weapon(id, slot).is_some() {
                        self.entities[id].weapon_set.current = slot;
                        self.entities[id].weapon_set.lock = weapons::Lock::Permanent;
                    }
                }
                // Zero Hour moved registration to onObjectCreated.
                "TunnelContain" => {}
                _ if self.update_objects[object].create_completed[index] => {}
                "GrantUpgradeCreate" => {
                    self.update_objects[object].create_completed[index] = true;
                    self.grant_native_upgrade_create(id, ordinal, false);
                }
                "SpecialPowerCreate" => {
                    self.update_objects[object].create_completed[index] = true;
                    self.created_special_powers(id);
                }
                "SupplyCenterCreate" => {
                    self.update_objects[object].create_completed[index] = true;
                    for player in self.players.iter_mut().rev() {
                        player.supply_centers.push(id);
                    }
                }
                "CaveContain" => {
                    self.update_objects[object].create_completed[index] = true;
                    self.cave_build_complete(id);
                }
                // All remaining CREATE families inherit the base empty callback.
                _ => self.update_objects[object].create_completed[index] = true,
            }
        }
    }

    fn created_special_powers(&mut self, id: usize) {
        let kind = self.entities[id].type_index;
        for index in 0..self.catalog[kind].behavior_modules.len() {
            let module = &self.catalog[kind].behavior_modules[index];
            if let Some(ordinal) = self.catalog[kind]
                .special_powers
                .iter()
                .position(|rules| rules.tag == module.tag)
            {
                self.special_power_on_creation(id, ordinal);
            }
        }
    }
}
