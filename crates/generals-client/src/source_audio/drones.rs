use crate::skirmish_data::{definition, field, scalar};
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::error::Error;

pub fn repair_sound(assets: &VirtualAssets) -> Result<String, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/miscaudio.ini")?)?];
    let lines = &definition(&files, "MiscAudio", "")?.lines;
    Ok(field(lines, 0, "RepairSparks")
        .map(scalar)
        .transpose()?
        .unwrap_or_default()
        .into())
}
