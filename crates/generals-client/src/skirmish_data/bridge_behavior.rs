//! Bounded original `BridgeBehavior` data; custom timed effects remain unadmitted.
use super::{blocks, definition, field, number};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::bridge_behavior::Rules;
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "BridgeBehavior")
        {
            continue;
        }
        if !matches!(
            object.name.as_str(),
            "GenericBridge" | "MonumentTrainBridge"
        ) {
            return Err(format!("{}: unadmitted BridgeBehavior template", object.name).into());
        }
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            if !matches!(
                line.tokens[0].as_str(),
                "LateralScaffoldSpeed" | "VerticalScaffoldSpeed"
            ) {
                return Err(format!(
                    "{}: unadmitted BridgeBehavior field {}",
                    object.name, line.tokens[0]
                )
                .into());
            }
        }
        if object.name == "MonumentTrainBridge" {
            let road = definition(files, "Bridge", &object.name)?;
            for line in &road.lines {
                if line.depth != Some(0) || line.tokens[0] == "End" {
                    continue;
                }
                if !matches!(
                    line.tokens[0].as_str(),
                    "RadarColor"
                        | "TowerObjectNameFromLeft"
                        | "TowerObjectNameFromRight"
                        | "TowerObjectNameToLeft"
                        | "TowerObjectNameToRight"
                        | "ScaffoldObjectName"
                        | "ScaffoldSupportObjectName"
                ) {
                    return Err(format!(
                        "{}: unadmitted bridge transition field {}",
                        object.name, line.tokens[0]
                    )
                    .into());
                }
            }
        }
        result.push(Rules {
            lateral_scaffold_speed: speed(module, "LateralScaffoldSpeed")?,
            vertical_scaffold_speed: speed(module, "VerticalScaffoldSpeed")?,
        });
    }
    Ok(result)
}

fn speed(
    lines: &[generals_assets::content::ContentLine],
    key: &str,
) -> Result<f32, Box<dyn Error>> {
    // The constructor default is already in internal units; authored values use
    // INI::parseVelocityReal's multiplication by LOGICFRAMES_PER_SECOND inverse.
    field(lines, 1, key).map_or(Ok(1.0), |_| Ok(number(lines, 1, key)? * (1.0 / 30.0)))
}

#[cfg(test)]
mod tests;
