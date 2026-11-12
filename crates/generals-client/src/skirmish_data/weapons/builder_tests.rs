use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::{DamageType, Faction, Kind, Prefire};

#[ignore = "requires original game installation"]
#[test]
fn installed_builders_keep_empty_working_set_and_original_disarm_weapon() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = super::super::source_files(&assets).unwrap();
    for (name, faction, weapon, delay, clip, reload, voice) in [
        (
            "AmericaVehicleDozer",
            Faction::Usa,
            "DozerMineDisarmingWeapon",
            36,
            1,
            4.0_f32,
            "DozerUSAVoiceClearMine",
        ),
        (
            "ChinaVehicleDozer",
            Faction::China,
            "DozerMineDisarmingWeapon",
            36,
            1,
            4.0,
            "DozerChinaVoiceClearMine",
        ),
        (
            "GLAInfantryWorker",
            Faction::Gla,
            "WorkerMineDisarmingWeapon",
            30,
            0,
            0.0,
            "WorkerVoiceClearMine",
        ),
    ] {
        let object = super::definition(&files, "Object", name).unwrap();
        let unit = super::super::unit_type(&files, object, faction, Kind::Builder)
            .and_then(|unit| super::super::finish_type(&files, object, unit))
            .unwrap();
        assert!(unit.weapon.name.is_empty());
        assert!(unit.weapon_upgrade_triggers.is_empty());
        let sets = &unit.weapon_set_rules.replacements;
        assert_eq!(sets.len(), 2, "{name}");
        assert_eq!(sets[0].flags, 0);
        assert!(sets[0].weapons.iter().all(Option::is_none));
        assert_eq!(sets[1].flags, 16);
        let clearing = unit.weapon_template(sets[1].weapons[0].unwrap()).unwrap();
        assert_eq!(clearing.name, weapon);
        assert!(matches!(clearing.damage_type, DamageType::Disarm));
        assert_eq!(clearing.range.to_bits(), 5.0_f32.to_bits());
        assert_eq!(
            clearing.continue_attack_range.to_bits(),
            100.0_f32.to_bits()
        );
        assert_eq!(clearing.pre_attack_frames, delay);
        assert_eq!(clearing.clip_size, clip);
        assert_eq!(clearing.reload_seconds.to_bits(), reload.to_bits());
        assert_eq!(clearing.anti_mask, 0x10);
        assert!(if clip == 0 {
            matches!(clearing.pre_attack_type, Prefire::Shot)
        } else {
            matches!(clearing.pre_attack_type, Prefire::Attack)
        });
        let sounds = super::blocks(&object.lines, 0, "UnitSpecificSounds").unwrap();
        assert_eq!(
            super::scalar(super::field(sounds[0], 1, "VoiceDisarm").unwrap()).unwrap(),
            voice
        );
    }
}
