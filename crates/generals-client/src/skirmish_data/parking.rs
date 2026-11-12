//! Original `ParkingPlaceBehavior::buildInfo` logical bone layout.
use super::{blocks, field, scalar};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::parking::Rules;
use std::error::Error;

#[cfg(test)]
mod tests;

pub(super) fn load(object: &Declaration) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let matching: Vec<_> = modules
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "ParkingPlaceBehavior")
        })
        .collect();
    if matching.len() > 1 {
        return Err(
            "multiple ParkingPlace interfaces require per-instance interface selection".into(),
        );
    }
    let Some(module) = matching.first() else {
        return Ok(None);
    };
    let (rows, columns) = dimensions(module)?;
    let has_runways = boolean(module, "HasRunways", false)?;
    if rows != 0 && columns != 0 && !has_runways {
        return Err("Parking spaces require their authored runways".into());
    }
    let rules = Rules {
        rows,
        columns,
        has_runways,
        approach_height: real(module, "ApproachHeight")?,
        deck_height: real(module, "LandingDeckHeightOffset")?,
        park_in_hangars: boolean(module, "ParkInHangars", false)?,
        heal_per_second: real(module, "HealAmountPerSecond")?,
    };
    Ok(Some(rules))
}

fn dimensions(module: &[ContentLine]) -> Result<(u16, u16), Box<dyn Error>> {
    let parse = |key| {
        field(module, 1, key)
            .map(scalar)
            .transpose()?
            .unwrap_or("0")
            .parse::<u16>()
            .map_err(|error| Box::new(error) as Box<dyn Error>)
    };
    let rows = parse("NumRows")?;
    let columns = parse("NumCols")?;
    if rows.checked_mul(columns).is_none_or(|count| count > 4) {
        return Err("Parking layout exceeds native DOOR_COUNT_MAX".into());
    }
    Ok((rows, columns))
}

fn boolean(module: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    match field(module, 1, key).map(scalar).transpose()? {
        Some(value) if value.eq_ignore_ascii_case("Yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("No") => Ok(false),
        None => Ok(default),
        Some(_) => Err(format!("invalid parking boolean {key}").into()),
    }
}

fn real(module: &[ContentLine], key: &str) -> Result<f32, Box<dyn Error>> {
    let value = field(module, 1, key)
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse::<f32>()?;
    if !value.is_finite() {
        return Err(format!("nonfinite parking field {key}").into());
    }
    Ok(value)
}
