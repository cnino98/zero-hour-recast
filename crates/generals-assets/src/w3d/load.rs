//! Native on-demand prototype lookup for HLOD bounds, with verified absence.
use super::{Chunks, Error, Model, read_name};
use crate::{
    AssetError,
    virtual_assets::{Asset, VirtualAssets},
};
use std::{
    collections::{BTreeMap, btree_map::Entry},
    error::Error as StdError,
};

/// Read a model, hierarchy or clip with the installed English profile's
/// `GameFileClass::Set_Name` search order.
///
/// # Errors
/// Returns invalid, missing or unreadable asset errors.
pub fn read_asset(assets: &VirtualAssets, name: &str) -> Result<Asset, AssetError> {
    match assets.read(&format!("data/english/art/w3d/{name}.w3d")) {
        Err(AssetError::Missing(_)) => assets.read(&format!("art/w3d/{name}.w3d")),
        result => result,
    }
}

impl Model {
    /// Load original geometry and its hierarchy through actual asset precedence.
    /// Missing HLOD children are omitted only after native on-demand lookup.
    ///
    /// # Errors
    /// Preserves malformed asset and I/O errors; a missing referenced prototype
    /// is distinct from an unresolved external prototype that actually exists.
    pub fn load(assets: &VirtualAssets, name: &str) -> Result<Self, Box<dyn StdError>> {
        if name.eq_ignore_ascii_case("NULL") {
            return Ok(Self::null());
        }
        let bytes = read_asset(assets, name)?.bytes;
        let hierarchy = Self::hierarchy_name(&bytes)?
            .map(|name| read_asset(assets, &name))
            .transpose()?;
        let parsed = Self::parse_with_hierarchy(
            &bytes,
            hierarchy.as_ref().map(|asset| asset.bytes.as_slice()),
        )?;
        if !parsed.hlod || parsed.scene_sphere.is_some() {
            return Ok(parsed);
        }
        let local = prototype_names(&bytes)?;
        let mut looked_up = BTreeMap::new();
        let mut absent = Vec::new();
        for chunk in Chunks::new(&bytes) {
            let chunk = chunk?;
            if chunk.id != 0x700 {
                continue;
            }
            for (prototype, _) in super::scene_bounds::all_bindings(chunk.body)? {
                if prototype.eq_ignore_ascii_case("NULL")
                    || local
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(&prototype))
                {
                    continue;
                }
                // Create_Render_Obj loads the prefix before the first dot.
                let filename = prototype
                    .split('.')
                    .next()
                    .unwrap_or(&prototype)
                    .to_ascii_lowercase();
                if let Entry::Vacant(entry) = looked_up.entry(filename.clone()) {
                    let definitions = if filename.eq_ignore_ascii_case(name) {
                        local.clone()
                    } else {
                        match on_demand_file(assets, &filename) {
                            Ok(Some(file)) => {
                                let hierarchy = Self::hierarchy_name(&file.bytes)?
                                    .map(|name| read_asset(assets, &name))
                                    .transpose()?;
                                Self::parse_with_hierarchy(
                                    &file.bytes,
                                    hierarchy.as_ref().map(|asset| asset.bytes.as_slice()),
                                )?;
                                prototype_names(&file.bytes)?
                            }
                            Ok(None) => Vec::new(),
                            Err(error) => return Err(error.into()),
                        }
                    };
                    entry.insert(definitions);
                }
                if !looked_up[&filename]
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&prototype))
                {
                    absent.push(prototype);
                }
            }
        }
        Self::parse_with_absent(
            &bytes,
            hierarchy.as_ref().map(|asset| asset.bytes.as_slice()),
            &absent,
        )
        .map_err(Into::into)
    }
}

fn on_demand_file(
    assets: &VirtualAssets,
    filename: &str,
) -> Result<Option<crate::virtual_assets::Asset>, AssetError> {
    match read_asset(assets, filename) {
        Ok(file) => Ok(Some(file)),
        Err(AssetError::Missing(_)) => match assets.read(&format!("art/{filename}.w3d")) {
            Ok(file) => Ok(Some(file)),
            Err(AssetError::Missing(_)) => Ok(None),
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    }
}

fn prototype_names(bytes: &[u8]) -> Result<Vec<String>, Error> {
    let mut result = Vec::new();
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        match chunk.id {
            0 => {
                chunk.require_container()?;
                for sub in Chunks::new(chunk.body) {
                    let sub = sub?;
                    if sub.id == 0x1f {
                        if sub.body.len() != 116 {
                            return Err(Error::InvalidChunk("prototype mesh header"));
                        }
                        let name = read_name(&sub.body[8..24])?;
                        let container = read_name(&sub.body[24..40])?;
                        result.push(if container.is_empty() {
                            name
                        } else {
                            format!("{container}.{name}")
                        });
                    }
                }
            }
            0x740 => {
                if chunk.body.len() != 68 {
                    return Err(Error::InvalidChunk("prototype box header"));
                }
                result.push(read_name(&chunk.body[8..40])?);
            }
            0x700 => {
                chunk.require_container()?;
                for sub in Chunks::new(chunk.body) {
                    let sub = sub?;
                    if sub.id == 0x701 {
                        if sub.body.len() != 40 {
                            return Err(Error::InvalidChunk("prototype HLOD header"));
                        }
                        result.push(read_name(&sub.body[8..24])?);
                    }
                }
            }
            0x500 => result.push(super::emitter::parse(chunk.body)?.name),
            0x750 => {
                if chunk.body.len() != 48 {
                    return Err(Error::InvalidChunk("prototype null header"));
                }
                result.push(read_name(&chunk.body[16..48])?);
            }
            0x300 | 0x400 | 0x420 | 0x600 | 0x800 | 0x900 => {
                return Err(Error::Unsupported("on-demand prototype class"));
            }
            _ => {}
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    #[ignore = "requires original game installation"]
    fn original_missing_children_are_omitted_after_actual_on_demand_filename_lookup() {
        let root = crate::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        for name in ["CBTHOUSE12_S", "ABWARFACT_A8", "UVCOMBIKEJACKER_D"] {
            let model = Model::load(&assets, name).unwrap();
            assert!(model.hlod);
            assert!(!model.meshes.is_empty());
            assert!(
                model
                    .scene_sphere
                    .as_ref()
                    .is_some_and(|sphere| sphere.radius > 0.0),
                "{name}"
            );
        }
    }

    #[test]
    fn on_demand_lookup_prefers_localized_then_w3d_then_parent_art_and_reports_absence() {
        let fixture = std::env::temp_dir().join(format!("w3d-on-demand-{}", std::process::id()));
        fs::create_dir_all(fixture.join("art/w3d")).unwrap();
        fs::create_dir_all(fixture.join("data/english/art/w3d")).unwrap();
        let localized = fixture.join("data/english/art/w3d/child.w3d");
        let native = fixture.join("art/w3d/child.w3d");
        let fallback = fixture.join("art/child.w3d");
        fs::write(&localized, b"localized").unwrap();
        fs::write(&native, b"native").unwrap();
        fs::write(&fallback, b"parent").unwrap();
        let assets =
            VirtualAssets::from_layers(vec![crate::virtual_assets::Layer::Loose(fixture.clone())]);

        let found = on_demand_file(&assets, "child").unwrap().unwrap();
        assert_eq!(found.bytes, b"localized");
        fs::remove_file(localized).unwrap();

        let found = on_demand_file(&assets, "child").unwrap().unwrap();
        assert_eq!(found.bytes, b"native");
        fs::remove_file(native).unwrap();

        let found = on_demand_file(&assets, "child").unwrap().unwrap();
        assert_eq!(found.bytes, b"parent");
        fs::remove_file(fallback).unwrap();

        assert!(on_demand_file(&assets, "child").unwrap().is_none());
        fs::remove_dir_all(fixture).unwrap();
    }
}
