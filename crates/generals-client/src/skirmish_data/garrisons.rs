//! Actual map garrisons join the object catalog before drawable/UI index binding.
use super::blocks;
use generals_assets::{content::ContentFile, map::MapObject};
use std::{collections::BTreeSet, error::Error};

pub(super) fn templates<'a>(
    files: &'a [ContentFile],
    records: &'a [MapObject],
) -> Result<Vec<(&'a str, generals_sim::skirmish::Kind)>, Box<dyn Error>> {
    let names = placed_names(files, records)?;
    let mut result = Vec::new();
    for name in names {
        if super::ROSTER
            .iter()
            .any(|(existing, _, _)| *existing == name)
        {
            continue;
        }
        let Some(object) = files
            .iter()
            .flat_map(|file| &file.declarations)
            .find(|object| object.kind == "Object" && object.name == name)
        else {
            continue;
        };
        // GameLogic creates neither Objects nor their behavior modules for these draws.
        if super::has_flag(&object.lines, "KindOf", "OPTIMIZED_TREE")
            || super::has_flag(&object.lines, "KindOf", "PROP")
        {
            continue;
        }
        let modules = blocks(&object.lines, 0, "Behavior")?;
        if name != "TechReinforcementPad"
            && modules.iter().any(|module| {
                module[0]
                    .tokens
                    .get(1)
                    .is_some_and(|name| name == "OCLUpdate")
            })
        {
            return Err(format!(
                "{name}: neutral OCLUpdate requires its timed faction vehicle-delivery consumer"
            )
            .into());
        }
        let body = blocks(&object.lines, 0, "Body")?;
        if super::has_flag(&object.lines, "KindOf", "SUPPLY_SOURCE") {
            result.push((name, generals_sim::skirmish::Kind::TechBuilding));
            continue;
        }
        let has_body = body.last().is_some_and(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name != "InactiveBody")
        });
        let container = modules.iter().any(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| matches!(name.as_str(), "GarrisonContain" | "CaveContain"))
        });
        if super::has_flag(&object.lines, "KindOf", "LANDMARK_BRIDGE")
            || super::has_flag(&object.lines, "KindOf", "WALK_ON_TOP_OF_WALL")
            || container
            || has_body
            || modules.iter().any(|module| {
                module[0]
                    .tokens
                    .get(1)
                    .is_some_and(|family| family == "ToppleUpdate")
            })
        {
            let kind = if super::has_flag(&object.lines, "KindOf", "DOZER") {
                generals_sim::skirmish::Kind::Builder
            } else if super::has_flag(&object.lines, "KindOf", "INFANTRY") {
                generals_sim::skirmish::Kind::Infantry
            } else if super::has_flag(&object.lines, "KindOf", "VEHICLE") {
                generals_sim::skirmish::Kind::Vehicle
            } else {
                generals_sim::skirmish::Kind::TechBuilding
            };
            result.push((object.name.as_str(), kind));
        }
    }
    Ok(result)
}

fn placed_names<'a>(
    files: &[ContentFile],
    records: &'a [MapObject],
) -> Result<BTreeSet<&'a str>, Box<dyn Error>> {
    let mut names: BTreeSet<_> = records
        .iter()
        .map(|record| record.template.as_str())
        .collect();
    if records
        .windows(2)
        .any(|pair| pair[0].flags & 0x10 != 0 && pair[1].flags & 0x20 != 0)
    {
        if !files
            .iter()
            .flat_map(|file| &file.declarations)
            .any(|object| object.kind == "Object" && object.name == "GenericBridge")
        {
            return Err(
                "Native regular bridge requires the original GenericBridge template".into(),
            );
        }
        names.insert("GenericBridge");
    }
    Ok(names)
}

// ActiveBody::setCorrectDamageState applies this before any OnDie callback.
// The authored DIE interfaces control subsequent retention, not rubble geometry.
pub(super) fn rubble_height(
    object: &generals_assets::content::Declaration,
    global: &[generals_assets::content::ContentLine],
) -> Result<Option<f32>, Box<dyn Error>> {
    let mut height = 0;
    for line in &object.lines {
        if line.depth == Some(0)
            && line
                .tokens
                .first()
                .is_some_and(|key| key == "StructureRubbleHeight")
        {
            let token = line
                .tokens
                .get(1)
                .ok_or("StructureRubbleHeight lacks its byte value")?;
            height = rubble_byte(token)?;
        }
    }
    if !super::has_flag(&object.lines, "KindOf", "STRUCTURE") {
        return Ok(None);
    }
    Ok(Some(if height == 0 {
        super::number(global, 0, "DefaultStructureRubbleHeight")?
    } else {
        f32::from(height)
    }))
}

// INI::scanType<Int> parses an Int64 prefix, consumes one leading plus, and
// ignores from_chars' end pointer. Int narrowing precedes parseUnsignedByte.
fn rubble_byte(token: &str) -> Result<u8, Box<dyn Error>> {
    let token = token.strip_prefix('+').unwrap_or(token);
    let start = usize::from(token.starts_with('-'));
    let digits = token.as_bytes()[start..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return Err("invalid StructureRubbleHeight integer prefix".into());
    }
    let bytes = token[..start + digits].parse::<i64>()?.to_le_bytes();
    let value = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Ok(u8::try_from(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};
    fn file(text: &str) -> ContentFile {
        content::scan(Asset {
            path: "fixture.ini".into(),
            origin: "fixture".into(),
            bytes: text.as_bytes().to_vec(),
        })
        .unwrap()
    }
    fn record(name: &str) -> MapObject {
        MapObject {
            template: name.into(),
            position: [100.0, 200.0, 0.0],
            angle: 0.5,
            flags: 0,
            properties: std::collections::BTreeMap::new(),
        }
    }
    #[test]
    fn native_release_uses_last_map_added_body_without_removing_earlier_module() {
        let files = [file(
            "Object EnglishTVStatio\nBody=StructureBody ModuleTag_02\nMaxHealth=2000\nInitialHealth=2000\nEnd\nBody=ActiveBody ModuleTag_02_Override\nMaxHealth=3000\nInitialHealth=2500\nEnd\nEnd\n",
        )];
        let object = &files[0].declarations[0];
        let selected =
            super::super::active_body(&files, object, generals_sim::skirmish::Kind::TechBuilding)
                .unwrap()
                .unwrap();
        assert_eq!(selected[0].tokens[2], "ModuleTag_02_Override");
        assert_eq!(
            super::super::body_health(selected, "InitialHealth")
                .unwrap()
                .to_bits(),
            2500.0_f32.to_bits()
        );
        assert_eq!(blocks(&object.lines, 0, "Body").unwrap().len(), 2);
    }

    #[test]
    fn appended_catalog_uses_only_placed_resolved_ground_garrisons_in_stable_order() {
        let files = [file(
            "Object Zed\nBehavior=GarrisonContain Tag\nContainMax=5\nEnd\nEnd\nObject Alpha\nBehavior=GarrisonContain Tag\nEnd\nEnd\nObject Wall\nKindOf=STRUCTURE WALK_ON_TOP_OF_WALL\nBehavior=GarrisonContain Tag\nEnd\nEnd\nObject Decoration\nEnd\nObject Absent\nBehavior=GarrisonContain Tag\nEnd\nEnd\n",
        )];
        let records = [
            record("Zed"),
            record("Alpha"),
            record("Zed"),
            record("Wall"),
            record("Decoration"),
        ];
        let types = templates(&files, &records).unwrap();
        assert_eq!(
            types.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["Alpha", "Wall", "Zed"]
        );
        assert!(
            types
                .iter()
                .all(|(_, kind)| matches!(kind, generals_sim::skirmish::Kind::TechBuilding))
        );
    }
    #[test]
    fn faction_triggered_pad_joins_the_catalog_for_its_bound_timer_and_delivery_closure() {
        let files = [file(
            "Object TechReinforcementPad\nKindOf=STRUCTURE CAPTURABLE TECH_BUILDING\nBody=ActiveBody BodyTag\nMaxHealth=1000\nEnd\nBehavior=OCLUpdate OCLTag\nMinDelay=120000\nMaxDelay=120000\nCreateAtEdge=Yes\nFactionTriggered=Yes\nFactionOCL=Faction:America OCL:OCL_ReinforcementPadUSAVehicle\nEnd\nBehavior=DefaultProductionExitUpdate ExitTag\nNaturalRallyPoint=X:0 Y:-60 Z:0\nUseSpawnRallyPoint=Yes\nEnd\nEnd\n",
        )];
        let records = [record("TechReinforcementPad")];
        let result = templates(&files, &records).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].0 == "TechReinforcementPad"
                && result[0].1 == generals_sim::skirmish::Kind::TechBuilding
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_rocky_rampage_pad_loads_real_timer_transport_parachute_and_payload_consumers() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let map = generals_assets::map::MapData::parse(
            &assets
                .read("maps/rocky rampage/rocky rampage.map")
                .unwrap()
                .bytes,
        )
        .unwrap();
        let records: Vec<_> = map
            .objects
            .into_iter()
            .filter(|record| record.template == "TechReinforcementPad")
            .collect();
        assert_eq!(records.len(), 1);
        let data = super::super::load(&assets, &records).unwrap();
        assert_eq!(data.deliveries.len(), 3);
        for name in [
            "TechReinforcementPad",
            "AmericaJetCargoPlane",
            "ChinaJetCargoPlane",
            "GLAJetCargoPlane",
            "LargeParachute",
        ] {
            let unit = data.types.iter().find(|unit| unit.name == name).unwrap();
            generals_sim::skirmish::module_updates::validate_registrations(unit).unwrap();
        }
    }

    #[test]
    fn all_structure_bodies_receive_rubble_height_before_their_die_interfaces() {
        let global = file("GameData\nDefaultStructureRubbleHeight=3\nEnd\n");
        for extra in [
            "",
            "Behavior=FXListDie TagFX\nEnd\n",
            "Behavior=KeepObjectDie TagKeep\nEnd\n",
            "Behavior=StructureCollapseUpdate TagCollapse\nEnd\n",
            "Behavior=StructureToppleUpdate TagTopple\nEnd\n",
            "Behavior=DestroyDie TagDestroy\nEnd\n",
        ] {
            let object = file(&format!(
                "Object Test\nKindOf=STRUCTURE IMMOBILE\n{extra}End\n"
            ));
            assert_eq!(
                rubble_height(&object.declarations[0], &global.declarations[0].lines)
                    .unwrap()
                    .map(f32::to_bits),
                Some(3.0_f32.to_bits())
            );
        }
    }

    #[test]
    fn rubble_height_uses_native_structure_flag_and_zero_byte_fallback() {
        let global = file("GameData\nDefaultStructureRubbleHeight=3\nEnd\n");
        for (kind, height, expected) in [
            ("STRUCTURE", "7", Some(7.0_f32)),
            ("STRUCTURE", "0", Some(3.0)),
            ("IMMOBILE", "7", None),
        ] {
            let object = file(&format!(
                "Object Test\nKindOf={kind}\nStructureRubbleHeight={height}\nEnd\n"
            ));
            assert_eq!(
                rubble_height(&object.declarations[0], &global.declarations[0].lines)
                    .unwrap()
                    .map(f32::to_bits),
                expected.map(f32::to_bits)
            );
        }
    }
    #[test]
    fn rubble_height_preserves_native_integer_prefix_sign_and_byte_range() {
        let global = file("GameData\nDefaultStructureRubbleHeight=3\nEnd\n");
        for (token, expected) in [
            ("+7", 7.0_f32),
            ("7.5", 7.0),
            ("255.9", 255.0),
            ("1e3", 1.0),
            ("7suffix", 7.0),
            ("7 ignored", 7.0),
            ("-0", 3.0),
            ("0x10", 3.0),
            ("4294967296", 3.0),
        ] {
            let object = file(&format!(
                "Object Test\nKindOf=STRUCTURE\nStructureRubbleHeight={token}\nEnd\n"
            ));
            assert_eq!(
                rubble_height(&object.declarations[0], &global.declarations[0].lines)
                    .unwrap()
                    .unwrap()
                    .to_bits(),
                expected.to_bits()
            );
        }
        for token in [
            "-2",
            "256",
            "2550.1",
            "2147483648",
            "9223372036854775808",
            "-9223372036854775809",
            "NaN",
            ".5",
            "+",
            "++7",
        ] {
            let object = file(&format!(
                "Object Test\nKindOf=STRUCTURE\nStructureRubbleHeight={token}\nEnd\n"
            ));
            assert!(
                rubble_height(&object.declarations[0], &global.declarations[0].lines).is_err(),
                "{token}"
            );
        }
    }

    #[test]
    fn invalid_authored_rubble_byte_is_not_hidden_by_later_values_or_nonstructure_kind() {
        let global = file("GameData\nDefaultStructureRubbleHeight=3\nEnd\n");
        for fields in [
            "KindOf=IMMOBILE\nStructureRubbleHeight=-2\n",
            "KindOf=STRUCTURE\nStructureRubbleHeight=256\nStructureRubbleHeight=7\n",
        ] {
            let object = file(&format!("Object Test\n{fields}End\n"));
            assert!(rubble_height(&object.declarations[0], &global.declarations[0].lines).is_err());
        }
    }
}
