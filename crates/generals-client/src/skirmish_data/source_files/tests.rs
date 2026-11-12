use super::*;

#[ignore = "requires original game installation"]
#[test]
fn installed_dozer_keeps_fields_after_non_module_blocks() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let (files, _) = effective(&assets, &["AmericaVehicleDozer"]).unwrap();
    let object = crate::skirmish_data::definition(&files, "Object", "AmericaVehicleDozer").unwrap();
    assert_eq!(
        crate::skirmish_data::locomotor::normal_set(&object.lines).unwrap(),
        ["AmericaVehicleDozerLocomotor"]
    );
    assert_eq!(
        crate::skirmish_data::text(&object.lines, 0, "VoiceSelect").unwrap(),
        "DozerUSAVoiceSelect"
    );
    let weapons = crate::skirmish_data::blocks(&object.lines, 0, "WeaponSet").unwrap();
    assert_eq!(weapons.len(), 2);
    assert_eq!(
        crate::skirmish_data::text(weapons[1], 1, "Conditions").unwrap(),
        "MINE_CLEARING_DETAIL"
    );
    let sounds = crate::skirmish_data::blocks(&object.lines, 0, "UnitSpecificSounds").unwrap();
    assert_eq!(sounds.len(), 1);
    assert_eq!(
        crate::skirmish_data::text(sounds[0], 1, "VoiceBuildResponse").unwrap(),
        "DozerUSAVoiceBuild"
    );
}

#[test]
fn actual_map_then_solo_loading_reconciles_inherited_modules_and_raw_ai_order() {
    use generals_assets::virtual_assets::Layer;
    let directory = write_map_override_fixture();
    let assets = VirtualAssets::from_layers(vec![Layer::Loose(directory.clone())]);
    let changes = crate::world_data::map_content(&assets, "maps/fixture/test.map").unwrap();
    assert_eq!(changes.raw_sources[0].bytes, MAP);
    assert_eq!(changes.raw_sources[1].bytes, SOLO);
    assert_eq!(changes.raw_sources[0].path, "maps/fixture/map.ini");
    assert_eq!(changes.raw_sources[1].path, "maps/fixture/solo.ini");
    let original_map = assets.read("maps/fixture/map.ini").unwrap();
    let original_solo = assets.read("maps/fixture/solo.ini").unwrap();
    assert_eq!(changes.raw_sources[0].origin, original_map.origin);
    assert_eq!(changes.raw_sources[1].origin, original_solo.origin);
    let assets = assets
        .with_map_ini_provenance(changes.effective_files, changes.raw_sources)
        .unwrap();
    let definitions = crate::world_data::object_definitions(&assets).unwrap();
    let object = &definitions["Tank"];
    let behaviors = crate::skirmish_data::blocks(&object.lines, 0, "Behavior").unwrap();
    let tags: Vec<_> = behaviors
        .iter()
        .map(|lines| lines[0].tokens[2].as_str())
        .collect();
    assert_eq!(tags, ["Physics", "SoloStealth", "SharedAI"]);
    assert_eq!(
        crate::skirmish_data::text(behaviors[1], 1, "StealthDelay").unwrap(),
        "99"
    );
    assert_eq!(
        crate::skirmish_data::text(behaviors[2], 1, "MoodAttackCheckRate").unwrap(),
        "100"
    );
    let (files, metadata) =
        crate::skirmish_data::source_files::effective(&assets, &["Tank"]).unwrap();
    let object = crate::skirmish_data::definition(&files, "Object", "Tank").unwrap();
    assert_eq!(
        crate::skirmish_data::blocks(&object.lines, 0, "Behavior")
            .unwrap()
            .len(),
        3
    );
    let modules = &metadata["Tank"].modules[0];
    assert!(metadata["Tank"].trainable);
    assert_eq!(
        metadata["Tank"]
            .kind_of
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["VEHICLE"]
    );
    assert_eq!(
        crate::skirmish_data::text(&object.lines, 0, "IsTrainable").unwrap(),
        "Yes"
    );
    assert_eq!(
        crate::skirmish_data::text(&object.lines, 0, "KindOf").unwrap(),
        "VEHICLE"
    );
    assert_eq!(
        modules
            .iter()
            .map(|entry| (
                entry.family.as_str(),
                entry.tag.as_str(),
                entry.copied_from_default
            ))
            .collect::<Vec<_>>(),
        [
            ("ActiveBody", "Body", true),
            ("PhysicsBehavior", "Physics", true),
            ("StealthUpdate", "SoloStealth", false),
            ("AIUpdateInterface", "SharedAI", false),
        ]
    );
    let raw_solo = content::scan(original_solo).unwrap();
    for entry in modules.iter().filter(|entry| !entry.copied_from_default) {
        let physical = raw_solo.declarations[0]
            .lines
            .iter()
            .find(|line| line.tokens.get(2) == Some(&entry.tag))
            .unwrap();
        assert_eq!(entry.source.line, physical.line);
        assert_eq!(entry.source.path, raw_solo.path);
        assert_eq!(entry.source.origin, raw_solo.origin);
    }

    std::fs::remove_dir_all(directory).unwrap();
}

const MAP: &[u8] = b"Object Tank\nRemoveModule=ModuleTag_DefaultAutoHealBehavior\nReplaceModule=DefaultStealth\nBehavior=StealthUpdate NewStealth\nStealthDelay=77\nEnd\nEnd\nAddModule\nBehavior=TransportAIUpdate SharedAI\nEnd\nEnd\nEnd\nObject Tank\nAddModule\nBehavior=WanderAIUpdate SharedAI\nMoodAttackCheckRate=67\nEnd\nEnd\nEnd\n";
const SOLO: &[u8] = b"Object Tank\nReplaceModule=NewStealth\nBehavior=StealthUpdate SoloStealth\nStealthDelay=99\nEnd\nEnd\nAddModule\nBehavior=AIUpdateInterface SharedAI\nMoodAttackCheckRate=100\nEnd\nEnd\nEnd\n";

fn write_map_override_fixture() -> std::path::PathBuf {
    let directory = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!("metadata-map-fixture-{}", std::process::id()));
    for name in crate::skirmish_data::source_files::SOURCE_NAMES {
        let path = directory.join(format!("data/ini/{name}.ini"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }
    std::fs::write(directory.join("data/ini/default/object.ini"), b"Object DefaultThingTemplate\nKindOf=NONE\nBody=InactiveBody ModuleTag_DefaultInactiveBody\nEnd\nInheritableModule\nBehavior=AutoHealBehavior ModuleTag_DefaultAutoHealBehavior\nHealingAmount=2\nEnd\nBehavior=StealthUpdate DefaultStealth\nEnd\nEnd\nEnd\n").unwrap();
    std::fs::write(directory.join("data/ini/object/americavehicle.ini"), b"Object Tank\nKindOf=VEHICLE\nIsTrainable=Yes\nBody=ActiveBody Body\nMaxHealth=100\nEnd\nBehavior=AIUpdateInterface OriginalAI\nEnd\nBehavior=PhysicsBehavior Physics\nMass=5\nEnd\nEnd\n").unwrap();
    std::fs::create_dir_all(directory.join("maps/fixture")).unwrap();
    std::fs::write(directory.join("maps/fixture/map.ini"), MAP).unwrap();
    std::fs::write(directory.join("maps/fixture/solo.ini"), SOLO).unwrap();
    directory
}

#[test]
fn runtime_feedback_variations_enter_the_actual_roster_dependency_closure() {
    use generals_sim::skirmish::{Faction, Kind};
    let directory = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!("assistance-closure-fixture-{}", std::process::id()));
    for name in SOURCE_NAMES {
        let path = directory.join(format!("data/ini/{name}.ini"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }
    std::fs::write(
        directory.join("data/ini/default/object.ini"),
        b"Object DefaultThingTemplate\nEnd\n",
    )
    .unwrap();
    std::fs::write(directory.join("data/ini/object/system.ini"), b"Object MapPatriot\nBehavior=AssistedTargetingUpdate Assist\nLaserToTarget=FeedbackChoice\nEnd\nEnd\nObject FeedbackChoice\nBuildVariations=FeedbackA FeedbackB\nEnd\nObject FeedbackA\nEnd\nObject FeedbackB\nEnd\n").unwrap();
    let assets = VirtualAssets::from_layers(vec![generals_assets::virtual_assets::Layer::Loose(
        directory.clone(),
    )]);
    let mut roster = vec![("MapPatriot".to_owned(), Faction::Usa, Kind::Defense)];
    let (_, metadata) = with_runtime_dependencies(&assets, &mut roster).unwrap();
    assert_eq!(
        roster
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["MapPatriot", "FeedbackChoice", "FeedbackA", "FeedbackB"]
    );
    assert!(metadata.contains_key("FeedbackA"));
    assert!(metadata.contains_key("FeedbackB"));
    std::fs::remove_dir_all(directory).unwrap();
}
