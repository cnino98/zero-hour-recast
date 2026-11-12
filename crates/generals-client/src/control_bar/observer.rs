//! Original observer windows and base-player artwork from installed data.
use super::{find, results, value};
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{collections::BTreeMap, error::Error};

pub struct Data {
    pub windows: BTreeMap<String, results::Window>,
    pub buttons: [String; 3],
    pub flags: [String; 3],
}

impl Data {
    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.windows
            .values()
            .filter_map(|window| window.image.as_deref())
            .chain(self.buttons.iter().map(String::as_str))
            .chain(self.flags.iter().map(String::as_str))
    }
}

pub fn load(
    assets: &VirtualAssets,
    labels: &BTreeMap<String, String>,
) -> Result<Data, Box<dyn Error>> {
    let asset = assets.read("window/controlbar.wnd")?;
    let source = std::str::from_utf8(&asset.bytes)?;
    let mut windows = BTreeMap::new();
    let names = [
        "WinGeneralPortrait",
        "WinFlag",
        "ButtonCancel",
        "StaticTextPlayerName",
        "StaticTextObsBuildings",
        "StaticTextObsUnits",
        "StaticTextObsUnitsLost",
        "StaticTextObsUnitsKilled",
        "StaticTextNumberOfUnitsLost",
        "StaticTextNumberOfUnitsKilled",
        "StaticTextNumberOfBuildings",
        "StaticTextNumberOfUnits",
    ];
    let mut wanted: Vec<_> = names.into_iter().map(str::to_owned).collect();
    for slot in 0..8 {
        wanted.push(format!("ButtonPlayer{slot}"));
        wanted.push(format!("StaticTextPlayer{slot}"));
    }
    for block in source.split("WINDOWTYPE = ").skip(1) {
        let name = results::property(block, "NAME")?.trim_matches('"');
        let Some(name) = name.strip_prefix("ControlBar.wnd:") else {
            continue;
        };
        if let Some(index) = wanted.iter().position(|wanted| wanted == name) {
            wanted.remove(index);
            windows.insert(name.into(), results::window(block, labels)?);
        }
    }
    if !wanted.is_empty() {
        return Err(format!("missing original observer windows: {wanted:?}").into());
    }
    let file = content::scan(assets.read("data/ini/playertemplate.ini")?)?;
    let mut buttons = [String::new(), String::new(), String::new()];
    let mut flags = [String::new(), String::new(), String::new()];
    for (index, name) in ["FactionAmerica", "FactionChina", "FactionGLA"]
        .into_iter()
        .enumerate()
    {
        let template = find(&file, "PlayerTemplate", name)?;
        buttons[index] = value(&template.lines, 0, "EnabledImage")?.into();
        flags[index] = value(&template.lines, 0, "FlagWaterMark")?.into();
    }
    Ok(Data {
        windows,
        buttons,
        flags,
    })
}
