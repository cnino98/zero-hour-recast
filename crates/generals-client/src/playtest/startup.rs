//! Shell.cpp keeps the original menu independent of a `GAME_SHELL` or skirmish world.
use super::{CameraRig, Match, ScreenCapture, load_map, maps};
use crate::{
    control_bar::UiData, map_selection::MapSelection, playtest_ui, world_data::CameraSettings,
};
use bevy::{ecs::system::RunSystemOnce, prelude::*};
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Resource)]
pub(super) struct Progress {
    pub(super) completed: Arc<AtomicUsize>,
    pub(super) cancelled: Arc<AtomicBool>,
}

pub struct Configuration {
    pub(crate) slots: Vec<generals_sim::skirmish::PlayerSlot>,
    pub(crate) seed: u32,
    pub(crate) starting_cash: u32,
    pub(crate) superweapon_restriction: u32,
    pub(crate) game_speed: u8,
}

impl Configuration {
    pub(crate) fn start(self, battle: &mut Match) -> Result<(), Box<dyn Error>> {
        battle.simulation.set_random_seed(self.seed);
        battle
            .simulation
            .set_match_options(self.starting_cash, self.superweapon_restriction);
        battle.game_speed = self.game_speed;
        battle.simulation.reset_slots(self.slots)?;
        battle.begin();
        Ok(())
    }
}

#[derive(Resource)]
pub struct Shell {
    root: PathBuf,
    path: String,
    setup_requested: bool,
    start_requested: bool,
    seed: u32,
    progress: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    task: Option<JoinHandle<Result<(World, CameraSettings), String>>>,
    started: Option<Instant>,
    startup_at: Instant,
    loading_slots: Option<generals_sim::skirmish::ResolvedSlots>,
}

impl Shell {
    pub(crate) fn open_setup(&mut self) {
        self.setup_requested = true;
        // SkirmishGameOptionsMenu initializes its seed from a 32-bit clock.
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_le_bytes();
        self.seed = u32::from_le_bytes(millis[..4].try_into().expect("four seed bytes"));
    }

    pub(crate) const fn start(&mut self) {
        if self.task.is_none() {
            self.start_requested = true;
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.start_requested = false;
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub(crate) const fn loading_slots(&self) -> Option<&generals_sim::skirmish::ResolvedSlots> {
        self.loading_slots.as_ref()
    }

    pub(crate) const fn loading(&self) -> bool {
        self.start_requested || self.task.is_some()
    }

    pub(crate) fn completed(&self) -> usize {
        self.progress.load(Ordering::Relaxed)
    }

    pub(crate) fn startup_elapsed_ms(&self) -> f64 {
        self.startup_at.elapsed().as_secs_f64() * 1000.0
    }

    pub(crate) const fn seed(&self) -> u32 {
        self.seed
    }
}

pub(super) fn phase_reporter<'a>(
    world: &World,
    map_name: &'a str,
) -> impl Fn(&str) -> Result<(), Box<dyn Error>> + use<'a> {
    let started = Instant::now();
    let progress = world
        .get_resource::<Progress>()
        .map(|progress| (progress.completed.clone(), progress.cancelled.clone()));
    move |phase| {
        if let Some((progress, cancelled)) = &progress {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Map loading cancelled".into());
            }
            progress.store(completed_phase(phase), Ordering::Relaxed);
        }
        eprintln!(
            "Map load: path={map_name} phase={phase} elapsed_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        Ok(())
    }
}

pub(super) fn completed_phase(phase: &str) -> usize {
    // GameLogic.cpp LOAD_PROGRESS_POST_LOAD_MAP=2 and MAX_ALL_THE_FREAKN_OBJECTS=80.
    // Rust loads shared assets before constructing the world; hold the last reached
    // source milestone until its actual map objects are ready.
    if phase == "complete" { 80 } else { 2 }
}

fn window_closing(
    mut events: MessageReader<bevy::window::WindowCloseRequested>,
    windows: Query<(), With<bevy::window::PrimaryWindow>>,
    mut shell: ResMut<Shell>,
) {
    if events.read().any(|event| windows.contains(event.window)) {
        shell.cancel();
    }
}

pub fn shell_active(world: &World) -> bool {
    !world.contains_resource::<Match>()
}

pub fn interactive(world: &World) -> bool {
    shell_active(world)
        && world
            .get_resource::<Shell>()
            .is_none_or(|shell| !shell.loading())
}

pub(super) fn run(
    mut app: App,
    root: &Path,
    capture: &ScreenCapture,
    map_path: &str,
    started: Instant,
) -> Result<(), Box<dyn Error>> {
    let assets = crate::content_overlay::assets(
        root,
        app.world()
            .get_resource::<crate::content_overlay::ContentOverlay>(),
    )?;
    let options = root.join("UserData/Options.ini");
    let preferences = crate::options_preferences::Preferences::load(&assets, &options)?;
    let mut audio = crate::source_audio::load_shell(&assets, &options)?;
    let values = preferences.applied();
    audio.apply_volumes(values.music, values.sound, values.sound3d, values.voice);
    let data = crate::control_bar::load_shell(&assets, &options)?;
    app.insert_resource(data)
        .insert_resource(preferences)
        .insert_resource(audio)
        .insert_resource(Shell {
            root: root.to_owned(),
            path: map_path.to_owned(),
            setup_requested: false,
            start_requested: false,
            seed: 0,
            progress: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::new(AtomicBool::new(false)),
            task: None,
            started: None,
            startup_at: started,
            loading_slots: None,
        });
    if let Some(interval) = crate::input::system_double_click_interval() {
        app.insert_resource(crate::map_selection::DoubleClickInterval(interval));
    }
    eprintln!(
        "Startup shell assets ready: elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    super::initialize_app(&mut app, false, !matches!(capture, ScreenCapture::None));
    super::configure_app(&mut app);
    app.add_systems(First, (window_created, window_closing));
    super::configure_frame_pacing(&mut app);
    super::register_capture(&mut app, root, capture);
    if matches!(capture, ScreenCapture::Options) {
        playtest_ui::options::verification::register(&mut app);
    }
    if matches!(capture, ScreenCapture::Menu) {
        playtest_ui::shell::verification::register_menu_probes(&mut app, root.join("output"));
    }
    if matches!(capture, ScreenCapture::ShellFlow) {
        playtest_ui::shell::verification::register(&mut app);
    }
    let surface = app
        .world()
        .resource::<crate::playtest_view::CaptureSurface>()
        .clone();
    eprintln!(
        "Startup shell event loop ready: elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    if app.run().is_error() {
        return Err("Game exited with an error; see the launch log.".into());
    }
    if !matches!(capture, ScreenCapture::None) && !surface.saved() {
        return Err("Game window closed before the diagnostic saved a verified screenshot".into());
    }
    Ok(())
}

fn window_created(
    mut events: MessageReader<bevy::window::WindowCreated>,
    windows: Query<(), With<bevy::window::PrimaryWindow>>,
    shell: Res<Shell>,
    mut reported: Local<bool>,
) {
    let shell = shell.into_inner();
    for event in events.read() {
        if !*reported && windows.contains(event.window) {
            *reported = true;
            eprintln!(
                "Startup native window created: elapsed_ms={:.3}",
                shell.startup_elapsed_ms()
            );
        }
    }
}

pub(super) fn update(world: &mut World) {
    if world.contains_resource::<Match>() {
        return;
    }
    if world
        .resource::<Shell>()
        .task
        .as_ref()
        .is_some_and(JoinHandle::is_finished)
    {
        let task = world
            .resource_mut::<Shell>()
            .task
            .take()
            .expect("completed map task");
        let result = task.join();
        if world.resource::<Shell>().cancelled.load(Ordering::Relaxed) {
            // Keep the sole worker until it has stopped; cancelled results never commit.
            world.resource_mut::<Shell>().started = None;
            world.resource_mut::<Shell>().loading_slots = None;
            world
                .resource_mut::<crate::source_audio::SourceAudio>()
                .end_load_screen();
            return;
        }
        match result {
            Ok(Ok((prepared, settings))) => commit(world, prepared, settings),
            Ok(Err(reason)) => reject(world, &reason),
            Err(_) => reject(world, "Map loading worker panicked; see the launch log"),
        }
        return;
    }
    if world.resource::<Shell>().loading() && !world.resource::<Shell>().start_requested {
        return;
    }
    if std::mem::take(&mut world.resource_mut::<Shell>().setup_requested)
        && let Err(error) = open_setup(world)
    {
        reject(world, &error.to_string());
    }
    if world.get_resource::<MapSelection>().is_some_and(|maps| {
        maps.open
            && matches!(
                maps.catalog_state,
                crate::map_selection::CatalogState::RefreshRequested
            )
    }) {
        if let Err(error) = maps::refresh(world) {
            world.resource_mut::<MapSelection>().catalog_state =
                crate::map_selection::CatalogState::Ready;
            reject(world, &error.to_string());
        } else {
            let maps = world.resource::<MapSelection>();
            let map = &maps.maps[maps.current];
            let data = world.resource::<UiData>();
            if data.map_title != map.title
                || data.map_preview != map.preview
                || data.map_extent != map.extent
                || data.map_starts != map.starts
            {
                let current = maps.current;
                select_metadata(world, current, false);
                world.resource_mut::<MapSelection>().open = true;
                rebuild_shell(world);
            }
        }
    }
    if let Some(index) = world
        .get_resource::<MapSelection>()
        .and_then(|maps| maps.requested)
    {
        select_metadata(world, index, false);
        rebuild_shell(world);
    }
    if std::mem::take(&mut world.resource_mut::<Shell>().start_requested) {
        start_loading(world);
    }
}

fn open_setup(world: &mut World) -> Result<(), Box<dyn Error>> {
    if !world.contains_resource::<MapSelection>() {
        let shell = world.resource::<Shell>();
        let root = shell.root.clone();
        let path = shell.path.clone();
        let assets = crate::content_overlay::assets(
            &root,
            world.get_resource::<crate::content_overlay::ContentOverlay>(),
        )?;
        let maps =
            crate::map_selection::load_shell(&assets, &mut world.resource_mut::<UiData>(), &path)?;
        let current = maps.current;
        world.insert_resource(maps);
        world.insert_resource(maps::Location(root));
        select_metadata(world, current, true);
        rebuild_shell(world);
    }
    playtest_ui::shell::enter_setup(world);
    Ok(())
}

fn select_metadata(world: &mut World, index: usize, initial: bool) {
    world.resource_scope(|world, mut maps: Mut<MapSelection>| {
        let map = &maps.maps[index];
        let mut data = world.resource_mut::<UiData>();
        data.map_title.clone_from(&map.title);
        data.map_preview.clone_from(&map.preview);
        data.map_extent = map.extent;
        data.map_starts.clone_from(&map.starts);
        let capacity = map.capacity;
        // Choosing metadata does not admit its gameplay. Only a successful strict load does.
        maps.current = index;
        maps.requested = None;
        maps.open = false;
        playtest_ui::shell::map_capacity(world, capacity, initial);
    });
}

fn rebuild_shell(world: &mut World) {
    let menu = world
        .remove_resource::<playtest_ui::Menu>()
        .expect("shell menu");
    let roots: Vec<_> = world
        .query_filtered::<Entity, (Without<ChildOf>, Or<(With<Node>, With<Camera2d>)>)>()
        .iter(world)
        .collect();
    for entity in roots {
        world.despawn(entity);
    }
    let result = world.run_system_once(playtest_ui::shell::setup);
    world.insert_resource(menu);
    if let Err(error) = result {
        error!("Shell presentation setup failed: {error}");
        world.write_message(AppExit::error());
    }
}

fn start_loading(world: &mut World) {
    let maps = world.resource::<MapSelection>();
    let map = &maps.maps[maps.current];
    if map.admission.rejection().is_some() {
        return;
    }
    let path = map.path.clone();
    let configuration = world
        .resource::<playtest_ui::Menu>()
        .configuration(world.resource::<Shell>().seed);
    if let Err(error) = prepare_loading_screen(world, &configuration) {
        reject(world, &error.to_string());
        return;
    }
    let preferences = world
        .resource::<crate::options_preferences::Preferences>()
        .clone();
    let overlay = world
        .get_resource::<crate::content_overlay::ContentOverlay>()
        .cloned();
    let shell = world.resource::<Shell>();
    let root = shell.root.clone();
    let progress = Arc::clone(&shell.progress);
    let cancelled = Arc::clone(&shell.cancelled);
    cancelled.store(false, Ordering::Relaxed);
    progress.store(0, Ordering::Relaxed);
    eprintln!("Selected map loading started: path={path}");
    let task = std::thread::spawn(move || {
        let mut prepared = World::new();
        if let Some(overlay) = overlay {
            prepared.insert_resource(overlay);
        }
        prepared.insert_resource(preferences);
        prepared.insert_resource(Progress {
            completed: Arc::clone(&progress),
            cancelled: Arc::clone(&cancelled),
        });
        maps::validate_request(&prepared, &root, &path).map_err(|error| error.to_string())?;
        if cancelled.load(Ordering::Relaxed) {
            return Err("Map loading cancelled".to_owned());
        }
        let settings = load_map(&mut prepared, &root, &ScreenCapture::None, &path, false)
            .map_err(|error| error.to_string())?;
        configuration
            .start(&mut prepared.resource_mut::<Match>())
            .map_err(|error| error.to_string())?;
        // LOAD_PROGRESS_POST_INITIAL_NETWORK_BUILDINGS, also used for local skirmish.
        progress.store(90, Ordering::Relaxed);
        Ok((prepared, settings))
    });
    let mut shell = world.resource_mut::<Shell>();
    shell.task = Some(task);
    shell.started = Some(Instant::now());
}

fn prepare_loading_screen(
    world: &mut World,
    configuration: &Configuration,
) -> Result<(), Box<dyn Error>> {
    let data = world.resource::<UiData>();
    let starts: Vec<_> = data.map_starts.iter().map(Vec2::to_array).collect();
    let slots = generals_sim::skirmish::resolve_player_slots(
        &configuration.slots,
        configuration.seed,
        data.color_labels.len(),
        &starts,
    )?;
    let music = data.loading.music(slots.factions[0]).to_owned();
    let assets = crate::content_overlay::assets(
        &world.resource::<Shell>().root,
        world.get_resource::<crate::content_overlay::ContentOverlay>(),
    )?;
    world
        .resource_mut::<crate::source_audio::SourceAudio>()
        .begin_load_screen(&assets, &music)?;
    world.resource_mut::<Shell>().loading_slots = Some(slots);
    Ok(())
}

fn reject(world: &mut World, reason: &str) {
    eprintln!("Unable to load selected original map: {reason}");
    if let Some(mut maps) = world.get_resource_mut::<MapSelection>() {
        let current = maps.current;
        maps.reject(current, reason.to_owned());
        maps.open = false;
    }
    let mut shell = world.resource_mut::<Shell>();
    shell.start_requested = false;
    shell.setup_requested = false;
    shell.task = None;
    shell.loading_slots = None;
    world
        .resource_mut::<crate::source_audio::SourceAudio>()
        .end_load_screen();
    world.resource_mut::<playtest_ui::Menu>().map_unavailable();
}

fn commit(world: &mut World, mut prepared: World, settings: CameraSettings) {
    let name = world.resource::<UiData>().preferences.name.clone();
    let menu = world
        .remove_resource::<playtest_ui::Menu>()
        .expect("shell menu");
    // Keep already uploaded chooser previews when the map-specific controls replace UiData.
    for entry in &world.resource::<MapSelection>().maps {
        if let Some(handle) = world.resource::<UiData>().handles.get(&entry.preview) {
            prepared
                .resource_mut::<UiData>()
                .handles
                .insert(entry.preview.clone(), handle.clone());
        }
    }
    crate::source_audio::finish_load_screen(world);
    crate::source_audio::retire_map_players(world);
    maps::clear_scene(world);
    install_resources(world, &mut prepared);
    world
        .resource::<Shell>()
        .progress
        .store(91, Ordering::Relaxed);
    world.resource_mut::<UiData>().preferences.name = name;
    let (index, title) = {
        let maps = world.resource::<MapSelection>();
        (maps.current, maps.maps[maps.current].title.clone())
    };
    world.resource_mut::<UiData>().map_title = title;
    world.resource_mut::<MapSelection>().commit(index);
    world.insert_resource(CameraRig {
        focus: Vec2::ZERO,
        distance: settings.max_height,
        generation: u32::MAX,
        settings,
    });
    let result = maps::setup_scene(world);
    world
        .resource::<Shell>()
        .progress
        .store(93, Ordering::Relaxed);
    world.insert_resource(menu);
    if let Err(error) = result {
        error!("Selected-map presentation setup failed: {error}");
        world.write_message(AppExit::error());
        return;
    }
    let shell = world.resource::<Shell>();
    shell.progress.store(100, Ordering::Relaxed);
    eprintln!(
        "Selected map committed: elapsed_ms={:.3}",
        shell
            .started
            .expect("map load started")
            .elapsed()
            .as_secs_f64()
            * 1000.0
    );
}

fn transfer<R: Resource>(world: &mut World, prepared: &mut World) {
    world.insert_resource(
        prepared
            .remove_resource::<R>()
            .unwrap_or_else(|| panic!("{}", std::any::type_name::<R>())),
    );
}

fn install_resources(world: &mut World, prepared: &mut World) {
    transfer::<Match>(world, prepared);
    transfer::<crate::projectile_render::Projectiles>(world, prepared);
    transfer::<crate::particles::Particles>(world, prepared);
    transfer::<crate::w3d_emitter::Emitters>(world, prepared);
    transfer::<crate::physical_debris::Models>(world, prepared);
    transfer::<crate::laser_beam::Lasers>(world, prepared);
    transfer::<crate::structure_view::Structures>(world, prepared);
    transfer::<crate::sway::Sway>(world, prepared);
    transfer::<crate::projectile_stream::Streams>(world, prepared);
    transfer::<crate::particles::orientation::Orientation>(world, prepared);
    transfer::<crate::model_animation::garrison::Guns>(world, prepared);
    transfer::<crate::visibility_view::Fog>(world, prepared);
    transfer::<crate::radar::Radar>(world, prepared);
    transfer::<crate::source_audio::SourceAudio>(world, prepared);
    transfer::<crate::terrain::Terrain>(world, prepared);
    transfer::<crate::playtest_view::Models>(world, prepared);
    transfer::<crate::water_view::WaterView>(world, prepared);
    transfer::<crate::weather_view::WeatherView>(world, prepared);
    transfer::<crate::model_animation::Animations>(world, prepared);
    transfer::<crate::playtest_view::WorldObjects>(world, prepared);
    transfer::<crate::tree_buffer::shadows::Shadows>(world, prepared);
    transfer::<crate::model_animation::TerrainTracks>(world, prepared);
    transfer::<UiData>(world, prepared);
    transfer::<crate::sticky_bomb_icons::Art>(world, prepared);
    transfer::<crate::world_hover::Style>(world, prepared);
    transfer::<crate::input::Mouse>(world, prepared);
    transfer::<crate::move_hint::MoveHints>(world, prepared);
    transfer::<crate::dynamic_lod::DynamicLod>(world, prepared);
    transfer::<crate::income::FloatingRules>(world, prepared);
    transfer::<crate::income::map_crates::Feedback>(world, prepared);
    transfer::<crate::notifications::Notifications>(world, prepared);
    transfer::<crate::weapon_fx::WeaponFx>(world, prepared);
    transfer::<crate::horde_decal::HordeDecals>(world, prepared);
    transfer::<crate::spectre_decals::SpectreDecals>(world, prepared);
    transfer::<crate::lockon_cursor::LockonCursors>(world, prepared);
    transfer::<crate::attack_cursor::AttackCursor>(world, prepared);
    transfer::<crate::radius_cursor::RadiusCursor>(world, prepared);
    transfer::<crate::selection_feedback::Feedback>(world, prepared);
    transfer::<crate::veterancy_feedback::VeterancyFeedback>(world, prepared);
    transfer::<crate::waypoint_display::Waypoints>(world, prepared);
    transfer::<crate::stealth_view::Cloaks>(world, prepared);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell_world() -> World {
        let root = &generals_assets::game_root().unwrap();
        let assets = crate::content_overlay::assets(root, None).unwrap();
        let options = root.join("UserData/Options.ini");
        let mut world = World::new();
        world.insert_resource(crate::control_bar::load_shell(&assets, &options).unwrap());
        world.insert_resource(crate::source_audio::load_shell(&assets, &options).unwrap());
        world.insert_resource(
            crate::options_preferences::Preferences::load(&assets, &options).unwrap(),
        );
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(Assets::<Font>::default());
        world.insert_resource(Shell {
            root: root.to_owned(),
            path: super::super::MAP_NAME.to_owned(),
            setup_requested: false,
            start_requested: false,
            seed: 37,
            progress: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::new(AtomicBool::new(false)),
            task: None,
            started: None,
            startup_at: Instant::now(),
            loading_slots: None,
        });
        world.run_system_once(playtest_ui::shell::setup).unwrap();
        world
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn ordinary_startup_builds_original_main_menu_without_a_match() {
        let mut world = shell_world();
        let data = world.resource::<UiData>();
        assert!(data.units.is_empty());
        assert!(data.ocl_timer.is_none());
        assert!(!world.resource::<Shell>().loading());
        assert!(!world.contains_resource::<Match>());
        assert!(!world.contains_resource::<MapSelection>());
        assert!(!world.contains_resource::<crate::terrain::Terrain>());
        assert!(!world.contains_resource::<crate::playtest_view::Models>());
        let mut buttons =
            world.query::<(&playtest_ui::Action, &playtest_ui::Layer, &Interaction)>();
        for expected in [
            playtest_ui::Action::SinglePlayer,
            playtest_ui::Action::Options,
        ] {
            assert!(buttons.iter(&world).any(|(action, layer, interaction)| {
                matches!(layer, playtest_ui::Layer::Menu)
                    && *interaction == Interaction::None
                    && matches!(
                        (&expected, action),
                        (
                            playtest_ui::Action::SinglePlayer,
                            playtest_ui::Action::SinglePlayer
                        ) | (playtest_ui::Action::Options, playtest_ui::Action::Options)
                    )
            }));
        }
        let root = generals_assets::game_root().unwrap();
        let assets = crate::content_overlay::assets(&root, None).unwrap();
        let error = crate::control_bar::load(&assets, &[], &root.join("UserData/Options.ini"))
            .err()
            .expect("a match requires its object catalog");
        assert_eq!(
            error.to_string(),
            "OCL timer command's original object template is absent"
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn shell_and_original_map_chooser_need_no_game_world_or_admission() {
        let mut world = shell_world();
        assert!(world.resource::<UiData>().units.is_empty());
        assert!(!world.contains_resource::<Match>());
        assert!(!world.contains_resource::<crate::terrain::Terrain>());
        assert!(!world.contains_resource::<crate::playtest_view::Models>());
        assert!(!world.contains_resource::<MapSelection>());
        world.resource_mut::<Shell>().path = "maps/absent remembered map/absent.map".to_owned();
        open_setup(&mut world).unwrap();
        let maps = world.resource::<MapSelection>();
        assert!(maps.maps.len() > 1);
        assert_eq!(
            &maps.maps[maps.current].path,
            maps.maps.iter().map(|entry| &entry.path).min().unwrap()
        );
        assert!(
            maps.maps
                .iter()
                .all(|entry| matches!(entry.admission, crate::map_selection::Admission::Unknown))
        );
        let next = (maps.current + 1) % maps.maps.len();
        let expected = maps.maps[next].title.clone();
        select_metadata(&mut world, next, false);
        assert_eq!(world.resource::<UiData>().map_title, expected);
        assert!(
            !world.resource::<MapSelection>().maps[next]
                .admission
                .is_verified()
        );
        assert!(!world.contains_resource::<Match>());
        assert!(world.contains_resource::<crate::source_audio::SourceAudio>());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn cancelled_start_retires_its_worker_before_retry_and_keeps_options_snapshot() {
        let mut world = shell_world();
        open_setup(&mut world).unwrap();
        let root = world.resource::<Shell>().root.clone();
        let fixture = root.join(format!("output/shell-options-test-{}", std::process::id()));
        let assets = crate::content_overlay::assets(&root, None).unwrap();
        let mut preferences = crate::options_preferences::Preferences::load(
            &assets,
            &fixture.join("UserData/Options.ini"),
        )
        .unwrap();
        let mut draft = preferences.draft();
        draft.alternate_mouse = !world
            .resource::<crate::options_preferences::Preferences>()
            .applied()
            .alternate_mouse;
        draft.scroll = 137;
        let values = preferences.save(&draft).unwrap();
        world.insert_resource(preferences);
        start_loading(&mut world);
        world.resource_mut::<Shell>().cancel();
        world.resource_mut::<Shell>().start();
        assert!(!world.resource::<Shell>().start_requested);
        assert!(world.resource::<Shell>().loading());
        let task = world.resource_mut::<Shell>().task.take().unwrap();
        assert!(task.join().unwrap().is_err());
        assert!(!world.contains_resource::<Match>());
        assert!(
            !world.resource::<MapSelection>().maps[world.resource::<MapSelection>().current]
                .admission
                .is_verified()
        );
        start_loading(&mut world);
        let task = world.resource_mut::<Shell>().task.take().unwrap();
        let (mut prepared, settings) = task.join().unwrap().unwrap();
        let battle = prepared.resource::<Match>();
        let expected = world.resource::<playtest_ui::Menu>().configuration(37);
        assert!(battle.started);
        assert_eq!(battle.simulation.random_seed(), expected.seed);
        assert_eq!(battle.game_speed, expected.game_speed);
        assert_eq!(battle.simulation.players[0].cash, expected.starting_cash);
        assert_eq!(battle.simulation.slots.len(), expected.slots.len());
        assert_eq!(
            settings.scroll_factor.to_bits(),
            values.scroll_factor.to_bits()
        );
        assert_eq!(
            prepared.resource::<crate::input::Mouse>().alternate_mouse(),
            values.alternate_mouse
        );
        assert_eq!(
            world
                .resource::<crate::options_preferences::Preferences>()
                .applied()
                .scroll_factor
                .to_bits(),
            values.scroll_factor.to_bits()
        );
        assert!(!world.contains_resource::<Match>());
        let mut installed = World::new();
        install_resources(&mut installed, &mut prepared);
        assert!(installed.contains_resource::<Match>());
        assert!(installed.contains_resource::<crate::terrain::Terrain>());
        assert!(installed.contains_resource::<crate::playtest_view::Models>());
        let leftovers: Vec<_> = prepared
            .iter_resources()
            .filter(|(info, _)| {
                ![
                    std::any::TypeId::of::<crate::options_preferences::Preferences>(),
                    std::any::TypeId::of::<Progress>(),
                    std::any::TypeId::of::<bevy::ecs::entity_disabling::DefaultQueryFilters>(),
                ]
                .contains(&info.type_id().unwrap())
            })
            .map(|(info, _)| info.name().to_string())
            .collect();
        assert!(
            leftovers.is_empty(),
            "uninstalled map resources: {leftovers:?}"
        );
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn rejected_unknown_map_leaves_setup_available_for_another_choice() {
        let mut world = shell_world();
        open_setup(&mut world).unwrap();
        let maps = world.resource::<MapSelection>();
        let supported = maps.current;
        let rejected = maps
            .maps
            .iter()
            .position(|entry| entry.path == "maps/thefrontline/thefrontline.map")
            .unwrap();
        select_metadata(&mut world, rejected, false);
        let title = world.resource::<UiData>().map_title.clone();
        start_loading(&mut world);
        let task = world.resource_mut::<Shell>().task.take().unwrap();
        let Err(reason) = task.join().unwrap() else {
            panic!("unsupported map unexpectedly loaded");
        };
        assert!(reason.contains("unsupported second texture stage"));
        reject(&mut world, &reason);
        assert!(!world.contains_resource::<Match>());
        assert_eq!(world.resource::<UiData>().map_title, title);
        assert!(
            world.resource::<MapSelection>().maps[rejected]
                .admission
                .rejection()
                .is_some()
        );
        world
            .resource_mut::<playtest_ui::Menu>()
            .clear_confirmation();
        select_metadata(&mut world, supported, false);
        assert!(
            !world.resource::<MapSelection>().maps[supported]
                .admission
                .is_verified()
        );
        assert!(interactive(&world));
    }
}
