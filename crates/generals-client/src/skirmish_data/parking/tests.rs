use super::*;
use generals_assets::{content, virtual_assets::Asset};

#[test]
fn original_door_limit_and_module_boolean_defaults_are_explicit() {
    let file = content::scan(Asset {
        path: "airfield.ini".into(),
        origin: "ParkingPlaceBehavior module bounds".into(),
        bytes: b"Object Airfield\nBehavior=ParkingPlaceBehavior Parking\nNumRows=2\nNumCols=2\nHasRunways=Yes\nEnd\nEnd\n".to_vec(),
    }).unwrap();
    let object = &file.declarations[0];
    let modules = blocks(&object.lines, 0, "Behavior").unwrap();
    let module = modules[0];
    assert_eq!(dimensions(module).unwrap(), (2, 2));
    assert!(boolean(module, "HasRunways", false).unwrap());
    assert!(!boolean(module, "ParkInHangars", false).unwrap());
    assert_eq!(
        real(module, "LandingDeckHeightOffset").unwrap().to_bits(),
        0.0_f32.to_bits()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_airfields_bind_all_four_spaces_to_original_model_bones() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = super::super::source_files(&assets).unwrap();
    for (name, hangars) in [("AmericaAirfield", false), ("ChinaAirfield", true)] {
        let object = super::super::definition(&files, "Object", name).unwrap();
        let model = super::super::normal_model(&assets, object).unwrap();
        let rules = load(object).unwrap().unwrap();
        assert_eq!((rules.rows, rules.columns), (2, 2));
        assert!(rules.has_runways);
        assert_eq!(rules.park_in_hangars, hangars);
        let doors = super::super::production::doors(object).unwrap();
        assert_eq!(doors.opening, 60);
        assert_eq!(doors.waiting, 90);
        assert_eq!(doors.closing, 60);
        assert_eq!(rules.heal_per_second.to_bits(), 10.0_f32.to_bits());
        for name in ["Runway1Park1Han", "Runway2Parking2", "HeliPark01"] {
            assert!(model.bone_position(name).is_some());
        }
    }
}
