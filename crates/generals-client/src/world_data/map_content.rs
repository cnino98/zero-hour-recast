//! Native map.ini/solo.ini overrides, including ordered tagged module mutation.
//! `GameLogic::loadMapINI`, `ThingTemplate::parseAdd/Remove/ReplaceModule`.
use generals_assets::{
    AssetError,
    content::{self, ContentLine, Declaration},
    virtual_assets::{Asset, VirtualAssets},
};
use std::{collections::BTreeMap, error::Error};

type BoxError = Box<dyn Error>;

pub struct MapContent {
    pub effective_files: Vec<Asset>,
    pub raw_sources: Vec<Asset>,
}

pub(super) fn load(assets: &VirtualAssets, map_path: &str) -> Result<MapContent, BoxError> {
    let map_path = map_path.replace('\\', "/");
    let Some((directory, _)) = map_path.rsplit_once('/') else {
        return Ok(MapContent {
            effective_files: Vec::new(),
            raw_sources: Vec::new(),
        });
    };
    let mut locals = Vec::new();
    let mut raw_sources = Vec::new();
    for local in ["map.ini", "solo.ini"] {
        let path = format!("{directory}/{local}");
        let asset = match assets.read(&path) {
            Ok(asset) => asset,
            Err(AssetError::Missing(_)) => continue,
            Err(error) => return Err(error.into()),
        };
        // Keep original bytes and source order before Add/Remove/Replace flattening.
        let file = content::scan(Asset {
            path: asset.path.clone(),
            origin: asset.origin.clone(),
            bytes: asset.bytes.clone(),
        })?;
        if file
            .declarations
            .iter()
            .any(|declaration| declaration.kind == "ObjectReskin")
        {
            return Err(
                "Map-local ObjectReskin requires its resolved native parent-copy consumer".into(),
            );
        }
        if !file.diagnostics.is_empty() || !file.unclassified.is_empty() {
            return Err("Malformed map-local INI".into());
        }
        raw_sources.push(asset);
        locals.push((path, file.declarations));
    }
    if locals.is_empty() {
        return Ok(MapContent {
            effective_files: Vec::new(),
            raw_sources,
        });
    }
    let baseline = crate::skirmish_data::source_files::metadata_baseline(assets)?;
    let overrides = raw_sources
        .iter()
        .map(|asset| {
            content::scan(Asset {
                path: asset.path.clone(),
                origin: asset.origin.clone(),
                bytes: asset.bytes.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let selected: Vec<_> = overrides
        .iter()
        .flat_map(|file| &file.declarations)
        .filter(|declaration| declaration.kind == "Object")
        .map(|declaration| declaration.name.as_str())
        .collect();
    let metadata = crate::skirmish_data::module_metadata::load(&baseline, &overrides, &selected)?;
    let index = declaration_index(assets, &locals)?;
    let mut changed: BTreeMap<String, Asset> = BTreeMap::new();
    for (path, declarations) in locals {
        for declaration in declarations {
            let kind = declaration.kind.as_str();
            let target = index
                .get(&(kind.to_owned(), declaration.name.clone()))
                .cloned()
                .or_else(|| new_declaration_file(kind).map(str::to_owned))
                .ok_or_else(|| {
                    format!(
                        "Map-local {} {} has no original declaration consumer",
                        declaration.kind, declaration.name
                    )
                })?;
            let baseline = changed
                .remove(&target)
                .map_or_else(|| assets.read(&target), Ok)?;
            let bytes = if kind == "Object" {
                merge_file(&baseline.bytes, &scalar_override(&declaration)?)?
            } else {
                merge_file(&baseline.bytes, &declaration)?
            };
            changed.insert(
                target.clone(),
                Asset {
                    path: target,
                    origin: path.clone(),
                    bytes,
                },
            );
        }
    }
    let effective_files = reconcile_files(changed, &baseline, &overrides, &metadata)?;
    Ok(MapContent {
        effective_files,
        raw_sources,
    })
}

fn reconcile_files(
    changed: BTreeMap<String, Asset>,
    baseline: &[content::ContentFile],
    overrides: &[content::ContentFile],
    metadata: &BTreeMap<String, crate::skirmish_data::module_metadata::TemplateMetadata>,
) -> Result<Vec<Asset>, BoxError> {
    let mut effective_files = Vec::new();
    for asset in changed.into_values() {
        let mut files = vec![content::scan(asset)?];
        crate::skirmish_data::module_metadata::reconcile(
            &mut files, baseline, overrides, metadata,
        )?;
        let file = files.remove(0);
        let mut text = String::new();
        for declaration in file.declarations {
            text.push_str(&declaration.header.raw);
            text.push('\n');
            for line in declaration.lines {
                text.push_str(&line.raw);
                text.push('\n');
            }
        }
        let bytes = text
            .chars()
            .map(|character| u8::try_from(u32::from(character)))
            .collect::<Result<Vec<_>, _>>()?;
        effective_files.push(Asset {
            path: file.path,
            origin: file.origin,
            bytes,
        });
    }
    Ok(effective_files)
}

fn scalar_override(declaration: &Declaration) -> Result<Declaration, BoxError> {
    let mut text = format!("{}\n", declaration.header.raw);
    for entry in entries(&declaration.lines)? {
        if entry.tokens[0] == "RemoveModule" || module(&entry) {
            continue;
        }
        if matches!(entry.tokens[0].as_str(), "AddModule" | "ReplaceModule") {
            for nested in wrapper(&entry)? {
                if !module(&nested) {
                    text.push_str(&nested.raw);
                    text.push('\n');
                }
            }
        } else {
            text.push_str(&entry.raw);
            text.push('\n');
        }
    }
    text.push_str("End\n");
    let bytes = text
        .chars()
        .map(|character| u8::try_from(u32::from(character)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(scan(&bytes)?.remove(0))
}

fn new_declaration_file(kind: &str) -> Option<&'static str> {
    match kind {
        "CommandSet" => Some("data/ini/commandset.ini"),
        "Weapon" => Some("data/ini/weapon.ini"),
        "Armor" => Some("data/ini/armor.ini"),
        _ => None,
    }
}

fn declaration_index(
    assets: &VirtualAssets,
    locals: &[(String, Vec<Declaration>)],
) -> Result<BTreeMap<(String, String), String>, BoxError> {
    let mut paths = std::collections::BTreeSet::new();
    for (_, declarations) in locals {
        for declaration in declarations {
            match declaration.kind.as_str() {
                "Object" | "ObjectReskin" => {
                    paths.extend(assets.list("data/ini/object/")?);
                }
                "WaterTransparency" => {
                    paths.insert("data/ini/water.ini".into());
                }
                "Weather" => {
                    paths.insert("data/ini/weather.ini".into());
                }
                "CommandSet" => {
                    paths.insert("data/ini/commandset.ini".into());
                }
                _ => {
                    paths.extend(assets.list("data/ini/")?);
                }
            }
        }
    }
    let mut index = BTreeMap::new();
    for path in paths.into_iter().filter(|path| {
        std::path::Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
            && !path.starts_with("data/ini/default/")
    }) {
        for declaration in content::scan(assets.read(&path)?)?.declarations {
            let kind = declaration.kind.as_str();
            let key = (kind.to_owned(), declaration.name.clone());
            if let Some(old) = index.insert(key, path.clone())
                && old != path
            {
                return Err("Ambiguous original INI declaration source".into());
            }
        }
    }
    Ok(index)
}

fn same_declaration(first: &Declaration, second: &Declaration) -> bool {
    (first.kind == second.kind
        || matches!(
            (first.kind.as_str(), second.kind.as_str()),
            ("ObjectReskin", "Object") | ("Object", "ObjectReskin")
        ))
        && first.name == second.name
}

fn scan(bytes: &[u8]) -> Result<Vec<Declaration>, BoxError> {
    let file = content::scan(Asset {
        path: "map-override.ini".into(),
        origin: "map-local source".into(),
        bytes: bytes.to_vec(),
    })?;
    if let Some(diagnostic) = file.diagnostics.iter().find(|diagnostic| {
        // An unchanged reskin remains in this lexical file; the object loader
        // resolves its parent. This merge does not flatten that declaration.
        diagnostic.code != "unresolved-parent"
            || !file.declarations.iter().any(|declaration| {
                declaration.line == diagnostic.line && declaration.kind == "ObjectReskin"
            })
    }) {
        return Err(format!(
            "Effective map INI line {}: {}: {}",
            diagnostic.line, diagnostic.code, diagnostic.message
        )
        .into());
    }
    if let Some(line) = file.unclassified.first() {
        return Err(format!(
            "Effective map INI line {} has no declaration: {}",
            line.line, line.raw
        )
        .into());
    }
    Ok(file.declarations)
}

fn merge_file(bytes: &[u8], replacement: &Declaration) -> Result<Vec<u8>, BoxError> {
    if replacement.kind == "ObjectReskin" {
        return Err(
            "Map-local ObjectReskin requires its resolved native parent-copy consumer".into(),
        );
    }
    let original: String = bytes.iter().map(|byte| char::from(*byte)).collect();
    let declarations = scan(bytes)?;
    let Some(index) = declarations
        .iter()
        .position(|declaration| same_declaration(declaration, replacement))
    else {
        if new_declaration_file(&replacement.kind).is_none() {
            return Err("Map override declaration disappeared".into());
        }
        let mut text = original;
        text.push('\n');
        text.push_str(&replacement.header.raw);
        text.push('\n');
        for entry in declaration_entries(Vec::new(), replacement)? {
            text.push_str(&entry.raw);
            text.push('\n');
        }
        text.push_str("End\n");
        return text
            .chars()
            .map(|character| {
                u8::try_from(u32::from(character)).map_err(|error| Box::new(error) as BoxError)
            })
            .collect();
    };
    let baseline = &declarations[index];
    if baseline.kind == "ObjectReskin" {
        return Err("Map override of a reskin requires its resolved native template".into());
    }
    let next_line = declarations.get(index + 1).map_or_else(
        || original.lines().count(),
        |declaration| usize::try_from(declaration.line - 1).expect("native line index"),
    );
    let start = usize::try_from(baseline.line - 1).expect("native line index");
    let lines: Vec<_> = original.split_inclusive('\n').collect();
    let body = declaration_entries(entries(&baseline.lines)?, replacement)?;
    let mut text: String = lines[..start].concat();
    text.push_str(&baseline.header.raw);
    text.push('\n');
    for entry in body {
        text.push_str(&entry.raw);
        text.push('\n');
    }
    text.push_str("End\n");
    text.push_str(&lines[next_line..].concat());
    text.chars()
        .map(|character| {
            u8::try_from(u32::from(character)).map_err(|error| Box::new(error) as BoxError)
        })
        .collect()
}

fn declaration_entries(
    baseline: Vec<Entry>,
    replacement: &Declaration,
) -> Result<Vec<Entry>, BoxError> {
    let changes = entries(&replacement.lines)?;
    if replacement.kind == "Armor" {
        // ArmorStore::parseArmorDefinition clears the existing template before
        // applying DEFAULT and individual coefficients in authored order.
        Ok(changes)
    } else {
        merge_entries(baseline, changes)
    }
}

struct Entry {
    tokens: Vec<String>,
    raw: String,
}

fn entries(lines: &[ContentLine]) -> Result<Vec<Entry>, BoxError> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index];
        if line.depth != Some(0) {
            return Err("Invalid map override nesting".into());
        }
        if line.tokens[0].eq_ignore_ascii_case("End") {
            break;
        }
        let start = index;
        index += 1;
        let block = matches!(
            line.tokens[0].as_str(),
            "AddModule"
                | "ReplaceModule"
                | "Draw"
                | "Behavior"
                | "Body"
                | "ClientUpdate"
                | "WeaponSet"
                | "ArmorSet"
                | "UnitSpecificSounds"
                | "UnitSpecificFX"
                | "Prerequisites"
        );
        if block {
            while index < lines.len() {
                let end = lines[index].depth == Some(0)
                    && lines[index].tokens[0].eq_ignore_ascii_case("End");
                index += 1;
                if end {
                    break;
                }
            }
        }
        result.push(Entry {
            tokens: line.tokens.clone(),
            raw: lines[start..index]
                .iter()
                .map(|line| line.raw.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        });
    }
    Ok(result)
}

fn module(entry: &Entry) -> bool {
    matches!(
        entry.tokens[0].as_str(),
        "Body" | "Behavior" | "Draw" | "ClientUpdate"
    )
}

fn wrapper(entry: &Entry) -> Result<Vec<Entry>, BoxError> {
    let lines: Vec<_> = entry.raw.lines().collect();
    let text = format!(
        "Object MapOverride\n{}\nEnd\n",
        lines[1..lines.len() - 1].join("\n")
    );
    let mut declaration = scan(text.as_bytes())?;
    entries(&declaration.remove(0).lines)
}

fn remove_module(entries: &mut Vec<Entry>, tag: &str) -> Result<String, BoxError> {
    let index = entries
        .iter()
        .position(|entry| {
            module(entry) && entry.tokens.get(2).is_some_and(|existing| existing == tag)
        })
        .ok_or("Map RemoveModule/ReplaceModule tag does not exist")?;
    Ok(entries.remove(index).tokens[1].clone())
}

fn add_module(
    entries: &mut Vec<Entry>,
    entry: Entry,
    replacement: Option<(&str, &str)>,
) -> Result<(), BoxError> {
    let tag = entry.tokens.get(2).ok_or("Map module lacks tag")?;
    if replacement.is_some_and(|(name, old_tag)| entry.tokens[1] != name || tag == old_tag) {
        return Err("ReplaceModule requires same module type and a new tag".into());
    }
    if native_ai_module(&entry.tokens[1]) {
        if entry.tokens[0] != "Behavior" || !crate::skirmish_data::ai_module(&entry.tokens[1]) {
            return Err(format!(
                "Map-local AI module {} lacks its actor consumer",
                entry.tokens[1]
            )
            .into());
        }
        // ThingTemplate::parseModuleName clears AI module data before addModuleInfo
        // appends the new entry. An AddModule AI may reuse the retired AI tag.
        entries.retain(|old| {
            old.tokens[0] != "Behavior"
                || old.tokens.get(1).is_none_or(|name| !native_ai_module(name))
        });
    }
    entries.push(entry);
    Ok(())
}

fn native_ai_module(name: &str) -> bool {
    // The pinned ModuleFactory registers sixteen AIUpdateModuleData descendants;
    // Transport/Wander use their inherited AIUpdateModuleData factory directly.
    crate::skirmish_data::ai_module(name)
        || matches!(
            name,
            "DeliverPayloadAIUpdate"
                | "FlightDeckBehavior"
                | "MissileAIUpdate"
                | "POWTruckAIUpdate"
                | "RailedTransportAIUpdate"
        )
}

fn merge_entries(mut baseline: Vec<Entry>, changes: Vec<Entry>) -> Result<Vec<Entry>, BoxError> {
    let mut replaced_sets = [false; 2];
    for change in changes {
        match change.tokens[0].as_str() {
            "WeaponSet" | "ArmorSet" => {
                // ThingFactory::newOverride marks both copied set families;
                // the first parsed set clears that entire family, then appends.
                let family = usize::from(change.tokens[0] == "ArmorSet");
                if !replaced_sets[family] {
                    baseline.retain(|entry| entry.tokens[0] != change.tokens[0]);
                    replaced_sets[family] = true;
                }
                baseline.push(change);
            }
            "Prerequisites" => {
                // parsePrerequisites clears its vector for each override block.
                baseline.retain(|entry| entry.tokens[0] != "Prerequisites");
                baseline.push(change);
            }
            "RemoveModule" => {
                remove_module(
                    &mut baseline,
                    change.tokens.get(1).ok_or("RemoveModule tag missing")?,
                )?;
            }
            "ReplaceModule" => {
                let tag = change.tokens.get(1).ok_or("ReplaceModule tag missing")?;
                let name = remove_module(&mut baseline, tag)?;
                for entry in wrapper(&change)? {
                    if module(&entry) {
                        add_module(&mut baseline, entry, Some((&name, tag)))?;
                    } else {
                        scalar(&mut baseline, entry)?;
                    }
                }
            }
            "AddModule" => {
                for entry in wrapper(&change)? {
                    if module(&entry) {
                        add_module(&mut baseline, entry, None)?;
                    } else {
                        scalar(&mut baseline, entry)?;
                    }
                }
            }
            _ if module(&change) => {
                return Err(
                    "Native map module override requires AddModule or ReplaceModule".into(),
                );
            }
            _ => {
                scalar(&mut baseline, change)?;
            }
        }
    }
    Ok(baseline)
}

fn scalar(baseline: &mut Vec<Entry>, change: Entry) -> Result<(), BoxError> {
    if change.raw.lines().count() > 1 {
        return Err(format!(
            "Map-local nested {} override requires its native keyed merge consumer",
            change.tokens[0]
        )
        .into());
    }
    if baseline
        .iter()
        .filter(|entry| entry.tokens[0].eq_ignore_ascii_case(&change.tokens[0]))
        .count()
        > 1
        || matches!(
            change.tokens[0].as_str(),
            "WeaponBonus" | "BuildVariation" | "Prerequisite" | "AddKindOf" | "RemoveKindOf"
        )
    {
        return Err(format!(
            "Map-local repeated {} requires its native list consumer",
            change.tokens[0]
        )
        .into());
    }
    baseline.retain(|entry| !entry.tokens[0].eq_ignore_ascii_case(&change.tokens[0]));
    baseline.push(change);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_reskin_cannot_silently_keep_the_old_object_parent_or_header() {
        let original = b"Object Tank\nVisionRange=100\nEnd\nObject Other\nVisionRange=300\nEnd\n";
        let replacement = content::scan(Asset {
            path: "map.ini".into(),
            origin: "map override".into(),
            bytes: b"ObjectReskin Tank Other\nVisionRange=200\nEnd\n".to_vec(),
        })
        .unwrap()
        .declarations
        .remove(0);
        let error = merge_file(original, &replacement).unwrap_err().to_string();
        assert!(error.contains("resolved native parent-copy consumer"));
    }

    #[test]
    fn adding_supported_ai_retires_old_ai_before_tag_check_and_preserves_other_modules() {
        let original = b"Object Tank\nDraw=W3DModelDraw DrawOld\nEnd\nBehavior=AIUpdateInterface SharedAI\nEnd\nBehavior=FXListDie Death\nEnd\nEnd\n";
        let change = scan(b"Object Tank\nAddModule\nBehavior=WanderAIUpdate SharedAI\nMoodAttackCheckRate=100\nEnd\nEnd\nEnd\n").unwrap();
        let merged = merge_file(original, &change[0]).unwrap();
        let object = scan(&merged).unwrap().remove(0);
        let modules = crate::skirmish_data::blocks(&object.lines, 0, "Behavior").unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0][0].tokens, ["Behavior", "FXListDie", "Death"]);
        assert_eq!(
            modules[1][0].tokens,
            ["Behavior", "WanderAIUpdate", "SharedAI"]
        );
        assert_eq!(
            crate::skirmish_data::blocks(&object.lines, 0, "Draw")
                .unwrap()
                .len(),
            1
        );
        let collision =
            scan(b"Object Tank\nAddModule\nBehavior=AIUpdateInterface DrawOld\nEnd\nEnd\nEnd\n")
                .unwrap();
        assert!(merge_file(&merged, &collision[0]).is_ok());
    }

    #[test]
    fn added_native_ai_requires_a_consumer_and_replaces_the_previous_ai() {
        let original = b"Object Tank\nBehavior=AIUpdateInterface OriginalAI\nEnd\nEnd\n";
        for name in [
            "DeliverPayloadAIUpdate",
            "FlightDeckBehavior",
            "MissileAIUpdate",
            "POWTruckAIUpdate",
            "RailedTransportAIUpdate",
        ] {
            let text = format!("Object Tank\nAddModule\nBehavior={name} NewAI\nEnd\nEnd\nEnd\n");
            let change = scan(text.as_bytes()).unwrap();
            if matches!(name, "DeliverPayloadAIUpdate" | "MissileAIUpdate") {
                let merged = merge_file(original, &change[0]).unwrap();
                let object = scan(&merged).unwrap().remove(0);
                let modules = crate::skirmish_data::blocks(&object.lines, 0, "Behavior").unwrap();
                assert_eq!(modules.len(), 1);
                assert_eq!(modules[0][0].tokens, ["Behavior", name, "NewAI"]);
            } else {
                let error = merge_file(original, &change[0]).unwrap_err().to_string();
                assert!(
                    error.contains("lacks its actor consumer"),
                    "{name}: {error}"
                );
            }
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn windows_and_archive_map_paths_load_identical_original_weather_and_object_overrides() {
        let (assets, _) = crate::cleanup_verification::tests::installed();
        for path in [
            "maps/rocky rampage/rocky rampage.map",
            "maps/mountain guns/mountain guns.map",
        ] {
            let archive = load(&assets, path).unwrap();
            let windows = load(&assets, &path.replace('/', "\\")).unwrap();
            assert!(!archive.effective_files.is_empty());
            assert_eq!(archive.effective_files.len(), windows.effective_files.len());
            for (archive, windows) in archive.effective_files.iter().zip(&windows.effective_files) {
                assert_eq!(archive.path, windows.path);
                assert_eq!(archive.origin, windows.origin);
                assert_eq!(archive.bytes, windows.bytes);
            }
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn map_weapon_and_new_armor_overrides_reach_the_installed_catalog() {
        use generals_sim::skirmish::DamageType;
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let object_change = scan(b"Object AmericaVehicleHumvee\nWeaponSet\nConditions=NONE\nWeapon=PRIMARY CrusaderTankGun\nEnd\nArmorSet\nConditions=NONE\nArmor=MapArmor\nEnd\nEnd\n").unwrap();
        let armor_change =
            scan(b"Armor MapArmor\nArmor=DEFAULT 25%\nArmor=WATER 50%\nEnd\n").unwrap();
        let mut changed = Vec::new();
        for (path, declaration) in [
            ("data/ini/object/americavehicle.ini", &object_change[0]),
            ("data/ini/armor.ini", &armor_change[0]),
        ] {
            let original = assets.read(path).unwrap();
            changed.push(Asset {
                path: path.into(),
                origin: "map.ini fixture".into(),
                bytes: merge_file(&original.bytes, declaration).unwrap(),
            });
        }
        let effective = assets.with_map_ini_provenance(changed, vec![Asset {
            path: "maps/fixture/map.ini".into(), origin: "map.ini fixture".into(),
            bytes: b"Object AmericaVehicleHumvee\nWeaponSet\nConditions=NONE\nWeapon=PRIMARY CrusaderTankGun\nEnd\nArmorSet\nConditions=NONE\nArmor=MapArmor\nEnd\nEnd\nArmor MapArmor\nArmor=DEFAULT 25%\nArmor=WATER 50%\nEnd\n".to_vec(),
        }]).unwrap();
        let catalog = crate::skirmish_data::load(&effective, &[]).unwrap();
        let humvee = catalog
            .types
            .iter()
            .find(|unit| unit.name == "AmericaVehicleHumvee")
            .unwrap();
        assert_eq!(humvee.weapon.name, "CrusaderTankGun");
        assert_eq!(
            humvee.armor[DamageType::ArmorPiercing as usize].to_bits(),
            0.25_f32.to_bits()
        );
        assert_eq!(
            humvee.armor[DamageType::Water as usize].to_bits(),
            0.5_f32.to_bits()
        );
    }

    #[test]
    fn unrelated_original_reskin_is_preserved_without_flattening_its_parent() {
        let original = b"Object Castle\nVisionRange=100\nEnd\nObjectReskin PaintedCastle Castle\nGeometryHeight=80\nEnd\n";
        let replacement = scan(b"Object Castle\nVisionRange=200\nEnd\n").unwrap();
        let changed = merge_file(original, &replacement[0]).unwrap();
        assert!(changed.ends_with(b"ObjectReskin PaintedCastle Castle\nGeometryHeight=80\nEnd\n"));
        let replacement = scan(b"Object PaintedCastle\nVisionRange=200\nEnd\n").unwrap();
        assert!(merge_file(original, &replacement[0]).is_err());
    }

    #[test]
    fn first_overridden_set_clears_its_family_and_later_sets_append_in_source_order() {
        let original = b"Object Tank\nWeaponSet\nConditions=NONE\nWeapon=PRIMARY OldGun\nEnd\nWeaponSet\nConditions=VETERAN\nWeapon=PRIMARY OldVeteran\nEnd\nArmorSet\nConditions=NONE\nArmor=OldArmor\nEnd\nPrerequisites\nObject=OldFactory\nEnd\nEnd\n";
        let changes = scan(b"Object Tank\nWeaponSet\nWeapon=PRIMARY NewGun\nEnd\nWeaponSet\nConditions=ELITE\nWeapon=PRIMARY EliteGun\nEnd\nPrerequisites\nObject=NewFactory\nEnd\nEnd\n").unwrap();
        let merged = merge_file(original, &changes[0]).unwrap();
        let object = scan(&merged).unwrap().remove(0);
        let weapons = crate::skirmish_data::blocks(&object.lines, 0, "WeaponSet").unwrap();
        assert_eq!(weapons.len(), 2);
        assert_eq!(weapons[0][1].tokens, ["Weapon", "PRIMARY", "NewGun"]);
        assert_eq!(weapons[1][1].tokens, ["Conditions", "ELITE"]);
        let armor = crate::skirmish_data::blocks(&object.lines, 0, "ArmorSet").unwrap();
        assert_eq!(armor[0][2].tokens, ["Armor", "OldArmor"]);
        let prerequisite = crate::skirmish_data::blocks(&object.lines, 0, "Prerequisites").unwrap();
        assert_eq!(prerequisite.len(), 1);
        assert_eq!(prerequisite[0][1].tokens, ["Object", "NewFactory"]);
        let solo = scan(b"Object Tank\nWeaponSet\nWeapon=PRIMARY SoloGun\nEnd\nEnd\n").unwrap();
        let merged = merge_file(&merged, &solo[0]).unwrap();
        let object = scan(&merged).unwrap().remove(0);
        let weapons = crate::skirmish_data::blocks(&object.lines, 0, "WeaponSet").unwrap();
        assert_eq!(weapons.len(), 1);
        assert_eq!(weapons[0][1].tokens, ["Weapon", "PRIMARY", "SoloGun"]);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn map_armor_replacement_clears_old_coefficients_and_preserves_ordered_default() {
        use generals_sim::skirmish::DamageType;
        let root = &generals_assets::game_root().unwrap();
        let armor_for = |bytes: &[u8], name: &str| {
            let assets = VirtualAssets::for_zero_hour(
                root.join("GameData/Command and Conquer Generals"),
                root.join("GameData/Command and Conquer Generals Zero Hour"),
            )
            .unwrap();
            let mut armors = assets.read("data/ini/armor.ini").unwrap().bytes;
            for declaration in scan(bytes).unwrap() {
                armors = merge_file(&armors, &declaration).unwrap();
            }
            let override_text = format!(
                "Object AmericaVehicleHumvee\nArmorSet\nConditions=NONE\nArmor={name}\nEnd\nEnd\n"
            );
            let declaration = scan(override_text.as_bytes()).unwrap().remove(0);
            let original = assets.read("data/ini/object/americavehicle.ini").unwrap();
            let effective = assets
                .with_map_ini_provenance(
                    vec![
                        Asset {
                            path: "data/ini/armor.ini".into(),
                            origin: "map.ini armor fixture".into(),
                            bytes: armors,
                        },
                        Asset {
                            path: original.path,
                            origin: "map.ini armor fixture".into(),
                            bytes: merge_file(&original.bytes, &declaration).unwrap(),
                        },
                    ],
                    vec![Asset {
                        path: "maps/fixture/map.ini".into(),
                        origin: "map.ini armor fixture".into(),
                        bytes: [override_text.as_bytes(), bytes].concat(),
                    }],
                )
                .unwrap();
            crate::skirmish_data::load(&effective, &[])
                .unwrap()
                .types
                .into_iter()
                .find(|unit| unit.name == "AmericaVehicleHumvee")
                .unwrap()
                .armor
        };
        let baseline = b"Armor TankArmor\nArmor=DEFAULT 5%\nArmor=LASER 0%\nEnd\n";
        let change = scan(
            b"Armor TankArmor\nArmor=SMALL_ARMS 0%\nArmor=DEFAULT 50%\nArmor=LASER 25%\nEnd\n",
        )
        .unwrap();
        let changed = merge_file(baseline, &change[0]).unwrap();
        let object = scan(&changed).unwrap().remove(0);
        let coefficients = armor_for(&changed, "TankArmor");
        assert_eq!(
            coefficients[DamageType::Laser as usize].to_bits(),
            0.25_f32.to_bits()
        );
        assert_eq!(
            coefficients[DamageType::SmallArms as usize].to_bits(),
            0.5_f32.to_bits()
        );
        assert_eq!(
            object
                .lines
                .iter()
                .filter(|line| line.tokens.first().is_some_and(|key| key == "Armor"))
                .count(),
            3
        );
        let new = scan(b"Armor MapArmor\nArmor=LASER 50%\nEnd\n").unwrap();
        let changed = merge_file(baseline, &new[0]).unwrap();
        let definitions = scan(&changed).unwrap();
        assert_eq!(definitions.len(), 2);
        assert_eq!(
            armor_for(&changed, "TankArmor")[DamageType::Laser as usize].to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            armor_for(&changed, "MapArmor")[DamageType::Laser as usize].to_bits(),
            0.5_f32.to_bits()
        );
    }

    #[test]
    fn tagged_module_override_preserves_order_other_fields_and_all_body_modules() {
        let original = b"Object Castle\nVisionRange=100\nBody=StructureBody BodyOriginal\nMaxHealth=2000\nEnd\nBehavior=GarrisonContain GarrisonOriginal\nContainMax=5\nEnd\nBehavior=FXListDie DeathOriginal\nDeathFX=FX_BuildingDie\nEnd\nEnd\nObject Other\nVisionRange=80\nEnd\n";
        let mut changes = scan(b"Object Castle\nReplaceModule GarrisonOriginal\nBehavior=GarrisonContain GarrisonNew\nContainMax=10\nEnd\nEnd\nAddModule\nBody=ActiveBody BodyNew\nMaxHealth=3000\nEnd\nEnd\nVisionRange=20\nEnd\n").unwrap();
        let bytes = merge_file(original, &changes.remove(0)).unwrap();
        let declarations = scan(&bytes).unwrap();
        let object = &declarations[0];
        let bodies = crate::skirmish_data::blocks(&object.lines, 0, "Body").unwrap();
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0][0].tokens[2], "BodyOriginal");
        assert_eq!(bodies[1][0].tokens[2], "BodyNew");
        let behaviors = crate::skirmish_data::blocks(&object.lines, 0, "Behavior").unwrap();
        assert_eq!(behaviors[0][0].tokens[2], "DeathOriginal");
        assert_eq!(behaviors[1][0].tokens[2], "GarrisonNew");
        assert_eq!(
            crate::skirmish_data::unsigned(&object.lines, 0, "VisionRange").unwrap(),
            20
        );
        assert_eq!(
            crate::skirmish_data::unsigned(&declarations[1].lines, 0, "VisionRange").unwrap(),
            80
        );
    }

    #[test]
    fn native_replace_module_requires_existing_tag_same_type_and_new_tag() {
        let original = b"Object Castle\nBehavior=GarrisonContain Old\nEnd\nEnd\n";
        for change in [
            b"Object Castle\nReplaceModule Missing\nBehavior=GarrisonContain New\nEnd\nEnd\nEnd\n"
                .as_slice(),
            b"Object Castle\nReplaceModule Old\nBehavior=KeepObjectDie New\nEnd\nEnd\nEnd\n",
            b"Object Castle\nReplaceModule Old\nBehavior=GarrisonContain Old\nEnd\nEnd\nEnd\n",
        ] {
            let mut changes = scan(change).unwrap();
            assert!(merge_file(original, &changes.remove(0)).is_err());
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_iron_dragon_and_mountain_guns_override_original_consumers_in_memory() {
        let (assets, _) = crate::cleanup_verification::tests::installed();
        let iron = load(&assets, "maps/iron dragon/iron dragon.map").unwrap();
        let iron = assets
            .with_map_ini_provenance(iron.effective_files, iron.raw_sources)
            .unwrap();
        let definitions = crate::world_data::object_definitions(&iron).unwrap();
        let castle = &definitions["GermanCastle"];
        let modules = crate::skirmish_data::blocks(&castle.lines, 0, "Behavior").unwrap();
        assert!(
            modules
                .iter()
                .any(|module| module[0].tokens[2] == "ModuleTag_03_Override"
                    && module[0].tokens[1] == "GarrisonContain")
        );
        assert!(
            !modules
                .iter()
                .any(|module| module[0].tokens[2] == "ModuleTag_03")
        );
        let (assets, _) = crate::cleanup_verification::tests::installed();
        let changes = load(&assets, "maps/mountain guns/mountain guns.map").unwrap();
        let mountain = assets
            .with_map_ini_provenance(changes.effective_files, changes.raw_sources)
            .unwrap();
        let definitions = crate::world_data::object_definitions(&mountain).unwrap();
        let station = &definitions["EnglishTVStatio"];
        assert_eq!(
            crate::skirmish_data::blocks(&station.lines, 0, "Body")
                .unwrap()
                .len(),
            2
        );
        assert!(
            !crate::skirmish_data::blocks(&station.lines, 0, "Behavior")
                .unwrap()
                .iter()
                .any(|module| module[0].tokens[1] == "GarrisonContain")
        );
        assert_eq!(
            crate::skirmish_data::number(&definitions["AmericaJetB52"].lines, 0, "VisionRange")
                .unwrap()
                .to_bits(),
            0.0_f32.to_bits()
        );
        let commands = content::scan(mountain.read("data/ini/commandset.ini").unwrap()).unwrap();
        let set = commands
            .declarations
            .iter()
            .find(|declaration| declaration.name == "EnglishTVStatioCommandSet")
            .unwrap();
        assert_eq!(
            crate::skirmish_data::text(&set.lines, 0, "1").unwrap(),
            "Command_CarpetBomb"
        );
        let (assets, _) = crate::cleanup_verification::tests::installed();
        let pristine = crate::world_data::object_definitions(&assets).unwrap();
        assert_eq!(
            crate::skirmish_data::blocks(&pristine["EnglishTVStatio"].lines, 0, "Body")
                .unwrap()
                .len(),
            1
        );
    }
}
