use super::*;
use generals_assets::{
    content,
    virtual_assets::{Asset, VirtualAssets},
};
use generals_sim::skirmish::{Faction, Kind};

#[ignore = "requires original game installation"]
#[test]
fn installed_base_ejection_lists_bind_original_pilot_parachute_force_and_invulnerability() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut roster = vec![("AmericaJetRaptor".to_owned(), Faction::Usa, Kind::Vehicle)];
    let (files, _) =
        crate::skirmish_data::source_files::with_runtime_dependencies(&assets, &mut roster)
            .unwrap();
    for expected in [
        "AmericaInfantryPilot",
        "AmericaParachute",
        "FlamingInfantry",
        "ToxicInfantry",
        "ToxicInfantryBeta",
        "ToxicInfantryGamma",
    ] {
        assert!(
            roster.iter().any(|(name, _, _)| name == expected),
            "missing actual dependency {expected}"
        );
    }
    for name in [
        "AmericaJetRaptor",
        "AmericaJetAurora",
        "AmericaJetStealthFighter",
        "AmericaVehicleHumvee",
        "AmericaVehicleTomahawk",
        "AmericaTankCrusader",
        "AmericaTankPaladin",
        "AmericaTankAvenger",
        "AmericaTankMicrowave",
    ] {
        let object = definition(&files, "Object", name).unwrap();
        let rules = load(&files, object).unwrap();
        assert_eq!(rules.len(), 1, "{name}");
        let rules = &rules[0];
        assert_eq!(rules.conditions.veterancy_mask, 14, "{name}");
        assert_eq!(rules.conditions.exempt_status, 1 << 10, "{name}");
        for (airborne, list) in [
            (false, rules.ground.as_ref().unwrap()),
            (true, rules.airborne.as_ref().unwrap()),
        ] {
            assert_eq!(list.objects.len(), 1);
            let object = &list.objects[0];
            assert_eq!(object.names, ["AmericaInfantryPilot"]);
            assert_eq!(
                object.container,
                if airborne { "AmericaParachute" } else { "" }
            );
            assert_eq!(object.invulnerable_time, 60);
            assert_eq!(object.count, 1);
            assert_eq!(
                object.disposition,
                generals_sim::skirmish::physical_ocl::RANDOM_FORCE
            );
            let expected = if airborne {
                [10.0_f32, 12.0]
            } else {
                [2.0_f32, 3.0]
            };
            assert_eq!(
                object.force_magnitude.map(f32::to_bits),
                expected.map(f32::to_bits)
            );
            assert_eq!(
                object.force_pitch.map(f32::to_bits),
                [50.0_f32.to_radians(), 60.0_f32.to_radians()].map(f32::to_bits)
            );
            assert_eq!(object.spin_rate.to_bits(), 0.0_f32.to_bits());
            let flags = generals_sim::skirmish::object_creation::REQUIRES_LIVE_PLAYER
                | generals_sim::skirmish::object_creation::INHERITS_VETERANCY
                | generals_sim::skirmish::object_creation::IGNORE_PRIMARY_OBSTACLE;
            assert_eq!(object.flags & flags, flags);
        }
    }
}

#[test]
fn nullable_default_and_authored_once_die_instances_preserve_filter_order() {
    let file = content::scan(Asset { path: "eject.ini".into(), origin: "native EjectPilotDie callbacks".into(), bytes: b"Object Parent\nBehavior=EjectPilotDie Empty\nEnd\nBehavior=EjectPilotDie Filtered\nVeterancyLevels=NONE +HEROIC\nExemptStatus=HIJACKED\nInvulnerableTime=2000\nEnd\nEnd\n".to_vec() }).unwrap();
    let rules = load(std::slice::from_ref(&file), &file.declarations[0]).unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].conditions.veterancy_mask, 15);
    assert!(rules[0].ground.is_none() && rules[0].airborne.is_none());
    assert_eq!(rules[1].conditions.veterancy_mask, 8);
    assert_eq!(rules[1].conditions.exempt_status, 1 << 10);
    assert!(rules[1].sounds.iter().all(String::is_empty));
}
