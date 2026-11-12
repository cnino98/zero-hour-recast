#[ignore = "requires original game installation"]
#[test]
fn installed_helix_uses_original_container_attachments_and_persistent_bomb_data() {
    use generals_assets::virtual_assets::VirtualAssets;
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let data = crate::skirmish_data::load(&assets, &[]).unwrap();
    let unit = data
        .types
        .iter()
        .find(|unit| unit.name == "ChinaVehicleHelix")
        .unwrap();
    // Helix inherits Chinook AI's collector defaults without acquiring supply capacity.
    assert_eq!(unit.supply.scan_distance.to_bits(), 100.0_f32.to_bits());
    assert!(unit.maintenance.collector_boost.is_none());
    assert_eq!(unit.cost, 1500);
    assert_eq!(unit.max_health.to_bits(), 300.0_f32.to_bits());
    assert_eq!(unit.supply.capacity, 0);
    assert!(unit.chinook.is_some());
    assert!(unit.aircraft.is_none());
    let contain = unit.contain.as_ref().unwrap();
    assert_eq!(contain.capacity, 5);
    assert_eq!(
        contain.allowed,
        ["INFANTRY", "VEHICLE", "PORTABLE_STRUCTURE"]
    );
    assert_eq!(contain.forbidden, ["AIRCRAFT"]);
    assert_eq!(contain.exit_frames, 3);
    assert!(!contain.weapons.fire);
    assert_eq!(unit.behavior.attachments.len(), 3);
    let ability = unit.behavior.special_abilities.iter().find(|rules| {
        rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialHelixNapalmBomb)
    }).unwrap();
    let power = unit
        .special_powers
        .iter()
        .find(|power| power.name == ability.power.as_ref().unwrap().name)
        .unwrap();
    let bomb = data
        .types
        .iter()
        .find(|unit| unit.name == ability.objects.name)
        .unwrap();
    let physics = &bomb.physical.as_ref().unwrap().motion;
    assert_eq!(
        unit.behavior_modules
            .iter()
            .find(|module| module.family == "PassengersFireUpgrade")
            .unwrap()
            .upgrade
            .triggers,
        ["Upgrade_ChinaHelixBattleBunker"]
    );
    assert_eq!(ability.objects.name, "NapalmBomb");
    assert_eq!(ability.approach.start_range.to_bits(), 3.0_f32.to_bits());
    assert_eq!(ability.timing.unpack, 15);
    assert_eq!(ability.timing.persistent, 3);
    assert_eq!(power.timer.reload_frames, 300);
    assert_eq!(physics.center_of_mass_offset.to_bits(), 0.13_f32.to_bits());
    assert_eq!(power.module_initiate_sound, "HelixVoiceAttack");
    let trigger = &unit
        .behavior_modules
        .iter()
        .find(|module| module.family == "UnpauseSpecialPowerUpgrade")
        .unwrap()
        .upgrade
        .triggers[0];
    let napalm = unit
        .maintenance
        .research
        .iter()
        .find(|upgrade| &upgrade.name == trigger)
        .unwrap();
    assert_eq!(napalm.cost, 800);
    assert_eq!(napalm.frames, 600);
    assert!(!napalm.player);
}
