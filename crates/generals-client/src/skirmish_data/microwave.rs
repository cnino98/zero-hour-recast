//! Generic `ActiveBody` subdual fields; `FireWeaponUpdate` is parsed by its module owner.
use super::{duration_frames, field, scalar};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::microwave::SubdualRules;
use std::error::Error;

pub(super) fn body(body: Option<&[ContentLine]>) -> Result<Option<SubdualRules>, Box<dyn Error>> {
    let Some(body) = body else {
        return Ok(None);
    };
    let Some(cap) = field(body, 1, "SubdualDamageCap") else {
        return Ok(None);
    };
    let cap = scalar(cap)?.parse::<f32>()?;
    if !cap.is_finite() || cap < 0.0 {
        return Err("invalid source subdual cap".into());
    }
    if cap == 0.0 {
        return Ok(None);
    }
    Ok(Some(SubdualRules {
        cap,
        heal_rate: field(body, 1, "SubdualDamageHealRate").map_or(Ok(0), duration_frames)?,
        heal_amount: match field(body, 1, "SubdualDamageHealAmount") {
            Some(line) => scalar(line)?.parse::<f32>()?,
            None => 0.0,
        },
    }))
}

#[cfg(test)]
mod tests;
