//! ActiveBody.cpp: damage interfaces run in authored behavior order, before onDie.
use crate::skirmish::{DamageType, DeathType, Skirmish, UnitType, hazards::PoisonSource};

#[cfg(test)]
mod tests;

pub(in crate::skirmish) fn validate(unit: &UnitType) -> Result<(), String> {
    for module in &unit.behavior_modules {
        if module.interfaces & 4 != 0 && !bound(&module.family) {
            return Err(format!(
                "{}: unbound damage callback {} {} at {}:{} ({})",
                unit.name,
                module.family,
                module.tag,
                module.source.path,
                module.source.line,
                module.source.origin,
            ));
        }
    }
    Ok(())
}

fn bound(family: &str) -> bool {
    matches!(
        family,
        "BridgeBehavior"
            | "MinefieldBehavior"
            | "SupplyWarehouseCripplingBehavior"
            | "SpawnBehavior"
            | "AutoHealBehavior"
            | "BaseRegenerateUpdate"
            | "PoisonedBehavior"
            | "FlammableUpdate"
            | "FireWeaponWhenDamagedBehavior"
            | "BoneFXDamage"
            | "TransitionDamageFX"
            | "OverchargeBehavior"
            | "CaveContain"
            | "GarrisonContain"
            | "HealContain"
            | "HelixContain"
            | "InternetHackContain"
            | "MobNexusContain"
            | "OpenContain"
            | "OverlordContain"
            | "ParachuteContain"
            | "RailedTransportContain"
            | "RiderChangeContain"
            | "TransportContain"
            | "TunnelContain"
    )
}

impl Skirmish {
    pub(super) fn notify_body_healing(&mut self, id: usize, source: Option<usize>) {
        let kind = self.entities[id].type_index;
        for index in 0..self.catalog[kind].behavior_modules.len() {
            let modules = &self.catalog[kind].behavior_modules;
            let module = &modules[index];
            if module.interfaces & 4 == 0 {
                continue;
            }
            let ordinal = modules[..index]
                .iter()
                .filter(|prior| prior.family == module.family)
                .count();
            match module.family.as_str() {
                "PoisonedBehavior" => self.poison_module_healed(id, ordinal),
                "MinefieldBehavior" => {
                    self.mine_body_changed(id, ordinal, source, DamageType::Healing);
                }
                _ => {} // Other admitted onHealing callbacks have empty native bodies.
            }
        }
    }

    pub(super) fn notify_body_damage(
        &mut self,
        id: usize,
        source: Option<usize>,
        amount: f32,
        damage: DamageType,
        death: DeathType,
    ) {
        let type_index = self.entities[id].type_index;
        for index in 0..self.catalog[type_index].behavior_modules.len() {
            let modules = &self.catalog[type_index].behavior_modules;
            let module = &modules[index];
            if module.interfaces & 4 == 0 {
                continue;
            }
            let ordinal = modules[..index]
                .iter()
                .filter(|prior| prior.family == module.family)
                .count();
            match module.family.as_str() {
                "FireWeaponWhenDamagedBehavior" => {
                    self.damage_weapon_on_damage(id, ordinal, damage, amount);
                }
                "AutoHealBehavior" => self.auto_heal_module_damaged(id, ordinal),
                "MinefieldBehavior" => self.mine_body_changed(id, ordinal, source, damage),
                "SpawnBehavior" => self.spawn_on_damage(id, ordinal, amount),
                "SupplyWarehouseCripplingBehavior" => self.supply_crippling_damaged(id, ordinal),
                "BaseRegenerateUpdate" => self.regeneration_damaged(id),
                "FlammableUpdate" => {
                    if matches!(damage, DamageType::Flame | DamageType::ParticleBeam) {
                        self.flame_exposed_module(id, ordinal, source, amount);
                    }
                }
                "PoisonedBehavior" => {
                    if matches!(damage, DamageType::Poison) {
                        self.poison_damage_received(
                            id,
                            ordinal,
                            source.map_or(PoisonSource::Invalid, PoisonSource::Object),
                            amount,
                            death,
                        );
                    }
                }
                // These source onDamage bodies are empty. Their healing and
                // body-state-change callbacks are separate interfaces.
                // TerrainLogic compiles no bridge towers in this profile, so
                // BridgeBehavior onDamage/onHealing have no tower recipients.
                "BridgeBehavior"
                | "BoneFXDamage"
                | "TransitionDamageFX"
                | "OverchargeBehavior"
                | "CaveContain"
                | "GarrisonContain"
                | "HealContain"
                | "HelixContain"
                | "InternetHackContain"
                | "MobNexusContain"
                | "OpenContain"
                | "OverlordContain"
                | "ParachuteContain"
                | "RailedTransportContain"
                | "RiderChangeContain"
                | "TransportContain"
                | "TunnelContain" => {}
                _ => unreachable!("unbound damage callback passed admission"),
            }
        }
    }
}
