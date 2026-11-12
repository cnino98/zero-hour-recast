//! SH 2bfaea60 `MapUtil::populateMapListbox` and `SkirmishMapSelectMenu`.
use crate::control_bar::UiData;
use bevy::prelude::*;
use generals_assets::{content, map::MapData, virtual_assets::VirtualAssets};
use std::error::Error;

mod custom;
mod pointer;
pub mod preview;
pub use pointer::{DoubleClickInterval, input as pointer_input};
mod ui;
pub use ui::{setup, update};
mod verification;
pub use verification::{enable_capacity as verify_capacity, enable_custom as verify_custom};

pub struct MapEntry {
    pub path: String,
    pub title: String,
    pub preview: String,
    pub capacity: usize,
    pub extent: Vec2,
    pub starts: Vec<Vec2>,
    pub supplies: Vec<Vec2>,
    pub tech: Vec<Vec2>,
    /// Metadata alone never authorizes a match.
    pub admission: Admission,
}

pub enum Admission {
    Unknown,
    Verified,
    Rejected(String),
}

impl Admission {
    pub const fn is_verified(&self) -> bool {
        matches!(self, Self::Verified)
    }

    pub fn rejection(&self) -> Option<&str> {
        match self {
            Self::Rejected(reason) => Some(reason),
            Self::Unknown | Self::Verified => None,
        }
    }
}

impl MapEntry {
    fn custom(&self) -> bool {
        self.path.starts_with("user-maps/")
    }
}

pub enum CatalogState {
    Ready,
    RefreshRequested,
}

#[derive(Resource)]
pub struct MapSelection {
    pub maps: Vec<MapEntry>,
    pub rejected: Vec<(String, String)>,
    pub current: usize,
    pub requested: Option<usize>,
    pub open: bool,
    pub user_maps: bool,
    pub catalog_state: CatalogState,
    unlisted: Option<usize>,
    highlighted: Option<usize>,
    top: usize,
    display_pos: f32,
    list_focused: bool,
}

impl MapSelection {
    fn count(&self) -> usize {
        self.maps
            .iter()
            .enumerate()
            .filter(|(index, map)| Some(*index) != self.unlisted && map.custom() == self.user_maps)
            .count()
    }

    fn index(&self, row: usize) -> Option<usize> {
        self.maps
            .iter()
            .enumerate()
            .filter(|(index, map)| Some(*index) != self.unlisted && map.custom() == self.user_maps)
            .nth(row)
            .map(|(index, _)| index)
    }

    fn row(&self, index: usize) -> Option<usize> {
        (0..self.count()).find(|&row| self.index(row) == Some(index))
    }

    fn select_category(&mut self, custom: bool) {
        self.user_maps = custom;
        self.highlighted = self
            .row(self.current)
            .and_then(|row| self.index(row))
            .or_else(|| self.index(0));
        let row = self
            .highlighted
            .and_then(|index| self.row(index))
            .unwrap_or(0);
        // MapUtil968–980 reveals an offscreen selection near the middle.
        self.top = if row >= ui::ROWS {
            row.saturating_sub((ui::ROWS / 2).max(1))
                .min(self.count().saturating_sub(ui::ROWS))
        } else {
            0
        };
        self.display_pos = pointer::row_position(self.top);
        self.list_focused = false;
    }

    pub(crate) fn refresh(
        &mut self,
        assets: &VirtualAssets,
        data: &mut UiData,
    ) -> Result<(), Box<dyn Error>> {
        let current = self.maps[self.current].path.clone();
        let requested = self.requested.map(|index| self.maps[index].path.clone());
        let Catalog { mut maps, rejected } = catalog(assets, data)?;
        // Installed content is fixed for this process. Custom map bytes and
        // map.ini may have changed even when their cache metadata matches.
        for map in maps.iter_mut().filter(|map| !map.custom()) {
            if let Some(previous) = self.maps.iter_mut().find(|old| old.path == map.path) {
                map.admission = std::mem::replace(&mut previous.admission, Admission::Unknown);
            }
        }
        self.current = if let Some(index) = maps.iter().position(|map| map.path == current) {
            self.unlisted = None;
            index
        } else {
            // Keep the loaded world identity while MapCache removes unseen rows.
            let mut loaded = self.maps.remove(self.current);
            loaded.admission = Admission::Rejected(
                rejected
                    .iter()
                    .find(|(path, _)| *path == current)
                    .map_or_else(
                        || "Loaded map is no longer in the map catalog".to_owned(),
                        |(_, reason)| reason.clone(),
                    ),
            );
            maps.push(loaded);
            let index = maps.len() - 1;
            self.unlisted = Some(index);
            index
        };
        self.requested = requested.and_then(|path| maps.iter().position(|map| map.path == path));
        self.maps = maps;
        self.rejected = rejected;
        self.select_category(self.maps[self.current].custom());
        self.catalog_state = CatalogState::Ready;
        Ok(())
    }

    pub fn reject(&mut self, index: usize, reason: String) {
        self.maps[index].admission = Admission::Rejected(reason);
        self.requested = None;
    }

    pub fn load_previews(&self, assets: &VirtualAssets, data: &mut UiData) {
        for map in &self.maps {
            if let Ok(image) = crate::source_texture::load(assets, &map.preview) {
                data.textures.insert(map.preview.clone(), image);
            }
        }
    }
    pub fn commit(&mut self, index: usize) {
        self.maps[index].admission = Admission::Verified;
        self.current = index;
        self.requested = None;
        self.open = false;
    }

    // GadgetListBoxInput: arrows include held-key repeats; acceptance and
    // parent Escape handling occur on key release, not key press.
    fn key(&mut self, key: bevy::prelude::KeyCode, state: bevy::input::ButtonState) {
        use bevy::{input::ButtonState, prelude::KeyCode};
        if !self.open {
            return;
        }
        if key == KeyCode::Escape && state == ButtonState::Released {
            self.open = false;
        } else if self.list_focused {
            match (key, state) {
                (KeyCode::Enter | KeyCode::Space, ButtonState::Released) => {
                    self.activate(&crate::playtest_ui::Action::MapAccept);
                }
                (KeyCode::ArrowUp | KeyCode::ArrowDown, ButtonState::Pressed) => {
                    let previous_top = self.top;
                    if let Some(index) = self.highlighted.and_then(|index| self.row(index)) {
                        let next = if key == KeyCode::ArrowUp {
                            index.saturating_sub(1)
                        } else {
                            (index + 1).min(self.count().saturating_sub(1))
                        };
                        self.highlighted = self.index(next);
                        if next < self.top {
                            self.top = next;
                        }
                        if next >= self.top + ui::ROWS {
                            self.top = next + 1 - ui::ROWS;
                        }
                    }
                    if self.top != previous_top {
                        self.display_pos = pointer::row_position(self.top);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn activate(&mut self, action: &crate::playtest_ui::Action) -> bool {
        use crate::playtest_ui::Action;
        match *action {
            Action::SelectMap => {
                self.open = true;
                self.catalog_state = CatalogState::RefreshRequested;
                self.select_category(self.maps[self.current].custom());
            }
            Action::MapBack => {
                self.open = false;
                self.catalog_state = CatalogState::Ready;
            }
            Action::MapCategory(user) => {
                self.select_category(user);
            }
            Action::MapRow(row) => {
                if let Some(index) = self.index(row) {
                    self.highlighted = Some(index);
                    self.list_focused = true;
                }
            }
            Action::MapAccept => {
                if let Some(index) = self.highlighted
                    && self.maps[index].admission.rejection().is_none()
                {
                    self.requested = Some(index);
                }
            }
            Action::MapScroll(delta) => {
                self.top = self
                    .top
                    .saturating_add_signed(delta)
                    .min(self.count().saturating_sub(ui::ROWS));
                self.display_pos = pointer::row_position(self.top);
            }
            _ => return false,
        }
        true
    }
}

pub fn load(
    assets: &VirtualAssets,
    data: &mut UiData,
    current_path: &str,
) -> Result<MapSelection, Box<dyn Error>> {
    let catalog = catalog(assets, data)?;
    let current_path = current_path.replace('\\', "/").to_lowercase();
    let current = catalog
        .maps
        .iter()
        .position(|map| map.path == current_path)
        .ok_or("current map is absent from original skirmish chooser")?;
    Ok(selection(catalog, current))
}

pub fn load_shell(
    assets: &VirtualAssets,
    data: &mut UiData,
    preferred_path: &str,
) -> Result<MapSelection, Box<dyn Error>> {
    let catalog = catalog(assets, data)?;
    let preferred = preferred_path.replace('\\', "/").to_lowercase();
    let current = if let Some(index) = catalog.maps.iter().position(|map| map.path == preferred) {
        index
    } else {
        // SkirmishPreferences::getPreferredMap / MapUtil::getDefaultMap(TRUE):
        // an absent remembered map selects the first normalized map-cache key.
        let (index, map) = catalog
            .maps
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| left.path.cmp(&right.path))
            .ok_or("original skirmish map catalog is empty")?;
        eprintln!(
            "Remembered skirmish map is unavailable; selected original default map: {}",
            map.path
        );
        index
    };
    Ok(selection(catalog, current))
}

fn selection(catalog: Catalog, current: usize) -> MapSelection {
    MapSelection {
        maps: catalog.maps,
        rejected: catalog.rejected,
        current,
        requested: None,
        open: false,
        user_maps: false,
        catalog_state: CatalogState::Ready,
        unlisted: None,
        highlighted: None,
        top: 0,
        display_pos: 0.0,
        list_focused: false,
    }
}

struct Catalog {
    maps: Vec<MapEntry>,
    rejected: Vec<(String, String)>,
}

fn catalog(assets: &VirtualAssets, data: &mut UiData) -> Result<Catalog, Box<dyn Error>> {
    let mut maps = Vec::new();
    let mut rejected = Vec::new();
    // Normal native startup reads the shipped standard cache; it does not
    // expose every campaign, editor test or stray map archive record.
    let cache = content::scan(assets.read("maps/mapcache.ini")?)?;
    for entry in cache
        .declarations
        .iter()
        .filter(|entry| entry.kind == "MapCache")
    {
        if !cache_value(entry, "isOfficial").eq_ignore_ascii_case("yes")
            || !cache_value(entry, "isMultiplayer").eq_ignore_ascii_case("yes")
        {
            continue;
        }
        let path = decode(&entry.name)?.replace('\\', "/").to_lowercase();
        if !path.starts_with("maps/") {
            continue;
        }
        // Zero Hour's explicit exclusions in addMapCollectionToMapListbox.
        if matches!(
            path.as_str(),
            "maps/armored fury/armored fury.map" | "maps/scorched earth/scorched earth.map"
        ) {
            continue;
        }
        let map = match assets
            .read(&path)
            .map_err(|error| error.to_string())
            .and_then(|asset| MapData::parse(&asset.bytes).map_err(|error| error.to_string()))
        {
            Ok(map) => map,
            Err(reason) => {
                eprintln!("Map chooser rejected metadata {path}: {reason}");
                rejected.push((path, reason));
                continue;
            }
        };
        let starts = start_positions(&map);
        if starts.len() < 2 {
            continue;
        }
        match metadata(assets, path.clone(), entry, &map, starts, data) {
            Ok(Some(row)) => maps.push(row),
            Ok(None) => {}
            Err(error) => rejected.push((path, error.to_string())),
        }
    }
    maps.extend(custom::load(assets, data, &mut rejected)?);
    Ok(Catalog {
        maps: order(maps),
        rejected,
    })
}

fn order(mut maps: Vec<MapEntry>) -> Vec<MapEntry> {
    // MapUtil builds display-to-filename globally before filtering category.
    // Its ordered cache visits absolute user filenames before installed Maps.
    maps.sort_by(|left, right| {
        right
            .custom()
            .cmp(&left.custom())
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut winners = std::collections::BTreeMap::new();
    for map in maps {
        winners.insert((map.capacity, map.title.to_lowercase()), map);
    }
    let mut maps: Vec<_> = winners.into_values().collect();
    maps.sort_by(|left, right| {
        left.capacity
            .cmp(&right.capacity)
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
    });
    maps
}

fn metadata(
    assets: &VirtualAssets,
    path: String,
    entry: &content::Declaration,
    map: &MapData,
    starts: Vec<Vec2>,
    data: &mut UiData,
) -> Result<Option<MapEntry>, Box<dyn Error>> {
    let filename = path
        .rsplit_once('/')
        .map_or(path.as_str(), |(_, name)| name);
    let tag = decode(cache_value(entry, "nameLookupTag"))?;
    let title = if tag.is_empty() {
        filename.to_owned()
    } else {
        lookup_title(data, &tag)
    };
    if title.is_empty() {
        return Ok(None);
    }
    let title = if starts.len() >= 2 {
        format!("{title} ({})", starts.len())
    } else {
        title
    };
    let boundary = map
        .heightmap
        .boundaries
        .first()
        .ok_or("map chooser: missing active boundary")?;
    let extent = Vec2::new(
        f32::from(u16::try_from(boundary[0])?),
        f32::from(u16::try_from(boundary[1])?),
    ) * 10.0;
    let preview = path.trim_end_matches(".map").to_owned() + ".tga";
    // positionStartSpots permits absent previews; presentation tries the
    // original UnknownMap mapped image and then leaves the window blank.
    if !data.handles.contains_key(&preview)
        && !data.textures.contains_key(&preview)
        && let Ok(image) = crate::source_texture::load(assets, &preview)
    {
        data.textures.insert(preview.clone(), image);
    }
    Ok(Some(MapEntry {
        path,
        title,
        preview,
        capacity: starts.len(),
        extent,
        starts,
        supplies: preview::positions(entry, "supplyPosition")?,
        tech: preview::positions(entry, "techPosition")?,
        admission: Admission::Unknown,
    }))
}

fn lookup_title(data: &UiData, tag: &str) -> String {
    // GameText::fetch checks global CSF first; the pinned chooser probe does
    // not load the ordinary adjacent map.str file into its map-string table.
    data.labels
        .get(&tag.to_ascii_lowercase())
        .cloned()
        .unwrap_or_else(|| format!("MISSING: '{tag}'"))
}

fn cache_value<'a>(entry: &'a content::Declaration, key: &str) -> &'a str {
    crate::skirmish_data::field(&entry.lines, 0, key)
        .and_then(|line| line.tokens.get(1))
        .map_or("", String::as_str)
}

fn start_positions(map: &MapData) -> Vec<Vec2> {
    let mut starts = Vec::new();
    for slot in 1..=8 {
        let name = format!("Player_{slot}_Start");
        let Some(start) = map
            .objects
            .iter()
            .find(|object| object.waypoint_name() == Some(name.as_str()))
        else {
            break;
        };
        starts.push(Vec2::new(start.position[0], start.position[1]));
    }
    starts
}

fn decode(value: &str) -> Result<String, Box<dyn Error>> {
    let mut bytes = Vec::new();
    let mut remaining = value.as_bytes();
    while let Some((&byte, rest)) = remaining.split_first() {
        if byte == b'_' {
            let pair = rest
                .get(..2)
                .ok_or("truncated map-cache quoted printable")?;
            bytes.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
            remaining = &rest[2..];
        } else {
            bytes.push(byte);
            remaining = rest;
        }
    }
    Ok(String::from_utf8(bytes)?)
}

#[cfg(test)]
mod tests {
    mod custom;
    use super::*;
    use crate::playtest_ui::Action;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_standard_cache_keeps_source_capacities_and_official_filter() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let mut ui =
            crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let maps = load(&assets, &mut ui, crate::playtest::MAP_NAME).unwrap();
        for (name, capacity) in [
            ("tournament desert", 2),
            ("fallen empire", 4),
            ("defcon6", 6),
            ("twilight flame", 8),
        ] {
            let path = format!("maps/{name}/{name}.map");
            let map = maps.maps.iter().find(|map| map.path == path).unwrap();
            assert_eq!(map.capacity, capacity);
            assert_eq!(map.starts.len(), capacity);
            assert!(map.extent.cmpgt(Vec2::ZERO).all());
            assert!(ui.textures.contains_key(&map.preview));
        }
        assert!(!maps.maps.iter().any(|map| map.path.contains("armored fury") || map.path.contains("scorched earth")));
        assert!(
            maps.maps
                .windows(2)
                .all(|pair| pair[0].capacity < pair[1].capacity
                    || (pair[0].capacity == pair[1].capacity
                        && pair[0].title.to_lowercase() <= pair[1].title.to_lowercase()))
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn refreshed_catalog_keeps_selected_current_map_visible_beyond_first_rows() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let mut ui =
            crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let mut maps = load(&assets, &mut ui, crate::playtest::MAP_NAME).unwrap();
        maps.current = maps.maps.len() - 1;
        let path = maps.maps[maps.current].path.clone();
        maps.activate(&Action::SelectMap);
        maps.refresh(&assets, &mut ui).unwrap();
        let row = maps.row(maps.current).unwrap();
        assert!(row >= ui::ROWS);
        assert!(maps.top > 0 && maps.top <= row && row < maps.top + ui::ROWS);
        assert_eq!(maps.maps[maps.current].path, path);
        assert_eq!(maps.highlighted, Some(maps.current));
        assert_eq!(
            maps.display_pos.to_bits(),
            pointer::row_position(maps.top).to_bits()
        );
    }

    #[test]
    fn original_back_keeps_map_and_accept_requests_unknown_but_not_rejected_selection() {
        let make = |path: &str, admission| MapEntry {
            path: path.to_owned(),
            title: path.to_owned(),
            preview: String::new(),
            capacity: 2,
            extent: Vec2::splat(1000.0),
            starts: vec![Vec2::ZERO, Vec2::splat(500.0)],
            supplies: Vec::new(),
            tech: Vec::new(),
            admission,
        };
        let mut maps = MapSelection {
            maps: vec![
                make("old", Admission::Verified),
                make(
                    "new",
                    Admission::Rejected("missing mandatory module".to_owned()),
                ),
            ],
            rejected: Vec::new(),
            current: 0,
            requested: None,
            open: false,
            user_maps: false,
            catalog_state: CatalogState::Ready,
            unlisted: None,
            highlighted: None,
            top: 0,
            display_pos: 0.0,
            list_focused: false,
        };
        maps.activate(&Action::SelectMap);
        maps.activate(&Action::MapRow(1));
        maps.activate(&Action::MapAccept);
        assert!(maps.requested.is_none());
        maps.activate(&Action::MapBack);
        assert_eq!(maps.current, 0);
        maps.maps[1].admission = Admission::Unknown;
        maps.activate(&Action::SelectMap);
        maps.activate(&Action::MapRow(1));
        maps.activate(&Action::MapAccept);
        assert_eq!(maps.requested, Some(1));
        assert_eq!(maps.current, 0);
        maps.reject(1, "atomic load failed".to_owned());
        assert_eq!(maps.current, 0);
        assert!(maps.requested.is_none());
        maps.maps[1].admission = Admission::Unknown;
        maps.commit(1);
        assert!(maps.maps[1].admission.is_verified());
        assert_eq!(maps.current, 1);
        assert!(!maps.open);
    }

    #[test]
    fn original_list_focus_repeated_arrows_scroll_and_accept_on_release() {
        use bevy::{input::ButtonState, prelude::KeyCode};
        let mut maps = MapSelection {
            maps: (0..20)
                .map(|index| MapEntry {
                    path: index.to_string(),
                    title: index.to_string(),
                    preview: String::new(),
                    capacity: 2,
                    extent: Vec2::splat(1000.0),
                    starts: Vec::new(),
                    supplies: Vec::new(),
                    tech: Vec::new(),
                    admission: Admission::Unknown,
                })
                .collect(),
            rejected: Vec::new(),
            current: 0,
            requested: None,
            open: false,
            user_maps: false,
            catalog_state: CatalogState::Ready,
            unlisted: None,
            highlighted: None,
            top: 0,
            display_pos: 0.0,
            list_focused: false,
        };
        maps.activate(&Action::SelectMap);
        maps.key(KeyCode::Enter, ButtonState::Released);
        assert!(maps.requested.is_none());
        maps.activate(&Action::MapRow(0));
        for _ in 0..12 {
            maps.key(KeyCode::ArrowDown, ButtonState::Pressed);
        }
        assert_eq!(maps.highlighted, Some(12));
        assert_eq!(maps.top, 4);
        maps.key(KeyCode::Space, ButtonState::Pressed);
        assert!(maps.requested.is_none());
        maps.key(KeyCode::Space, ButtonState::Released);
        assert_eq!(maps.requested, Some(12));
        maps.requested = None;
        maps.key(KeyCode::Escape, ButtonState::Pressed);
        assert!(maps.open);
        maps.key(KeyCode::Escape, ButtonState::Released);
        assert!(!maps.open);
        maps.activate(&Action::SelectMap);
        maps.activate(&Action::MapScroll(isize::MAX));
        assert_eq!(maps.top, 11);
        maps.activate(&Action::MapCategory(true));
        maps.key(KeyCode::ArrowDown, ButtonState::Pressed);
        maps.key(KeyCode::Enter, ButtonState::Released);
        assert!(maps.highlighted.is_none());
        assert!(maps.requested.is_none());
    }

    #[test]
    fn map_cache_path_decoding_preserves_literal_underscore_and_rejects_corruption() {
        assert_eq!(
            decode("maps_5Cfallen_20empire_5Cfallen_20empire_2Emap").unwrap(),
            "maps\\fallen empire\\fallen empire.map"
        );
        assert_eq!(decode("test_5Fmap").unwrap(), "test_map");
        assert!(decode("bad_2").is_err());
        assert!(decode("bad_ZZ").is_err());
    }
}
