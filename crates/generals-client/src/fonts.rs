//! Resolve the original UI typefaces through the local system font inventory.
use bevy::prelude::Font;
use std::{io, sync::OnceLock};

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
