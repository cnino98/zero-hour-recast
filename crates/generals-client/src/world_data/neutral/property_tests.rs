use super::*;

#[ignore = "requires original game installation"]
#[test]
fn original_cairo_and_desert_properties_retain_native_values() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    for name in ["Cairo Commandos", "Tournament Desert"] {
        let map = generals_assets::map::MapData::parse(
            &assets
                .read(&format!("maps/{name}/{name}.map"))
                .unwrap()
                .bytes,
        )
        .unwrap();
        let mut layout = super::super::map_layout(&assets, &map.objects).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        let mut selected = 0;
        for record in map.objects.iter().filter(|record| {
            matches!(
                record.template.as_str(),
                "RoadBarricade01"
                    | "WaterTower"
                    | "GasStorageTank01"
                    | "TourBus"
                    | "DoubleDeckerTourBus"
                    | "CarScrap"
            )
        }) {
            register(&mut layout, record).unwrap();
            seen.insert(record.template.as_str());
            let placed = layout.neutral_objects.last().unwrap();
            assert_eq!(
                placed.initial_health,
                property_int(record, "objectInitialHealth").unwrap()
            );
            assert_eq!(
                placed.attitude,
                property_int(record, "objectAggressiveness").unwrap()
            );
            if let Some(MapValue::Bool(value)) = record.properties.get("objectRecruitableAI") {
                assert_eq!(placed.recruitable, Some(*value));
            }
            if record.template == "RoadBarricade01" {
                assert!(matches!(
                    record.properties.get("objectRecruitableAI"),
                    Some(MapValue::Bool(false))
                ));
            }
            let id = match record.properties.get("uniqueID") {
                Some(MapValue::Ascii(value)) => value.as_str(),
                _ => panic!("original selected map record lost its uniqueID"),
            };
            let sleeping = matches!(
                id,
                "TourBus 1303" | "DoubleDeckerTourBus 1302" | "CarScrap 58"
            );
            assert_eq!(
                property_int(record, "objectAggressiveness").unwrap(),
                sleeping.then_some(-2)
            );
            assert_eq!(
                property_int(record, "objectMaxHPs").unwrap(),
                (id == "CarScrap 58").then_some(-1)
            );
            selected += 1;
        }
        assert_eq!(
            seen,
            if name == "Cairo Commandos" {
                [
                    "RoadBarricade01",
                    "WaterTower",
                    "GasStorageTank01",
                    "TourBus",
                    "DoubleDeckerTourBus",
                    "CarScrap",
                ]
                .into_iter()
                .collect()
            } else {
                std::iter::once("CarScrap").collect()
            }
        );
        assert_eq!(selected, if name == "Cairo Commandos" { 12 } else { 1 });
        let half = map
            .objects
            .iter()
            .filter(|record| property_int(record, "objectInitialHealth").unwrap() == Some(50))
            .count();
        assert_eq!(half, if name == "Cairo Commandos" { 3 } else { 0 });
    }
}

#[test]
fn malformed_properties_and_untranslated_max_health_remain_rejected() {
    let mut record = MapObject {
        template: "Mapped".into(),
        position: [0.0; 3],
        angle: 0.0,
        flags: 0,
        properties: std::collections::BTreeMap::new(),
    };
    for (key, value) in [
        ("objectRecruitableAI", MapValue::Int(0)),
        ("objectInitialHealth", MapValue::Real(50.0)),
        ("objectAggressiveness", MapValue::Int(3)),
        ("objectMaxHPs", MapValue::Int(0)),
    ] {
        record.properties.insert(key.into(), value);
        assert!(validate(&record).unwrap_err().to_string().contains(key));
        record.properties.remove(key);
    }
}
