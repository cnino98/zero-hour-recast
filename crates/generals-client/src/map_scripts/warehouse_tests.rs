//! Original named warehouse indexes are independent of catalog admission.
use super::*;
use generals_assets::map::MapData;

#[ignore = "requires original game installation"]
#[test]
fn original_tournament_warehouses_keep_names_and_order_for_catalog_and_legacy_sources() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let path = "maps/tournament desert/tournament desert.map";
    let mut map = MapData::parse(&assets.read(path).unwrap().bytes).unwrap();
    let mut fixture = crate::playtest::map_fixture::Fixture::load(root, path).unwrap();
    let definitions = crate::world_data::object_definitions(&assets).unwrap();
    assert_eq!(fixture.layout.supplies.len(), 16);
    let expected = BTreeMap::from([
        ("SupplyDockSouth".to_owned(), 0),
        ("SupplyDockNorth".to_owned(), 1),
        ("Supply Dock SE Corner".to_owned(), 8),
        ("Supply Dock NW Corner".to_owned(), 9),
    ]);
    assert_eq!(
        warehouses(
            &map.objects,
            &fixture.layout,
            &fixture.data.types,
            &definitions
        )
        .unwrap(),
        expected,
    );
    // SupplyDock uses the original declaration path; unnamed small piles retain
    // the catalog path. Both consume their original positions in the same list.
    fixture.data.types.retain(|unit| unit.name != "SupplyDock");
    assert!(
        fixture
            .data
            .types
            .iter()
            .any(|unit| { unit.name == "SupplyPileSmall" && unit.supply.warehouse.is_some() })
    );
    for catalog in [&fixture.data.types[..], &[]] {
        assert_eq!(
            warehouses(&map.objects, &fixture.layout, catalog, &definitions).unwrap(),
            expected
        );
    }
    verify_skipped_sources(&mut map, &fixture, &definitions, &expected);
    verify_rejected_order_and_names(&mut map, &mut fixture, &definitions);
}

fn verify_skipped_sources(
    map: &mut MapData,
    fixture: &crate::playtest::map_fixture::Fixture,
    definitions: &BTreeMap<String, Declaration>,
    expected: &BTreeMap<String, usize>,
) {
    let source = map
        .objects
        .iter()
        .find(|record| object_name(record) == Some("SupplyDockSouth"))
        .unwrap();
    let skipped = [0x02, 0x04, 0x10, 0x20].map(|flags| copy_record(source, flags));
    let mut waypoint = copy_record(source, 0);
    waypoint
        .properties
        .insert("waypointID".into(), MapValue::Int(12345));
    map.objects.extend(skipped);
    map.objects.push(waypoint);
    assert_eq!(
        warehouses(
            &map.objects,
            &fixture.layout,
            &fixture.data.types,
            definitions
        )
        .unwrap(),
        *expected
    );
}

fn verify_rejected_order_and_names(
    map: &mut MapData,
    fixture: &mut crate::playtest::map_fixture::Fixture,
    definitions: &BTreeMap<String, Declaration>,
) {
    fixture.layout.supplies.swap(0, 1);
    assert!(
        warehouses(
            &map.objects,
            &fixture.layout,
            &fixture.data.types,
            definitions
        )
        .unwrap_err()
        .to_string()
        .contains("source order differs")
    );
    fixture.layout.supplies.swap(0, 1);
    fixture.layout.supplies.push(fixture.layout.supplies[0]);
    assert!(
        warehouses(
            &map.objects,
            &fixture.layout,
            &fixture.data.types,
            definitions
        )
        .unwrap_err()
        .to_string()
        .contains("source count differs")
    );
    fixture.layout.supplies.pop();
    let north = map
        .objects
        .iter_mut()
        .find(|record| object_name(record) == Some("SupplyDockNorth"))
        .unwrap();
    north.properties.insert(
        "objectName".into(),
        MapValue::Ascii("SupplyDockSouth".into()),
    );
    assert!(
        warehouses(
            &map.objects,
            &fixture.layout,
            &fixture.data.types,
            definitions
        )
        .unwrap_err()
        .to_string()
        .contains("Ambiguous named warehouse")
    );
}

fn copy_record(source: &MapObject, flags: u32) -> MapObject {
    MapObject {
        template: source.template.clone(),
        position: source.position,
        angle: source.angle,
        flags,
        properties: BTreeMap::from([(
            "objectName".into(),
            MapValue::Ascii(object_name(source).unwrap().into()),
        )]),
    }
}
