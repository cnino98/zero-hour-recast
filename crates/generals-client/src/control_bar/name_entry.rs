//! Original skirmish name gadget properties.
use super::results;
use bevy::prelude::*;
use generals_assets::virtual_assets::VirtualAssets;
use std::{collections::BTreeMap, error::Error};

pub struct Entry {
    pub window: results::Window,
    pub max_length: usize,
    pub hilite: Color,
    pub hilite_border: Color,
}

pub(super) fn load(
    assets: &VirtualAssets,
    labels: &BTreeMap<String, String>,
) -> Result<Entry, Box<dyn Error>> {
    let bytes = assets.read("window/menus/skirmishgameoptionsmenu.wnd")?;
    let source = std::str::from_utf8(&bytes.bytes)?;
    let block = source
        .split("WINDOWTYPE = ")
        .find(|block| block.contains("NAME = \"SkirmishGameOptionsMenu.wnd:TextEntryPlayerName\";"))
        .ok_or("missing native name entry")?;
    let max_length = results::property(block, "TEXTENTRYDATA")?
        .strip_prefix("MAXLEN: ")
        .and_then(|value| value.split_once(','))
        .ok_or("missing name entry maximum")?
        .0
        .trim()
        .parse()?;
    if max_length == 0 {
        return Err("empty name entry capacity".into());
    }
    Ok(Entry {
        window: results::window(block, labels)?,
        max_length,
        hilite: results::color(results::property(block, "TEXTCOLOR")?, "HILITE:")?,
        hilite_border: results::color(results::property(block, "TEXTCOLOR")?, "HILITEBORDER:")?,
    })
}
