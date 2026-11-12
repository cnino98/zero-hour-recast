use generals_sim::skirmish::{DamageType, FlightRules};

#[ignore = "requires original game installation"]
#[test]
fn installed_tractor_retains_six_sets_real_zero_primary_sprayer_and_original_upgrade_edges() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut data = super::super::load(&assets, &[]).unwrap();
    let index = data
        .types
        .iter()
        .position(|unit| unit.name == "GLAVehicleToxinTruck")
        .unwrap();
    let unit = &data.types[index];
    assert_eq!(unit.cost, 600);
    assert_eq!(unit.behavior.contamination.len(), 2);
    assert_eq!(unit.behavior.contamination[0].slot, 1);
    assert_eq!(unit.behavior.contamination[0].minimum_shots, 4);
    assert_eq!(unit.behavior.contamination[0].maximum_lifetime, 5400);
    assert!(matches!(
        unit.behavior.contamination[0].field.death_type,
        generals_sim::skirmish::DeathType::Poisoned
    ));
    assert!(matches!(
        unit.behavior.contamination[1].field.death_type,
        generals_sim::skirmish::DeathType::PoisonedBeta
    ));
    let triggers = ["Upgrade_GLAAnthraxBeta", "Upgrade_Veterancy_HEROIC"];
    assert_eq!(unit.behavior.contamination[0].conflicts, triggers);
    assert_eq!(unit.behavior.contamination[1].triggers, triggers);
    assert!(!unit.behavior.contamination[1].requires_all);
    assert_eq!(unit.weapon_upgrade_triggers, triggers);
    assert_eq!(unit.weapon_set_rules.replacements.len(), 6);
    assert_eq!(unit.behavior.death_weapons.len(), 2);
    for (set, suffix) in unit.weapon_set_rules.replacements.iter().zip([
        "",
        "Upgraded",
        "PlusOne",
        "UpgradedPlusOne",
        "PlusTwo",
        "UpgradedPlusTwo",
    ]) {
        for (slot, prefix) in ["ToxinTruckGun", "ToxinTruckSprayer"].iter().enumerate() {
            let weapon = unit.weapon_template(set.weapons[slot].unwrap()).unwrap();
            assert_eq!(weapon.name, format!("{prefix}{suffix}"));
        }
    }
    let animations = crate::model_animation::load(&assets, &mut data.models, &data.types).unwrap();
    crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations).unwrap();
    let unit = &data.types[index];
    let mut sprayers = 0;
    for index in 0..unit.weapon_template_count() {
        let Some(weapon) = unit.weapon_template(index) else {
            continue;
        };
        if !weapon.name.starts_with("ToxinTruckSprayer") {
            continue;
        }
        sprayers += 1;
        assert_eq!(weapon.damage.to_bits(), 0.0_f32.to_bits());
        assert!(
            weapon
                .area
                .as_ref()
                .is_some_and(|area| area.secondary_damage > 0.0)
        );
        assert_eq!(weapon.auto_choose, 0);
        assert!(matches!(weapon.damage_type, DamageType::Poison));
        let motion = weapon
            .motion
            .as_ref()
            .expect("zero-primary source sprayer must have a real missile");
        let FlightRules::Missile(missile) =
            &data.types[motion.projectile_type.unwrap()].projectile_rules[0]
        else {
            panic!("original sprayer is MissileAIUpdate");
        };
        assert!(!missile.follow_target);
        assert_eq!(missile.fuel_lifetime, 1);
        assert!(motion.launches.iter().all(|bank| !bank.is_empty()));
    }
    assert_eq!(sprayers, 6);
    animations
        .verify_tractor_art(&data.models, &data.types)
        .unwrap();
    drop(crate::projectile_stream::load(&assets, &data.types).unwrap());
}
