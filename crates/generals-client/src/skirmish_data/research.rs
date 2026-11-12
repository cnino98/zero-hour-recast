use super::{blocks, definition, field, finite_number, scalar, text};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::Research;
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Research>, Box<dyn Error>> {
    let Some(set) = field(&object.lines, 0, "CommandSet") else {
        return Ok(Vec::new());
    };
    let mut sets = vec![definition(files, "CommandSet", scalar(set)?)?];
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "CommandSetUpgrade")
            || !field(module, 1, "TriggeredBy").is_some_and(|line| {
                line.tokens
                    .iter()
                    .skip(1)
                    .any(|name| super::upgrades::supported(name))
            })
        {
            continue;
        }
        for key in ["CommandSet", "CommandSetAlt"] {
            if let Some(set) = field(module, 1, key) {
                sets.push(definition(files, "CommandSet", scalar(set)?)?);
            }
        }
    }
    let mut result = Vec::new();
    for line in sets.iter().flat_map(|set| &set.lines) {
        if line.depth != Some(0) || line.tokens.len() != 2 || line.tokens[0].parse::<u32>().is_err()
        {
            continue;
        }
        let button = definition(files, "CommandButton", &line.tokens[1])?;
        if !matches!(
            text(&button.lines, 0, "Command")?,
            "PLAYER_UPGRADE" | "OBJECT_UPGRADE"
        ) {
            continue;
        }
        let name = text(&button.lines, 0, "Upgrade")?;
        if result
            .iter()
            .any(|research: &Research| research.name == name)
        {
            continue;
        }
        if ![
            "Upgrade_InfantryCaptureBuilding",
            "Upgrade_ChinaRadar",
            "Upgrade_Nationalism",
        ]
        .contains(&name)
            && !super::upgrades::supported(name)
            && !super::overlord::supported(name)
            && !super::drones::supported(name)
            && !super::helix::supported(name)
        {
            continue;
        }
        let upgrade = &definition(files, "Upgrade", name)?.lines;
        let seconds = finite_number(text(upgrade, 0, "BuildTime")?)?;
        if seconds <= 0.0 || seconds * 30.0 > f32::from(u16::MAX) {
            return Err("unsupported upgrade duration".into());
        }
        // UpgradeTemplate::calcTimeToBuild truncates seconds*30 and ignores power.
        let frames = (seconds * 30.0).trunc().to_string().parse()?;
        let player = match field(upgrade, 0, "Type")
            .map(scalar)
            .transpose()?
            .unwrap_or("PLAYER")
        {
            "PLAYER" => true,
            "OBJECT" => false,
            _ => return Err("invalid upgrade type".into()),
        };
        if text(upgrade, 0, "ButtonImage")? != text(&button.lines, 0, "ButtonImage")? {
            return Err("upgrade queue image differs from its command image".into());
        }
        result.push(Research {
            name: name.into(),
            cost: text(upgrade, 0, "BuildCost")?.parse()?,
            frames,
            player,
        });
    }
    Ok(result)
}
