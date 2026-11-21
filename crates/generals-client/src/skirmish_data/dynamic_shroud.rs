//! Native shrink/grow frames and optional radius-grid decal.
use super::{blocks, field, finite_number, scalar, source_numbers};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::dynamic_shroud::Rules;
use std::error::Error;

pub(super) fn load(
    _files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0].tokens[1] != "DynamicShroudClearingRangeUpdate" {
            continue;
        }
        let frames = |key| {
            field(module, 1, key)
                .map_or(Ok(0), |line| source_numbers::duration_frames(scalar(line)?))
        };
        let parsed = super::spectre::decal(module, "GridDecalTemplate")?;
        let grid_decal = (!parsed.texture.is_empty()).then_some(parsed);
        result.push(Rules {
            final_vision: field(module, 1, "FinalVision")
                .map_or(Ok(0.0), |line| finite_number(scalar(line)?))?,
            change_interval: frames("ChangeInterval")?,
            grow_interval: frames("GrowInterval")?,
            shrink_delay: frames("ShrinkDelay")?,
            shrink_time: frames("ShrinkTime")?,
            grow_delay: frames("GrowDelay")?,
            grow_time: frames("GrowTime")?,
            grid_decal,
        });
    }
    Ok(result)
}
