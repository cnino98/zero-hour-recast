use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

#[ignore = "requires original game installation"]
#[test]
fn save_keeps_original_file_unrelated_keys_native_balance_and_out_of_range_scroll() {
    let directory = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!(
            "options-save-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(directory.join("UserData")).unwrap();
    let original = directory.join("UserData/Options.ini");
    let source = "MusicVolume = 139.5\nVoiceVolume = 37.8\nSpeechVolume = 7\nScrollFactor = 300\nResolution = 1920 1200\nUseAlternateMouse = no\n";
    fs::write(&original, source).unwrap();
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let mut preferences = Preferences::load(&assets, &original).unwrap();
    let draft = preferences.draft();
    assert_eq!(draft.music, 139);
    assert_eq!(draft.voice, 37);
    assert_eq!(draft.scroll, 0);
    assert_eq!(
        preferences.applied().scroll_factor.to_bits(),
        3.0_f32.to_bits()
    );
    for relative in [-0.25, 0.25] {
        preferences.relative = relative;
        let mut draft = preferences.draft();
        draft.music = 0;
        draft.sfx = 80;
        draft.voice = 19;
        draft.alternate_mouse = true;
        let values = preferences.save(&draft).unwrap();
        assert_eq!(values.scroll_factor.to_bits(), 3.0_f32.to_bits());
        let (sound, sound3d) = if relative < 0.0 {
            (0.8_f32 * 0.75, 0.8_f32)
        } else {
            (0.8_f32, 0.8_f32 * 0.75)
        };
        assert_eq!(values.sound.to_bits(), sound.to_bits());
        assert_eq!(values.sound3d.to_bits(), sound3d.to_bits());
        let saved = fs::read_to_string(&preferences.path).unwrap();
        assert!(saved.contains("VoiceVolume = 19"));
        assert!(saved.contains("SpeechVolume = 7"));
        assert!(saved.contains("Resolution = 1920 1200"));
        assert!(saved.contains("ScrollFactor = 300"));
        assert_eq!(fs::read_to_string(&original).unwrap(), source);
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn original_scroll_factor_uses_integer_prefix_and_minimum_one() {
    for (input, expected) in [
        ("0", 0.01_f32),
        ("-12", 0.01),
        ("50.9", 0.5),
        ("  +145tail", 1.45),
        ("garbage", 0.01),
        ("300", 3.0),
    ] {
        let fields = fields(&format!("ScrollFactor = {input}"));
        assert_eq!(scroll(&fields, 0.5).unwrap().to_bits(), expected.to_bits());
    }
    assert_eq!(
        scroll(&BTreeMap::new(), 0.5).unwrap().to_bits(),
        0.5_f32.to_bits()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn actual_camera_and_editor_share_native_imported_scroll_factor_semantics() {
    let directory = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!(
            "options-camera-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(directory.join("UserData")).unwrap();
    let original = directory.join("UserData/Options.ini");
    let (assets, _) = crate::cleanup_verification::tests::installed();
    for (input, expected) in [
        ("0", 0.01_f32),
        ("-20", 0.01),
        ("50.9", 0.5),
        ("+145tail", 1.45),
        ("garbage", 0.01),
        ("300", 3.0),
    ] {
        fs::write(&original, format!("ScrollFactor = {input}")).unwrap();
        let camera = crate::world_data::camera_settings(&assets, &directory).unwrap();
        let preferences = Preferences::load(&assets, &original).unwrap();
        assert_eq!(camera.scroll_factor.to_bits(), expected.to_bits());
        assert_eq!(
            preferences.applied().scroll_factor.to_bits(),
            camera.scroll_factor.to_bits()
        );
    }
    fs::remove_file(&original).unwrap();
    assert_eq!(
        crate::world_data::camera_settings(&assets, &directory)
            .unwrap()
            .scroll_factor
            .to_bits(),
        0.5_f32.to_bits()
    );
    fs::remove_dir_all(directory).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
fn original_game_controls_use_yes_defaults_and_accept_keeps_unedited_money_volume() {
    let directory = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!(
            "options-game-controls-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(directory.join("UserData")).unwrap();
    let original = directory.join("UserData/Options.ini");
    let source = "Retaliation = no\nUseDoubleClickAttackMove = YES\nMoneyTransactionVolume = 37.8\nUnrelated = preserved\n";
    fs::write(&original, source).unwrap();
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let mut preferences = Preferences::load(&assets, &original).unwrap();
    assert!(!preferences.applied().retaliation);
    assert!(preferences.applied().double_click_attack_move);
    assert!(preferences.defaults().retaliation);
    assert!(!preferences.defaults().double_click_attack_move);
    let mut pending = preferences.draft();
    pending.retaliation = true;
    pending.double_click_attack_move = false;
    assert!(!preferences.applied().retaliation);
    assert!(preferences.applied().double_click_attack_move);
    let values = preferences.save(&pending).unwrap();
    assert!(values.retaliation);
    assert!(!values.double_click_attack_move);
    assert_eq!(
        values.money_transaction_volume.to_bits(),
        (37.0_f32 / 100.0).to_bits()
    );
    let saved = fs::read_to_string(&preferences.path).unwrap();
    assert!(saved.contains("Retaliation = yes"));
    assert!(saved.contains("UseDoubleClickAttackMove = no"));
    assert!(saved.contains("MoneyTransactionVolume = 37"));
    assert!(saved.contains("Unrelated = preserved"));
    assert_eq!(fs::read_to_string(&original).unwrap(), source);
    fs::remove_file(&preferences.path).unwrap();
    fs::write(
        &original,
        "Retaliation = TRUE\nUseDoubleClickAttackMove = true\nMoneyTransactionVolume = -5\n",
    )
    .unwrap();
    let preferences = Preferences::load(&assets, &original).unwrap();
    assert!(!preferences.applied().retaliation);
    assert!(!preferences.applied().double_click_attack_move);
    assert_eq!(
        preferences.applied().money_transaction_volume.to_bits(),
        0.0_f32.to_bits()
    );
    fs::remove_file(&original).unwrap();
    let preferences = Preferences::load(&assets, &original).unwrap();
    assert!(preferences.applied().retaliation);
    assert!(!preferences.applied().double_click_attack_move);
    assert_eq!(
        preferences.applied().money_transaction_volume.to_bits(),
        0.0_f32.to_bits()
    );
    fs::remove_dir_all(directory).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
fn zero_hour_money_default_is_constructor_zero_until_authored_audio_override() {
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let audio = content::scan(assets.read("data/ini/audiosettings.ini").unwrap()).unwrap();
    let mut lines = audio
        .declarations
        .into_iter()
        .find(|entry| entry.kind == "AudioSettings")
        .unwrap()
        .lines;
    assert!(crate::skirmish_data::field(&lines, 0, "DefaultMoneyTransactionVolume").is_none());
    assert_eq!(
        default_values(&assets, &lines)
            .unwrap()
            .money_transaction_volume
            .to_bits(),
        0.0_f32.to_bits()
    );
    lines.push(content::ContentLine {
        line: 0,
        tokens: vec!["DefaultMoneyTransactionVolume".into(), "45%".into()],
        raw: "DefaultMoneyTransactionVolume = 45%".into(),
        assigned: true,
        depth: Some(0),
    });
    assert_eq!(
        default_values(&assets, &lines)
            .unwrap()
            .money_transaction_volume
            .to_bits(),
        0.45_f32.to_bits()
    );
}

#[test]
fn source_resolution_parsing_keeps_custom_dimensions_and_rejects_invalid_windows() {
    for (text, expected) in [
        ("1920 1200", Some([1920, 1200])),
        ("+1217 +763 trailing", Some([1217, 763])),
        ("1920", None),
        ("1920.0 1080", None),
        ("garbage", None),
        ("0 600", None),
        ("-800 600", None),
        ("2147483648 600", None),
    ] {
        assert_eq!(resolution(Some(text)), expected);
    }
    assert_eq!(resolution(None), None);
    assert_eq!(integer_prefix("+73.8tail"), 73);
    assert_eq!(integer_prefix("garbage"), 0);
}

#[ignore = "requires original game installation"]
#[test]
fn display_preferences_defaults_preserve_resolution_and_accept_persists_gamma() {
    let directory = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!(
            "options-display-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(directory.join("UserData")).unwrap();
    let original = directory.join("UserData/Options.ini");
    let source = "Resolution = 1217 763\nGamma = 73.8\nUnrelated = preserved\n";
    fs::write(&original, source).unwrap();
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let mut preferences = Preferences::load(&assets, &original).unwrap();
    assert_eq!(preferences.applied().resolution, [1217, 763]);
    assert_eq!(preferences.draft().gamma, 73);
    let defaults = preferences.defaults();
    assert_eq!(defaults.resolution, [1217, 763]);
    assert_eq!(defaults.gamma, 50);
    assert_eq!(preferences.applied().gamma, 73);
    assert!(!preferences.path.exists());
    let mut draft = preferences.draft();
    draft.resolution = [1920, 1080];
    draft.gamma = 100;
    let accepted = preferences.save(&draft).unwrap();
    assert_eq!(accepted.resolution, [1920, 1080]);
    assert_eq!(accepted.gamma, 100);
    let saved = fs::read_to_string(&preferences.path).unwrap();
    assert!(saved.contains("Resolution = 1920 1080"));
    assert!(saved.contains("Gamma = 100"));
    assert!(saved.contains("Unrelated = preserved"));
    assert_eq!(fs::read_to_string(&original).unwrap(), source);
    draft.resolution = [0, 1080];
    assert!(preferences.save(&draft).is_err());
    assert_eq!(preferences.applied().resolution, [1920, 1080]);
    assert_eq!(fs::read_to_string(&preferences.path).unwrap(), saved);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn source_display_defaults_honor_authored_game_data_dimensions() {
    let lines = [
        content::ContentLine {
            line: 1,
            tokens: vec!["XResolution".into(), "1600".into()],
            raw: "XResolution = 1600".into(),
            assigned: true,
            depth: Some(0),
        },
        content::ContentLine {
            line: 2,
            tokens: vec!["YResolution".into(), "900".into()],
            raw: "YResolution = 900".into(),
            assigned: true,
            depth: Some(0),
        },
    ];
    assert_eq!(display_defaults(&[]).unwrap(), [800, 600]);
    assert_eq!(display_defaults(&lines[..1]).unwrap(), [1600, 600]);
    assert_eq!(display_defaults(&lines).unwrap(), [1600, 900]);
    let invalid = [content::ContentLine {
        line: 1,
        tokens: vec!["XResolution".into(), "0".into()],
        raw: "XResolution = 0".into(),
        assigned: true,
        depth: Some(0),
    }];
    assert!(display_defaults(&invalid).is_err());
}
