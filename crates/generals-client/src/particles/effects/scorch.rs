//! `TerrainScorchFXNugget` defaults and its original five texture indices.
use super::Nugget;
use crate::skirmish_data::scalar;
use generals_assets::content::ContentLine;
use std::error::Error;

pub(super) fn parse(fields: &[ContentLine]) -> Result<Nugget, Box<dyn Error>> {
    let mut kind = -1;
    let mut radius = 0.0;
    for line in &fields[1..] {
        let value = scalar(line)?;
        match line.tokens[0].to_ascii_lowercase().as_str() {
            "type" => {
                kind = match value.to_ascii_uppercase().as_str() {
                    "RANDOM" => -1,
                    "SCORCH_1" => 0,
                    "SCORCH_2" => 1,
                    "SCORCH_3" => 2,
                    "SCORCH_4" => 3,
                    "SHADOW_SCORCH" => 4,
                    _ => return Err(format!("unknown TerrainScorch Type {value}").into()),
                };
            }
            "radius" => radius = super::super::data::real(value)?,
            _ => return Err(format!("untranslated TerrainScorch field {}", line.tokens[0]).into()),
        }
    }
    Ok(Nugget::Scorch { kind, radius })
}

#[cfg(test)]
mod tests;
