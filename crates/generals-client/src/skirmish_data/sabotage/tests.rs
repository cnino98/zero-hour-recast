use super::*;
use generals_assets::{content, virtual_assets::Asset};

fn object(modules: &str) -> Declaration {
    let mut file = content::scan(Asset {
        path: "saboteur.ini".into(),
        origin: "Saboteur source crate fixture".into(),
        bytes: format!("Object GLAInfantrySaboteur\n{modules}\nEnd\n").into_bytes(),
    })
    .unwrap();
    file.declarations.remove(0)
}

#[test]
fn ordered_crates_keep_distinct_duration_units_and_supply_amount() {
    let object = object(
        "Behavior=SabotagePowerPlantCrateCollide Power\nBuildingPickup=Yes\nSabotagePowerDuration=30000\nEnd\nBehavior=SabotageSupplyCenterCrateCollide Supply\nBuildingPickup=Yes\nStealCashAmount=1000\nEnd\nBehavior=SabotageInternetCenterCrateCollide Internet\nBuildingPickup=Yes\nSabotageDuration=15000\nEnd",
    );
    let rules = load(&object).unwrap();
    assert!(matches!(
        rules[0].effect,
        Effect::PowerPlant { frames: 900 }
    ));
    assert!(matches!(
        rules[1].effect,
        Effect::SupplyCenter { amount: 1000 }
    ));
    assert!(matches!(
        rules[2].effect,
        Effect::InternetCenter { frames: 450 }
    ));
}

#[test]
fn absent_optional_dropzone_module_is_not_reintroduced_and_unknown_crates_fail() {
    assert!(
        load(&object(
            ";Behavior=SabotageSupplyDropzoneCrateCollide Drop\n;End"
        ))
        .unwrap()
        .is_empty()
    );
    assert!(
        load(&object(
            "Behavior=SabotageSupplyDropzoneCrateCollide Drop\nBuildingPickup=Yes\nEnd"
        ))
        .is_err()
    );
    assert!(load(&object("Behavior=SabotageSupplyCenterCrateCollide Supply\nBuildingPickup=Yes\nHumanOnly=Yes\nEnd")).is_err());
    assert!(load(&object("Behavior=SabotageSupplyCenterCrateCollide Supply\nBuildingPickup=Yes\nEnd\nBehavior=SquishCollide Squish\nEnd")).is_err());
}
