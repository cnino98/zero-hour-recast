use super::*;

#[test]
fn native_missing_and_corrupt_settings_use_fresh_defaults_without_partial_slots() {
    let defaults = Preferences::parse(PathBuf::new(), "", "Machine", 30);
    assert_eq!(defaults.name, "Machine");
    assert!(defaults.slots.iter().all(|slot| slot.faction.is_none()));
    assert!(matches!(defaults.slots[1].opponent, OpponentSlot::Easy));
    let corrupted = Preferences::parse(
        PathBuf::new(),
        "UserName = _00_D8\nColor = 99\nPlayerTemplate = 99\nSlotList = S=Hbroken:CH,0,3,0,0:;",
        "Machine",
        30,
    );
    assert_eq!(corrupted.name, "Machine");
    assert!(corrupted.slots.iter().all(|slot| slot.color.is_none()));
    assert!(corrupted.slots.iter().all(|slot| slot.faction.is_none()));
    let preferred = Preferences::parse(
        PathBuf::new(),
        "UserName = _20_00_20_00\nColor = 5\nPlayerTemplate = 3\nSlotList = broken",
        "Machine",
        30,
    );
    assert_eq!(preferred.name, "Machine");
    for slot in &preferred.slots[..2] {
        assert!(slot.faction == Some(Faction::China));
        assert_eq!(slot.color, Some(5));
    }
}

#[test]
fn native_slot_records_round_trip_all_represented_choices_and_utf8_names() {
    let mut selections: Vec<_> = [
        OpponentSlot::Open,
        OpponentSlot::Hard,
        OpponentSlot::Medium,
        OpponentSlot::Closed,
    ]
    .into_iter()
    .map(Slot::new)
    .collect();
    selections[0].faction = Some(Faction::Gla);
    selections[0].color = Some(7);
    selections[0].start = Some(3);
    selections[0].team = Some(0);
    selections[1].team = Some(0);
    selections[2].faction = Some(Faction::Usa);
    selections[2].start = Some(1);
    let encoded = slots::encode(
        &selections,
        "Café🚀",
        "maps\\fallen empire\\fallen empire.map",
        501,
        10_000,
        false,
    );
    let (decoded, name) = slots::parse(&encoded).unwrap();
    assert_eq!(name, "Café🚀");
    assert_eq!(decoded.len(), 8);
    for (original, loaded) in selections.iter().zip(decoded) {
        assert!(original.faction == loaded.faction);
        assert_eq!(original.color, loaded.color);
        assert_eq!(original.start, loaded.start);
        assert_eq!(original.team, loaded.team);
        assert!(original.opponent == loaded.opponent);
    }
    assert_eq!(name::decode_name(&name::encode(&name)).unwrap(), name);
    assert!(slots::parse(&encoded.replace(",4,3,0,0:", ",14,3,0,0:")).is_err());
    assert!(slots::parse(&encoded.replace("MC=0;", "")).is_err());
}

#[test]
fn persistence_is_local_atomic_and_preserves_unrepresented_preferences() {
    let directory = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!("skirmish-preferences-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("Skirmish.ini");
    assert!(read(&directory.join("missing.ini")).unwrap().is_none());
    let preferences = Preferences::parse(
        path.clone(),
        "FPS = 60\nUserName = N_00a_00m_00e_00",
        "Machine",
        30,
    );
    preferences
        .save(
            &preferences.slots,
            "maps\\fallen empire\\fallen empire.map",
            42,
            20_000,
            true,
            60,
        )
        .unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("FPS = 60\n"));
    assert!(contents.contains("SR=1;SC=20000;"));
    let restored = Preferences::parse(path.clone(), &contents, "DifferentMachine", 30);
    assert_eq!(restored.name, "Name");
    assert_eq!(restored.starting_cash, Some(20_000));
    assert!(restored.superweapon_restricted);
    assert_eq!(restored.game_speed, 60);
    assert!(restored.slots[0].faction.is_none());
    assert!(matches!(restored.slots[1].opponent, OpponentSlot::Easy));
    assert!(!path.with_extension("ini.tmp").exists());
    fs::write(&path, [0xff, 0xfe]).unwrap();
    assert_eq!(read(&path).unwrap().as_deref(), Some(""));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn native_options_load_independently_and_clamp_speed() {
    let invalid = Preferences::parse(
        PathBuf::new(),
        "StartingCash = broken\nFPS = -20\nSuperweaponRestrict = yEs",
        "Machine",
        30,
    );
    assert_eq!(invalid.starting_cash, None);
    assert_eq!(invalid.game_speed, 15);
    assert!(invalid.superweapon_restricted);
    let custom = Preferences::parse(
        PathBuf::new(),
        "StartingCash = 12345\nFPS = 1000\nSuperweaponRestrict = invalid",
        "Machine",
        30,
    );
    assert_eq!(custom.starting_cash, Some(12_345));
    assert_eq!(custom.game_speed, 61);
    assert!(!custom.superweapon_restricted);
}

#[test]
fn missing_speed_uses_installed_default_while_explicit_speed_is_retained() {
    let absent = Preferences::parse(PathBuf::new(), "", "Machine", 45);
    let explicit = Preferences::parse(PathBuf::new(), "FPS = 30", "Machine", 45);
    assert_eq!(absent.game_speed, 45);
    assert_eq!(explicit.game_speed, 30);
}
