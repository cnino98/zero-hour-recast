//! `MapUtil::loadMapsFromDisk/addMap`: refresh user metadata from actual map bytes.
use super::{MapEntry, start_positions};
use crate::{control_bar::UiData, skirmish_data, world_data};
use bevy::prelude::Vec2;
use generals_assets::{
    content::Declaration,
    map::{MapData, MapValue},
    virtual_assets::VirtualAssets,
};
use std::{collections::BTreeMap, error::Error, fmt::Write, path::Path};

pub(super) fn load(
    assets: &VirtualAssets,
    data: &mut UiData,
    rejected: &mut Vec<(String, String)>,
) -> Result<Vec<MapEntry>, Box<dyn Error>> {
    let paths = match assets.list("user-maps") {
        Ok(paths) => paths,
        Err(error) => {
            rejected.push(("user-maps".to_owned(), error.to_string()));
            return Ok(Vec::new());
        }
    };
    if !paths.iter().any(|path| {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("map"))
    }) {
        return Ok(Vec::new());
    }
    let definitions = world_data::object_definitions(assets)?;
    let cache = match assets.read("user-maps/mapcache.ini") {
        Ok(asset) => match generals_assets::content::scan(asset) {
            Ok(cache) if cache.diagnostics.is_empty() && cache.unclassified.is_empty() => {
                Some(cache)
            }
            Ok(_) => {
                rejected.push((
                    "user-maps/mapcache.ini".to_owned(),
                    "Malformed user map cache".to_owned(),
                ));
                None
            }
            Err(error) => {
                rejected.push(("user-maps/mapcache.ini".to_owned(), error.to_string()));
                None
            }
        },
        Err(generals_assets::AssetError::Missing(_)) => None,
        Err(error) => {
            rejected.push(("user-maps/mapcache.ini".to_owned(), error.to_string()));
            None
        }
    };
    let mut maps = Vec::new();
    for path in paths.into_iter().filter(|path| {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("map"))
    }) {
        // Native recursively lists *.map, but requires <name>/<name>.map.
        let Some((directory, filename)) = path.rsplit_once('/') else {
            continue;
        };
        if directory.rsplit('/').next() != filename.strip_suffix(".map") {
            continue;
        }
        match metadata(assets, &path, data, &definitions, cache.as_ref()) {
            Ok(Some(map)) => maps.push(map),
            Ok(None) => {}
            Err(error) => rejected.push((path, error.to_string())),
        }
    }
    Ok(maps)
}

fn metadata(
    assets: &VirtualAssets,
    path: &str,
    data: &mut UiData,
    definitions: &BTreeMap<String, Declaration>,
    cache: Option<&generals_assets::content::ContentFile>,
) -> Result<Option<MapEntry>, Box<dyn Error>> {
    let asset = assets.read(path)?;
    let map = MapData::parse(&asset.bytes)?;
    let starts = start_positions(&map);
    if starts.is_empty() {
        return Ok(None);
    }
    let filename = std::path::Path::new(&asset.origin)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("user map has no UTF-8 filename")?;
    if let Some(entry) = cached(cache, &asset, starts.len())? {
        let mut result = super::metadata(assets, path.to_owned(), entry, &map, starts, data)?;
        if super::cache_value(entry, "nameLookupTag").is_empty()
            && let Some(map) = &mut result
        {
            map.title = if map.capacity >= 2 {
                format!("{filename} ({})", map.capacity)
            } else {
                filename.to_owned()
            };
        }
        return Ok(result);
    }
    let tag = match map.world_properties.get("mapName") {
        Some(MapValue::Ascii(tag)) => tag.as_str(),
        None => "",
        _ => return Err("mapName must be an original ASCII dictionary value".into()),
    };
    // MapUtil669–672 duplicates mapDir in its map.str probe and appends
    // map.str under the map stem. Do not substitute the adjacent runtime file.
    let mut title = if tag.is_empty() {
        filename.to_owned()
    } else {
        super::lookup_title(data, tag)
    };
    if title.is_empty() {
        return Ok(None);
    }
    if starts.len() >= 2 {
        write!(title, " ({})", starts.len())?;
    }
    let boundary = map
        .heightmap
        .boundaries
        .first()
        .ok_or("map has no active boundary")?;
    let extent = Vec2::new(
        f32::from(u16::try_from(boundary[0])?),
        f32::from(u16::try_from(boundary[1])?),
    ) * 10.0;
    if !extent.cmpgt(Vec2::ZERO).all() || !starts.iter().all(|point| point.is_finite()) {
        return Err("map preview coordinates must be finite with positive extent".into());
    }
    let mut supplies = Vec::new();
    let mut tech = Vec::new();
    for object in &map.objects {
        if object.waypoint_id().is_some() {
            continue;
        }
        let Some(definition) = definitions.get(&object.template) else {
            continue;
        };
        let kinds = skirmish_data::field(&definition.lines, 0, "KindOf");
        let has = |kind: &str| {
            kinds.is_some_and(|line| line.tokens.iter().skip(1).any(|token| token == kind))
        };
        let position = Vec2::new(object.position[0], object.position[1]);
        if !position.is_finite() {
            return Err("map object preview coordinates are nonfinite".into());
        }
        if has("TECH_BUILDING") {
            tech.push(position);
        } else if has("SUPPLY_SOURCE_ON_PREVIEW") {
            supplies.push(position);
        }
    }
    let preview = path.trim_end_matches(".map").to_owned() + ".tga";
    if !data.handles.contains_key(&preview)
        && !data.textures.contains_key(&preview)
        && let Ok(image) = crate::source_texture::load(assets, &preview)
    {
        data.textures.insert(preview.clone(), image);
    }
    Ok(Some(MapEntry {
        path: path.to_owned(),
        title,
        preview,
        capacity: starts.len(),
        extent,
        starts,
        supplies,
        tech,
        admission: super::Admission::Unknown,
    }))
}

fn cached<'a>(
    cache: Option<&'a generals_assets::content::ContentFile>,
    asset: &generals_assets::virtual_assets::Asset,
    players: usize,
) -> Result<Option<&'a Declaration>, Box<dyn Error>> {
    let Some(cache) = cache else {
        return Ok(None);
    };
    for entry in cache
        .declarations
        .iter()
        .filter(|entry| entry.kind == "MapCache")
    {
        let Ok(path) = super::decode(&entry.name) else {
            continue;
        };
        if !path
            .replace('\\', "/")
            .eq_ignore_ascii_case(&asset.origin.replace('\\', "/"))
        {
            continue;
        }
        let size = super::cache_value(entry, "fileSize").parse::<usize>()?;
        let crc = super::cache_value(entry, "fileCRC").parse::<u32>()?;
        if size != asset.bytes.len() || crc == 0 {
            continue;
        }
        if super::cache_value(entry, "numPlayers").parse::<usize>()? != players {
            return Err("cached player capacity differs from actual map starts".into());
        }
        return Ok(Some(entry));
    }
    Ok(None)
}
