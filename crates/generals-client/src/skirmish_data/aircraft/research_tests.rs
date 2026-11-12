use super::super::{definition, finish_type, source_files, unit_type};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::{Faction, Kind};

#[ignore = "requires original game installation"]
#[test]
fn installed_aircraft_research_uses_original_projectile_bonus_and_health_module() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = source_files(&assets).unwrap();
    // Catalog admission loads conditional weapons and their dependencies in
    // finish_type after the base object fields are parsed.
    for (name, faction, research, cost) in [
        (
            "AmericaAirfield",
            Faction::Usa,
            "Upgrade_AmericaLaserMissiles",
            1500,
        ),
        (
            "ChinaAirfield",
            Faction::China,
            "Upgrade_ChinaAircraftArmor",
            1000,
        ),
    ] {
        let object = definition(&files, "Object", name).unwrap();
        let unit = unit_type(&files, object, faction, Kind::Factory)
            .and_then(|unit| finish_type(&files, object, unit))
            .unwrap();
        let upgrade = unit
            .maintenance
            .research
            .iter()
            .find(|upgrade| upgrade.name == research)
            .expect("original aircraft research command");
        assert_eq!(upgrade.cost, cost);
        assert_eq!(upgrade.frames, 1200);
        assert!(upgrade.player);
    }
    let object = definition(&files, "Object", "AmericaJetRaptor").unwrap();
    let raptor = unit_type(&files, object, Faction::Usa, Kind::Vehicle)
        .and_then(|unit| finish_type(&files, object, unit))
        .unwrap();
    assert!(raptor.maintenance.weapon_upgrade.is_none());
    assert_eq!(
        raptor.weapon.projectile.upgrade.as_deref(),
        Some("Upgrade_AmericaLaserMissiles")
    );
    assert_eq!(
        raptor.weapon.bonuses.template[5][0].to_bits(),
        1.25_f32.to_bits()
    );
    let object = definition(&files, "Object", "ChinaJetMIG").unwrap();
    let mig = unit_type(&files, object, Faction::China, Kind::Vehicle)
        .and_then(|unit| finish_type(&files, object, unit))
        .unwrap();
    let health = &mig.maintenance.health_upgrades;
    assert_eq!(health.len(), 1);
    assert_eq!(health[0].trigger, "Upgrade_ChinaAircraftArmor");
    assert_eq!(health[0].amount.to_bits(), 40.0_f32.to_bits());
    assert_eq!(mig.weapon_upgrade_triggers, ["Upgrade_ChinaBlackNapalm"]);
    assert!(mig.weapon_set_rules.templates.iter().any(|weapon| {
        weapon.flags == 1 && weapon.slot == 0 && weapon.rules.name == "BlackNapalmMissileWeapon"
    }));
}

#[ignore = "requires original game installation"]
#[test]
fn installed_mig_napalm_keeps_distinct_damage_reload_and_eight_hit_histories() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = source_files(&assets).unwrap();
    let object = definition(&files, "Object", "ChinaJetMIG").unwrap();
    let mig = unit_type(&files, object, Faction::China, Kind::Vehicle)
        .and_then(|unit| finish_type(&files, object, unit))
        .unwrap();
    let upgraded = &mig
        .weapon_set_rules
        .templates
        .iter()
        .find(|weapon| weapon.flags == 1 && weapon.slot == 0)
        .unwrap()
        .rules;
    for (weapon, reload, field, storm) in [
        (
            &mig.weapon,
            8.0_f32,
            "FireFieldSmall",
            "FirestormSmallCreationWeapon",
        ),
        (
            upgraded,
            2.0_f32,
            "FireFieldUpgradedSmall",
            "BlackNapalmFirestormSmallCreationWeapon",
        ),
    ] {
        assert_eq!(weapon.clip_size, 2);
        assert_eq!(weapon.reload_seconds.to_bits(), reload.to_bits());
        assert!(matches!(
            weapon.reload,
            generals_sim::skirmish::ClipReload::ReturnToBase
        ));
        let incendiary = weapon.incendiary.as_ref().unwrap();
        assert_eq!(incendiary.impact.as_ref().unwrap().names, [field]);
        let historic = &incendiary.historic.as_ref().unwrap().rules;
        assert_eq!(historic.count, 8);
        assert_eq!(historic.frames, 90);
        assert_eq!(historic.radius.to_bits(), 100.0_f32.to_bits());
        assert_eq!(historic.weapon, storm);
    }
    assert!(matches!(
        mig.weapon.damage_type,
        generals_sim::skirmish::DamageType::JetMissiles
    ));
    assert!(matches!(
        upgraded.damage_type,
        generals_sim::skirmish::DamageType::Explosion
    ));
    assert!(matches!(
        mig.weapon.death_type,
        generals_sim::skirmish::DeathType::Burned
    ));
    assert!(matches!(
        upgraded.death_type,
        generals_sim::skirmish::DeathType::Exploded
    ));
}
