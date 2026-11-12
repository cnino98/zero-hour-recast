//! Native `TerrainRoadCollection` defaults, shared road/bridge IDs and head insertion.
use super::Road;
use generals_assets::{
    AssetError,
    content::{self, Declaration},
    virtual_assets::VirtualAssets,
};
use std::{collections::BTreeSet, error::Error};

pub(super) struct Limits {
    pub segments: usize,
    pub vertices: usize,
    pub indices: usize,
    pub types: usize,
}

pub(super) fn load(assets: &VirtualAssets) -> Result<(Vec<Road>, Limits), Box<dyn Error>> {
    let mut declarations = Vec::new();
    for prefix in ["data/ini/default/roads", "data/ini/roads"] {
        match assets.read(&format!("{prefix}.ini")) {
            Ok(asset) => declarations.extend(content::scan(asset)?.declarations),
            Err(AssetError::Missing(_)) => {}
            Err(error) => return Err(error.into()),
        }
        for path in assets.list(prefix)?.into_iter().filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
        }) {
            declarations.extend(content::scan(assets.read(&path)?)?.declarations);
        }
    }
    let roads = definitions(declarations)?;
    let file = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let global = file
        .declarations
        .iter()
        .find(|entry| entry.kind == "GameData")
        .ok_or("Missing GameData road limits")?;
    let count = |key: &str| -> Result<usize, Box<dyn Error>> {
        Ok(value(global, key).unwrap_or("0").parse()?)
    };
    Ok((
        roads,
        Limits {
            segments: count("MaxRoadSegments")?,
            vertices: count("MaxRoadVertex")?,
            indices: count("MaxRoadIndex")?,
            types: count("MaxRoadTypes")?,
        },
    ))
}

fn definitions(declarations: Vec<Declaration>) -> Result<Vec<Road>, Box<dyn Error>> {
    let mut roads: Vec<Road> = Vec::new();
    let mut bridges = BTreeSet::new();
    let mut id = 1;
    for entry in declarations {
        if entry.kind == "Bridge" {
            if !bridges.insert(entry.name) {
                return Err("Repeated native Bridge definition".into());
            }
            id += 1;
            continue;
        }
        if entry.kind != "Road" {
            continue;
        }
        if roads.iter().any(|road| road.name == entry.name) {
            return Err("Repeated native Road definition".into());
        }
        let default = roads.iter().find(|road| road.name == "DefaultRoad");
        let texture = value(&entry, "Texture").map_or_else(
            || default.map_or_else(String::new, |road| road.texture.clone()),
            str::to_owned,
        );
        let number = |key, default| -> Result<f32, Box<dyn Error>> {
            value(&entry, key).map_or(Ok(default), |value| Ok(value.parse()?))
        };
        let scale = number("RoadWidth", default.map_or(0.0, |road| road.scale))?;
        let width = number("RoadWidthInTexture", default.map_or(0.0, |road| road.width))?;
        if !scale.is_finite() || !width.is_finite() {
            return Err("Nonfinite native road width".into());
        }
        roads.insert(
            0,
            Road {
                name: entry.name,
                texture,
                scale,
                width,
                id,
                stacking: 0,
            },
        );
        id += 1;
    }
    Ok(roads)
}

fn value<'a>(entry: &'a Declaration, name: &str) -> Option<&'a str> {
    entry
        .lines
        .iter()
        .rev()
        .find(|line| line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == name))
        .and_then(|line| line.tokens.get(1))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;

    fn read(text: &str) -> Vec<Declaration> {
        content::scan(Asset {
            path: "roads.ini".into(),
            origin: "source regression".into(),
            bytes: text.as_bytes().to_vec(),
        })
        .unwrap()
        .declarations
    }

    #[test]
    fn roads_inherit_native_default_and_share_bridge_id_sequence_in_head_order() {
        let roads = definitions(read("Road DefaultRoad\n Texture = RoadA.tga\n RoadWidth = 32\n RoadWidthInTexture = 0.7\nEnd\nBridge DefaultBridge\nEnd\nRoad Narrow\n RoadWidth = 16\nEnd\nRoad Wide\n Texture = RoadB.tga\n RoadWidthInTexture = 0.5\nEnd\n")).unwrap();
        assert_eq!(
            roads
                .iter()
                .map(|road| (road.name.as_str(), road.id))
                .collect::<Vec<_>>(),
            [("Wide", 4), ("Narrow", 3), ("DefaultRoad", 1)]
        );
        assert_eq!(roads[1].texture, "RoadA.tga");
        assert_eq!(roads[1].scale.to_bits(), 16.0_f32.to_bits());
        assert_eq!(roads[1].width.to_bits(), 0.7_f32.to_bits());
        assert_eq!(roads[0].scale.to_bits(), 32.0_f32.to_bits());
        assert_eq!(roads[0].width.to_bits(), 0.5_f32.to_bits());
        assert!(definitions(read("Road Duplicate\nEnd\nRoad Duplicate\nEnd\n")).is_err());
    }
}
