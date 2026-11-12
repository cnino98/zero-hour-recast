//! Native `PilotFindVehicleUpdateModuleData` defaults and ordered instances.
use super::{blocks, duration_frames, field, finite_number, scalar};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::pilot_find_vehicle::Rules;
use std::error::Error;

pub(super) fn load(lines: &[ContentLine]) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "PilotFindVehicleUpdate")
        {
            continue;
        }
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            if !matches!(
                line.tokens.first().map(String::as_str),
                Some("ScanRate" | "ScanRange" | "MinHealth" | "End")
            ) {
                return Err("untranslated PilotFindVehicleUpdate field".into());
            }
        }
        let real = |key, default| {
            field(module, 1, key).map_or(Ok(default), |line| finite_number(scalar(line)?))
        };
        result.push(Rules {
            scan_frames: field(module, 1, "ScanRate").map_or(Ok(0), duration_frames)?,
            scan_range: real("ScanRange", 0.0)?,
            min_health: real("MinHealth", 0.5)?,
        });
    }
    Ok(result)
}
