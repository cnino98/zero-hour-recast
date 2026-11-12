use super::*;
use bevy::input::ButtonState;

fn entry(path: &str, title: &str) -> MapEntry {
    MapEntry {
        path: path.to_owned(),
        title: title.to_owned(),
        preview: String::new(),
        capacity: 2,
        extent: Vec2::splat(1000.0),
        starts: Vec::new(),
        supplies: Vec::new(),
        tech: Vec::new(),
        admission: crate::map_selection::Admission::Unknown,
    }
}

#[test]
fn original_display_collision_resolves_before_category_filter_and_keeps_global_ids() {
    let mut selection = MapSelection {
        maps: order(vec![
            entry("user-maps/copy/copy.map", "Same (2)"),
            entry("maps/original/original.map", "SAME (2)"),
            entry("user-maps/custom/custom.map", "Custom (2)"),
        ]),
        rejected: Vec::new(),
        current: 1,
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
    assert_eq!(selection.maps.len(), 2);
    assert!(!selection.maps.iter().any(|map| map.path.contains("copy")));
    selection.activate(&Action::SelectMap);
    assert_eq!(selection.highlighted, Some(1));
    selection.activate(&Action::MapCategory(true));
    assert_eq!(selection.index(0), Some(0));
    selection.activate(&Action::MapRow(0));
    selection.key(KeyCode::Enter, ButtonState::Released);
    assert_eq!(selection.requested, Some(0));
    selection.reject(0, "unsupported map script".to_owned());
    selection.activate(&Action::MapAccept);
    assert!(selection.requested.is_none());
    assert_eq!(selection.current, 1);
    selection.activate(&Action::MapCategory(false));
    assert_eq!(selection.index(0), Some(1));
    selection.activate(&Action::MapRow(0));
    selection.activate(&Action::MapAccept);
    assert_eq!(selection.requested, Some(1));
    selection.commit(0);
    selection.activate(&Action::SelectMap);
    assert!(selection.user_maps);
    assert_eq!(selection.highlighted, Some(0));
}

#[ignore = "requires original game installation"]
#[test]
fn installed_custom_map_scan_refreshes_stale_cache_rejects_malformed_files_and_keeps_local_preview()
{
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!(
            "custom-map-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    let user = fixture.join("UserData");
    let directory = user.join("Maps/Tournament Desert");
    std::fs::create_dir_all(&directory).unwrap();
    let (original, battle) = crate::cleanup_verification::tests::installed();
    let bytes = original
        .read("maps/tournament desert/tournament desert.map")
        .unwrap()
        .bytes;
    let physical = directory.join("Tournament Desert.map");
    std::fs::write(&physical, &bytes).unwrap();
    let parsed = generals_assets::map::MapData::parse(&bytes).unwrap();
    let Some(generals_assets::map::MapValue::Ascii(tag)) = parsed.world_properties.get("mapName")
    else {
        panic!("original Tournament Desert lacks mapName");
    };
    std::fs::write(
        directory.join("map.str"),
        format!("{tag}\n\"Incorrect adjacent chooser title\"\nEND\n"),
    )
    .unwrap();

    std::fs::write(
        directory.join("Tournament Desert.tga"),
        original
            .read("maps/tournament desert/tournament desert.tga")
            .unwrap()
            .bytes,
    )
    .unwrap();
    std::fs::create_dir_all(user.join("Maps/Broken")).unwrap();
    std::fs::write(user.join("Maps/Broken/Broken.map"), b"malformed map").unwrap();
    std::fs::write(user.join("Maps/misplaced.map"), &bytes).unwrap();
    let root = generals_assets::game_root().unwrap();
    let assets = VirtualAssets::with_user_maps(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
        &user,
    )
    .unwrap();
    let mut ui = crate::control_bar::load(
        &assets,
        &battle.simulation.catalog,
        &root.join("UserData/Options.ini"),
    )
    .unwrap();
    let mut rejected = Vec::new();
    let refreshed = super::super::custom::load(&assets, &mut ui, &mut rejected).unwrap();
    assert_eq!(refreshed.len(), 1);
    assert_eq!(
        refreshed[0].title,
        format!("{} (2)", ui.labels[&tag.to_ascii_lowercase()])
    );
    assert_eq!(
        lookup_title(&ui, "Map:UnboundCustomTitle"),
        "MISSING: 'Map:UnboundCustomTitle'"
    );

    assert_eq!(
        refreshed[0].path,
        "user-maps/tournament desert/tournament desert.map"
    );
    assert_eq!(refreshed[0].capacity, 2);
    assert!(!refreshed[0].supplies.is_empty());
    assert!(ui.textures.contains_key(&refreshed[0].preview));
    assert!(
        rejected
            .iter()
            .any(|(path, _)| path == "user-maps/broken/broken.map")
    );
    cache_metadata(
        &assets,
        &mut ui,
        &user,
        &physical,
        bytes.len(),
        &refreshed[0].title,
    );
    std::fs::remove_dir_all(fixture).unwrap();
}

fn cache_metadata(
    assets: &VirtualAssets,
    ui: &mut crate::control_bar::UiData,
    user: &std::path::Path,
    physical: &std::path::Path,
    size: usize,
    title: &str,
) {
    let encoded: String = physical
        .to_string_lossy()
        .chars()
        .map(|ch| {
            if ch == ' ' {
                "_20".to_owned()
            } else if ch == '_' {
                "_5F".to_owned()
            } else {
                ch.to_string()
            }
        })
        .collect();
    let cache = |size: usize| {
        format!(
            "MapCache {encoded}\n fileSize = {size}\n fileCRC = 1\n numPlayers = 2\n nameLookupTag =\n End\n"
        )
    };
    std::fs::write(user.join("Maps/MapCache.ini"), cache(size)).unwrap();
    let cached = super::super::custom::load(assets, ui, &mut Vec::new()).unwrap();
    assert_eq!(cached[0].title, "Tournament Desert.map (2)");
    std::fs::write(
        user.join("Maps/MapCache.ini"),
        cache(size).replace("nameLookupTag =", "nameLookupTag = Map:UnboundCustomTitle"),
    )
    .unwrap();
    let missing = super::super::custom::load(assets, ui, &mut Vec::new()).unwrap();
    assert_eq!(missing[0].title, "MISSING: 'Map:UnboundCustomTitle' (2)");

    std::fs::write(user.join("Maps/MapCache.ini"), cache(size + 1)).unwrap();
    let stale = super::super::custom::load(assets, ui, &mut Vec::new()).unwrap();
    assert_eq!(stale[0].title, title);
    std::fs::write(user.join("Maps/MapCache.ini"), b"Malformed cache\n").unwrap();
    let mut malformed = Vec::new();
    assert_eq!(
        super::super::custom::load(assets, ui, &mut malformed)
            .unwrap()
            .len(),
        1
    );
    assert!(
        malformed
            .iter()
            .any(|(path, _)| path == "user-maps/mapcache.ini")
    );
}
