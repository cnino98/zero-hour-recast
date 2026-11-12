//! Hacker data reaches the shared native special-power and ability parsers.
#[cfg(test)]
mod tests {
    use crate::skirmish_data::{special_ability, special_power};
    use generals_assets::{content, virtual_assets::Asset};
    fn fixture(source: &str) -> content::ContentFile {
        content::scan(Asset {
            path: "HackerDisable.ini".into(),
            origin: "Native Hacker modules".into(),
            bytes: format!("{source}\nGameData\nEnd\n").into_bytes(),
        })
        .unwrap()
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_disable_modules_keep_source_rounding_and_initial_recharge() {
        let files = [fixture(
            &std::fs::read_to_string(
                generals_assets::game_root()
                    .unwrap()
                    .join("test-fixtures/hacker-disable.ini"),
            )
            .unwrap(),
        )];
        let rules = special_ability::load(&files, &files[0].declarations[0]).unwrap();
        let power = special_power::load(&files, &files[0].declarations[0]).unwrap();
        let rules = &rules[0];
        assert_eq!(rules.approach.start_range.to_bits(), 150.0_f32.to_bits());
        assert_eq!(
            (
                rules.timing.unpack,
                rules.timing.pack,
                rules.timing.preparation,
                rules.timing.persistent,
                rules.timing.effect,
                power[0].timer.reload_frames
            ),
            (219, 154, 90, 10, 60, 15)
        );
        assert!(!power[0].startup.ready);
        assert_eq!(rules.objects.name, "BinaryDataStream");
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn authored_abort_range_reaches_native_ability_without_name_specific_rejection() {
        let source = std::fs::read_to_string(
            generals_assets::game_root()
                .unwrap()
                .join("test-fixtures/hacker-disable.ini"),
        )
        .unwrap()
        .replace(
            "StartAbilityRange = 150.0",
            "StartAbilityRange = 150.0\n AbilityAbortRange = 200.0",
        );
        let files = [fixture(&source)];
        let rules = special_ability::load(&files, &files[0].declarations[0]).unwrap();
        assert_eq!(rules[0].approach.abort_range.to_bits(), 200.0_f32.to_bits());
    }
}
