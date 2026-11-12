use super::*;
use generals_assets::{
    content,
    virtual_assets::{Asset, VirtualAssets},
};

fn read(body: &str) -> ContentFile {
    content::scan(Asset { path: "bonefx.ini".into(), origin: "native fixed-Once BoneFX contract".into(), bytes: format!("Object Stream\nBehavior=BoneFXUpdate Update\n{body}\nEnd\nBehavior=BoneFXDamage Damage\nEnd\nEnd\nParticleSystem First\nEnd\nParticleSystem Sixth\nEnd\nFXList Rubble\nEnd\n").into_bytes() }).unwrap()
}

#[test]
fn fixed_slots_keep_numeric_identity_null_token_and_duration_real_truncation() {
    let file = read(
        "PristineParticleSystem6=bone:NULL OnlyOnce:Yes 0 0 PSys:Sixth\nPristineParticleSystem1=bone:NULL OnlyOnce:Yes 0 0 PSys:First\nRubbleFXList1=bone:Smoke01 OnlyOnce:Yes 99 99 FXList:Rubble\nDamageFXTypes=ALL -WATER",
    );
    let rules = load(std::slice::from_ref(&file), &file.declarations[0]).unwrap();
    assert_eq!(
        rules[0].slots[0][0].particle.as_ref().unwrap().name,
        "First"
    );
    assert_eq!(
        rules[0].slots[0][5].particle.as_ref().unwrap().name,
        "Sixth"
    );
    assert_eq!(rules[0].slots[0][0].particle.as_ref().unwrap().bone, "NULL");
    assert_eq!(rules[0].slots[3][0].fx.as_ref().unwrap().delay, 2);
    assert_eq!(
        rules[0].damage_fx & (1 << generals_sim::skirmish::DamageType::Water as u32),
        0
    );
    for body in [
        "PristineParticleSystem1=bone:NULL OnlyOnce:No 0 0 PSys:First",
        "PristineParticleSystem1=bone:NULL OnlyOnce:Yes 0 1 PSys:First",
        "PristineOCL1=bone:NULL OnlyOnce:Yes 0 0 OCL:Actual",
        "PristineParticleSystem9=bone:NULL OnlyOnce:Yes 0 0 PSys:First",
    ] {
        let file = read(body);
        assert!(
            load(std::slice::from_ref(&file), &file.declarations[0]).is_err(),
            "{body}"
        );
    }
    let mut file = read("PristineParticleSystem1=bone:NULL OnlyOnce:Yes 0 0 PSys:First");
    let mut missing = file.declarations.remove(0);
    let start = missing
        .lines
        .iter()
        .position(|line| {
            line.tokens
                .get(1)
                .is_some_and(|family| family == "BoneFXDamage")
        })
        .unwrap();
    missing.lines.drain(start..start + 2);
    assert!(load(&[file], &missing).is_err());
}

#[ignore = "requires original game installation"]
#[test]
fn installed_streams_and_tower_bind_original_fixed_once_modules() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = crate::skirmish_data::source_files(&assets).unwrap();
    for (name, expected) in [
        ("CleanupStreamProjectile", vec!["CleanupTrail01"]),
        ("BioHazardCleanupStreamProjectile", vec!["CleanupTrail01"]),
        ("ToxinTruckStreamProjectile", vec!["ToxinTrail01"]),
        ("ToxinTruckStreamProjectileUpgraded", vec!["AnthraxTrail01"]),
        (
            "DragonTankFlameProjectile",
            vec![
                "FlameThrowerTrail01",
                "FlameThrowerTrail02",
                "FlameThrowerTrail03",
                "FlameThrowerTrail04",
                "FlameThrowerTrail06",
                "FlameThrowerSmoke01",
            ],
        ),
        (
            "DragonTankFlameProjectileUpgraded",
            vec![
                "FlameThrowerTrail01Upgraded",
                "FlameThrowerTrail02Upgraded",
                "FlameThrowerTrail03Upgraded",
                "FlameThrowerTrail04Upgraded",
                "FlameThrowerTrail06Upgraded",
                "FlameThrowerSmoke01",
            ],
        ),
    ] {
        let rules = load(&files, definition(&files, "Object", name).unwrap()).unwrap();
        assert_eq!(rules.len(), 1, "{name}");
        let particles: Vec<_> = rules[0].slots[0]
            .iter()
            .filter_map(|slot| slot.particle.as_ref())
            .collect();
        assert_eq!(
            particles
                .iter()
                .map(|particle| particle.name.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{name}"
        );
        assert!(
            particles
                .iter()
                .all(|particle| particle.delay == 0 && particle.bone == "NULL")
        );
    }
    let tower = load(
        &files,
        definition(&files, "Object", "MogadishuTower04").unwrap(),
    )
    .unwrap();
    assert_eq!(tower[0].slots[3][0].fx.as_ref().unwrap().delay, 90);
    assert_eq!(tower[0].slots[3][0].particle.as_ref().unwrap().delay, 90);
}
