//! Bounded authored map properties; unsupported overrides remain explicit.
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod selectable_tests;
use generals_assets::map::{MapObject, MapValue};
use generals_sim::skirmish::{MapLayout, MapObstacle, NeutralObject};
use std::error::Error;

pub(super) fn register(layout: &mut MapLayout, record: &MapObject) -> Result<(), Box<dyn Error>> {
    validate(record)?;
    layout
        .obstacle_order
        .push(MapObstacle::Neutral(layout.neutral_objects.len()));
    layout.neutral_objects.push(NeutralObject {
        initial_health: property_int(record, "objectInitialHealth")?,
        attitude: property_int(record, "objectAggressiveness")?,
        recruitable: match record.properties.get("objectRecruitableAI") {
            Some(MapValue::Bool(value)) => Some(*value),
            _ => None,
        },
        selectable: match record.properties.get("objectSelectable") {
            Some(MapValue::Bool(value)) => Some(*value),
            _ => None,
        },
        script_targetable: property_bool(record, "objectTargetable")?,
        indestructible: property_bool(record, "objectIndestructible")?,
        original_team: match record.properties.get("originalOwner") {
            Some(MapValue::Ascii(name)) => name.clone(),
            _ => "team".into(),
        },
        script_name: script_name(record),
        name: record.template.clone(),
        position: record.position,
        angle: record.angle,
    });
    Ok(())
}

fn script_name(record: &MapObject) -> Option<String> {
    match record.properties.get("objectName") {
        Some(MapValue::Ascii(name)) if !name.is_empty() => Some(name.clone()),
        _ => None,
    }
}

fn validate(record: &MapObject) -> Result<(), Box<dyn Error>> {
    property_bool(record, "objectTargetable")?;
    property_bool(record, "objectIndestructible")?;
    property_bool(record, "objectRecruitableAI")?;
    property_int(record, "objectInitialHealth")?;
    if property_int(record, "objectAggressiveness")?.is_some_and(|value| !(-2..=2).contains(&value))
    {
        return Err(format!(
            "{} has invalid native objectAggressiveness",
            record.template
        )
        .into());
    }
    if property_int(record, "objectMaxHPs")?.is_some_and(|value| value >= 0) {
        return Err(format!(
            "{} has unsupported nonnegative objectMaxHPs",
            record.template
        )
        .into());
    }
    property_bool(record, "objectSelectable")?;
    for (key, expected) in [
        ("objectEnabled", true),
        ("objectPowered", true),
        ("objectUnsellable", false),
    ] {
        if record
            .properties
            .get(key)
            .is_some_and(|value| !matches!(value,MapValue::Bool(value) if *value==expected))
        {
            return Err(
                format!("{} has an unsupported map override {key}", record.template).into(),
            );
        }
    }
    if !record.position[2].is_finite()
        || record.flags != 0
        || record
            .properties
            .get("originalOwner")
            .is_some_and(|value| !matches!(value, MapValue::Ascii(_)))
        || record
            .properties
            .get("objectLayer")
            .is_some_and(|value| !matches!(value, MapValue::Ascii(_)))
    {
        return Err(format!(
            "{} has unsupported neutral map ownership, height, flags, layer or initial health",
            record.template
        )
        .into());
    }
    Ok(())
}

fn property_int(record: &MapObject, key: &str) -> Result<Option<i32>, Box<dyn Error>> {
    match record.properties.get(key) {
        None => Ok(None),
        Some(MapValue::Int(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("{} has a non-integer map property {key}", record.template).into()),
    }
}

pub(super) fn property_bool(record: &MapObject, key: &str) -> Result<bool, Box<dyn Error>> {
    match record.properties.get(key) {
        None => Ok(false),
        Some(MapValue::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{} has a non-boolean map property {key}", record.template).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[ignore = "requires original game installation"]
    #[test]
    fn original_world_map_targetable_and_indestructible_properties_are_typed() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        for (name, expected_targets, expected_indestructible) in
            [("golden oasis", 14, 0), ("eastern everglades", 0, 1)]
        {
            let map = generals_assets::map::MapData::parse(
                &assets
                    .read(&format!("maps/{name}/{name}.map"))
                    .unwrap()
                    .bytes,
            )
            .unwrap();
            let data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
            let mut targetable = Vec::new();
            let mut indestructible = Vec::new();
            let mut unselectable = Vec::new();
            let mut unsupported = Vec::new();
            let mut layout = super::super::map_layout(&assets, &map.objects).unwrap();
            for record in &map.objects {
                let Some(unit) = data.types.iter().find(|unit| unit.name == record.template) else {
                    continue;
                };
                if let Err(error) = register(&mut layout, record) {
                    unsupported.push(error.to_string());
                }
                if unit.selectable
                    && matches!(
                        record.properties.get("objectSelectable"),
                        Some(MapValue::Bool(false))
                    )
                {
                    unselectable.push(record.template.as_str());
                }
                if property_bool(record, "objectTargetable").unwrap() {
                    targetable.push(record.template.as_str());
                }
                if property_bool(record, "objectIndestructible").unwrap() {
                    indestructible.push(record.template.as_str());
                }
            }
            assert!(
                unsupported.is_empty(),
                "Unsupported original map actor controls: {unsupported:?}"
            );
            assert_eq!(
                layout
                    .neutral_objects
                    .iter()
                    .filter(|object| object.script_targetable)
                    .count(),
                expected_targets
            );
            assert_eq!(
                layout
                    .neutral_objects
                    .iter()
                    .filter(|object| object.indestructible)
                    .count(),
                expected_indestructible
            );
            assert_eq!(targetable.len(), expected_targets);
            assert_eq!(indestructible.len(), expected_indestructible);
            if name == "golden oasis" {
                assert_eq!(unselectable, ["MarketVender01", "JunkDealer"]);
                assert_eq!(
                    targetable
                        .iter()
                        .filter(|&&name| name == "StanWatchTowerTall")
                        .count(),
                    4
                );
                assert_eq!(
                    targetable.iter().filter(|&&name| name == "Barrel").count(),
                    10
                );
            } else {
                assert_eq!(indestructible, ["FirePit"]);
                assert!(unselectable.is_empty());
            }
        }
    }

    #[test]
    fn map_body_controls_reject_non_boolean_values() {
        let mut record = MapObject {
            template: "FirePit".into(),
            position: [0.0; 3],
            angle: 0.0,
            flags: 0,
            properties: std::collections::BTreeMap::new(),
        };
        for key in [
            "objectTargetable",
            "objectIndestructible",
            "objectSelectable",
        ] {
            record.properties.insert(key.into(), MapValue::Int(1));
            assert!(validate(&record).unwrap_err().to_string().contains(key));
            record.properties.remove(key);
        }
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn original_golden_oasis_bib_keeps_authored_selectability_separate_from_template() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let map = generals_assets::map::MapData::parse(
            &assets
                .read("maps/golden oasis/golden oasis.map")
                .unwrap()
                .bytes,
        )
        .unwrap();
        let data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
        let unit = data
            .types
            .iter()
            .find(|unit| unit.name == "StanTallTowerBib")
            .unwrap();
        assert!(!unit.selectable);
        let mut record = map
            .objects
            .into_iter()
            .find(|record| record.template == unit.name)
            .unwrap();
        assert!(matches!(
            record.properties.get("objectSelectable"),
            Some(MapValue::Bool(false))
        ));
        // Object initializes selection from KindOf, then changes it only when
        // the authored map property differs from isSelectable().
        assert!(validate(&record).is_ok());
        record
            .properties
            .insert("objectSelectable".into(), MapValue::Bool(true));
        assert!(validate(&record).is_ok());
        let mut layout = super::super::map_layout(&assets, std::slice::from_ref(&record)).unwrap();
        register(&mut layout, &record).unwrap();
        assert_eq!(
            layout.neutral_objects.last().unwrap().selectable,
            Some(true)
        );
        assert!(!unit.selectable);
        record
            .properties
            .insert("objectSelectable".into(), MapValue::Bool(false));
        record
            .properties
            .insert("objectEnabled".into(), MapValue::Bool(false));
        assert!(
            validate(&record)
                .unwrap_err()
                .to_string()
                .contains("objectEnabled")
        );
    }
    #[test]
    fn map_team_names_are_preserved_while_untranslated_controls_are_rejected() {
        let mut record = MapObject {
            template: "Warehouse".into(),
            position: [100.0, 200.0, 0.0],
            angle: 0.5,
            flags: 0,
            properties: std::collections::BTreeMap::from([
                ("objectInitialHealth".into(), MapValue::Int(100)),
                (
                    "objectName".into(),
                    MapValue::Ascii("Player1 Garrison1".into()),
                ),
            ]),
        };
        assert!(validate(&record).is_ok());
        // Player::setDefaultTeam prefixes "team" to the neutral empty name.
        for owner in ["team", "teamPlyrCivilian"] {
            record
                .properties
                .insert("originalOwner".into(), MapValue::Ascii(owner.into()));
            assert!(validate(&record).is_ok());
        }
        record.properties.insert(
            "originalOwner".into(),
            MapValue::Ascii("teamSkirmishAmerica".into()),
        );
        // Live team binding is validated by PlayerList::validateTeam, whose
        // release missing-team branch uses the neutral default team.
        assert!(validate(&record).is_ok());
        record
            .properties
            .insert("originalOwner".into(), MapValue::Int(3));
        assert!(validate(&record).is_err());
        record.properties.remove("originalOwner");
        assert_eq!(script_name(&record).as_deref(), Some("Player1 Garrison1"));
        record
            .properties
            .insert("objectEnabled".into(), MapValue::Bool(false));
        assert!(validate(&record).is_err());
        record.properties.remove("objectEnabled");
        record.position[2] = 30.0;
        assert!(validate(&record).is_ok());
        record.position[2] = f32::NAN;
        assert!(validate(&record).is_err());
    }
    #[test]
    fn tournament_city_tech_records_preserve_original_script_status_defaults() {
        for (template, position, angle) in [
            (
                "TechOilRefinery",
                [2251.0986, 2706.9497, 0.0],
                -std::f32::consts::FRAC_PI_4,
            ),
            ("TechHospital", [1991.044, 2184.7825, 0.0], -0.741_764_9),
        ] {
            // Installed MapsZH/Tournament City records; Object's constructor
            // clears SCRIPT_TARGETABLE, and the explicit false retains it.
            let record = MapObject {
                template: template.into(),
                position,
                angle,
                flags: 0,
                properties: std::collections::BTreeMap::from([
                    ("objectInitialHealth".into(), MapValue::Int(100)),
                    ("objectEnabled".into(), MapValue::Bool(true)),
                    ("objectIndestructible".into(), MapValue::Bool(false)),
                    ("objectUnsellable".into(), MapValue::Bool(false)),
                    ("objectPowered".into(), MapValue::Bool(true)),
                    ("objectRecruitableAI".into(), MapValue::Bool(true)),
                    ("objectTargetable".into(), MapValue::Bool(false)),
                    (
                        "originalOwner".into(),
                        MapValue::Ascii("teamPlyrCivilian".into()),
                    ),
                    ("objectLayer".into(), MapValue::Ascii(String::new())),
                    ("objectSelectable".into(), MapValue::Bool(true)),
                ]),
            };
            assert!(validate(&record).is_ok());
            assert!(
                matches!(record.properties.get("originalOwner"), Some(MapValue::Ascii(owner)) if owner == "teamPlyrCivilian")
            );
        }
    }
    #[test]
    fn editor_layer_names_preserve_registered_object_data_and_reject_malformed_values() {
        let mut layout = MapLayout {
            partition_cell_size: 10.0,
            random_seed: 1,
            starts: Vec::new(),
            neutral_objects: Vec::new(),
            regular_bridges: Vec::new(),
            supplies: Vec::new(),
            obstacles: Vec::new(),
            fences: Vec::new(),
            scripts: Vec::new(),
            map_template_order: Vec::new(),
            height_map: None,
            map_sides: Vec::new(),
            map_teams: Vec::new(),
            map_build_lists: Vec::new(),
            obstacle_teams: Vec::new(),
            world_objects: Vec::new(),
            world_object_order: Vec::new(),
            obstacle_objects: Vec::new(),
            obstacle_order: Vec::new(),
        };
        let mut record = MapObject {
            template: "TechHospital".into(),
            position: [1800.0, 1350.0, 0.0],
            angle: 0.25,
            flags: 0,
            properties: std::collections::BTreeMap::from([
                (
                    "objectLayer".into(),
                    MapValue::Ascii("City Buildings".into()),
                ),
                (
                    "originalOwner".into(),
                    MapValue::Ascii("teamPlyrCivilian".into()),
                ),
                (
                    "objectName".into(),
                    MapValue::Ascii("Named Hospital".into()),
                ),
                ("objectTargetable".into(), MapValue::Bool(false)),
            ]),
        };
        // WorldBuilder stores editor grouping here; GameLogic obtains actual
        // terrain/pathfind layers independently from the destination position.
        register(&mut layout, &record).unwrap();
        let registered = &layout.neutral_objects[0];
        assert_eq!(registered.name, "TechHospital");
        assert_eq!(registered.script_name.as_deref(), Some("Named Hospital"));
        assert_eq!(registered.original_team, "teamPlyrCivilian");
        assert_eq!(
            registered.position.map(f32::to_bits),
            record.position.map(f32::to_bits)
        );
        assert_eq!(registered.angle.to_bits(), record.angle.to_bits());
        record
            .properties
            .insert("objectLayer".into(), MapValue::Int(2));
        assert!(validate(&record).is_err());
        record.properties.insert(
            "objectLayer".into(),
            MapValue::Ascii("Tech Buildings".into()),
        );
        record.position[2] = -2.0;
        register(&mut layout, &record).unwrap();
        assert_eq!(
            layout.neutral_objects[1].position[2].to_bits(),
            (-2.0_f32).to_bits()
        );
        record.flags = 1;
        assert!(validate(&record).is_err());
    }
}
