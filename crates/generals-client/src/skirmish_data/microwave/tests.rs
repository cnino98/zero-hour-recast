use crate::skirmish_data::{definition, fire::weapon_update};

#[ignore = "requires original game installation"]
#[test]
fn installed_ecm_uses_paid_original_disabler_emitter_and_missile_body() {
    let (_, battle) = crate::cleanup_verification::tests::installed();
    let kind = crate::strategy_verification::template(&battle.simulation, "ChinaTankECM").unwrap();
    let unit = &battle.simulation.catalog[kind];
    assert_eq!(unit.cost, 800);
    assert_eq!(unit.build_seconds.to_bits(), 10.0_f32.to_bits());
    assert_eq!(unit.weapon.name, "ECMTankVehicleDisabler");
    assert!(unit.weapon.laser);
    assert!(matches!(
        unit.weapon.damage_type,
        generals_sim::skirmish::DamageType::SubdualVehicle
    ));
    let [emitter] = unit.behavior.fire_weapons.as_slice() else {
        panic!("original sole emitter");
    };
    let weapon = emitter.weapon.as_ref().unwrap();
    assert_eq!(emitter.exclusive_delay, 30);
    assert_eq!(weapon.name, "ECMTankMissileJammer");
    assert_eq!(weapon.damage.to_bits(), 100.0_f32.to_bits());
    assert_eq!(weapon.delay.to_bits(), (20.0_f32 * (1.0 / 30.0)).to_bits());
    assert_eq!(weapon.area.as_ref().unwrap().affects, 2 | 4);
    let kind = crate::strategy_verification::template(&battle.simulation, "MissileDefenderMissile")
        .unwrap();
    let body = &battle.simulation.catalog[kind];
    assert!(matches!(
        body.body_kind,
        generals_sim::skirmish::BodyKind::Active
    ));
    assert_eq!(
        body.armor[generals_sim::skirmish::DamageType::SubdualMissile as usize].to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        body.maintenance.subdual.as_ref().unwrap().cap.to_bits(),
        200.0_f32.to_bits()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_microwave_emitter_and_body_keep_original_numeric_contracts() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let files = super::super::source_files(&assets).unwrap();
    let object = definition(&files, "Object", "AmericaTankMicrowave").unwrap();
    let modules = weapon_update::load(&files, object).unwrap();
    let [emitter] = modules.as_slice() else {
        panic!("original sole emitter");
    };
    let weapon = emitter.weapon.as_ref().unwrap();
    assert_eq!(weapon.name, "MicrowaveTankEmitterWeapon");
    assert_eq!(emitter.exclusive_delay, 30);
    assert_eq!(emitter.initial_delay, 0);
    assert_eq!(weapon.damage.to_bits(), 8.0_f32.to_bits());
    // INI duration rounds 250ms up to eight logic frames before returning seconds.
    assert_eq!(weapon.delay.to_bits(), (8.0_f32 * (1.0 / 30.0)).to_bits());
    let area = weapon.area.as_ref().unwrap();
    assert!(area.at_source);
    assert_eq!(area.affects, 2 | 64);
    assert_eq!(area.primary_radius.to_bits(), 100.0_f32.to_bits());
    let kind =
        crate::strategy_verification::template(&battle.simulation, "AmericaTankMicrowave").unwrap();
    let unit = &battle.simulation.catalog[kind];
    let rules = unit.maintenance.subdual.as_ref().unwrap();
    assert_eq!(rules.cap.to_bits(), 960.0_f32.to_bits());
    assert_eq!(rules.heal_rate, 15);
    assert_eq!(rules.heal_amount.to_bits(), 50.0_f32.to_bits());
    assert_eq!(
        unit.secondary_weapon.as_ref().unwrap().name,
        "MicrowaveTankBuildingClearer"
    );
    assert_eq!(
        unit.weapon_template(unit.fire_weapon_index(0).unwrap())
            .unwrap()
            .name,
        "MicrowaveTankEmitterWeapon"
    );
    assert_eq!(unit.behavior.drone_upgrades.len(), 3);
}

#[test]
fn generic_emitter_parser_keeps_custom_names_null_ordinals_and_owned_fields() {
    use generals_assets::{content, virtual_assets::Asset};
    let file = content::scan(Asset {
        path: "fire-weapon.ini".into(), origin: "generic authored FireWeaponUpdate".into(),
        bytes: b"Object AmericaTankMicrowave\nBehavior=FireWeaponUpdate First\nWeapon=CustomEmitter\nEnd\nBehavior=FireWeaponUpdate Empty\nWeapon=NONE\nEnd\nBehavior=FireWeaponUpdate Last\nWeapon=OtherEmitter\nEnd\nEnd\nWeapon CustomEmitter\nPrimaryDamage=8\nPrimaryDamageRadius=100\nDelayBetweenShots=250\nDamageType=MICROWAVE\nDeathType=BURNED\nClipSize=1\nScatterRadius=10\nScatterRadiusVsInfantry=5\nProjectileObject=Missile\nEnd\nWeapon OtherEmitter\nPrimaryDamage=9\nEnd\n".to_vec(),
    }).unwrap();
    let modules = weapon_update::load(std::slice::from_ref(&file), &file.declarations[0]).unwrap();
    assert_eq!(modules.len(), 3);
    let weapon = modules[0].weapon.as_ref().unwrap();
    assert_eq!(weapon.name, "CustomEmitter");
    assert_eq!(weapon.clip_size, 1);
    assert_eq!(weapon.scatter.to_bits(), 10.0_f32.to_bits());
    assert_eq!(weapon.infantry_scatter.to_bits(), 5.0_f32.to_bits());
    assert!(modules[1].weapon.is_none());
    assert_eq!(modules[2].weapon.as_ref().unwrap().name, "OtherEmitter");
    // The later projectile dependency/art binding reads the retained source
    // definition. Parsing this reference alone does not admit a missing Missile.
    let raw = definition(std::slice::from_ref(&file), "Weapon", "CustomEmitter").unwrap();
    assert_eq!(
        crate::skirmish_data::scalar(
            crate::skirmish_data::field(&raw.lines, 0, "ProjectileObject").unwrap()
        )
        .unwrap(),
        "Missile"
    );
}

#[ignore = "requires original game installation"]
#[test]
fn paid_original_microwave_and_ecm_emit_the_authored_module_binding() {
    use crate::skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    };
    use generals_sim::skirmish::{Faction, Kind};
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    for (faction, enemy, prerequisite, name, weapon_name) in [
        (
            Faction::Usa,
            Faction::China,
            "AmericaStrategyCenter",
            "AmericaTankMicrowave",
            "MicrowaveTankEmitterWeapon",
        ),
        (
            Faction::China,
            Faction::Usa,
            "ChinaPropagandaCenter",
            "ChinaTankECM",
            "ECMTankMissileJammer",
        ),
    ] {
        game.reset(faction, enemy).unwrap();
        let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
        construct(game, builder, Kind::PowerPlant, 0).unwrap();
        construct(game, builder, Kind::SupplyCenter, 0).unwrap();
        let factory = construct(game, builder, Kind::Factory, 0).unwrap();
        let prerequisite = crate::strategy_verification::template(game, prerequisite).unwrap();
        construct_type(game, builder, prerequisite, 0).unwrap();
        let kind = crate::strategy_verification::template(game, name).unwrap();
        let tank = produce_type(game, factory, kind, 0).unwrap();
        let kind = game.entities[tank].type_index;
        let unit = &game.catalog[kind];
        let [module] = unit.behavior.fire_weapons.as_slice() else {
            panic!("original sole emitter");
        };
        let index = unit.fire_weapon_index(0).unwrap();
        assert!(unit.fire_weapon_index(1).is_none());
        assert!(std::ptr::eq(
            unit.weapon_template(index).unwrap(),
            module.weapon.as_ref().unwrap()
        ));
        assert_eq!(unit.weapon_template(index).unwrap().name, weapon_name);
        game.stop(tank).unwrap();
        advance_until(game, 300, "actual authored emitter shot", |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == tank && shot.weapon_index == index)
        })
        .unwrap();
        let shot = game
            .presentation
            .shots
            .iter()
            .find(|shot| shot.source == tank && shot.weapon_index == index)
            .unwrap();
        assert_eq!(shot.weapon_slot, 0);
        assert!(shot.target.is_none());
        assert!(std::ptr::eq(
            game.catalog[kind]
                .weapon_template(shot.weapon_index)
                .unwrap(),
            game.catalog[kind].behavior.fire_weapons[0]
                .weapon
                .as_ref()
                .unwrap()
        ));
    }
}
