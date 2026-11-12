use super::{blocks, definition, duration_frames, field, number, text, unsigned};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::income::HackerRules;
use std::error::Error;

pub(super) fn validate_command(files: &[ContentFile]) -> Result<(), Box<dyn Error>> {
    let command = definition(
        files,
        "CommandButton",
        "Command_ChinaInfantryHackerInternetHack",
    )?;
    if text(&command.lines, 0, "Command")? != "HACK_INTERNET" {
        return Err("Hacker's authored AI button no longer uses HACK_INTERNET".into());
    }
    Ok(())
}

pub(super) fn load(object: &Declaration) -> Result<Option<HackerRules>, Box<dyn Error>> {
    let Some(module) = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "HackInternetAIUpdate")
        })
    else {
        return Ok(None);
    };
    let duration = |name| field(module, 1, name).map_or(Ok(0), duration_frames);
    Ok(Some(HackerRules {
        unpack: duration("UnpackTime")?,
        pack: duration("PackTime")?,
        variation: field(module, 1, "PackUnpackVariationFactor")
            .map_or(Ok(0.0), |_| number(module, 1, "PackUnpackVariationFactor"))?,
        delay: duration("CashUpdateDelay")?,
        contained_delay: duration("CashUpdateDelayFast")?,
        amounts: [
            field(module, 1, "RegularCashAmount")
                .map_or(Ok(0), |_| unsigned(module, 1, "RegularCashAmount"))?,
            field(module, 1, "VeteranCashAmount")
                .map_or(Ok(0), |_| unsigned(module, 1, "VeteranCashAmount"))?,
            field(module, 1, "EliteCashAmount")
                .map_or(Ok(0), |_| unsigned(module, 1, "EliteCashAmount"))?,
            field(module, 1, "HeroicCashAmount")
                .map_or(Ok(0), |_| unsigned(module, 1, "HeroicCashAmount"))?,
        ],
        experience: field(module, 1, "XpPerCashUpdate")
            .map_or(Ok(0), |_| unsigned(module, 1, "XpPerCashUpdate"))?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn absent_native_hacker_fields_keep_constructor_zero_defaults() {
        let file = content::scan(Asset {
            path: "Defaults.ini".into(),
            origin: "native module defaults".into(),
            bytes: b"Object Hacker\n Behavior = HackInternetAIUpdate AI\n End\nEnd\n".to_vec(),
        })
        .unwrap();
        let rules = load(&file.declarations[0]).unwrap().unwrap();
        assert_eq!(
            [rules.pack, rules.unpack, rules.delay, rules.contained_delay],
            [0; 4]
        );
        assert_eq!(rules.amounts, [0; 4]);
        assert_eq!(rules.experience, 0);
        assert_eq!(rules.variation.to_bits(), 0.0_f32.to_bits());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn imported_original_hacker_module_retains_frame_rounding_and_rank_amounts() {
        let file = content::scan(Asset {
            path: "ChinaInfantry.ini".into(),
            origin: "Installed original Hacker AI fixture".into(),
            bytes: std::fs::read(
                generals_assets::game_root()
                    .unwrap()
                    .join("test-fixtures/hacker.ini"),
            )
            .unwrap(),
        })
        .unwrap();
        let rules = load(&file.declarations[0]).unwrap().unwrap();
        assert_eq!((rules.unpack, rules.pack, rules.delay), (219, 154, 60));
        assert_eq!(rules.contained_delay, 54);
        assert_eq!(rules.amounts, [5, 6, 8, 10]);
        assert_eq!(rules.experience, 1);
        assert_eq!(rules.variation.to_bits(), 0.5_f32.to_bits());
    }
}
