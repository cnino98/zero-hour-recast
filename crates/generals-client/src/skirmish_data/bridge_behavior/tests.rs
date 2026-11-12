use super::*;
use generals_assets::{
    content,
    virtual_assets::{Asset, VirtualAssets},
};
use generals_sim::skirmish::{Faction, Kind};

fn files(extra: &str) -> Vec<ContentFile> {
    vec![content::scan(Asset {
        path: "bridge.ini".into(), origin: "bounded source fixture".into(),
        bytes: format!("Object MonumentTrainBridge\nBehavior=BridgeBehavior Bridge\nLateralScaffoldSpeed=100\nVerticalScaffoldSpeed=70\n{extra}\nEnd\nEnd\nBridge MonumentTrainBridge\nRadarColor=R:192 G:192 B:192\nEnd\n").into_bytes(),
    }).unwrap()]
}

#[test]
fn bridge_parser_preserves_native_velocity_and_rejects_unimplemented_timed_routes() {
    let source = files("");
    let rules = load(
        &source,
        definition(&source, "Object", "MonumentTrainBridge").unwrap(),
    )
    .unwrap();
    assert_eq!(
        rules[0].lateral_scaffold_speed.to_bits(),
        (100.0_f32 * (1.0 / 30.0)).to_bits()
    );
    assert_eq!(
        rules[0].vertical_scaffold_speed.to_bits(),
        (70.0_f32 * (1.0 / 30.0)).to_bits()
    );
    for entry in [
        "BridgeDieFX=FX:FX_RailroadBridgeCollapse Delay:100",
        "BridgeDieOCL=OCL:OCL_StructureCollapseFinal Delay:100",
    ] {
        let source = files(entry);
        assert!(
            load(
                &source,
                definition(&source, "Object", "MonumentTrainBridge").unwrap()
            )
            .is_err()
        );
    }
}

#[ignore = "requires original game installation"]
#[test]
fn original_bridge_templates_bind_update_damage_die_and_retain_immortal_and_rubble_contracts() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let names = ["GenericBridge", "MonumentTrainBridge"];
    let (source, metadata) =
        crate::skirmish_data::source_files::effective(&assets, &names).unwrap();
    for name in names {
        let object = definition(&source, "Object", name).unwrap();
        let unit =
            crate::skirmish_data::unit_type(&source, object, Faction::Civilian, Kind::TechBuilding)
                .unwrap();
        let mut unit = crate::skirmish_data::finish_type(&source, object, unit).unwrap();
        let authored = metadata.get(name).unwrap();
        unit.update_modules = authored.runtime_registrations(false);
        unit.behavior_modules = authored.runtime_behaviors(&source).unwrap();
        generals_sim::skirmish::module_updates::validate_registrations(&unit).unwrap();
        assert_eq!(unit.behavior.bridges.len(), 1);
        assert!(
            unit.behavior_modules
                .iter()
                .any(|module| module.family == "BridgeBehavior" && module.interfaces == 7)
        );
        assert_eq!(
            unit.behavior.bridges[0].lateral_scaffold_speed.to_bits(),
            (100.0_f32 * (1.0 / 30.0)).to_bits()
        );
        if name == "GenericBridge" {
            assert_eq!(unit.minimum_health.to_bits(), 1.0_f32.to_bits());
            assert_eq!(unit.max_health.to_bits(), 1.0_f32.to_bits());
        } else {
            assert_eq!(unit.max_health.to_bits(), 2000.0_f32.to_bits());
            assert!(unit.kind_of.iter().any(|kind| kind == "UNATTACKABLE"));
            assert_eq!(
                unit.behavior.death_fx[0].effect,
                "FX_RailroadBridgeCollapse"
            );
            assert!(
                object
                    .lines
                    .iter()
                    .any(|line| line.tokens == ["ConditionState", "RUBBLE"])
            );
            assert!(
                object
                    .lines
                    .iter()
                    .any(|line| line.tokens == ["Animation", "CMTBridge_A1.CMTBridge_A1"])
            );
        }
    }
}
