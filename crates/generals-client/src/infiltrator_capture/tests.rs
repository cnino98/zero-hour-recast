use generals_assets::{content, virtual_assets::VirtualAssets};

#[ignore = "requires original game installation"]
#[test]
fn original_lotus_trigger_is_bound_for_all_three_powers_but_has_no_audio_samples() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let objects = content::scan(assets.read("data/ini/object/chinainfantry.ini").unwrap()).unwrap();
    let lotus = objects
        .declarations
        .iter()
        .find(|declaration| {
            declaration.kind == "Object" && declaration.name == "ChinaInfantryBlackLotus"
        })
        .unwrap();
    let triggers: Vec<_> = lotus
        .lines
        .iter()
        .filter(|line| {
            line.tokens
                .first()
                .is_some_and(|field| field == "TriggerSound")
        })
        .collect();
    assert_eq!(triggers.len(), super::ABILITIES.len());
    assert!(
        triggers
            .iter()
            .all(|line| line.tokens == ["TriggerSound", "BlackLotusTrigger"])
    );
    let sounds = content::scan(assets.read("data/ini/soundeffects.ini").unwrap()).unwrap();
    let trigger = sounds
        .declarations
        .iter()
        .find(|declaration| {
            declaration.kind == "AudioEvent" && declaration.name == "BlackLotusTrigger"
        })
        .unwrap();
    let samples = trigger
        .lines
        .iter()
        .find(|line| line.tokens.first().is_some_and(|field| field == "Sounds"))
        .unwrap();
    assert_eq!(samples.tokens, ["Sounds"]);
    assert!(!trigger.lines.iter().any(|line| {
        line.tokens
            .first()
            .is_some_and(|field| matches!(field.as_str(), "Attack" | "Decay"))
            && line.tokens.len() > 1
    }));
}
