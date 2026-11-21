//! SpyVisionSpecialPower.cpp authored activation durations.
use super::{blocks, field, scalar, source_numbers};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::spy_vision_power::Rules;
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|block| {
            block[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "SpyVisionSpecialPower")
        })
        .map(parse)
        .collect()
}

fn parse(module: &[ContentLine]) -> Result<Rules, Box<dyn Error>> {
    let time = |key| {
        field(module, 1, key).map_or(Ok(0), |line| source_numbers::duration_frames(scalar(line)?))
    };
    Ok(Rules {
        base_duration: time("BaseDuration")?,
        captured_bonus: time("BonusDurationPerCaptured")?,
        maximum: time("MaxDuration")?,
    })
}
