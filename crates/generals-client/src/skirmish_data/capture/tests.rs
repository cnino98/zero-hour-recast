use super::*;
use generals_assets::{content, virtual_assets::Asset};
use std::fmt::Write;

#[test]
fn deposit_pairs_retain_order_signed_boost_and_nullable_upgrade_lookup() {
    let file = content::scan(Asset {
        path: "oil.ini".into(),
        origin: "native ordered boost pairs".into(),
        bytes: b"Object Oil\nBehavior=AutoDepositUpdate Income\nUpgradedBoost=UpgradeType:First Boost:-20\nUpgradedBoost=UpgradeType:Missing Boost:40\nEnd\nEnd\nUpgrade First\nEnd\n".to_vec(),
    }).unwrap();
    let pairs = deposit_upgrades(std::slice::from_ref(&file), &file.declarations[0].lines).unwrap();
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].trigger.as_deref(), Some("First"));
    assert_eq!(pairs[0].amount, -20);
    assert_eq!(pairs[1].trigger, None);
    assert_eq!(pairs[1].amount, 40);
    let mut bad = file;
    let line = bad.declarations[0]
        .lines
        .iter_mut()
        .find(|line| {
            line.tokens
                .first()
                .is_some_and(|key| key == "UpgradedBoost")
        })
        .unwrap();
    line.tokens[2] = "Wrong:20".into();
    assert!(deposit_upgrades(std::slice::from_ref(&bad), &bad.declarations[0].lines).is_err());
}

#[test]
fn economy_defaults_and_repeated_module_data_preserve_native_values() {
    let file = content::scan(Asset {
        path: "economy.ini".into(), origin: "source native module defaults".into(),
        bytes: b"Object Oil\nBehavior=AutoDepositUpdate First\nEnd\nBehavior=AutoDepositUpdate Second\nDepositTiming=1000\nDepositAmount=-3\nActualMoney=No\nEnd\nBehavior=PowerPlantUpdate Rods\nRodsExtendTime=500\nEnd\nBehavior=OverchargeBehavior Hot\nHealthPercentToDrainPerSecond=2%\nNotAllowedWhenHealthBelowPercent=50%\nEnd\nEnergyBonus=100\nEnd\n".to_vec(),
    }).unwrap();
    let mut rules = MaintenanceRules::none();
    load(
        std::slice::from_ref(&file),
        &file.declarations[0],
        &mut rules,
    )
    .unwrap();
    assert_eq!(rules.deposits.len(), 2);
    assert_eq!(rules.deposits[0].interval, 0);
    assert!(rules.deposits[0].actual_money);
    assert_eq!(rules.deposits[1].interval, 30);
    assert_eq!(rules.deposits[1].amount, -3);
    assert!(!rules.deposits[1].actual_money);
    assert_eq!(rules.power_plants, [15]);
    assert_eq!(
        rules.overcharges[0].drain.to_bits(),
        (2.0_f32 / 100.0).to_bits()
    );
    assert_eq!(rules.overcharges[0].threshold.to_bits(), 0.5_f32.to_bits());
    assert_eq!(rules.overcharges[0].energy_bonus, 100);
}

#[test]
fn lotus_finite_powers_preserve_original_ranges_timers_and_distinct_effects() {
    let modules = [
        (
            "SpecialAbilityBlackLotusCaptureBuilding",
            6730,
            2800,
            6000,
            0,
            20,
            "DoCaptureFX=Yes",
        ),
        (
            "SpecialAbilityBlackLotusDisableVehicleHack",
            2000,
            1000,
            2000,
            0,
            0,
            "EffectDuration=15000",
        ),
        (
            "SpecialAbilityBlackLotusStealCashHack",
            6730,
            5800,
            6000,
            2000,
            20,
            // Native profile intentionally ignores this value for cash theft.
            "EffectValue=500",
        ),
    ];
    let mut source = String::from("Object ChinaInfantryBlackLotus\n");
    for (power, unpack, pack, preparation, _, xp, effect) in modules {
        writeln!(source, "Behavior=SpecialAbility Power\nSpecialPowerTemplate={power}\nUpdateModuleStartsAttack=Yes\nEnd\nBehavior=SpecialAbilityUpdate Update\nSpecialPowerTemplate={power}\nStartAbilityRange=150\nUnpackTime={unpack}\nPackTime={pack}\nPreparationTime={preparation}\nSpecialObject=BinaryDataStream\nAwardXPForTriggering={xp}\n{effect}\nEnd").unwrap();
    }
    source.push_str("End\n");
    for (power, _, _, _, reload, _, _) in modules {
        let kind = match power {
            "SpecialAbilityBlackLotusCaptureBuilding" => "SPECIAL_BLACKLOTUS_CAPTURE_BUILDING",
            "SpecialAbilityBlackLotusDisableVehicleHack" => {
                "SPECIAL_BLACKLOTUS_DISABLE_VEHICLE_HACK"
            }
            _ => "SPECIAL_BLACKLOTUS_STEAL_CASH_HACK",
        };
        writeln!(
            source,
            "SpecialPower {power}\nEnum={kind}\nReloadTime={reload}\nEnd"
        )
        .unwrap();
    }
    source.push_str("GameData\nEnd\n");
    let files = [content::scan(Asset {
        path: "lotus.ini".into(),
        origin: "native Lotus ability field contract".into(),
        bytes: source.into_bytes(),
    })
    .unwrap()];
    let abilities =
        crate::skirmish_data::special_ability::load(&files, &files[0].declarations[0]).unwrap();
    let powers =
        crate::skirmish_data::special_power::load(&files, &files[0].declarations[0]).unwrap();
    assert_eq!(abilities.len(), 3);
    assert_eq!(powers.len(), 3);
    let capture = &abilities[0];
    assert_eq!(
        [
            capture.timing.unpack,
            capture.timing.pack,
            capture.timing.preparation,
            powers[0].timer.reload_frames
        ],
        [202, 84, 180, 0]
    );
    assert!(capture.preparation.capture_fx);
    let vehicle = &abilities[1];
    assert_eq!(vehicle.timing.effect, 450);
    assert_eq!(
        [
            vehicle.timing.unpack,
            vehicle.timing.pack,
            vehicle.timing.preparation,
            powers[1].timer.reload_frames
        ],
        [60, 30, 60, 0]
    );
    let cash = &abilities[2];
    assert_eq!(cash.effects.value, 500);
    assert_eq!(
        [
            cash.timing.unpack,
            cash.timing.pack,
            cash.timing.preparation,
            powers[2].timer.reload_frames
        ],
        [202, 174, 180, 60]
    );
    assert_eq!(
        [vehicle.effects.experience, cash.effects.experience],
        [0, 20]
    );
}

#[test]
fn unsigned_economy_duration_uses_native_binary32_before_ceil() {
    let file = content::scan(Asset {
        path: "economy-duration.ini".into(),
        origin: "INI.cpp1798 binary32 unsigned duration".into(),
        bytes: b"Object Oil\nBehavior=AutoDepositUpdate Income\nDepositTiming=4294967295\nEnd\nBehavior=PowerPlantUpdate Rods\nRodsExtendTime=4294967295\nEnd\nEnd\n".to_vec(),
    }).unwrap();
    let mut rules = MaintenanceRules::none();
    load(
        std::slice::from_ref(&file),
        &file.declarations[0],
        &mut rules,
    )
    .unwrap();
    // UIntMAX rounds to2^32; binary32(30/1000)*2^32 is128849016.
    assert_eq!(rules.deposits[0].interval, 128_849_016);
    assert_eq!(rules.power_plants, [128_849_016]);
}
