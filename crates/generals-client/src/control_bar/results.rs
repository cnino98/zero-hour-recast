//! Installed skirmish score windows and `PlayerTemplate` side artwork.
use super::{find, translate, value, window_area};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

pub const COLUMNS: [(&str, &str, &str); 7] = [
    ("UnitsBuilt", "UnitsBuilt", "UnitsCreated"),
    ("UnitsLost", "UnitsLost", "UnitsLost"),
    ("UnitsDestroyed", "UnitsDestroyed", "UnitsDestroyed"),
    ("BuildingsConstructed", "BuildingsBuilt", "BuildingCreated"),
    ("BuildingsLost", "BuildingsLost", "BuildingLost"),
    (
        "BuildingsDestroyed",
        "BuildingsDestroyed",
        "BuildingDestroyed",
    ),
    ("SuppliesCollected", "Resources", "SuppliesCollected"),
];

pub struct Results {
    pub windows: BTreeMap<String, Window>,
    pub side_icons: [String; 3],
    pub colors: Vec<[f32; 3]>,
}

pub struct Window {
    pub area: Rect,
    pub text: String,
    pub font_size: f32,
    pub text_color: Color,
    pub text_border: Color,
    pub wrap_centered: bool,
    pub centered: bool,
    pub background: Color,
    pub border: Color,
    pub image: Option<String>,
}

impl Results {
    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.windows
            .values()
            .filter_map(|window| window.image.as_deref())
            .chain(self.side_icons.iter().map(String::as_str))
    }
}

pub fn load(
    assets: &VirtualAssets,
    labels: &BTreeMap<String, String>,
) -> Result<Results, Box<dyn Error>> {
    let mut names: BTreeSet<String> = [
        "Victorious.wnd:",
        "Victorious.wnd:Victorious",
        "Defeat.wnd:Defeat",
        "Defeat.wnd:DefeatImage",
        "LocalDefeat.wnd:",
        "LocalDefeat.wnd:Defeat",
        "ObserverQuit.wnd:Defeat",
        "ObserverQuit.wnd:DefeatImage",
        "ScoreScreen.wnd:ParentScoreScreen",
        "ScoreScreen.wnd:MainBackdrop",
        "ScoreScreen.wnd:GadgetParent",
        "ScoreScreen.wnd:ButtonOk",
        "ScoreScreen.wnd:StaticTextPlayerName",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for (header, column, icon) in COLUMNS {
        names.insert(format!("ScoreScreen.wnd:StaticText{header}"));
        names.insert(format!("ScoreScreen.wnd:Cameo{icon}"));
        for slot in 0..8 {
            names.insert(format!("ScoreScreen.wnd:StaticText{column}{slot}"));
        }
    }
    for slot in 0..8 {
        names.insert(format!("ScoreScreen.wnd:StaticTextPlayer{slot}"));
        names.insert(format!("ScoreScreen.wnd:GameWindowWinner{slot}"));
    }
    let mut windows = BTreeMap::new();
    for file in [
        "victorious",
        "defeat",
        "localdefeat",
        "observerquit",
        "scorescreen",
    ] {
        let asset = assets.read(&format!("window/menus/{file}.wnd"))?;
        for block in std::str::from_utf8(&asset.bytes)?
            .split("WINDOWTYPE = ")
            .skip(1)
        {
            let name = quoted(property(block, "NAME")?)?;
            if names.remove(name) {
                windows.insert(name.to_owned(), window(block, labels)?);
            }
        }
    }
    if !names.is_empty() {
        return Err(format!("missing result windows: {names:?}").into());
    }
    windows
        .get_mut("ScoreScreen.wnd:ParentScoreScreen")
        .ok_or("score parent missing")?
        .image = Some("MutiPlayer_ScoreScreen".into());
    let templates = content::scan(assets.read("data/ini/playertemplate.ini")?)?;
    let icons = ["FactionAmerica", "FactionChina", "FactionGLA"].map(|name| {
        let template = find(&templates, "PlayerTemplate", name)?;
        Ok::<_, Box<dyn Error>>(value(&template.lines, 0, "SideIconImage")?.to_owned())
    });
    let [usa, china, gla] = icons;
    Ok(Results {
        windows,
        side_icons: [usa?, china?, gla?],
        colors: crate::playtest_view::player_colors(assets)?,
    })
}

pub(super) fn window(
    block: &str,
    labels: &BTreeMap<String, String>,
) -> Result<Window, Box<dyn Error>> {
    let font = property(block, "FONT")?;
    let font_size = font
        .split_once("SIZE:")
        .and_then(|(_, value)| value.split_once(','))
        .ok_or("result font size missing")?
        .0
        .trim()
        .parse::<u16>()?;
    let image = property(block, "ENABLEDDRAWDATA")?
        .strip_prefix("IMAGE: ")
        .and_then(|value| value.split_once(','))
        .ok_or("result image missing")?
        .0;
    let text = block
        .lines()
        .find(|line| line.trim_start().starts_with("TEXT = "))
        .map(|line| property(line, "TEXT").and_then(quoted))
        .transpose()?;
    let text = if let Some(label) = text.filter(|label| label.contains(':')) {
        translate(labels, label)?
    } else {
        String::new()
    };
    Ok(Window {
        area: window_area(block)?,
        text,
        font_size: f32::from(font_size),
        text_color: color(property(block, "TEXTCOLOR")?, "ENABLED:")?,
        text_border: color(property(block, "TEXTCOLOR")?, "ENABLEDBORDER:")?,
        wrap_centered: property(block, "STATUS")?
            .split('+')
            .any(|flag| flag == "WRAP_CENTERED"),
        centered: block.contains("STATICTEXTDATA = CENTERED: 1;"),
        background: color(property(block, "ENABLEDDRAWDATA")?, "COLOR:")?,
        border: color(property(block, "ENABLEDDRAWDATA")?, "BORDERCOLOR:")?,
        image: (image != "NoImage").then(|| image.to_owned()),
    })
}

pub(super) fn color(value: &str, key: &str) -> Result<Color, Box<dyn Error>> {
    let value = value
        .split_once(key)
        .ok_or("result window color missing")?
        .1;
    let channels = value
        .split(',')
        .next()
        .ok_or("result window color missing")?
        .split_whitespace()
        .map(str::parse::<u8>)
        .collect::<Result<Vec<_>, _>>()?;
    let [red, green, blue, alpha] = channels.as_slice() else {
        return Err("invalid result window color".into());
    };
    Ok(Color::srgba_u8(*red, *green, *blue, *alpha))
}

pub(super) fn property<'a>(block: &'a str, key: &str) -> Result<&'a str, Box<dyn Error>> {
    block
        .split_once(&format!("{key} = "))
        .and_then(|(_, value)| value.split_once(';'))
        .map(|(value, _)| value.trim())
        .ok_or_else(|| format!("missing result WND {key}").into())
}

fn quoted(value: &str) -> Result<&str, Box<dyn Error>> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| "invalid WND string".into())
}
