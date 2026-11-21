//! Original faction shortcut definitions and WND tray geometry.
use super::{
    Command, CommandAction, UnitType, command_details, find, native_power_action, value,
    window_area,
};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{collections::BTreeMap, error::Error};

pub struct Data {
    pub factions: Vec<FactionTray>,
    pub countdown: Countdown,
}

pub struct Countdown {
    pub position: Vec2,
    pub flash_frames: u32,
    pub flash_color: Color,
    pub point_sizes: [f32; 2],
    pub resolution_adjustment: f32,
    pub ready_font: Option<Font>,
    pub ready_handle: Handle<Font>,
}

pub struct FactionTray {
    pub commands: Vec<Command>,
    pub slots: Vec<Slot>,
}

pub struct Slot {
    pub button: Rect,
    pub parent: Rect,
    pub image: String,
}

impl Data {
    pub(super) fn images(&self) -> impl Iterator<Item = &str> {
        self.factions.iter().flat_map(|faction| {
            faction
                .commands
                .iter()
                .map(|button| button.image.as_str())
                .chain(faction.slots.iter().map(|slot| slot.image.as_str()))
        })
    }
}

pub(super) fn load(
    assets: &VirtualAssets,
    catalog: &[UnitType],
    labels: &BTreeMap<String, String>,
) -> Result<Data, Box<dyn Error>> {
    let read = |name| content::scan(assets.read(name)?).map_err(Into::<Box<dyn Error>>::into);
    let players = read("data/ini/playertemplate.ini")?;
    let sets = read("data/ini/commandset.ini")?;
    let buttons = read("data/ini/commandbutton.ini")?;
    let upgrades = read("data/ini/upgrade.ini")?;
    let mut factions = Vec::new();
    for faction in ["FactionAmerica", "FactionChina", "FactionGLA"] {
        let config = &find(&players, "PlayerTemplate", faction)?.lines;
        let name = value(config, 0, "SpecialPowerShortcutWinName")?;
        let count: usize = value(config, 0, "SpecialPowerShortcutButtonCount")?.parse()?;
        let set = find(
            &sets,
            "CommandSet",
            value(config, 0, "SpecialPowerShortcutCommandSet")?,
        )?;
        let file = assets.read(&format!("window/{name}"))?;
        let layout = std::str::from_utf8(&file.bytes)?;
        let window = |name: &str| -> Result<_, Box<dyn Error>> {
            layout
                .split("WINDOWTYPE = ")
                .skip(1)
                .find(|block| block.contains(&format!("NAME = \"{name}\"")))
                .ok_or_else(|| format!("missing shortcut WND window {name}").into())
        };
        let mut slots = Vec::new();
        let mut commands = Vec::new();
        for slot in 1..=count {
            let parent = window(&format!("{name}:ButtonParent{slot}"))?;
            let image = parent
                .split_once("ENABLEDDRAWDATA = IMAGE: ")
                .and_then(|(_, next)| next.split_once(','))
                .map(|(name, _)| name)
                .ok_or("missing shortcut authored tray image")?;
            slots.push(Slot {
                parent: window_area(parent)?,
                button: window_area(window(&format!("{name}:ButtonCommand{slot}"))?)?,
                image: image.into(),
            });
        }
        for (slot, name) in super::garrison::command_slots(Some(set), false)? {
            let button = find(&buttons, "CommandButton", name)?;
            let action = match super::optional_value(&button.lines, "SpecialPower")? {
                Some("SuperweaponCashHack") => Some(CommandAction::CashHack),
                Some("SuperweaponEmergencyRepair") => Some(CommandAction::EmergencyRepair),
                Some("SuperweaponSpectreGunship") => Some(CommandAction::SpectreGunship),
                _ => native_power_action(button, catalog)?,
            };
            if let Some(action) = action {
                commands.push(command_details(slot, button, action, labels, &upgrades)?);
            }
        }
        commands.sort_by_key(|button| button.slot);
        factions.push(FactionTray { commands, slots });
    }
    let input = read("data/ini/ingameui.ini")?;
    let language = read("data/english/language.ini")?;
    Ok(Data {
        factions,
        countdown: countdown(
            &find(&input, "InGameUI", "")?.lines,
            &find(&language, "Language", "")?.lines,
        )?,
    })
}

fn countdown(
    lines: &[generals_assets::content::ContentLine],
    language: &[generals_assets::content::ContentLine],
) -> Result<Countdown, Box<dyn Error>> {
    let tokens = |key| -> Result<Vec<&str>, Box<dyn Error>> {
        Ok(super::field(lines, 0, key)?
            .iter()
            .flat_map(|token| token.split(':'))
            .filter(|token| !token.is_empty())
            .collect())
    };
    let coordinates = tokens("SuperweaponCountdownPosition")?;
    let ["X", left, "Y", top] = coordinates.as_slice() else {
        return Err("invalid public superweapon timer coordinates".into());
    };
    let colors = tokens("SuperweaponCountdownFlashColor")?;
    let ["R", red, "G", green, "B", blue] = colors.as_slice() else {
        return Err("invalid public superweapon timer color".into());
    };
    let [normal_family, normal_size, normal_bold] =
        super::field(language, 0, "SuperweaponCountdownNormalFont")?
    else {
        return Err("invalid localized normal superweapon timer font".into());
    };
    let [ready_family, ready_size, ready_bold] =
        super::field(language, 0, "SuperweaponCountdownReadyFont")?
    else {
        return Err("invalid localized ready superweapon timer font".into());
    };
    if normal_family != "Arial" || !normal_bold.eq_ignore_ascii_case("no") {
        return Err("unsupported original normal superweapon timer font".into());
    }
    let ready_font = crate::fonts::load(ready_family, ready_bold.eq_ignore_ascii_case("yes"))?;
    Ok(Countdown {
        position: Vec2::new(left.parse()?, top.parse()?),
        flash_frames: crate::skirmish_data::source_numbers::duration_frames(value(
            lines,
            0,
            "SuperweaponCountdownFlashDuration",
        )?)?,
        flash_color: Color::srgb_u8(red.parse()?, green.parse()?, blue.parse()?),
        point_sizes: [
            f32::from(normal_size.parse::<u16>()?),
            f32::from(ready_size.parse::<u16>()?),
        ],
        resolution_adjustment: crate::fonts::resolution_adjustment(language)?,
        ready_font: Some(ready_font),
        ready_handle: Handle::default(),
    })
}
