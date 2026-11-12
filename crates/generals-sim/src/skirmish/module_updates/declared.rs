//! Concrete callback bindings. Families enter here only with their real instance data.
use super::{Descriptor, FOREVER, ModuleRegistration};
use crate::skirmish::Skirmish;

pub(super) fn descriptor(family: &str) -> Option<Descriptor> {
    let (phase, disabled_types, constructor_wakes): (u32, u32, &[u32]) = match family {
        "PhysicsBehavior" => (1, (1 << 13) - 1, &[1]),
        "AIUpdateInterface"
        | "DozerAIUpdate"
        | "SupplyTruckAIUpdate"
        | "ChinookAIUpdate"
        | "JetAIUpdate"
        | "HackInternetAIUpdate"
        | "DeployStyleAIUpdate"
        | "WorkerAIUpdate"
        | "WanderAIUpdate"
        | "TransportAIUpdate"
        | "AssaultTransportAIUpdate"
        | "DeliverPayloadAIUpdate"
        | "MissileAIUpdate" => (0, 1 << 3, &[]),
        "SpectreGunshipUpdate" | "SpectreGunshipDeploymentUpdate" => {
            (2, (1 << 10) | (1 << 6) | (1 << 2) | (1 << 1), &[])
        }
        "SupplyWarehouseDockUpdate"
        | "SupplyCenterDockUpdate"
        | "RepairDockUpdate"
        | "TunnelContain"
        | "CaveContain"
        | "OpenContain"
        | "ParachuteContain"
        | "HealContain"
        | "TransportContain"
        | "GarrisonContain"
        | "HelixContain"
        | "OverlordContain"
        | "InternetHackContain"
        | "DemoTrapUpdate"
        | "RadarUpdate"
        | "SlavedUpdate"
        | "MobMemberSlavedUpdate"
        | "SpawnBehavior"
        | "GenerateMinefieldBehavior"
        | "RebuildHoleBehavior"
        | "HijackerUpdate"
        | "StickyBombUpdate"
        | "ProneUpdate"
        | "QueueProductionExitUpdate"
        | "DumbProjectileBehavior"
        | "FireWeaponUpdate"
        | "FireWeaponWhenDamagedBehavior"
        | "FireOCLAfterWeaponCooldownUpdate"
        | "FirestormDynamicGeometryInfoUpdate"
        | "BridgeBehavior"
        | "BoneFXUpdate"
        | "CleanupHazardUpdate"
        | "LifetimeUpdate"
        | "DeletionUpdate"
        | "HeightDieUpdate"
        | "HordeUpdate"
        | "BattlePlanUpdate"
        | "AutoFindHealingUpdate"
        | "AutoDepositUpdate" => (2, 0, &[]),
        "OCLUpdate" => (2, (1 << 13) - 1, &[]),
        "CountermeasuresBehavior" => (2, 1 << 3, &[1]),
        "PilotFindVehicleUpdate"
        | "MinefieldBehavior"
        | "ParkingPlaceBehavior"
        | "BunkerBusterBehavior"
        | "PointDefenseLaserUpdate"
        | "TechBuildingBehavior" => (2, 0, &[1]),
        // BaseRegenerateUpdate.h:76, BaseRenerateUpdate.cpp:72–84.
        "BaseRegenerateUpdate" => (2, 1 << 6, &[]),
        "PoisonedBehavior"
        | "SlowDeathBehavior"
        | "JetSlowDeathBehavior"
        | "HelicopterSlowDeathBehavior"
        | "BattleBusSlowDeathBehavior" => (2, (1 << 13) - 1, &[FOREVER]),
        "NeutronBlastBehavior"
        | "StructureCollapseUpdate"
        | "StructureToppleUpdate"
        | "ToppleUpdate"
        | "AssistedTargetingUpdate"
        | "SpawnPointProductionExitUpdate"
        | "DefaultProductionExitUpdate"
        | "SupplyCenterProductionExitUpdate"
        | "FlammableUpdate"
        | "FireSpreadUpdate"
        | "SpyVisionUpdate"
        | "SpecialAbilityUpdate"
        | "CommandButtonHuntUpdate"
        | "PowerPlantUpdate"
        | "OverchargeBehavior"
        | "SupplyWarehouseCripplingBehavior" => (2, 0, &[FOREVER]),
        "ProductionUpdate" | "AutoHealBehavior" | "StealthUpdate" | "StealthDetectorUpdate" => {
            (2, 1 << 3, &[])
        }
        "PropagandaTowerBehavior" => (2, (1 << 13) - 1, &[1]),
        _ => return None,
    };
    Some(Descriptor {
        phase,
        disabled_types,
        constructor_wakes,
    })
}

/// Run before admitting a concrete catalog to shared dispatch. Preserve source diagnostics.
pub(super) fn validate(unit: &crate::skirmish::UnitType) -> Result<(), String> {
    validate_instance_data(unit)?;
    crate::skirmish::structure_death::validate(unit)?;
    crate::skirmish::pilot_find_vehicle::validate(unit)?;
    crate::skirmish::bridge_behavior::validate(unit)?;
    crate::skirmish::bone_fx::validate(unit)?;
    crate::skirmish::special_ability::validate(unit)?;
    let mut spies = 0;
    for entry in &unit.update_modules {
        let ModuleRegistration::Behavior {
            family,
            tag,
            source,
        } = entry
        else {
            continue;
        };
        validate_update_data(unit, family, tag, source)?;
        spies += usize::from(family == "SpyVisionUpdate");
    }
    validate_exits(unit)?;
    if spies != unit.behavior.spy_vision.len() {
        return Err(format!(
            "{}: ordered SpyVisionUpdate data count differs from registrations",
            unit.name
        ));
    }
    validate_instance_counts(unit)
}

fn validate_update_data(
    unit: &crate::skirmish::UnitType,
    family: &str,
    tag: &str,
    source: &super::ModuleSource,
) -> Result<(), String> {
    if descriptor(family).is_none() {
        return Err(format!(
            "{}: unbound update {} {} at {}:{} ({})",
            unit.name, family, tag, source.path, source.line, source.origin
        ));
    }
    match family {
        "SlowDeathBehavior"
        | "BattleBusSlowDeathBehavior"
        | "JetSlowDeathBehavior"
        | "HelicopterSlowDeathBehavior" => {
            validate_death_registration(unit, family, tag)?;
        }
        "DefaultProductionExitUpdate"
        | "SupplyCenterProductionExitUpdate"
        | "QueueProductionExitUpdate"
            if unit.behavior.production_exit.is_none() =>
        {
            return Err(format!(
                "{}: {family} lacks its concrete exit data",
                unit.name
            ));
        }
        "ProductionUpdate" if unit.production.is_none() => {
            return Err(format!(
                "{}: ProductionUpdate lacks its concrete data",
                unit.name
            ));
        }
        "DeployStyleAIUpdate" if unit.deployment.is_none() => {
            return Err(format!(
                "{}: DeployStyleAIUpdate lacks concrete instance data",
                unit.name
            ));
        }
        "HackInternetAIUpdate" if unit.behavior.hacking.is_none() => {
            return Err(format!(
                "{}: HackInternetAIUpdate lacks concrete instance data",
                unit.name
            ));
        }
        "JetAIUpdate" if unit.aircraft.is_none() => {
            return Err(format!(
                "{}: JetAIUpdate lacks concrete instance data",
                unit.name
            ));
        }
        "OCLUpdate"
            if unit.name != "TechReinforcementPad" || unit.behavior.ocl_update.is_none() =>
        {
            return Err(format!(
                "{}: OCLUpdate lacks an actual supported timed delivery consumer",
                unit.name
            ));
        }
        "DeliverPayloadAIUpdate" if unit.deliver_payload.is_none() => {
            return Err(format!(
                "{}: DeliverPayloadAIUpdate lacks concrete transport data",
                unit.name
            ));
        }
        "PhysicsBehavior" if unit.physical.is_none() && unit.live_physics.is_none() => {
            return Err(format!(
                "{}: PhysicsBehavior lacks concrete instance data at {}:{}",
                unit.name, source.path, source.line
            ));
        }
        "BaseRegenerateUpdate" if unit.maintenance.regeneration.is_none() => {
            return Err(format!(
                "{}: {} {} lacks its concrete data",
                unit.name, family, tag
            ));
        }
        "SupplyWarehouseDockUpdate"
            if unit.supply.warehouse.is_none() || unit.supply.dock.is_none() =>
        {
            return Err(format!("{}: {family} lacks warehouse/dock data", unit.name));
        }
        "SupplyCenterDockUpdate" | "RepairDockUpdate" if unit.supply.dock.is_none() => {
            return Err(format!("{}: {family} lacks dock data", unit.name));
        }
        "RepairDockUpdate" if unit.neutral_tech.repair_dock.is_none() => {
            return Err(format!("{}: {family} lacks repair action data", unit.name));
        }
        _ => {}
    }
    Ok(())
}

fn validate_instance_data(unit: &crate::skirmish::UnitType) -> Result<(), String> {
    if unit.behavior.radar_updates.iter().any(|rules| {
        !rules.extend_frames.is_finite() || !(0.0..4_294_967_296.0).contains(&rules.extend_frames)
    }) {
        return Err(format!(
            "{}: unsupported RadarUpdate duration: port supports only finite nonnegative durations below 2^32 frames",
            unit.name
        ));
    }
    let upgrades = unit
        .behavior_modules
        .iter()
        .filter(|module| module.family == "RadarUpgrade")
        .count();
    if upgrades != unit.behavior.radar_upgrades.len() {
        return Err(format!(
            "{}: RadarUpgrade instance data differs from registrations",
            unit.name
        ));
    }
    let parking = unit
        .behavior_modules
        .iter()
        .filter(|module| module.family == "ParkingPlaceBehavior")
        .count();
    if parking > 1 || (parking == 1) != unit.parking.is_some() {
        return Err(format!(
            "{}: ParkingPlaceBehavior requires exactly one concrete parking interface",
            unit.name
        ));
    }
    let tunnels = unit
        .behavior_modules
        .iter()
        .filter(|module| module.family == "TunnelContain")
        .count();
    if tunnels > 1
        || (tunnels == 1)
            != unit
                .contain
                .as_ref()
                .is_some_and(|rules| rules.tunnel.is_some() && rules.cave_index.is_none())
    {
        return Err(format!(
            "{}: TunnelContain requires exactly one concrete container interface",
            unit.name
        ));
    }
    let caves = unit
        .behavior_modules
        .iter()
        .filter(|module| module.family == "CaveContain")
        .count();
    if caves > 1
        || (caves == 1)
            != unit
                .contain
                .as_ref()
                .is_some_and(|rules| rules.cave_index.is_some() && rules.tunnel.is_some())
    {
        return Err(format!(
            "{}: CaveContain requires exactly one concrete container interface",
            unit.name
        ));
    }
    let parachutes = unit
        .behavior_modules
        .iter()
        .filter(|module| module.family == "ParachuteContain")
        .count();
    if parachutes > 1 || (parachutes == 1) != unit.parachute.is_some() {
        return Err(format!(
            "{}: ParachuteContain requires exactly one concrete container interface",
            unit.name
        ));
    }
    Ok(())
}

fn validate_death_registration(
    unit: &crate::skirmish::UnitType,
    family: &str,
    tag: &str,
) -> Result<(), String> {
    let Some(rules) = &unit.death_lifecycle else {
        return Err(format!(
            "{}: SlowDeathBehavior lacks authored rules",
            unit.name
        ));
    };
    if rules
        .slow
        .iter()
        .filter(|rule| rule.tag == tag && rule.family() == family)
        .count()
        != 1
    {
        return Err(format!(
            "{}: SlowDeathBehavior {tag} must bind exactly one authored rule",
            unit.name
        ));
    }
    if matches!(
        family,
        "JetSlowDeathBehavior" | "HelicopterSlowDeathBehavior"
    ) && unit.live_physics.is_none()
    {
        return Err(format!(
            "{}: {family} requires the native Physics interface",
            unit.name
        ));
    }
    if family == "BattleBusSlowDeathBehavior"
        && (unit.physical.is_some() || !unit.projectile_rules.is_empty())
    {
        return Err(format!(
            "{}: BattleBus shock requires ordinary Object Physics",
            unit.name
        ));
    }
    let needs_locomotor = family == "JetSlowDeathBehavior"
        || (family == "HelicopterSlowDeathBehavior"
            && !matches!(unit.ai.update, crate::skirmish::UnitAiUpdate::Absent));
    if needs_locomotor
        && (matches!(unit.ai.update, crate::skirmish::UnitAiUpdate::Absent)
            || unit.locomotor.flight.is_none())
    {
        return Err(format!(
            "{}: {family} requires the current flight locomotor limits",
            unit.name
        ));
    }
    if unit
        .behavior_modules
        .iter()
        .any(|module| matches!(module.family.as_str(), "NeutronMissileSlowDeathBehavior"))
    {
        return Err(format!(
            "{}: mixed ordinary and derived SlowDeath selectors require the derived interface",
            unit.name
        ));
    }
    if unit.kind_of.iter().any(|kind| kind == "PROJECTILE")
        && rules.slow.iter().any(|rule| rule.fling.is_some())
    {
        return Err(format!(
            "{}: projectile SlowDeath fling requires the projectile Physics motion binding",
            unit.name
        ));
    }
    Ok(())
}

fn validate_exits(unit: &crate::skirmish::UnitType) -> Result<(), String> {
    let exits = unit.update_modules.iter().filter(|entry| matches!(entry, ModuleRegistration::Behavior { family, .. } if matches!(family.as_str(), "DefaultProductionExitUpdate" | "SupplyCenterProductionExitUpdate" | "QueueProductionExitUpdate"))).count();
    if exits > 1 {
        return Err(format!(
            "{}: multiple exit modules share one exit state",
            unit.name
        ));
    }
    for entry in &unit.update_modules {
        if let ModuleRegistration::Behavior { family, .. } = entry
            && matches!(
                family.as_str(),
                "DefaultProductionExitUpdate"
                    | "SupplyCenterProductionExitUpdate"
                    | "QueueProductionExitUpdate"
            )
        {
            let exit = unit
                .behavior
                .production_exit
                .as_ref()
                .expect("validated exit data");
            if (family == "QueueProductionExitUpdate") != exit.queue.is_some()
                || (family == "SupplyCenterProductionExitUpdate") == exit.extend_rally
            {
                return Err(format!(
                    "{}: {family} mismatches concrete exit data",
                    unit.name
                ));
            }
        }
    }
    Ok(())
}

fn instance_counts(unit: &crate::skirmish::UnitType) -> [(&'static str, usize); 53] {
    let behavior = &unit.behavior;
    [
        (
            "DumbProjectileBehavior",
            unit.projectile_rules
                .iter()
                .filter(|rules| matches!(rules, crate::skirmish::FlightRules::Shell(_)))
                .count(),
        ),
        (
            "MissileAIUpdate",
            unit.projectile_rules
                .iter()
                .filter(|rules| matches!(rules, crate::skirmish::FlightRules::Missile(_)))
                .count(),
        ),
        ("RadarUpdate", behavior.radar_updates.len()),
        ("GenerateMinefieldBehavior", behavior.mine_generators.len()),
        ("MinefieldBehavior", behavior.minefields.len()),
        ("ToppleUpdate", behavior.toppling.len()),
        ("RebuildHoleBehavior", unit.maintenance.rebuild.len()),
        (
            "TechBuildingBehavior",
            unit.neutral_tech.tech_buildings.len(),
        ),
        ("SlavedUpdate", behavior.slaves.len()),
        ("MobMemberSlavedUpdate", behavior.mobs.len()),
        ("AssistedTargetingUpdate", behavior.assisted_targeting.len()),
        ("SpawnBehavior", behavior.spawns.len()),
        ("HijackerUpdate", behavior.hijackers.len()),
        ("StickyBombUpdate", behavior.sticky_bombs.len()),
        ("SpecialAbilityUpdate", behavior.special_abilities.len()),
        (
            "CommandButtonHuntUpdate",
            behavior.command_button_hunts.len(),
        ),
        ("SpawnPointProductionExitUpdate", behavior.spawn_exits.len()),
        ("ProneUpdate", behavior.prone.len()),
        (
            "SupplyWarehouseCripplingBehavior",
            behavior.supply_crippling.len(),
        ),
        ("DemoTrapUpdate", unit.demo_trap.len()),
        ("SpectreGunshipUpdate", usize::from(unit.spectre.is_some())),
        (
            "SpectreGunshipDeploymentUpdate",
            unit.spectre_deployments.len(),
        ),
        ("PowerPlantUpdate", unit.maintenance.power_plants.len()),
        ("OverchargeBehavior", unit.maintenance.overcharges.len()),
        ("AutoDepositUpdate", unit.maintenance.deposits.len()),
        ("OCLUpdate", usize::from(behavior.ocl_update.is_some())),
        (
            "DeliverPayloadAIUpdate",
            usize::from(unit.deliver_payload.is_some()),
        ),
        (
            "ProductionUpdate",
            usize::from(unit.production.is_some()) + unit.additional_production.len(),
        ),
        ("PointDefenseLaserUpdate", behavior.point_defense.len()),
        ("BunkerBusterBehavior", behavior.bunker_busters.len()),
        ("CountermeasuresBehavior", behavior.countermeasures.len()),
        ("FireWeaponUpdate", behavior.fire_weapons.len()),
        (
            "FireWeaponWhenDamagedBehavior",
            behavior.damage_weapons.len(),
        ),
        (
            "FirestormDynamicGeometryInfoUpdate",
            behavior.firestorms.len(),
        ),
        ("BridgeBehavior", behavior.bridges.len()),
        ("BoneFXUpdate", behavior.bone_fx.len()),
        ("NeutronBlastBehavior", behavior.neutron_blast.len()),
        ("CleanupHazardUpdate", behavior.cleanup.len()),
        ("LifetimeUpdate", behavior.lifetimes.len()),
        ("DeletionUpdate", behavior.deletions.len()),
        ("HeightDieUpdate", behavior.height_die.len()),
        (
            "FireOCLAfterWeaponCooldownUpdate",
            behavior.contamination.len(),
        ),
        ("FlammableUpdate", behavior.flammable.len()),
        ("FireSpreadUpdate", behavior.fire_spread.len()),
        ("PoisonedBehavior", behavior.poisoned.len()),
        ("AutoFindHealingUpdate", behavior.auto_find_healing.len()),
        ("PilotFindVehicleUpdate", behavior.pilot_find_vehicle.len()),
        ("AutoHealBehavior", unit.maintenance.auto_heal.len()),
        ("StealthDetectorUpdate", behavior.detectors.len()),
        ("StealthUpdate", usize::from(behavior.stealth.is_some())),
        ("HordeUpdate", usize::from(behavior.horde.is_some())),
        ("BattlePlanUpdate", usize::from(unit.battle_plan.is_some())),
        (
            "PropagandaTowerBehavior",
            usize::from(behavior.propaganda.is_some()),
        ),
    ]
}

fn validate_instance_counts(unit: &crate::skirmish::UnitType) -> Result<(), String> {
    for (family, expected) in instance_counts(unit) {
        let actual = unit
            .update_modules
            .iter()
            .filter(|entry| {
                matches!(entry,
            ModuleRegistration::Behavior { family: name, .. } if name == family)
            })
            .count();
        if actual != expected {
            return Err(format!(
                "{}: ordered {family} instance data differs from registrations",
                unit.name
            ));
        }
    }
    Ok(())
}

fn registered_family(unit: &crate::skirmish::UnitType, registration: usize) -> (&str, usize) {
    let ModuleRegistration::Behavior { family, .. } = &unit.update_modules[registration] else {
        unreachable!("helper routed through declared callback");
    };
    let ordinal = unit.update_modules[..registration].iter().filter(|entry| {
        matches!(entry, ModuleRegistration::Behavior { family: prior, .. } if prior == family)
    }).count();
    (family, ordinal)
}

impl Skirmish {
    pub(in crate::skirmish) fn update_declared_module(
        &mut self,
        owner: usize,
        registration: usize,
    ) -> i32 {
        let (family, ordinal) =
            registered_family(&self.catalog[self.entities[owner].type_index], registration);
        match family {
            "AIUpdateInterface"
            | "TransportAIUpdate"
            | "JetAIUpdate"
            | "HackInternetAIUpdate"
            | "DeployStyleAIUpdate" => self.update_ai_module(owner),
            "WanderAIUpdate" => self.update_wander_ai_module(owner),
            "DeliverPayloadAIUpdate" => self.update_delivery_module(owner),
            "OCLUpdate" => self.update_reinforcement_timer(owner, ordinal),
            "AssaultTransportAIUpdate" => self.update_assault_module(owner),
            "DozerAIUpdate" => self.update_dozer_module(owner),
            "SupplyTruckAIUpdate" => self.update_supply_module(owner),
            "ChinookAIUpdate" => self.update_chinook_module(owner),
            "WorkerAIUpdate" => self.update_worker_module(owner),
            "SupplyWarehouseDockUpdate" | "SupplyCenterDockUpdate" | "RepairDockUpdate" => {
                self.update_dock_module(owner)
            }
            "TunnelContain" => self.update_tunnel(owner),
            "CaveContain" | "OpenContain" => self.update_open_contain(owner),
            "ParachuteContain" => self.update_parachute(owner),
            "ParkingPlaceBehavior" => self.update_parking_module(owner),
            "ToppleUpdate" => self.update_toppling(owner, ordinal),
            "TechBuildingBehavior" => self.update_tech_building(owner, ordinal),
            "RebuildHoleBehavior" => self.update_rebuild_hole(owner, ordinal),
            "GenerateMinefieldBehavior" => self.update_mine_generator(owner, ordinal),
            "MinefieldBehavior" => self.update_land_mine(owner, ordinal),
            "HealContain" => self.update_heal_container(owner),
            "GarrisonContain" => self.update_garrison(owner),
            "HelixContain" => self.update_helix_contain(owner),
            "TransportContain" | "InternetHackContain" | "OverlordContain" => {
                self.update_transport_contain(owner)
            }
            "QueueProductionExitUpdate" => self.update_queue_exit(owner),
            "RadarUpdate" => self.update_radar(owner, ordinal),
            "SlavedUpdate" => self.update_drone(owner, ordinal),
            "MobMemberSlavedUpdate" => self.update_mob_member(owner, ordinal),
            "SpawnBehavior" => self.update_spawn(owner, ordinal),
            "HijackerUpdate" => self.update_hijacker(owner, ordinal),
            "StickyBombUpdate" => self.update_sticky_bomb(owner, ordinal),
            "SpecialAbilityUpdate" => self.update_special_ability(owner, ordinal),
            "CommandButtonHuntUpdate" => self.update_command_button_hunt(owner, ordinal),
            // Native SpawnPointProductionExitUpdate.h85 override sleeps forever.
            "DefaultProductionExitUpdate"
            | "SupplyCenterProductionExitUpdate"
            | "AssistedTargetingUpdate"
            | "SpawnPointProductionExitUpdate"
            | "NeutronBlastBehavior" => i32::from_ne_bytes(FOREVER.to_ne_bytes()),
            "ProneUpdate" => self.update_prone(owner, ordinal),
            "SupplyWarehouseCripplingBehavior" => self.update_supply_crippling(owner, ordinal),
            "DemoTrapUpdate" => self.update_demo_trap(owner, ordinal),
            "SpectreGunshipUpdate" => self.update_spectre(owner),
            "SpectreGunshipDeploymentUpdate" => self.update_spectre_deployment(owner, ordinal),
            "PowerPlantUpdate" => self.update_power_plant(owner, ordinal),
            "OverchargeBehavior" => self.update_overcharge(owner, ordinal),
            "AutoDepositUpdate" => self.update_auto_deposit(owner, ordinal),
            "ProductionUpdate" => {
                self.update_production_instance(owner, ordinal);
                1
            }
            "DumbProjectileBehavior" => self.update_shell_module(owner, registration),
            "MissileAIUpdate" => self.update_missile_module(owner, registration),
            "PhysicsBehavior" => self.update_object_physics(owner),
            "SlowDeathBehavior" => self.update_slow_death(owner, ordinal),
            "JetSlowDeathBehavior" => self.update_jet_slow_death(owner, ordinal),
            "HelicopterSlowDeathBehavior" => self.update_helicopter_slow_death(owner, ordinal),
            "BattleBusSlowDeathBehavior" => self.update_battle_bus_slow_death(owner, ordinal),
            "AutoFindHealingUpdate" => self.update_auto_find_healing(owner, ordinal),
            "PilotFindVehicleUpdate" => self.update_pilot_find_vehicle(owner, ordinal),
            "PointDefenseLaserUpdate" => self.update_point_defense(owner, ordinal),
            "BunkerBusterBehavior" => self.update_bunker_buster(owner, ordinal),
            "CountermeasuresBehavior" => self.update_countermeasures(owner, ordinal),
            "FireWeaponUpdate" => self.update_fire_weapon_module(owner, ordinal),
            "FireWeaponWhenDamagedBehavior" => self.update_damage_weapon_module(owner, ordinal),
            "FirestormDynamicGeometryInfoUpdate" => self.update_firestorm_module(owner, ordinal),
            // Both admitted originals have empty BridgeDieFX/OCL lists. The native
            // update still runs every frame, including while retained as rubble.
            "BridgeBehavior" => 1,
            "BoneFXUpdate" => self.update_bone_fx(owner, ordinal),
            "StructureCollapseUpdate" | "StructureToppleUpdate" => {
                self.update_structure_death(owner)
            }
            "CleanupHazardUpdate" => self.update_cleanup_module(owner, ordinal),
            "LifetimeUpdate" => self.update_lifetime(owner),
            "DeletionUpdate" => self.update_deletion(owner),
            "FireOCLAfterWeaponCooldownUpdate" => self.update_contamination_module(owner, ordinal),
            "HeightDieUpdate" => self.update_height_die(owner, ordinal),
            "BaseRegenerateUpdate" => self.update_base_regeneration(owner),
            "FlammableUpdate" => self.update_flammable_module(owner, ordinal),
            "FireSpreadUpdate" => self.update_fire_spread_module(owner, ordinal),
            "PoisonedBehavior" => self.update_poison_module(owner, ordinal),
            "SpyVisionUpdate" => self.update_spy_module(owner, ordinal),
            "AutoHealBehavior" => self.update_auto_heal_module(owner, ordinal),
            "HordeUpdate" => self.update_horde_module(owner),
            "StealthUpdate" => self.update_stealth_module(owner),
            "StealthDetectorUpdate" => self.update_stealth_detector(owner, ordinal),
            "PropagandaTowerBehavior" => self.update_propaganda_module(owner),
            "BattlePlanUpdate" => self.update_battle_plan_module(owner),
            _ => unreachable!("unbound update family passed admission"),
        }
    }

    pub(in crate::skirmish) fn register_object_updates(&mut self, owner: usize) {
        assert!(
            self.update_objects[self.entities[owner].update_owner]
                .handles
                .is_empty()
        );
        let type_index = self.entities[owner].type_index;
        self.update_objects[self.entities[owner].update_owner].upgrade_executed =
            vec![false; self.catalog[type_index].behavior_modules.len()];
        self.update_objects[self.entities[owner].update_owner].create_completed =
            vec![false; self.catalog[type_index].behavior_modules.len()];
        let helpers = self.catalog[type_index]
            .update_modules
            .iter()
            .take_while(|entry| matches!(entry, ModuleRegistration::Helper(_)))
            .count();
        for registration in 0..helpers {
            self.construct_registered_update(owner, registration);
        }
        let count = self.catalog[type_index].behavior_modules.len();
        for index in 0..count {
            let module = &self.catalog[type_index].behavior_modules[index];
            let registration = self.catalog[type_index]
                .update_modules
                .iter()
                .position(|entry| {
                    matches!(entry, ModuleRegistration::Behavior { family, tag, .. }
                    if family == &module.family && tag == &module.tag)
                });
            if let Some(registration) = registration {
                self.construct_registered_update(owner, registration);
            }
            if self.catalog[type_index].behavior_modules[index].family
                == "FireWeaponWhenDeadBehavior"
            {
                let ordinal = self.catalog[type_index].behavior_modules[..index]
                    .iter()
                    .filter(|module| module.family == "FireWeaponWhenDeadBehavior")
                    .count();
                self.construct_death_weapon(owner, ordinal);
            }
            if self.catalog[type_index].behavior_modules[index].family == "FireWeaponCollide" {
                let ordinal = self.catalog[type_index].behavior_modules[..index]
                    .iter()
                    .filter(|module| module.family == "FireWeaponCollide")
                    .count();
                self.construct_collision_weapon(owner, ordinal);
            }
            if self.catalog[type_index].behavior_modules[index].family == "RadarUpgrade" {
                self.construct_radar_upgrade(owner);
            }
            if self.catalog[type_index].behavior_modules[index].family == "FXListDie" {
                let ordinal = self.catalog[type_index].behavior_modules[..index]
                    .iter()
                    .filter(|module| module.family == "FXListDie")
                    .count();
                self.construct_death_fx(owner, ordinal);
            }
            if self.catalog[type_index].behavior_modules[index].family == "SpecialAbility" {
                let tag = &self.catalog[type_index].behavior_modules[index].tag;
                if let Some(ordinal) = self.catalog[type_index]
                    .special_powers
                    .iter()
                    .position(|power| &power.tag == tag)
                {
                    self.construct_special_power(owner, ordinal);
                }
            }
            // Object::init inserts the current behavior pointer after its constructor returns.
            self.update_objects[self.entities[owner].update_owner].constructed_behaviors =
                index + 1;
        }
        assert_eq!(
            self.update_objects[self.entities[owner].update_owner]
                .handles
                .len(),
            self.catalog[type_index].update_modules.len(),
            "UPDATE registration lacks authored behavior"
        );
        self.initialize_created_object(owner);
        self.created_object_callbacks(owner);
        // Object::onObjectCreated sends Drawable construction after all module
        // onObjectCreated callbacks, before ThingFactory's ordered CREATE phase.
        self.presentation
            .weapon_fx
            .push(crate::skirmish::WeaponFxEvent::DrawableCreated {
                object: owner,
                type_index: self.entities[owner].type_index,
                validate_logic: self.logic_update || self.map_loading,
            });
        self.create_object_callbacks(owner);
        // ThingFactory::newObject registers after the create callbacks.
        self.register_object_partition(owner);
        self.finish_object_update_registration(owner);
        self.initialize_safe_occlusion(owner);
    }

    fn created_object_callbacks(&mut self, owner: usize) {
        let type_index = self.entities[owner].type_index;
        for index in 0..self.catalog[type_index].behavior_modules.len() {
            let modules = &self.catalog[type_index].behavior_modules;
            let family = &modules[index].family;
            let ordinal = modules[..index]
                .iter()
                .filter(|module| &module.family == family)
                .count();
            match family.as_str() {
                "AIUpdateInterface"
                | "DozerAIUpdate"
                | "SupplyTruckAIUpdate"
                | "ChinookAIUpdate"
                | "WorkerAIUpdate"
                | "WanderAIUpdate"
                | "HackInternetAIUpdate"
                | "DeployStyleAIUpdate"
                | "TransportAIUpdate"
                | "AssaultTransportAIUpdate"
                | "DeliverPayloadAIUpdate"
                | "MissileAIUpdate" => self.initialize_primary_ai(owner),
                "JetAIUpdate" => self.create_jet_module(owner),
                "TunnelContain" => self.created_tunnel(owner),
                "DemoTrapUpdate" => self.created_demo_trap(owner, ordinal),
                "StickyBombUpdate" => self.created_sticky_bomb(owner, ordinal),
                "SpectreGunshipUpdate" => self.create_spectre(owner),
                "HelixContain" | "OverlordContain" => self.create_contain_template_payload(owner),
                "BoneFXUpdate" | "BoneFXDamage" => self.bone_fx_created(owner),
                "CleanupHazardUpdate" => self.cleanup_on_created(owner, ordinal),
                "BattlePlanUpdate" => self.created_battle_plan(owner),
                "SlavedUpdate" => self.created_slaved(owner, ordinal),
                "MobMemberSlavedUpdate" => self.created_mob_member(owner, ordinal),
                "BunkerBusterBehavior" => self.bunker_buster_on_created(owner, ordinal),
                _ => {}
            }
        }
    }

    fn construct_registered_update(&mut self, owner: usize, registration: usize) {
        assert_eq!(
            registration,
            self.update_objects[self.entities[owner].update_owner]
                .handles
                .len()
        );
        let entry = &self.catalog[self.entities[owner].type_index].update_modules[registration];
        let (helper, descriptor) = match entry {
            ModuleRegistration::Helper(helper) => {
                (Some(*helper), super::helper_descriptor(*helper))
            }
            ModuleRegistration::Behavior { family, .. } => (
                None,
                descriptor(family)
                    .expect("all update families must be bound before object creation"),
            ),
        };
        let disabled_types = if matches!(entry, ModuleRegistration::Behavior { family, .. } if family == "ProductionUpdate")
        {
            let unit = &self.catalog[self.entities[owner].type_index];
            let (_, ordinal) = registered_family(unit, registration);
            unit.production_disabled_at(ordinal)
        } else {
            descriptor.disabled_types
        };
        self.allocate_object_update(owner, descriptor.phase, disabled_types);
        if let Some(helper) = helper {
            self.construct_builtin_helper(owner, helper);
        }
        for &delay in descriptor.constructor_wakes {
            self.wake_update(owner, registration, delay);
        }
        if helper.is_none() {
            self.construct_declared_update(owner, registration);
        }
    }

    fn construct_declared_update(&mut self, owner: usize, registration: usize) {
        let (family, ordinal) =
            registered_family(&self.catalog[self.entities[owner].type_index], registration);
        match family {
            "AIUpdateInterface" | "WanderAIUpdate" | "TransportAIUpdate" => {
                self.construct_ai_module(owner, registration);
            }
            "AssaultTransportAIUpdate" => self.construct_assault_module(owner, registration),
            "DeliverPayloadAIUpdate" => self.construct_delivery_module(owner, registration),
            "OCLUpdate" => self.construct_reinforcement_timer(owner, ordinal),
            "DozerAIUpdate" => self.construct_dozer_module(owner, registration),
            "SupplyTruckAIUpdate" => self.construct_supply_module(owner, registration),
            "ChinookAIUpdate" => self.construct_chinook_module(owner, registration),
            "JetAIUpdate" => self.construct_jet_module(owner, registration),
            "HackInternetAIUpdate" => self.construct_hacker_module(owner, registration),
            "DeployStyleAIUpdate" => self.construct_deployment_module(owner, registration),
            "WorkerAIUpdate" => self.construct_worker_module(owner, registration),
            "SupplyCenterDockUpdate" | "RepairDockUpdate" => self.construct_dock_module(owner),
            "SupplyWarehouseDockUpdate" => self.construct_warehouse_module(owner),
            "OpenContain" | "HealContain" | "GarrisonContain" => self.construct_open_contain(owner),
            "TunnelContain" => self.construct_tunnel(owner),
            "CaveContain" => self.construct_cave(owner),
            "ParachuteContain" => self.construct_parachute(owner),
            "ParkingPlaceBehavior" => self.construct_parking(owner),
            "ToppleUpdate" => self.construct_toppling(owner, ordinal),
            "RebuildHoleBehavior" => self.construct_rebuild_hole(owner, ordinal),
            "GenerateMinefieldBehavior" => self.construct_mine_generator(owner, ordinal),
            "MinefieldBehavior" => self.construct_land_mine(owner, ordinal),
            // TechBuildingBehavior has no instance fields; its constructor only sets wake 1.
            "TransportContain" | "InternetHackContain" | "HelixContain" | "OverlordContain" => {
                self.construct_transport_contain(owner);
            }
            "DumbProjectileBehavior" | "MissileAIUpdate" => {
                self.construct_projectile_module(owner, registration);
            }
            "RadarUpdate" => self.construct_radar(owner),
            "SlavedUpdate" => self.construct_slaved(owner, ordinal),
            "MobMemberSlavedUpdate" => self.construct_mob_member(owner, ordinal),
            "AssistedTargetingUpdate" => self.construct_assisted_targeting(owner, ordinal),
            "SpawnBehavior" => self.construct_spawn(owner, ordinal),
            "HijackerUpdate" => self.construct_hijacker(owner, ordinal),
            "StickyBombUpdate" => self.construct_sticky_bomb(owner, ordinal),
            "SpecialAbilityUpdate" => self.construct_special_ability(owner, ordinal),
            "CommandButtonHuntUpdate" => self.construct_command_button_hunt(owner, ordinal),
            "SpawnPointProductionExitUpdate" => self.construct_spawn_exit(owner, ordinal),
            "ProneUpdate" => self.construct_prone(owner, ordinal),
            "SupplyWarehouseCripplingBehavior" => self.construct_supply_crippling(owner, ordinal),
            "SlowDeathBehavior"
            | "JetSlowDeathBehavior"
            | "HelicopterSlowDeathBehavior"
            | "BattleBusSlowDeathBehavior" => {
                let index = self.slow_death_index(owner, family, ordinal);
                self.construct_slow_death(owner, index);
            }
            "DemoTrapUpdate" => self.construct_demo_trap(owner, ordinal),
            "SpectreGunshipUpdate" => self.construct_spectre(owner),
            "SpectreGunshipDeploymentUpdate" => self.construct_spectre_deployment(owner),
            "PowerPlantUpdate" => self.construct_power_plant(owner, ordinal),
            "OverchargeBehavior" => self.construct_overcharge(owner, ordinal),
            "AutoDepositUpdate" => self.construct_auto_deposit(owner, ordinal),
            "ProductionUpdate" => self.construct_production_instance(owner, ordinal),
            "PhysicsBehavior" => self.construct_object_physics(owner),
            "PointDefenseLaserUpdate" => self.construct_point_defense(owner, ordinal),
            "BunkerBusterBehavior" => self.construct_bunker_buster(owner, ordinal),
            "CountermeasuresBehavior" => self.construct_countermeasures(owner, ordinal),
            "FireWeaponUpdate" => self.construct_fire_weapon_module(owner, ordinal),
            "FireWeaponWhenDamagedBehavior" => self.construct_damage_weapon_module(owner, ordinal),
            "FirestormDynamicGeometryInfoUpdate" => self.construct_firestorm_module(owner, ordinal),
            "BridgeBehavior" => self.construct_bridge_behavior(owner, ordinal),
            "BoneFXUpdate" => self.construct_bone_fx(owner, ordinal),
            "StructureCollapseUpdate" | "StructureToppleUpdate" => {
                self.initialize_structure_death(owner);
            }
            "CleanupHazardUpdate" => self.construct_cleanup_module(owner, ordinal),
            "LifetimeUpdate" => self.construct_lifetime(owner, registration),
            "DeletionUpdate" => self.construct_deletion(owner, registration),
            "FireOCLAfterWeaponCooldownUpdate" => {
                self.construct_contamination_module(owner, ordinal);
            }
            "HeightDieUpdate" => self.construct_height_die(owner),
            "BaseRegenerateUpdate" => self.construct_base_regeneration(owner, registration),
            "PoisonedBehavior" => self.construct_poison_module(owner, ordinal),
            "FlammableUpdate" => self.construct_flammable_module(owner, ordinal),
            "AutoFindHealingUpdate" => self.construct_auto_find_healing(owner, ordinal),
            "PilotFindVehicleUpdate" => self.construct_pilot_find_vehicle(owner, ordinal),
            "SpyVisionUpdate" => self.construct_spy_vision_module(owner),
            "AutoHealBehavior" => self.construct_auto_heal(owner, registration),
            "HordeUpdate" => self.construct_horde_update(owner, registration),
            "StealthUpdate" => self.construct_stealth_update(owner, registration),
            "StealthDetectorUpdate" => self.construct_stealth_detector(owner, registration),
            // Their concrete fields are initialized with the object and constructors
            // make no further mutations beyond their descriptor wakes.
            "DefaultProductionExitUpdate"
            | "SupplyCenterProductionExitUpdate"
            | "QueueProductionExitUpdate"
            | "PropagandaTowerBehavior"
            | "BattlePlanUpdate"
            | "NeutronBlastBehavior"
            | "FireSpreadUpdate"
            | "TechBuildingBehavior" => {}
            _ => unreachable!("unbound update constructor passed admission"),
        }
    }
}
