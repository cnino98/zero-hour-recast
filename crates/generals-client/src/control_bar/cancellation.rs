use super::{Command, CommandAction, find, translate, value};
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{collections::BTreeMap, error::Error};

pub(super) fn load(
    assets: &VirtualAssets,
    labels: &BTreeMap<String, String>,
) -> Result<Command, Box<dyn Error>> {
    let buttons = content::scan(assets.read("data/ini/commandbutton.ini")?)?;
    let queue = find(&buttons, "CommandButton", "Command_CancelUnitCreate")?;
    if value(&queue.lines, 0, "Command")? != "CANCEL_UNIT_BUILD" {
        return Err("unsupported unit queue cancellation command".into());
    }
    let button = find(&buttons, "CommandButton", "Command_CancelConstruction")?;
    if value(&button.lines, 0, "Command")? != "DOZER_CONSTRUCT_CANCEL" {
        return Err("unsupported construction cancellation command".into());
    }
    let label_key = value(&button.lines, 0, "TextLabel")?;
    let label = labels
        .get(&label_key.to_ascii_lowercase())
        .ok_or("missing construction cancellation label")?;
    Ok(Command {
        name: button.name.clone(),
        slot: 0, // Original standalone ButtonCancelConstruction, outside the 14 command slots.
        action: CommandAction::CancelConstruction,
        accelerator: label
            .split_once('&')
            .and_then(|(_, suffix)| suffix.chars().next()),
        multi_select: false,
        image: value(&button.lines, 0, "ButtonImage")?.into(),
        label: translate(labels, label_key)?,
        description: translate(labels, value(&button.lines, 0, "DescriptLabel")?)?,
        sciences: Vec::new(),
        required_upgrade: None,
    })
}
