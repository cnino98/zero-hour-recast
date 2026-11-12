use super::*;
use crate::skirmish_data::{blocks, definition};
use generals_assets::virtual_assets::VirtualAssets;

fn assets() -> VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

#[ignore = "requires original game installation"]
#[test]
fn installed_jet_death_targets_are_admitted_before_the_first_crash() {
    let assets = assets();
    let mut data = super::super::load(&assets, &[]).unwrap();
    validate_targets(&data.types).unwrap();
    let hulk = data
        .types
        .iter()
        .position(|unit| unit.name == "ChinaJetMIGHulk")
        .unwrap();
    let removed = data.types.remove(hulk);
    let error = validate_targets(&data.types).unwrap_err().to_string();
    assert!(
        error.contains("ChinaJetMIG") && error.contains("ChinaJetMIGHulk"),
        "{error}"
    );
    data.types.push(removed);
    let hulk = data.types.last_mut().unwrap();
    hulk.physical = None;
    assert!(validate_targets(&data.types).is_err());
}

#[ignore = "requires original game installation"]
#[test]
fn installed_jet_death_keeps_real_roll_units_ordered_debris_and_pilot_presence() {
    let assets = assets();
    let files = super::super::source_files(&assets).unwrap();
    let object = definition(&files, "Object", "AmericaJetRaptor").unwrap();
    let rules = super::super::eject_pilot::load(&files, object).unwrap();
    let modules = blocks(&object.lines, 0, "Behavior").unwrap();
    let jet = jet(
        &files,
        modules
            .iter()
            .copied()
            .find(|module| {
                module[0]
                    .tokens
                    .get(1)
                    .is_some_and(|name| name == "JetSlowDeathBehavior")
            })
            .unwrap(),
    )
    .unwrap();
    assert_eq!(jet.roll_rate.to_bits(), 0.2_f32.to_bits());
    assert_eq!(jet.roll_delta.to_bits(), 1.0_f32.to_bits());
    assert_eq!(jet.secondary_delay, 15);
    assert_eq!(jet.final_delay, 6);
    assert_eq!(
        jet.phases[1].objects.as_ref().unwrap().nuggets[0].names,
        ["AVRaptor_D4"]
    );
    assert!(jet.phases[2].objects.as_ref().unwrap().nuggets.is_empty());
    assert_eq!(jet.phases[3].objects.as_ref().unwrap().nuggets.len(), 3);
    let collision = physical_ocl::live_rules(&files, object)
        .unwrap()
        .unwrap()
        .collision;
    assert!(collision.allow_force);
    let building = collision.weapons[0].as_ref().unwrap();
    assert_eq!(building.damage.to_bits(), 30.0_f32.to_bits());
    assert!(matches!(
        building.damage_type,
        generals_sim::skirmish::DamageType::Explosion
    ));
    assert_eq!(building.effects[0], "FX_VehicleCrashesIntoBuilding");
    assert!(matches!(
        collision.weapons[1].as_ref().unwrap().damage_type,
        generals_sim::skirmish::DamageType::Crush
    ));
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].conditions.veterancy_mask, 14);
    assert_eq!(rules[0].conditions.exempt_status, 1 << 10);
    for name in ["ChinaJetMIG", "AmericaVehicleComanche"] {
        let object = definition(&files, "Object", name).unwrap();
        assert!(
            super::super::eject_pilot::load(&files, object)
                .unwrap()
                .is_empty()
        );
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_physical_objects_keep_original_mass_gravity_body_and_resting_rules() {
    let assets = assets();
    let files = super::super::source_files(&assets).unwrap();
    for name in physical_ocl::OBJECTS {
        let object = definition(&files, "Object", name).unwrap();
        let lifecycle = super::super::death_lifecycle::load(&files, object).unwrap();
        let rule = physical_ocl::rules(&files, object, lifecycle.as_ref())
            .unwrap()
            .unwrap();
        assert!(rule.motion.mass > 0.0 && rule.motion.gravity < 0.0);
        // These original PhysicsBehavior records retain the native false default.
        assert_eq!(
            rule.kill_when_resting,
            !matches!(
                name,
                "NapalmBomb"
                    | "AmericaVehicleHumveeDeadHull"
                    | "ChinaTankDragonDeadHull"
                    | "ChinaTankOverlordBarrelDebris"
                    | "ChinaTankOverlordDeadHull"
                    | "ChinaVehicleBattleMasterDeadHull"
                    | "DeadMicrowaveHulk"
                    | "GLARocketBuggyFullDebris"
            ),
            "{name}"
        );
        assert_eq!(
            rule.active_body,
            matches!(
                name,
                "NapalmBomb"
                    | "ChinaVehicleListeningOutpostDeadHull"
                    | "ChinaVehicleTroopCrawlerDeadHull"
                    | "AmericaVehicleHumveeDeadHull"
                    | "ChinaTankDragonDeadHull"
                    | "ChinaTankOverlordBarrelDebris"
                    | "ChinaTankOverlordDeadHull"
                    | "ChinaVehicleBattleMasterDeadHull"
                    | "DeadMicrowaveHulk"
                    | "GLARocketBuggyFullDebris"
            ),
            "{name}"
        );
    }
}
