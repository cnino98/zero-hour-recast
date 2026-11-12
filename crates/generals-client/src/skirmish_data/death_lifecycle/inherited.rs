//! `ThingTemplate::ModuleInfo::clearCopiedFromDefault` removes the default DIE
//! interface when any authored module supplies it, including contain/update modules.
pub(in crate::skirmish_data) fn has_die_interface(name: &str) -> bool {
    // SH 2bfaea60 GeneralsMD Include/GameLogic/Module: getInterfaceMask and
    // inherited masks, not class suffixes or the number of translated actions.
    matches!(
        name,
        "BattleBusSlowDeathBehavior"
            | "BridgeBehavior"
            | "BridgeTowerBehavior"
            | "BunkerBusterBehavior"
            | "CaveContain"
            | "CreateCrateDie"
            | "CreateObjectDie"
            | "CrushDie"
            | "DamDie"
            | "DestroyDie"
            | "EjectPilotDie"
            | "FXListDie"
            | "FireWeaponWhenDeadBehavior"
            | "FlightDeckBehavior"
            | "GarrisonContain"
            | "GenerateMinefieldBehavior"
            | "HealContain"
            | "HelicopterSlowDeathBehavior"
            | "HelixContain"
            | "InstantDeathBehavior"
            | "InternetHackContain"
            | "JetSlowDeathBehavior"
            | "KeepObjectDie"
            | "MinefieldBehavior"
            | "MobNexusContain"
            | "NeutronBlastBehavior"
            | "NeutronMissileSlowDeathBehavior"
            | "NeutronMissileUpdate"
            | "OpenContain"
            | "OverlordContain"
            | "POWTruckBehavior"
            | "ParachuteContain"
            | "ParkingPlaceBehavior"
            | "PrisonBehavior"
            | "ProductionUpdate"
            | "PropagandaCenterBehavior"
            | "PropagandaTowerBehavior"
            | "RailedTransportContain"
            | "RebuildHoleBehavior"
            | "RebuildHoleExposeDie"
            | "RiderChangeContain"
            | "SlowDeathBehavior"
            | "SpawnBehavior"
            | "SpecialPowerCompletionDie"
            | "StructureCollapseUpdate"
            | "StructureToppleUpdate"
            | "TechBuildingBehavior"
            | "TransportContain"
            | "TunnelContain"
            | "UpgradeDie"
    )
}

#[cfg(test)]
mod tests {
    use super::has_die_interface;

    #[test]
    fn contain_and_update_die_interfaces_are_not_inferred_from_family_suffixes() {
        for family in [
            "ProductionUpdate",
            "GarrisonContain",
            "CaveContain",
            "KeepObjectDie",
            "TechBuildingBehavior",
            "StructureToppleUpdate",
        ] {
            assert!(has_die_interface(family), "{family}");
        }
        for family in ["ToppleUpdate", "AutoHealBehavior"] {
            assert!(!has_die_interface(family), "{family}");
        }
    }
}
