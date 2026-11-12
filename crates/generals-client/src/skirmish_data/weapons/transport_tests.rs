#[ignore = "requires original game installation"]
#[test]
fn installed_outpost_uses_armed_riders_without_a_research_trigger() {
    use generals_assets::virtual_assets::VirtualAssets;
    use generals_sim::skirmish::{Faction, Kind};

    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = super::super::source_files(&assets).unwrap();
    let object = super::definition(&files, "Object", "ChinaVehicleListeningOutpost").unwrap();
    let unit = super::super::unit_type(&files, object, Faction::China, Kind::Vehicle)
        .and_then(|unit| super::super::finish_type(&files, object, unit))
        .unwrap();
    let rules = unit.contain.as_ref().unwrap();
    assert!(rules.weapons.upgrade_transport);
    assert!(unit.weapon_upgrade_triggers.is_empty());
    assert_eq!(rules.capacity, 2);
    assert_eq!(
        rules.payload.as_ref().unwrap().template,
        "ChinaInfantryTankHunter"
    );
    assert_eq!(rules.payload.as_ref().unwrap().count, 2);
    assert_eq!(unit.weapon_set_rules.replacements.len(), 2);
    assert!(
        unit.weapon_set_rules.replacements[0]
            .weapons
            .iter()
            .all(Option::is_none)
    );
    let armed = unit.weapon_set_rules.replacements[1].weapons[0].unwrap();
    assert_eq!(
        unit.weapon_template(armed).unwrap().name,
        "ListeningOutpostUpgradedDummyWeapon"
    );
    assert_eq!(unit.weapon_set_rules.replacements[1].flags, 1);
    let detector = &unit.behavior.detectors[0];
    assert_eq!(detector.rate, 27);
    let feedback = detector.feedback.as_ref().unwrap();
    assert_eq!(feedback.bone, "IRFX");
    assert_eq!(feedback.particles, ["", "", "IRLenzflare"]);
    assert_eq!(feedback.sounds, ["IRPing", "IRPingLoud"]);
}
