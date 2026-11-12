use super::*;
use generals_sim::skirmish::{ClipReload, DamageType, Faction, Kind};

#[ignore = "requires original game installation"]
#[test]
fn installed_raptor_and_comanche_keep_native_taxi_sets_and_reload_rules() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = super::super::source_files(&assets).unwrap();
    for name in ["AmericaAirfield", "ChinaAirfield"] {
        let object = definition(&files, "Object", name).unwrap();
        let unit = super::super::unit_type(&files, object, Faction::Usa, Kind::Factory)
            .and_then(|unit| super::super::finish_type(&files, object, unit))
            .unwrap();
        assert!(
            unit.behavior.production_exit.is_none(),
            "native parking is the production exit"
        );
        if name == "AmericaAirfield" {
            let upgrade = unit
                .maintenance
                .research
                .iter()
                .find(|upgrade| upgrade.name == "Upgrade_ComancheRocketPods")
                .expect("original airfield must expose its supported rocket-pod research");
            assert_eq!(upgrade.cost, 800);
            assert_eq!(upgrade.frames, 1200);
            assert!(upgrade.player);
        }
    }
    let object = definition(&files, "Object", "AmericaJetRaptor").unwrap();
    let unit = super::super::unit_type(&files, object, Faction::Usa, Kind::Vehicle)
        .and_then(|unit| super::super::finish_type(&files, object, unit))
        .unwrap();
    let rules = unit.aircraft.as_ref().unwrap();
    assert!(rules.needs_runway && rules.keeps_space && !rules.helipad);
    assert!(rules.taxi.locomotor.ground.is_some());
    assert_eq!(rules.takeoff_pause, 15);
    assert_eq!(rules.idle_return, 300);
    assert_eq!(rules.parking_offset.to_bits(), 3.0_f32.to_bits());
    assert!(matches!(unit.weapon.reload, ClipReload::ReturnToBase));
    assert!(matches!(unit.weapon.damage_type, DamageType::JetMissiles));
    assert_eq!(unit.weapon.clip_size, 4);
    let object = definition(&files, "Object", "AmericaVehicleComanche").unwrap();
    let unit = super::super::unit_type(&files, object, Faction::Usa, Kind::Vehicle)
        .and_then(|unit| super::super::finish_type(&files, object, unit))
        .unwrap();
    let rules = unit.aircraft.as_ref().unwrap();
    assert!(!rules.needs_runway && !rules.keeps_space && rules.helipad);
    assert!(rules.taxi.locomotor.ground.is_some());
    assert!(rules.taxi.locomotor.flight.is_none());
    assert_eq!(unit.weapon_upgrade_triggers, ["Upgrade_ComancheRocketPods"]);
    assert!(unit.tertiary_weapon.is_none());
    assert!(unit.weapon_set_rules.templates.iter().any(|weapon| {
        weapon.flags == 1 && weapon.slot == 2 && weapon.rules.name == "ComancheRocketPodWeapon"
    }));
    let object = definition(&files, "Object", "FireWallSegment").unwrap();
    let unit = super::super::unit_type(&files, object, Faction::China, Kind::Vehicle)
        .and_then(|unit| super::super::finish_type(&files, object, unit))
        .unwrap();
    let rules = unit.locomotor.flight.as_ref().unwrap();
    assert!(matches!(
        rules.appearance,
        generals_sim::skirmish::flight::Appearance::Hover
    ));
    assert_eq!(
        rules.min_speed.to_bits(),
        (25.0_f32 * (1.0 / 30.0)).to_bits()
    );
    generals_sim::skirmish::flight::validate(rules).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
fn installed_aurora_keeps_distinct_supersonic_return_and_targeting_deadlines() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = super::super::source_files(&assets).unwrap();
    let object = definition(&files, "Object", "AmericaJetAurora").unwrap();
    let unit = super::super::unit_type(&files, object, Faction::Usa, Kind::Vehicle)
        .and_then(|unit| super::super::finish_type(&files, object, unit))
        .unwrap();
    let rules = unit.aircraft.as_ref().unwrap();
    assert_eq!(unit.cost, 2500);
    assert_eq!(rules.attack_locomotor_frames, 3);
    assert_eq!(rules.attackers_miss_frames, 60);
    assert_eq!(rules.sneaky_offset.to_bits(), (-20.0_f32).to_bits());
    assert_eq!(
        rules.attacking.as_ref().unwrap().speed.to_bits(),
        480.0_f32.to_bits()
    );
    assert_eq!(
        rules.returning.as_ref().unwrap().speed.to_bits(),
        150.0_f32.to_bits()
    );
    assert!(rules.needs_runway && rules.keeps_space && !rules.helipad);
    assert_eq!(unit.weapon.name, "AuroraBombWeapon");
    assert!(matches!(unit.weapon.damage_type, DamageType::AuroraBomb));
    assert_eq!(unit.weapon.damage.to_bits(), 400.0_f32.to_bits());
    assert_eq!(unit.weapon.clip_size, 1);
    assert!(matches!(unit.weapon.reload, ClipReload::ReturnToBase));
    assert!(unit.has_eject_pilot());
}

#[ignore = "requires original game installation"]
#[test]
fn installed_aurora_bomb_damage_keeps_distinct_structure_and_defense_armor() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = super::super::source_files(&assets).unwrap();
    for (name, expected) in [
        ("StructureArmor", 2.5_f32),
        ("BaseDefenseArmor", 2.0),
        ("GLAUpgradedStructureArmor", 1.5),
    ] {
        let armor = definition(&files, "Armor", name).unwrap();
        let coefficients = super::super::armor_coefficients(&armor.lines).unwrap();
        assert_eq!(
            coefficients[DamageType::AuroraBomb as usize].to_bits(),
            expected.to_bits()
        );
        assert_ne!(
            coefficients[DamageType::AuroraBomb as usize].to_bits(),
            coefficients[DamageType::Explosion as usize].to_bits()
        );
    }
}
