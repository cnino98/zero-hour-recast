use super::*;
use generals_assets::{
    content,
    virtual_assets::{Asset, VirtualAssets},
};

#[ignore = "requires original game installation"]
#[test]
fn defaults_and_original_installed_neutron_module_bind_real_update_and_die() {
    let file = content::scan(Asset {
        path: "neutron.ini".into(),
        origin: "native defaults".into(),
        bytes: b"Object Blast\nBehavior=NeutronBlastBehavior Blast\nEnd\nEnd\n".to_vec(),
    })
    .unwrap();
    let rules = load(&file.declarations[0]).unwrap();
    assert_eq!(rules[0].radius.to_bits(), 10.0_f32.to_bits());
    assert!(rules[0].affect_airborne && rules[0].affect_allies);
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut roster = vec![(
        "NeutronBlastObject".to_owned(),
        generals_sim::skirmish::Faction::Civilian,
        generals_sim::skirmish::Kind::Effect,
    )];
    let (files, metadata) =
        crate::skirmish_data::source_files::with_runtime_dependencies(&assets, &mut roster)
            .unwrap();
    let object = crate::skirmish_data::definition(&files, "Object", "NeutronBlastObject").unwrap();
    let rules = load(object).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].radius.to_bits(), 20.0_f32.to_bits());
    assert!(!rules[0].affect_airborne && !rules[0].affect_allies);
    let unit = crate::skirmish_data::unit_type(
        &files,
        object,
        generals_sim::skirmish::Faction::Civilian,
        generals_sim::skirmish::Kind::Effect,
    )
    .unwrap();
    let mut unit = crate::skirmish_data::finish_type(&files, object, unit).unwrap();
    let ordered = metadata.get("NeutronBlastObject").unwrap();
    unit.update_modules = ordered.runtime_registrations(false);
    unit.behavior_modules = ordered.runtime_behaviors(&files).unwrap();
    generals_sim::skirmish::module_updates::validate_registrations(&unit).unwrap();
    assert_eq!(unit.behavior.neutron_blast.len(), 1);
    assert!(
        unit.behavior_modules
            .iter()
            .any(|module| module.family == "NeutronBlastBehavior" && module.interfaces & 3 == 3)
    );
}
