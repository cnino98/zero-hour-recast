//! Resolve the original UI typefaces through the local system font inventory.
use bevy::prelude::Font;
use generals_assets::content::ContentLine;
use std::{error::Error, io, sync::OnceLock};

pub fn resolution_adjustment(language: &[ContentLine]) -> Result<f32, Box<dyn Error>> {
    use crate::skirmish_data::{field, text};
    if field(language, 0, "ResolutionFontSizeMethod").is_some_and(|line| {
        line.tokens
            .get(1)
            .is_none_or(|value| value != "CLASSIC_NO_CEILING")
    }) {
        return Err("unrepresented localized font scaling method".into());
    }
    Ok(text(language, 0, "ResolutionFontAdjustment")?.parse()?)
}

pub fn display_pixels(point_size: f32, width: f32, resolution_adjustment: f32) -> f32 {
    // GlobalLanguage::adjustFontSize with the pinned default CLASSIC_NO_CEILING,
    // followed by FontCharsClass::Create_GDI_Font's MulDiv(points, 96, 72).
    let increase = (width / 800.0 - 1.0) * resolution_adjustment;
    let adjustment = (1.0 + increase).max(1.0);
    let points = (point_size * adjustment).floor().min(512.0);
    (points * 96.0 / 72.0).round()
}

pub fn load(family: &str, bold: bool) -> io::Result<Font> {
    static DATABASE: OnceLock<fontdb::Database> = OnceLock::new();
    let database = DATABASE.get_or_init(|| {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        database
    });
    let weight = if bold {
        fontdb::Weight::BOLD
    } else {
        fontdb::Weight::NORMAL
    };
    let id = database.query(&fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        weight,
        ..fontdb::Query::default()
    }).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound,
        format!("Original UI font {family} (bold={bold}) is not installed; install that font locally")))?;
    let face = database
        .face(id)
        .ok_or_else(|| io::Error::other("system font disappeared"))?;
    if face.weight != weight || face.style != fontdb::Style::Normal {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Original UI font {family} (bold={bold}) has no matching installed face"),
        ));
    }
    if face.index != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "Original UI font {family} is in a font collection; a standalone TTF or OTF is required"
            ),
        ));
    }
    database
        .with_face_data(id, |bytes, _| Font::from_bytes(bytes.to_vec()))
        .ok_or_else(|| io::Error::other(format!("Unable to read original UI font {family}")))
}

#[cfg(test)]
mod tests {
    use super::display_pixels;

    #[test]
    fn stock_arial_countdown_adjusts_by_width_then_rounds_native_points() {
        let widths = [640.0, 800.0, 1024.0, 1280.0, 1920.0];
        assert_eq!(
            widths.map(|width| display_pixels(10.0, width, 0.7).to_bits()),
            [13.0_f32, 13.0, 15.0, 19.0, 25.0].map(f32::to_bits)
        );
    }
}
