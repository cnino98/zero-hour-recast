use super::{find, optional_value, translate, value, window_area};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{collections::BTreeMap, error::Error, path::Path};

pub struct Promotions {
    pub buttons: Vec<Vec<ScienceButton>>,
    pub labels: Vec<ScienceLabel>,
    pub experience_frame: Rect,
    pub close_label: String,
    pub accelerator: Option<char>,
    pub transition_speed: f32,
}

pub struct ScienceButton {
    pub science: String,
    pub window: String,
    pub image: String,
    pub label: String,
    pub description: String,
    pub cost: u32,
}

pub struct ScienceLabel {
    pub area: Rect,
    pub label: String,
    pub size: f32,
}

pub(super) fn load(
    assets: &VirtualAssets,
    labels: &BTreeMap<String, String>,
    options: &Path,
) -> Result<Promotions, Box<dyn Error>> {
    validate_transition(assets)?;
    let players = content::scan(assets.read("data/ini/playertemplate.ini")?)?;
    let sets = content::scan(assets.read("data/ini/commandset.ini")?)?;
    let commands = content::scan(assets.read("data/ini/commandbutton.ini")?)?;
    let sciences = content::scan(assets.read("data/ini/science.ini")?)?;
    let mut buttons = Vec::new();
    for faction in ["FactionAmerica", "FactionChina", "FactionGLA"] {
        let player = find(&players, "PlayerTemplate", faction)?;
        let mut entries = Vec::new();
        for rank in [1, 3, 8] {
            let field = format!("PurchaseScienceCommandSetRank{rank}");
            let set = find(&sets, "CommandSet", value(&player.lines, 0, &field)?)?;
            for line in &set.lines {
                if line.depth != Some(0) || line.tokens.len() != 2 {
                    continue;
                }
                let Ok(slot) = line.tokens[0].parse::<u8>() else {
                    continue;
                };
                let command = find(&commands, "CommandButton", &line.tokens[1])?;
                if command.lines.iter().any(|line| {
                    line.tokens.first().is_some_and(|name| name == "Options")
                        && line.tokens.iter().any(|value| value == "SCRIPT_ONLY")
                }) {
                    continue;
                }
                if value(&command.lines, 0, "Command")? != "PURCHASE_SCIENCE" {
                    return Err("unsupported promotion command".into());
                }
                let science = value(&command.lines, 0, "Science")?;
                let definition = find(&sciences, "Science", science)?;
                let index = slot.checked_sub(1).ok_or("zero promotion command slot")?;
                entries.push(ScienceButton {
                    science: science.into(),
                    window: format!("GeneralsExpPoints.wnd:ButtonRank{rank}Number{index}"),
                    image: value(&command.lines, 0, "ButtonImage")?.into(),
                    label: translate(labels, value(&definition.lines, 0, "DisplayName")?)?,
                    description: translate(labels, value(&definition.lines, 0, "Description")?)?,
                    cost: value(&definition.lines, 0, "SciencePurchasePointCost")?.parse()?,
                });
            }
        }
        buttons.push(entries);
    }
    let window = assets.read("window/generalsexppoints.wnd")?;
    let window = std::str::from_utf8(&window.bytes)?;
    let mut text_labels = Vec::new();
    let mut experience_frame = None;
    for block in window.split("WINDOWTYPE = ").skip(1) {
        if block.contains("IMAGE: ExperienceBar,") {
            experience_frame = Some(window_area(block)?);
        }
        if !block.starts_with("STATICTEXT;") {
            continue;
        }
        if let Some(key) = block
            .split_once("\n    TEXT = \"")
            .or_else(|| block.split_once("TEXT = \""))
            .and_then(|(_, suffix)| suffix.split_once('"').map(|(value, _)| value))
        {
            text_labels.push(ScienceLabel {
                area: window_area(block)?,
                label: translate(labels, key)?,
                size: 10.0, // All three source rank headings use Arial 10.
            });
        }
    }
    let command = find(&commands, "CommandButton", "NonCommand_GeneralsExperience")?;
    let label = labels
        .get(&value(&command.lines, 0, "TextLabel")?.to_ascii_lowercase())
        .ok_or("missing promotions label")?;
    Ok(Promotions {
        buttons,
        labels: text_labels,
        experience_frame: experience_frame.ok_or("missing experience frame")?,
        close_label: translate(labels, "GUI:CloseExpScreen")?,
        transition_speed: transition_speed(options)?,
        accelerator: label
            .split_once('&')
            .and_then(|(_, suffix)| suffix.chars().next()),
    })
}

pub(super) fn special_power(
    lines: &[generals_assets::content::ContentLine],
) -> Result<Option<&str>, Box<dyn Error>> {
    let Some(name) = optional_value(lines, "SpecialPower")? else {
        return Ok(None);
    };
    Ok(matches!(
        name,
        "SuperweaponCashHack"
            | "SuperweaponEmergencyRepair"
            | "SuperweaponSpectreGunship"
            | "AirF_SuperweaponSpectreGunship"
            | "SpecialAbilityRangerCaptureBuilding"
            | "SpecialAbilityRedGuardCaptureBuilding"
            | "SpecialAbilityRebelCaptureBuilding"
            | "SpecialAbilityBlackLotusCaptureBuilding"
            | "SpecialAbilityBlackLotusDisableVehicleHack"
            | "SpecialAbilityBlackLotusStealCashHack"
            | "SpecialAbilityMissileDefenderLaserGuidedMissiles"
            | "SpecialAbilityHackerDisableBuilding"
            | "SpecialAbilityChangeBattlePlans"
            | "SpecialAbilityAmbulanceCleanupArea"
            | "SpecialAbilityColonelBurtonTimedCharges"
            | "SpecialAbilityColonelBurtonRemoteCharges"
            | "SpecialAbilityDisguiseAsVehicle"
    )
    .then_some(name))
}

fn validate_transition(assets: &VirtualAssets) -> Result<(), Box<dyn Error>> {
    let transitions = content::scan(assets.read("data/ini/windowtransitions.ini")?)?;
    let lines = &find(&transitions, "WindowTransition", "GenExpFade")?.lines;
    if value(lines, 1, "WinName")? != "GeneralsExpPoints.wnd:GenExpParent"
        || value(lines, 1, "Style")? != "WINFADE"
        || value(lines, 1, "FrameDelay")? != "0"
        || !value(lines, 0, "FireOnce")?.eq_ignore_ascii_case("Yes")
    {
        return Err("unsupported GenExpFade transition definition".into());
    }
    Ok(())
}

fn transition_speed(options: &Path) -> Result<f32, Box<dyn Error>> {
    let text = match std::fs::read_to_string(options) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(1.0),
        Err(error) => return Err(error.into()),
    };
    let mut speed = 1.0;
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=')
            && key
                .trim()
                .eq_ignore_ascii_case("GameWindowTransitionSpeedMultiplier")
        {
            let parsed: f32 = value.trim().parse()?;
            if !parsed.is_finite() {
                return Err("nonfinite GameWindowTransitionSpeedMultiplier".into());
            }
            // OptionPreferences::getGameWindowTransitionSpeedMultiplier.
            speed = parsed.clamp(1.0, 1000.0);
        }
    }
    Ok(speed)
}

#[cfg(test)]
mod tests {
    #[test]
    fn hacker_disable_command_survives_supported_special_power_filter() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "CommandButton.ini".into(),
            origin: "Installed base China Hacker disable command".into(),
            bytes: b"CommandButton Command_ChinaInfantryHackerDisableBuilding\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityHackerDisableBuilding\n  Options = NEED_SPECIAL_POWER_SCIENCE OK_FOR_MULTI_SELECT NEED_TARGET_ENEMY_OBJECT NEED_TARGET_NEUTRAL_OBJECT\n  ButtonImage = SSBuildingHack\n  CursorName = Hack\n  InvalidCursorName = GenericInvalid\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            super::special_power(&file.declarations[0].lines).unwrap(),
            Some("SpecialAbilityHackerDisableBuilding")
        );
    }

    #[test]
    fn laser_lock_command_survives_supported_special_power_filter() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "CommandButton.ini".into(),
            origin: "Original Missile Defender command fixture".into(),
            bytes: b"CommandButton Command_AmericaMissileDefenderLaserGuidedMissiles\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityMissileDefenderLaserGuidedMissiles\nEnd\n".to_vec(),
        })
        .unwrap();
        assert_eq!(
            super::special_power(&file.declarations[0].lines).unwrap(),
            Some("SpecialAbilityMissileDefenderLaserGuidedMissiles")
        );
    }
}
