//! Original map choice reloads every map-dependent world before returning to setup.
use super::{CameraRig, Match, ScreenCapture, load_map};
use crate::{
    control_bar::UiData,
    map_selection::{Admission, MapSelection},
    playtest_ui, playtest_view, world_data,
};
use bevy::{ecs::system::RunSystemOnce, prelude::*};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Resource)]
pub(super) struct Location(pub(super) PathBuf);

pub(super) fn initialize(world: &mut World, root: &Path, path: &str) -> Result<(), Box<dyn Error>> {
    let assets = crate::content_overlay::assets(
        root,
        world.get_resource::<crate::content_overlay::ContentOverlay>(),
    )?;
    let mut selection =
        crate::map_selection::load(&assets, &mut world.resource_mut::<UiData>(), path)?;
    for (path, reason) in &selection.rejected {
        eprintln!("Map metadata unavailable: {path}: {reason}");
    }
    let definitions = world_data::object_definitions(&assets)?;
    let settings = world_data::camera_settings(&assets, root)?;
    // Native MapUtil catalogs metadata without loading every map's gameplay.
    // The startup scene may allow script previews, so validate the current map
    // strictly before allowing Start; other maps are checked when requested.
    let entry = &mut selection.maps[selection.current];
    entry.admission =
        match candidate_preflight(world, root, &assets, &entry.path, &settings, &definitions) {
            Ok(()) => Admission::Verified,
            Err(error) => {
                eprintln!("Map unavailable: {}: {error}", entry.path);
                Admission::Rejected(error.to_string())
            }
        };
    world
        .resource_mut::<UiData>()
        .map_title
        .clone_from(&selection.maps[selection.current].title);
    world.insert_resource(selection);
    world.insert_resource(Location(root.to_owned()));
    Ok(())
}

fn candidate_preflight(
    world: &World,
    root: &Path,
    assets: &VirtualAssets,
    path: &str,
    settings: &world_data::CameraSettings,
    definitions: &std::collections::BTreeMap<String, generals_assets::content::Declaration>,
) -> Result<(), Box<dyn Error>> {
    let overrides = world_data::map_content(assets, path)?;
    if overrides.effective_files.is_empty() {
        return preflight(assets, path, settings, definitions);
    }
    let effective = crate::content_overlay::assets(
        root,
        world.get_resource::<crate::content_overlay::ContentOverlay>(),
    )?
    .with_map_ini_provenance(overrides.effective_files, overrides.raw_sources)?;
    let definitions = world_data::object_definitions(&effective)?;
    let settings = world_data::camera_settings(&effective, root)?;
    preflight(&effective, path, &settings, &definitions)
}

fn preflight(
    assets: &VirtualAssets,
    path: &str,
    settings: &world_data::CameraSettings,
    definitions: &std::collections::BTreeMap<String, generals_assets::content::Declaration>,
) -> Result<(), Box<dyn Error>> {
    let bytes = assets.read(path)?.bytes;
    let map = MapData::parse(&bytes)?;
    let mut data = crate::skirmish_data::load(assets, &map.objects)?;
    let types = &data.types;
    let scripts = generals_assets::scripts::map_scripts(&bytes)?;
    let startup = crate::map_scripts::load_startup(&scripts, types, assets)?;
    world_data::map_labels(assets, path, &mut std::collections::BTreeMap::new())?;
    crate::water_view::data::load(assets, &settings.time_of_day, [1.0; 3])?;
    crate::weather_view::data::load(assets)?;
    world_data::validate_map_objects(&map.objects, definitions, types)?;
    let opponent =
        crate::opponent_data::load(assets, &map).map_err(|error| format!("Map AI: {error}"))?;
    let graph = crate::opponent_data::map_data(&map)?;
    let terrain = crate::terrain::Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )?;
    crate::road_view::load(assets, &map.objects, &terrain)?;
    let mut movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .map_err(|error| format!("Map navigation: {error}"))?;
    terrain.configure_build_restrictions(assets, &mut movement)?;
    let regular_bridges =
        world_data::install_bridges(assets, &map.objects, &terrain, &mut movement)?;
    let (mut layout, _) = world_data::load(assets, &map.objects, &terrain, types, &mut data.models)
        .map_err(|error| format!("Placed map objects: {error}"))?;
    for model in &data.models {
        crate::model_render::parts(model)
            .map_err(|error| format!("Original world models: {error}"))?;
    }
    layout.regular_bridges = regular_bridges;
    layout.map_sides = startup.sides;
    layout.map_teams = startup.teams;
    layout.map_build_lists = startup.build_lists;
    layout.map_template_order = startup.template_order;
    crate::map_scripts::load(assets, &scripts, &map.objects, &layout, types, &graph)?;
    if !(2..=8).contains(&layout.starts.len()) {
        return Err("original skirmish map must have two to eight start positions".into());
    }
    for [from, to] in &map.waypoint_links {
        if [from, to].into_iter().any(|id| {
            !opponent
                .map
                .waypoints
                .iter()
                .any(|waypoint| waypoint.id == *id)
        }) {
            return Err("map waypoint link references an absent waypoint".into());
        }
    }
    Ok(())
}

pub fn verify(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut world = World::new();
    load_map(
        &mut world,
        root,
        &ScreenCapture::None,
        super::FOUR_PLAYER_MAP,
        true,
    )?;
    initialize(&mut world, root, super::FOUR_PLAYER_MAP)?;
    preflight_catalog(&mut world, root);
    let available: Vec<_> = world
        .resource::<MapSelection>()
        .maps
        .iter()
        .filter(|entry| entry.admission.is_verified())
        .map(|entry| (entry.path.clone(), entry.capacity))
        .collect();
    if available.len() < 2 {
        return Err("Original map chooser has fewer than two admitted maps".into());
    }
    let source_assets = crate::content_overlay::assets(
        root,
        world.get_resource::<crate::content_overlay::ContentOverlay>(),
    )?;
    for (path, capacity) in &available {
        load_map(&mut world, root, &ScreenCapture::None, path, false)
            .map_err(|error| format!("Admitted original map {path}: {error}"))?;
        let mut battle = world.resource_mut::<Match>();
        let game = &mut battle.simulation;
        let slots = (0..*capacity)
            .map(|slot| generals_sim::skirmish::PlayerSlot {
                faction: Some(
                    [
                        generals_sim::skirmish::Faction::Usa,
                        generals_sim::skirmish::Faction::China,
                        generals_sim::skirmish::Faction::Gla,
                    ][slot % 3],
                ),
                occupied: true,
                difficulty: (slot != 0).then_some(generals_sim::skirmish::OpponentDifficulty::Hard),
                team: None,
                start: Some(slot),
                color: Some(slot),
            })
            .collect();
        game.reset_slots(slots)?;
        let metadata = generals_assets::scripts::map_scripts(&source_assets.read(path)?.bytes)?;
        let retained = metadata.sides.iter()
            .filter(|side| matches!(side.properties.get("playerName"), Some(generals_assets::map::MapValue::Ascii(name)) if !name.is_empty())
                && matches!(side.properties.get("playerFaction"), Some(generals_assets::map::MapValue::Ascii(faction)) if faction == "FactionCivilian"))
            .count();
        if game.slots.len() != *capacity || game.players.len() != capacity + 1 + retained {
            return Err(format!("Original map {path} player capacity changed").into());
        }
        for owner in 0..*capacity {
            let command = game
                .entities
                .iter()
                .find(|entity| {
                    entity.owner == owner
                        && game.catalog[entity.type_index].kind
                            == generals_sim::skirmish::Kind::CommandCenter
                })
                .ok_or("Original map slot lacks its starting command center")?;
            if !command.position.into_iter().all(f32::is_finite) {
                return Err("Original map starting coordinates are not finite".into());
            }
        }
        for _ in 0..300 {
            game.step();
        }
        if game.supplies.is_empty()
            || !game
                .entities
                .iter()
                .all(|actor| actor.position.into_iter().all(f32::is_finite))
        {
            return Err(
                format!("Original map {path} lost supplies or valid actor positions").into(),
            );
        }
        println!(
            "MAP_LOAD_PASSED path={path} capacity={capacity} actors={} tick={}",
            game.entities.len(),
            game.tick
        );
    }
    println!(
        "Original map chooser: {} complete map-resource loads and occupied-slot initializations passed",
        available.len()
    );
    Ok(())
}

fn preflight_catalog(world: &mut World, root: &Path) {
    let paths: Vec<_> = world
        .resource::<MapSelection>()
        .maps
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    for (index, path) in paths.iter().enumerate() {
        let admission = match validate_request(world, root, path) {
            Ok(()) => Admission::Verified,
            Err(error) => {
                eprintln!("Map unavailable: {path}: {error}");
                Admission::Rejected(error.to_string())
            }
        };
        world.resource_mut::<MapSelection>().maps[index].admission = admission;
    }
}

pub(super) fn reload(world: &mut World) {
    if world.get_resource::<MapSelection>().is_some_and(|maps| {
        matches!(
            maps.catalog_state,
            crate::map_selection::CatalogState::RefreshRequested
        ) && maps.open
    }) && let Err(error) = refresh(world)
    {
        let mut maps = world.resource_mut::<MapSelection>();
        maps.catalog_state = crate::map_selection::CatalogState::Ready;
        let current = maps.current;
        maps.reject(current, error.to_string());
        maps.open = false;
        if let Some(mut menu) = world.get_resource_mut::<playtest_ui::Menu>() {
            menu.map_unavailable();
        }
        eprintln!("Unable to refresh original map chooser: {error}");
    }
    let Some(index) = world
        .get_resource::<MapSelection>()
        .and_then(|maps| maps.requested)
    else {
        return;
    };
    let root = world.resource::<Location>().0.clone();
    let path = world.resource::<MapSelection>().maps[index].path.clone();
    if world.resource::<MapSelection>().maps[index]
        .admission
        .rejection()
        .is_some()
    {
        world.resource_mut::<MapSelection>().requested = None;
        return;
    }
    let generation = world.resource::<Match>().generation.wrapping_add(1);
    let seed = world.resource::<Match>().simulation.random_seed();
    let name = world.resource::<UiData>().preferences.name.clone();
    let settings = match validate_request(world, &root, &path)
        .and_then(|()| load_map(world, &root, &ScreenCapture::None, &path, false))
    {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("Unable to load selected original map {path}: {error}");
            let mut maps = world.resource_mut::<MapSelection>();
            maps.reject(index, error.to_string());
            maps.open = false;
            if let Some(mut menu) = world.get_resource_mut::<playtest_ui::Menu>() {
                menu.map_unavailable();
            }
            return;
        }
    };
    // Keep the current window and engine entities; retire only game-owned scene,
    // UI and sound roots. ChildOf linked despawn retires their complete trees.
    crate::source_audio::retire_map_players(world);
    clear_scene(world);
    let mut menu = world
        .remove_resource::<playtest_ui::Menu>()
        .expect("loaded skirmish menu");
    let capacity = {
        let mut battle = world.resource_mut::<Match>();
        battle.generation = generation;
        battle.simulation.set_random_seed(seed);
        battle.simulation.slots.len()
    };
    menu.apply_map(capacity);
    world.resource_mut::<UiData>().preferences.name = name;
    let title = world.resource::<MapSelection>().maps[index].title.clone();
    world.resource_mut::<UiData>().map_title = title;
    world.insert_resource(CameraRig {
        focus: Vec2::ZERO,
        distance: settings.max_height,
        generation: u32::MAX,
        settings,
    });
    let result = setup_scene(world);
    world.insert_resource(menu);
    if let Err(error) = result {
        // Setup systems only register prepared resources. A schedule failure is
        // an engine error, not a successful map selection or a partial fallback.
        error!("Selected-map presentation setup failed: {error}");
        world.write_message(AppExit::error());
        return;
    }
    world.resource_mut::<MapSelection>().commit(index);
    eprintln!(
        "Selected original map loaded: {path}; {capacity} player slots; generation {generation}"
    );
}

pub(super) fn refresh(world: &mut World) -> Result<(), Box<dyn Error>> {
    let root = world.resource::<Location>().0.clone();
    let assets = crate::content_overlay::assets(
        &root,
        world.get_resource::<crate::content_overlay::ContentOverlay>(),
    )?;
    world.resource_scope(|world, mut maps: Mut<MapSelection>| {
        maps.refresh(&assets, &mut world.resource_mut::<UiData>())?;
        if world.contains_resource::<Assets<Image>>() {
            let textures = std::mem::take(&mut world.resource_mut::<UiData>().textures);
            for (name, texture) in textures {
                let handle = world.resource_mut::<Assets<Image>>().add(texture);
                world.resource_mut::<UiData>().handles.insert(name, handle);
            }
        }
        Ok(())
    })
}

pub(super) fn validate_request(
    world: &World,
    root: &Path,
    path: &str,
) -> Result<(), Box<dyn Error>> {
    let assets = crate::content_overlay::assets(
        root,
        world.get_resource::<crate::content_overlay::ContentOverlay>(),
    )?;
    let settings = world_data::camera_settings(&assets, root)?;
    let definitions = world_data::object_definitions(&assets)?;
    candidate_preflight(world, root, &assets, path, &settings, &definitions)
}

pub(super) fn clear_scene(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, (
            Without<ChildOf>,
            Without<crate::source_audio::RetiringMusic>,
            Or<(With<Transform>, With<Node>, With<AudioPlayer>)>,
        )>()
        .iter(world)
        .collect();
    for entity in roots {
        world.despawn(entity);
    }
}

pub(super) fn setup_scene(world: &mut World) -> Result<(), bevy::ecs::system::RunSystemError> {
    world.run_system_once(playtest_view::setup)?;
    world.run_system_once(playtest_ui::setup)?;
    world.run_system_once(crate::input::setup)?;
    world.run_system_once(crate::waypoint_display::setup)?;
    world.run_system_once(crate::move_hint::setup)?;
    world.run_system_once(crate::attack_cursor::setup)?;
    world.run_system_once(crate::radius_cursor::setup)?;
    world.run_system_once(crate::radar::setup)?;
    world.run_system_once(crate::notifications::setup)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn admission_checks_the_same_mesh_material_support_as_runtime() {
        let root = &generals_assets::game_root().unwrap();
        let assets = crate::content_overlay::assets(root, None).unwrap();
        let definitions = world_data::object_definitions(&assets).unwrap();
        let settings = world_data::camera_settings(&assets, root).unwrap();
        let error = preflight(
            &assets,
            "maps/thefrontline/thefrontline.map",
            &settings,
            &definitions,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Original world models: CBHANGR03.LIGHTG uses an unsupported second texture stage"
        );
        preflight(&assets, super::super::MAP_NAME, &settings, &definitions).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn startup_checks_only_current_and_unknown_request_rejects_without_replacing_world() {
        let root = &generals_assets::game_root().unwrap();
        let mut world = World::new();
        load_map(
            &mut world,
            root,
            &ScreenCapture::None,
            super::super::MAP_NAME,
            false,
        )
        .unwrap();
        initialize(&mut world, root, super::super::MAP_NAME).unwrap();
        let selection = world.resource::<MapSelection>();
        assert!(selection.maps[selection.current].admission.is_verified());
        assert!(
            selection
                .maps
                .iter()
                .enumerate()
                .all(|(index, map)| index == selection.current
                    || matches!(map.admission, Admission::Unknown))
        );
        let rejected = selection
            .maps
            .iter()
            .position(|map| map.path == "maps/thefrontline/thefrontline.map")
            .unwrap();
        let current = selection.current;
        let title = world.resource::<UiData>().map_title.clone();
        let supplies = world.resource::<Match>().simulation.supplies.len();
        world.resource_mut::<Match>().generation = 31;
        world.resource_mut::<MapSelection>().requested = Some(rejected);
        reload(&mut world);
        let selection = world.resource::<MapSelection>();
        assert!(
            selection.maps[rejected]
                .admission
                .rejection()
                .unwrap()
                .contains("unsupported second texture stage")
        );
        assert!(selection.requested.is_none());
        assert_eq!(selection.current, current);
        assert_eq!(world.resource::<UiData>().map_title, title);
        assert_eq!(world.resource::<Match>().generation, 31);
        assert_eq!(
            world.resource::<Match>().simulation.supplies.len(),
            supplies
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn unavailable_model_reload_preserves_the_prior_world_instead_of_omitting_animation() {
        let root = &generals_assets::game_root().unwrap();
        let mut world = World::new();
        load_map(
            &mut world,
            root,
            &ScreenCapture::None,
            super::super::MAP_NAME,
            false,
        )
        .unwrap();
        let title = world.resource::<UiData>().map_title.clone();
        world.resource_mut::<Match>().generation = 17;
        let supplies = world.resource::<Match>().simulation.supplies.len();
        let result = load_map(
            &mut world,
            root,
            &ScreenCapture::None,
            "maps/thefrontline/thefrontline.map",
            false,
        );
        let Err(error) = result else {
            panic!("Reload accepted an unsupported mandatory world-model animation state");
        };
        assert_eq!(
            error.to_string(),
            "ValveStation draw 0 animation state: model zbvalvstat_g mixes valid and unavailable animations on a retained render object"
        );
        assert_eq!(world.resource::<UiData>().map_title, title);
        assert_eq!(world.resource::<Match>().generation, 17);
        assert_eq!(
            world.resource::<Match>().simulation.supplies.len(),
            supplies
        );
    }

    #[cfg(unix)]
    #[ignore = "requires original game installation"]
    #[test]
    fn custom_reload_revalidates_changed_local_data_and_preserves_loaded_resources() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = &generals_assets::game_root().unwrap();
        let fixture = std::env::temp_dir()
            .join("generals-client-tests")
            .join(format!(
                "custom-reload-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let user = fixture.join("UserData");
        let directory = user.join("Maps/Copy");
        std::fs::create_dir_all(&directory).unwrap();
        std::os::unix::fs::symlink(root.join("GameData"), fixture.join("GameData")).unwrap();
        std::fs::copy(root.join("UserData/Options.ini"), user.join("Options.ini")).unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let bytes = assets.read(super::super::MAP_NAME).unwrap().bytes;
        let physical = directory.join("Copy.map");
        std::fs::write(&physical, &bytes).unwrap();
        std::fs::write(user.join("Maps/MapCache.ini"), format!("MapCache {}\n fileSize = {}\n fileCRC = 1\n numPlayers = 2\n nameLookupTag =\n End\n", physical.display(), bytes.len())).unwrap();
        let mut world = World::new();
        load_map(
            &mut world,
            root,
            &ScreenCapture::None,
            super::super::MAP_NAME,
            false,
        )
        .unwrap();
        initialize(&mut world, &fixture, super::super::MAP_NAME).unwrap();
        let index = world
            .resource::<MapSelection>()
            .maps
            .iter()
            .position(|map| map.path == "user-maps/copy/copy.map")
            .unwrap();
        assert!(matches!(
            world.resource::<MapSelection>().maps[index].admission,
            Admission::Unknown
        ));
        let title = world.resource::<UiData>().map_title.clone();
        world.resource_mut::<Match>().generation = 23;
        let count = world.resource::<Match>().simulation.supplies.len();
        // Metadata discovery cannot authorize newly added unsupported overrides.
        std::fs::write(
            directory.join("map.ini"),
            b"ObjectReskin RevalidationCrusader AmericaTankCrusader\n End\n",
        )
        .unwrap();
        world.resource_mut::<MapSelection>().requested = Some(index);
        reload(&mut world);
        assert_eq!(world.resource::<Match>().generation, 23);
        assert_eq!(world.resource::<Match>().simulation.supplies.len(), count);
        assert_eq!(world.resource::<UiData>().map_title, title);
        let selection = world.resource::<MapSelection>();
        assert!(selection.requested.is_none());
        assert_eq!(
            selection.maps[index].admission.rejection(),
            Some("Map-local ObjectReskin requires its resolved native parent-copy consumer")
        );
        assert_eq!(
            selection.maps[selection.current].path,
            super::super::MAP_NAME.replace('\\', "/").to_lowercase()
        );
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn map_retirement_keeps_original_loading_music_until_its_fade_finishes() {
        let (mut world, music) = crate::source_audio::shell_tests::retiring_load_screen();
        let handle = world.get::<AudioPlayer>(music).unwrap().0.clone();
        let obsolete = world
            .spawn(AudioPlayer::<AudioSource>(Handle::default()))
            .id();
        clear_scene(&mut world);
        assert_eq!(world.get::<AudioPlayer>(music).unwrap().0, handle);
        assert!(
            world
                .get::<crate::source_audio::RetiringMusic>(music)
                .is_some()
        );
        assert!(world.get_entity(obsolete).is_err());
    }

    #[test]
    fn map_retirement_removes_scene_children_and_keeps_window_and_engine_state() {
        let mut world = World::new();
        let window = world
            .spawn((Window::default(), bevy::window::PrimaryWindow))
            .id();
        let engine = world.spawn_empty().id();
        let scene = world.spawn(Transform::default()).id();
        let child = world.spawn((Transform::default(), ChildOf(scene))).id();
        let ui = world.spawn(Node::default()).id();
        let sound = world
            .spawn(AudioPlayer::<AudioSource>(Handle::default()))
            .id();
        clear_scene(&mut world);
        for entity in [scene, child, ui, sound] {
            assert!(world.get_entity(entity).is_err());
        }
        assert!(world.get_entity(window).is_ok());
        assert!(world.get_entity(engine).is_ok());
    }
}

#[cfg(all(test, unix))]
mod reopen_tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn copied_map(assets: &VirtualAssets, user: &Path, folder: &str, source: &str) -> String {
        let directory = user.join("Maps").join(folder);
        fs::create_dir_all(&directory).unwrap();
        let bytes = assets.read(source).unwrap().bytes;
        let path = directory.join(format!("{folder}.map"));
        fs::write(&path, &bytes).unwrap();
        fs::write(
            directory.join(format!("{folder}.tga")),
            assets.read(&source.replace(".map", ".tga")).unwrap().bytes,
        )
        .unwrap();
        format!(
            "MapCache {}\n fileSize = {}\n fileCRC = 1\n numPlayers = 2\n nameLookupTag =\n End\n",
            path.display(),
            bytes.len()
        )
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn reopening_discovers_new_user_map_removes_deleted_row_and_keeps_loaded_world() {
        let root = &generals_assets::game_root().unwrap();
        let fixture = std::env::temp_dir()
            .join("generals-client-tests")
            .join(format!(
                "map-reopen-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let user = fixture.join("UserData");
        fs::create_dir_all(&user).unwrap();
        fs::copy(root.join("UserData/Options.ini"), user.join("Options.ini")).unwrap();
        std::os::unix::fs::symlink(root.join("GameData"), fixture.join("GameData")).unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let first = copied_map(&assets, &user, "First", super::super::MAP_NAME);
        fs::write(user.join("Maps/MapCache.ini"), &first).unwrap();
        let mut world = World::new();
        load_map(
            &mut world,
            &fixture,
            &ScreenCapture::None,
            "user-maps/first/first.map",
            false,
        )
        .unwrap();
        initialize(&mut world, &fixture, "user-maps/first/first.map").unwrap();
        world.insert_resource(Assets::<Image>::default());
        world.resource_mut::<Match>().generation = 47;
        let supplies = world.resource::<Match>().simulation.supplies.len();
        let title = world.resource::<UiData>().map_title.clone();
        let second = copied_map(&assets, &user, "Second", "maps/flash fire/flash fire.map");
        fs::write(user.join("Maps/MapCache.ini"), first + &second).unwrap();
        world
            .resource_mut::<MapSelection>()
            .activate(&playtest_ui::Action::SelectMap);
        reload(&mut world);
        {
            let mut selection = world.resource_mut::<MapSelection>();
            assert!(matches!(
                selection.maps[selection.current].admission,
                Admission::Unknown
            ));
            // Refresh invalidates the loaded custom file too. Accept can request
            // its strict validation again, while Start remains unauthorized.
            selection.activate(&playtest_ui::Action::MapAccept);
            assert_eq!(selection.requested, Some(selection.current));
            selection.requested = None;
        }
        assert!(
            world
                .resource::<MapSelection>()
                .maps
                .iter()
                .any(|map| map.path == "user-maps/second/second.map"
                    && matches!(map.admission, Admission::Unknown))
        );
        assert!(
            world
                .resource::<UiData>()
                .handles
                .contains_key("user-maps/second/second.tga")
        );
        fs::remove_file(user.join("Maps/First/First.map")).unwrap();
        world
            .resource_mut::<MapSelection>()
            .activate(&playtest_ui::Action::MapBack);
        world
            .resource_mut::<MapSelection>()
            .activate(&playtest_ui::Action::SelectMap);
        reload(&mut world);
        let mut selection = world.resource_mut::<MapSelection>();
        assert_eq!(
            selection.maps[selection.current].path,
            "user-maps/first/first.map"
        );
        assert!(
            selection.maps[selection.current]
                .admission
                .rejection()
                .is_some()
        );
        selection.activate(&playtest_ui::Action::MapRow(0));
        selection.activate(&playtest_ui::Action::MapAccept);
        assert_eq!(
            selection.maps[selection.requested.unwrap()].path,
            "user-maps/second/second.map"
        );
        assert_eq!(world.resource::<Match>().generation, 47);
        assert_eq!(
            world.resource::<Match>().simulation.supplies.len(),
            supplies
        );
        assert_eq!(world.resource::<UiData>().map_title, title);
        fs::remove_dir_all(fixture).unwrap();
    }
}
