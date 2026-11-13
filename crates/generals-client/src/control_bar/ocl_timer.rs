use super::{Command, find, read_command, results, translate, window_area};
use bevy::prelude::{Color, Rect};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeMap, error::Error};

pub struct Data {
    pub sell: Command,
    pub rally: Command,
    pub bar: Rect,
    pub bar_background: Color,
    pub bar_border: Color,
    pub fill: Color,
    pub fill_border: Color,
    pub description: results::Window,
    pub padded_description: String,
}

pub(super) fn load(
    assets: &VirtualAssets,
    catalog: &[UnitType],
    labels: &BTreeMap<String, String>,
) -> Result<Data, Box<dyn Error>> {
    let wnd = assets.read("window/controlbar.wnd")?;
    let wnd = std::str::from_utf8(&wnd.bytes)?;
    let window = |name| {
        wnd.split("WINDOWTYPE = ")
            .skip(1)
            .find(|block| results::property(block, "NAME").is_ok_and(|value| value == name))
            .ok_or("missing original ControlBar OCL timer window")
    };
    let description = results::window(window("\"ControlBar.wnd:OCLTimerStaticText\"")?, labels)?;
    let bar = window("\"ControlBar.wnd:OCLTimerProgressBar\"")?;
    let colors = results::property(bar, "ENABLEDDRAWDATA")?;
    let fill_colors = colors
        .match_indices("IMAGE:")
        .nth(4)
        .map(|(offset, _)| &colors[offset..])
        .ok_or("missing original OCL timer fill colors")?;
    let buttons = content::scan(assets.read("data/ini/commandbutton.ini")?)?;
    let upgrades = content::scan(assets.read("data/ini/upgrade.ini")?)?;
    // Context buttons are attached by ControlBar rather than the object CommandSet.
    // The fixed War Factory catalog also resolves the rally button on maps
    // without the neutral Pad.
    let command = |button, name| -> Result<Command, Box<dyn Error>> {
        let unit = catalog
            .iter()
            .find(|unit| unit.name == name)
            .ok_or("OCL timer command's original object template is absent")?;
        read_command(
            0,
            find(&buttons, "CommandButton", button)?,
            unit,
            catalog,
            labels,
            &upgrades,
        )?
        .ok_or_else(|| "original OCL timer command is unsupported".into())
    };
    let rally_provider = if catalog
        .iter()
        .any(|unit| unit.name == "TechReinforcementPad")
    {
        "TechReinforcementPad"
    } else {
        "AmericaWarFactory"
    };
    let padded_description = translate(labels, "CONTROLBAR:OCLTimerDescWithPadding")?;
    if description.text.matches("%d").count() != 2 || padded_description.matches("%d").count() != 2
    {
        return Err("invalid original OCL countdown label format".into());
    }
    Ok(Data {
        sell: command("Command_Sell", "AmericaSupplyDropZone")?,
        rally: command("Command_SetRallyPoint", rally_provider)?,
        bar: window_area(bar)?,
        bar_background: results::color(colors, "COLOR:")?,
        bar_border: results::color(colors, "BORDERCOLOR:")?,
        fill: results::color(fill_colors, "COLOR:")?,
        fill_border: results::color(fill_colors, "BORDERCOLOR:")?,
        description,
        padded_description,
    })
}
