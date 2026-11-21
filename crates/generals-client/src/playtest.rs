#[cfg(test)]
pub mod map_fixture;
mod maps;
#[cfg(test)]
mod pad_descent_tests;
pub mod startup;
pub use maps::verify as verify_maps;
#[cfg(test)]
pub mod schedule_tests;
#[cfg(test)]
mod world_route_tests;

use crate::{
    control_bar, input, playtest_ui, playtest_view, selection_feedback, skirmish_data,
    terrain::Terrain,
    waypoint_display,
    world_data::{self, CameraSettings},
};
use bevy::{input::mouse::AccumulatedMouseScroll, prelude::*, window::PrimaryWindow};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, OpponentDifficulty, PlayerSlot, Skirmish};
use std::{error::Error, path::Path, time::Instant};

#[derive(Resource)]
pub struct Match {
    pub observer_player: Option<usize>,
    pub observer_players: Vec<usize>,
    pub simulation: Skirmish,
    pub commands: input::commands::Queue,
    pub selected: Vec<usize>,
    /// Player current-selection Squad, separate from unique selected drawables.
    pub logic_selection: Vec<usize>,
    pub placing: Option<input::placement::Placement>,
    pub started: bool,
    pub paused: bool,
    pub promotions_open: bool,
    pub generation: u32,
    pub opponent_difficulty: Option<OpponentDifficulty>,
    pub opponent_faction: Faction,
    pub game_speed: u8,
}

impl Match {
    fn new(simulation: Skirmish, capture: &ScreenCapture) -> Self {
        let mut battle = Self {
            observer_player: None,
            observer_players: Vec::new(),
            simulation,
            commands: input::commands::Queue::default(),
            selected: Vec::new(),
            logic_selection: Vec::new(),
            placing: None,
            started: false,
            paused: false,
            promotions_open: false,
            generation: 0,
            opponent_difficulty: Some(OpponentDifficulty::Easy),
            opponent_faction: Faction::China,
            game_speed: 30,
        };
        if matches!(
            capture,
            ScreenCapture::Match
                | ScreenCapture::Controls
                | ScreenCapture::Upgrades
                | ScreenCapture::Hacker
                | ScreenCapture::HackerDisable
                | ScreenCapture::InfantryCapture
                | ScreenCapture::Tunnel
                | ScreenCapture::Crawler
                | ScreenCapture::Strategy
                | ScreenCapture::PointDefense(_)
                | ScreenCapture::Paladin
                | ScreenCapture::Drones
                | ScreenCapture::Speaker
                | ScreenCapture::FireBase
                | ScreenCapture::Stinger
                | ScreenCapture::Infiltrators
                | ScreenCapture::Jarmen
                | ScreenCapture::Outpost
                | ScreenCapture::Sentry
                | ScreenCapture::Disarm
                | ScreenCapture::DemoTrap(_)
                | ScreenCapture::Scout
                | ScreenCapture::Pathfinder
                | ScreenCapture::Aircraft
                | ScreenCapture::AircraftCrash
                | ScreenCapture::PilotEjection
                | ScreenCapture::ParachuteTerrain
                | ScreenCapture::MigCrash
                | ScreenCapture::ComancheCountermeasures
                | ScreenCapture::HijackerAirborne
                | ScreenCapture::ChinaAircraft
                | ScreenCapture::Terrorist
                | ScreenCapture::Sabotage
                | ScreenCapture::Hijacker
                | ScreenCapture::Helix
                | ScreenCapture::Ground(_)
                | ScreenCapture::Recon
                | ScreenCapture::Powers(_)
                | ScreenCapture::AirPower(_)
                | ScreenCapture::ChinaPower(_)
                | ScreenCapture::GlaPower(_)
                | ScreenCapture::Aurora
                | ScreenCapture::AngryMob
                | ScreenCapture::BattleBus
                | ScreenCapture::StealthFighter
                | ScreenCapture::Microwave
                | ScreenCapture::Ecm
                | ScreenCapture::LightPulse
                | ScreenCapture::Structure(_)
                | ScreenCapture::World(_)
                | ScreenCapture::BridgeDestruction
                | ScreenCapture::ReinforcementPad(_)
                | ScreenCapture::SupplyDropZone
                | ScreenCapture::NeutralExpanded
                | ScreenCapture::NeutralDamage
                | ScreenCapture::RepairBay
                | ScreenCapture::RepairPad
                | ScreenCapture::FakeBuildings
                | ScreenCapture::Tractor
                | ScreenCapture::Fire(_)
                | ScreenCapture::Cleanup
                | ScreenCapture::Artillery(_)
                | ScreenCapture::Horde
                | ScreenCapture::Technical
                | ScreenCapture::Research
                | ScreenCapture::InternetCenter
                | ScreenCapture::Weapons
                | ScreenCapture::Defenses
                | ScreenCapture::Promotions
                | ScreenCapture::Transports
                | ScreenCapture::BarracksHealing
                | ScreenCapture::Chinook
                | ScreenCapture::Overlord
                | ScreenCapture::Garrisons
                | ScreenCapture::NeutralGarrison
                | ScreenCapture::Movement
                | ScreenCapture::Waypoints
                | ScreenCapture::Audio
        ) {
            battle.start(Faction::Usa);
        }
        battle
    }

    pub fn start(&mut self, faction: Faction) {
        let opponent = match faction {
            Faction::Usa => Faction::China,
            Faction::China => Faction::Gla,
            Faction::Gla => Faction::Usa,
            Faction::Civilian => unreachable!("Neutral is not a local player faction"),
        };
        self.start_configured(faction, opponent, Some(OpponentDifficulty::Easy));
    }

    pub fn start_configured(
        &mut self,
        faction: Faction,
        opponent_faction: Faction,
        difficulty: Option<OpponentDifficulty>,
    ) {
        let reset = match difficulty {
            Some(difficulty) => {
                self.simulation
                    .reset_with_difficulty(faction, opponent_faction, difficulty)
            }
            None => self.simulation.reset_sandbox(faction),
        };
        match reset {
            Ok(()) => self.begin(),
            Err(error) => eprintln!("Unable to start match: {error}"),
        }
    }

    pub fn start_slots(&mut self, slots: Vec<PlayerSlot>) {
        match self.simulation.reset_slots(slots) {
            Ok(()) => self.begin(),
            Err(error) => eprintln!("Unable to start match: {error}"),
        }
    }

    fn begin(&mut self) {
        self.commands.clear();
        self.observer_player = None;
        self.observer_players.clear();
        self.opponent_faction = self.simulation.players[1].faction;
        self.opponent_difficulty = self.simulation.slots[1]
            .difficulty
            .filter(|_| self.simulation.slots[1].occupied);
        self.selected.clear();
        self.logic_selection.clear();
        for owner in 0..self.simulation.players.len() {
            if self.simulation.player_active(owner)
                && let Some(center) = self.simulation.initialize_command_center_selection(owner)
                && owner == 0
            {
                self.selected.push(center);
                self.logic_selection.push(center);
            }
        }
        self.placing = None;
        self.started = true;
        self.paused = false;
        self.promotions_open = false;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn restart(&mut self) {
        self.start_slots(self.simulation.slots.clone());
    }

    /// `CommandXlat` audio retains GUI drawables while logic uses its current Squad.
    pub(crate) fn command_drawables(&self) -> &[usize] {
        if self.commands.executing {
            &self.commands.drawables
        } else {
            &self.selected
        }
    }

    pub fn resource_owner(&self) -> Option<usize> {
        if self.simulation.player_active(0) {
            Some(0)
        } else {
            self.observer_player
        }
    }
}

#[derive(Resource)]
pub struct CameraRig {
    pub focus: Vec2,
    pub distance: f32,
    pub generation: u32,
    pub settings: CameraSettings,
}

pub enum ScreenCapture {
    None,
    Match,
    Menu,
    ShellFlow,
    Skirmish,
    Controls,
    Upgrades,
    Hacker,
    HackerDisable,
    InfantryCapture,
    Tunnel,
    Crawler,
    Strategy,
    PointDefense(bool),
    Paladin,
    Drones,
    Speaker,
    FireBase,
    Stinger,
    Infiltrators,
    Jarmen,
    Outpost,
    Sentry,
    Disarm,
    DemoTrap(bool),
    Scout,
    Pathfinder,
    Aircraft,
    AircraftCrash,
    PilotEjection,
    ParachuteTerrain,
    MigCrash,
    ComancheCountermeasures,
    HijackerAirborne,
    ChinaAircraft,
    MapSelection(usize),
    CustomMap,
    Options,
    Terrorist,
    Sabotage,
    Hijacker,
    Helix,
    Ground(crate::p02_ground_capture::Kind),
    Recon,
    Powers(Faction),
    AirPower(crate::p03_air_capture::Kind),
    ChinaPower(crate::p03_china_capture::Kind),
    GlaPower(crate::p03_gla_capture::Kind),
    Aurora,
    AngryMob,
    BattleBus,
    StealthFighter,
    Microwave,
    Ecm,
    LightPulse,
    Structure(bool),
    World(bool),
    BridgeDestruction,
    ReinforcementPad(Faction),
    SupplyDropZone,
    NeutralExpanded,
    NeutralDamage,
    RepairBay,
    RepairPad,
    FakeBuildings,
    Tractor,
    Fire(bool),
    Cleanup,
    Artillery(u8),
    Horde,
    Technical,
    Research,
    InternetCenter,
    Weapons,
    Defenses,
    Flashbang,
    Transports,
    BarracksHealing,
    AnimationClock,
    UsaResearch,
    NeutralTech(bool),
    Chinook,
    Overlord,
    Garrisons,
    NeutralGarrison,
    Placement,
    Targeting,
    Guard,
    Infantry(Faction),
    InfantryMoving(Faction),
    Promotions,
    Movement,
    Waypoints,
    Supplies(Faction),
    SupplyLift,
    Projectiles,
    Missiles(Faction),
    Results(bool),
    ResultsFour,
    LocalDefeat,
    ObserverQuit,
    ObserverInfo,
    Audio,
}

impl ScreenCapture {
    fn load_world_scene(
        &self,
        world: &mut World,
        objects: &[generals_assets::map::MapObject],
        triggers: &[generals_assets::map::PolygonTrigger],
        terrain: &Terrain,
        movement: &generals_sim::terrain::TerrainMap,
    ) -> Result<(), Box<dyn Error>> {
        if matches!(self, Self::World(_)) {
            world.insert_resource(crate::world_capture::Scene::load(
                objects, triggers, terrain, movement,
            )?);
        }
        Ok(())
    }

    fn load_opponent(
        &self,
        assets: &VirtualAssets,
        map: &MapData,
    ) -> Result<Option<generals_sim::skirmish::OpponentRules>, Box<dyn Error>> {
        matches!(
            self,
            Self::None
                | Self::MapSelection(_)
                | Self::CustomMap
                | Self::Results(true)
                | Self::ResultsFour
                | Self::LocalDefeat
                | Self::ObserverQuit
                | Self::ObserverInfo
        )
        .then(|| crate::opponent_data::load(assets, map))
        .transpose()
    }

    const fn map_name(&self) -> &'static str {
        if matches!(self, Self::BridgeDestruction | Self::ParachuteTerrain) {
            return crate::radar::bridge_destruction_capture::map_name();
        }
        if matches!(self, Self::ReinforcementPad(_) | Self::SupplyDropZone) {
            return "Maps\\Cairo Commandos\\Cairo Commandos.map";
        }
        if let Self::World(bridge) = self {
            return crate::world_capture::map_name(*bridge);
        }
        if let Self::Structure(topple) = self {
            return crate::structure_capture::map_name(*topple);
        }
        if matches!(self, Self::RepairBay) {
            return crate::neutral_expanded_verification::repair_bay_map_name();
        }
        if matches!(self, Self::RepairPad) {
            return crate::neutral_expanded_verification::repair_pad_map_name();
        }
        if matches!(self, Self::NeutralExpanded | Self::NeutralDamage) {
            return crate::neutral_expanded_verification::map_name();
        }
        if let Self::NeutralTech(refinery) = self {
            return crate::neutral_tech_verification::map_name(*refinery);
        }
        if matches!(
            self,
            Self::None
                | Self::MapSelection(_)
                | Self::CustomMap
                | Self::Match
                | Self::Skirmish
                | Self::Menu
                | Self::ResultsFour
                | Self::LocalDefeat
                | Self::ObserverQuit
                | Self::ObserverInfo
        ) {
            FOUR_PLAYER_MAP
        } else {
            MAP_NAME
        }
    }
}

pub enum Profile {
    None,
    Startup,
    Skirmish,
    Defenses,
}

pub fn run(
    root: &Path,
    capture: &ScreenCapture,
    profile: &Profile,
    overlay: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    let mut app = App::new();
    if let Some(path) = overlay {
        app.insert_resource(crate::content_overlay::ContentOverlay::load(root, path)?);
    }
    let preferences =
        crate::skirmish_preferences::Preferences::load(&root.join("UserData/Options.ini"), 30)?;
    let map_path = if matches!(profile, Profile::Defenses) {
        "Maps\\Dark Mountain\\Dark Mountain.map"
    } else if matches!(capture, ScreenCapture::None) && matches!(profile, Profile::None) {
        preferences.map_path().unwrap_or_else(|| capture.map_name())
    } else {
        capture.map_name()
    };
    if matches!(profile, Profile::None)
        && matches!(
            capture,
            ScreenCapture::None
                | ScreenCapture::Menu
                | ScreenCapture::Options
                | ScreenCapture::ShellFlow
        )
    {
        return startup::run(app, root, capture, map_path, started);
    }
    let allow_preview = matches!(profile, Profile::None)
        && matches!(
            capture,
            ScreenCapture::None
                | ScreenCapture::Menu
                | ScreenCapture::Skirmish
                | ScreenCapture::MapSelection(_)
                | ScreenCapture::CustomMap
        );
    let settings = load_map(app.world_mut(), root, capture, map_path, allow_preview)?;
    eprintln!(
        "Startup map ready: elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    maps::initialize(app.world_mut(), root, map_path)?;
    eprintln!(
        "Startup catalog ready: elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    if let Some(interval) = input::system_double_click_interval() {
        app.insert_resource(crate::map_selection::DoubleClickInterval(interval));
    }
    if !matches!(profile, Profile::None) {
        let maps = app.world().resource::<crate::map_selection::MapSelection>();
        if let Some(reason) = maps.maps[maps.current].admission.rejection() {
            return Err(format!("Cannot profile unavailable map {map_path}: {reason}").into());
        }
    }
    run_app(app, root, capture, profile, settings, started)
}

fn load_map(
    world: &mut World,
    root: &Path,
    capture: &ScreenCapture,
    map_name: &str,
    allow_preview: bool,
) -> Result<CameraSettings, Box<dyn Error>> {
    let report = startup::phase_reporter(world, map_name);
    let (assets, map, metadata, weather) = load_map_inputs(world, root, map_name)?;
    report("inputs")?;
    let mut data = skirmish_data::load(&assets, &map.objects)?;
    report("catalog")?;
    let (settings, preferences, values) = map_settings(world, &assets, root)?;

    let opponent = capture.load_opponent(&assets, &map)?;
    let script_map = crate::opponent_data::map_data(&map)?;
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )?;
    let (movement_terrain, regular_bridges) =
        load_navigation(&assets, &terrain, &map.triggers, &map.objects)?;
    report("navigation")?;
    let (layout, world_objects) = load_map_world(
        &assets,
        &metadata,
        &map.objects,
        &terrain,
        &mut data,
        regular_bridges,
        (&script_map, allow_preview),
    )?;
    report("world")?;
    let ui = load_controls(
        world,
        &assets,
        root,
        &data.types,
        (map_name, &map.world_properties),
        &terrain,
        &layout,
    )?;
    let move_hints = crate::move_hint::load(&assets, map.lighting.as_ref())?;
    let mouse = load_mouse(&assets, root, &data.types, &values)?;
    report("controls")?;
    let (mut animations, projectiles) = load_unit_art(&assets, &mut data, &world_objects)?;
    let mut tracks = crate::model_animation::load_tracks(&assets, root, &animations)?;
    report("animations")?;
    let (audio, mut particles, tree_shadows) = load_environment_effects(
        &assets,
        &data.types,
        root,
        &layout.scripts,
        &values,
        &world_objects,
    )?;

    report("audio_particles")?;
    let effects = load_unit_effects(&assets, &mut data)?;
    let (models, water, fog, mut radar) = load_world_models(
        &assets,
        &data.models,
        &settings,
        &terrain,
        map.lighting.as_ref(),
        &map.objects,
        &map.triggers,
    )?;
    report("models")?;
    particles.scorches.set_color(models.scorch_color());
    tracks.set_color(models.track_color());
    animations.retain_models(std::mem::take(&mut data.models), layout.random_seed);
    capture.load_world_scene(
        world,
        &map.objects,
        &map.triggers,
        &terrain,
        &movement_terrain,
    )?;
    let mut simulation = prepare_simulation(world, data, movement_terrain, layout, opponent)?;
    terrain.configure_projectiles(&mut simulation, projectiles.partition_cell_size)?;
    load_feedback(world, &assets, &simulation)?;
    let battle = Match::new(simulation, capture);
    radar.configure_bridges(&assets, &terrain, &battle.simulation, battle.generation)?;
    // Prepare every fallible resource before replacing the current map.
    if let Some(preferences) = preferences {
        world.insert_resource(preferences);
    }
    world.insert_resource(battle);
    world.insert_resource(projectiles);
    world.insert_resource(particles);
    effects.insert(world);
    world.insert_resource(fog);
    world.insert_resource(radar);
    world.insert_resource(audio);
    world.insert_resource(terrain);
    world.insert_resource(models);
    world.insert_resource(water);
    world.insert_resource(weather);
    world.insert_resource(animations);
    world.insert_resource(tracks);
    world.insert_resource(world_objects);
    world.insert_resource(tree_shadows);
    world.insert_resource(ui);
    world.insert_resource(mouse);
    world.insert_resource(move_hints);
    report("complete")?;
    Ok(settings)
}

fn load_mouse(
    assets: &VirtualAssets,
    root: &Path,
    catalog: &[generals_sim::skirmish::UnitType],
    values: &crate::options_preferences::Values,
) -> Result<input::Mouse, Box<dyn Error>> {
    let mut mouse = input::load(assets, &root.join("UserData/Options.ini"), catalog)?;
    mouse.set_alternate_mouse(values.alternate_mouse);
    mouse.set_double_click_attack_move(values.double_click_attack_move);
    Ok(mouse)
}

fn prepare_simulation(
    world: &World,
    data: skirmish_data::SkirmishData,
    movement_terrain: generals_sim::terrain::TerrainMap,
    layout: generals_sim::skirmish::MapLayout,
    opponent: Option<generals_sim::skirmish::OpponentRules>,
) -> Result<Skirmish, Box<dyn Error>> {
    let deposit_cache = world.get_resource::<Match>().map_or(
        generals_sim::skirmish::AutoDepositUpgradeLookup::Unresolved,
        |battle| battle.simulation.auto_deposit_upgrade_cache(),
    );
    let mut simulation = data.into_simulation(movement_terrain, layout)?;
    simulation.restore_auto_deposit_upgrade_cache(deposit_cache);
    if let Some(opponent) = opponent {
        simulation.configure_opponent(opponent, 1, OpponentDifficulty::Easy)?;
    }
    Ok(simulation)
}

fn load_map_overrides(
    assets: VirtualAssets,
    map_name: &str,
    previous_weather: Option<&crate::weather_view::WeatherView>,
) -> Result<(VirtualAssets, crate::weather_view::WeatherView), Box<dyn Error>> {
    let baseline = crate::weather_view::data::load(&assets)?;
    let map_ini = world_data::map_content(&assets, map_name)?;
    let overrides_weather = map_ini
        .effective_files
        .iter()
        .any(|asset| asset.path.eq_ignore_ascii_case("data/ini/weather.ini"));
    let assets = assets.with_map_ini_provenance(map_ini.effective_files, map_ini.raw_sources)?;
    let weather = crate::weather_view::WeatherView::load(
        &assets,
        baseline,
        overrides_weather,
        previous_weather,
    )?;
    Ok((assets, weather))
}

fn load_navigation(
    assets: &VirtualAssets,
    terrain: &Terrain,
    triggers: &[generals_assets::map::PolygonTrigger],
    records: &[generals_assets::map::MapObject],
) -> Result<
    (
        generals_sim::terrain::TerrainMap,
        Vec<generals_sim::skirmish::RegularBridge>,
    ),
    Box<dyn Error>,
> {
    let mut movement = terrain.classify_movement(triggers, records)?;
    terrain.configure_build_restrictions(assets, &mut movement)?;
    let bridges = world_data::install_bridges(assets, records, terrain, &mut movement)?;
    Ok((movement, bridges))
}

fn load_map_world(
    assets: &VirtualAssets,
    metadata: &generals_assets::scripts::MapScripts,
    records: &[generals_assets::map::MapObject],
    terrain: &Terrain,
    data: &mut skirmish_data::SkirmishData,
    regular_bridges: Vec<generals_sim::skirmish::RegularBridge>,
    (script_map, allow_preview): (&generals_sim::skirmish::OpponentMap, bool),
) -> Result<
    (
        generals_sim::skirmish::MapLayout,
        playtest_view::WorldObjects,
    ),
    Box<dyn Error>,
> {
    let (mut layout, objects) =
        world_data::load(assets, records, terrain, &data.types, &mut data.models)?;
    layout.regular_bridges = regular_bridges;
    crate::map_scripts::load_startup(metadata, &data.types, assets)?.apply(&mut layout);
    layout.scripts = load_scripts(
        assets,
        metadata,
        records,
        &layout,
        &data.types,
        script_map,
        allow_preview,
    )?;
    Ok((layout, objects))
}

fn load_unit_art(
    assets: &VirtualAssets,
    data: &mut skirmish_data::SkirmishData,
    objects: &playtest_view::WorldObjects,
) -> Result<
    (
        crate::model_animation::Animations,
        crate::projectile_render::Projectiles,
    ),
    Box<dyn Error>,
> {
    let mut animations = crate::model_animation::load(assets, &mut data.models, &data.types)?;
    animations.load_world(assets, &mut data.models, objects)?;
    animations.bind_logical_bones(&mut data.types);
    crate::skirmish_data::structure_death::bind_bones(&mut data.types, &data.models, &animations);
    let projectiles =
        crate::projectile_data::load(assets, &mut data.types, &data.models, &animations)?;
    Ok((animations, projectiles))
}

fn load_environment_effects(
    assets: &VirtualAssets,
    catalog: &[generals_sim::skirmish::UnitType],
    root: &Path,
    scripts: &[generals_sim::skirmish::map_scripts::Script],
    values: &crate::options_preferences::Values,
    world_objects: &playtest_view::WorldObjects,
) -> Result<
    (
        crate::source_audio::SourceAudio,
        crate::particles::Particles,
        crate::tree_buffer::shadows::Shadows,
    ),
    Box<dyn Error>,
> {
    let tree_shadows = crate::tree_buffer::shadows::load(
        assets,
        root,
        world_objects
            .0
            .iter()
            .any(|object| object.tree_data.is_some()),
    )?;
    let effects = crate::tree_buffer::effect_names(world_objects);
    let mut audio = load_map_audio(assets, catalog, root, scripts, values)?;
    audio.load_position_sounds(assets, &effects)?;
    let particles = crate::particles::load_with_position_effects(assets, catalog, &effects)?;

    Ok((audio, particles, tree_shadows))
}

fn load_map_audio(
    assets: &VirtualAssets,
    catalog: &[generals_sim::skirmish::UnitType],
    root: &Path,
    scripts: &[generals_sim::skirmish::map_scripts::Script],
    values: &crate::options_preferences::Values,
) -> Result<crate::source_audio::SourceAudio, Box<dyn Error>> {
    // Projectile modules supply the Bunker Buster object-FX audio dependencies.
    let mut audio = crate::source_audio::load(assets, catalog, &root.join("UserData/Options.ini"))?;
    audio.load_map_sounds(assets, scripts)?;
    audio.apply_volumes(values.music, values.sound, values.sound3d, values.voice);
    audio.apply_money_transaction_volume(values.money_transaction_volume);
    Ok(audio)
}

struct UnitEffects {
    particle_uplink: crate::particle_uplink::Presentation,
    emitters: crate::w3d_emitter::Definitions,
    lasers: crate::laser_beam::Lasers,
    streams: crate::projectile_stream::Streams,
    garrison_guns: crate::model_animation::garrison::Guns,
    orientation: crate::particles::orientation::Orientation,
    debris: crate::physical_debris::Models,
}

impl UnitEffects {
    fn insert(self, world: &mut World) {
        crate::w3d_emitter::replace_scene(world, self.emitters);
        world.insert_resource(self.debris);
        world.insert_resource(self.particle_uplink);
        world.insert_resource(self.lasers);
        world.init_resource::<crate::structure_view::Structures>();
        world.init_resource::<crate::sway::Sway>();
        world.insert_resource(self.streams);
        world.insert_resource(self.orientation);
        world.insert_resource(self.garrison_guns);
    }
}

fn load_unit_effects(
    assets: &VirtualAssets,
    data: &mut skirmish_data::SkirmishData,
) -> Result<UnitEffects, Box<dyn Error>> {
    let emitters = crate::w3d_emitter::load(assets, &data.types, &data.models)?;
    let lasers = crate::laser_beam::load(assets, &data.types)?;
    let streams = crate::projectile_stream::load(assets, &data.types)?;
    let garrison_guns = crate::model_animation::garrison::load(assets, &mut data.models)?;
    let orientation = crate::particles::orientation::load(assets, &mut data.models)?;
    let debris = crate::physical_debris::load(assets, &data.types, &mut data.models)?;
    let particle_uplink = crate::particle_uplink::load(assets, &data.types)?;
    Ok(UnitEffects {
        particle_uplink,
        emitters,
        lasers,
        streams,
        garrison_guns,
        orientation,
        debris,
    })
}

fn load_world_models(
    assets: &VirtualAssets,
    names: &[generals_assets::w3d::Model],
    settings: &CameraSettings,
    terrain: &Terrain,
    lighting: Option<&generals_assets::map::MapLighting>,
    objects: &[generals_assets::map::MapObject],
    triggers: &[generals_assets::map::PolygonTrigger],
) -> Result<
    (
        playtest_view::Models,
        crate::water_view::WaterView,
        crate::visibility_view::Fog,
        crate::radar::Radar,
    ),
    Box<dyn Error>,
> {
    let fog = crate::visibility_view::Fog::load(assets)?;
    let radar = crate::radar::Radar::load(assets, terrain, triggers)?;
    let mut models =
        playtest_view::Models::load(assets, names, &settings.time_of_day, terrain, lighting)
            .map_err(|error| format!("Original world models: {error}"))?;
    models.load_roads(assets, objects, terrain)?;
    let water = crate::water_view::WaterView::load(
        assets,
        triggers,
        terrain,
        models.water_lighting(),
        &settings.time_of_day,
    )?;
    Ok((models, water, fog, radar))
}

fn load_controls(
    world: &World,
    assets: &VirtualAssets,
    root: &Path,
    catalog: &[generals_sim::skirmish::UnitType],
    map: (
        &str,
        &std::collections::BTreeMap<String, generals_assets::map::MapValue>,
    ),
    terrain: &Terrain,
    layout: &generals_sim::skirmish::MapLayout,
) -> Result<control_bar::UiData, Box<dyn Error>> {
    let mut ui = control_bar::load_for_map(
        assets,
        catalog,
        &root.join("UserData/Options.ini"),
        Some(map.0),
    )
    .map_err(|error| format!("Original control bar: {error}"))?;
    configure_preview(&mut ui, assets, map.0, map.1, terrain, layout);
    if let Some(maps) = world.get_resource::<crate::map_selection::MapSelection>() {
        maps.load_previews(assets, &mut ui);
    }
    Ok(ui)
}

fn map_settings(
    world: &mut World,
    assets: &VirtualAssets,
    root: &Path,
) -> Result<
    (
        CameraSettings,
        Option<crate::options_preferences::Preferences>,
        crate::options_preferences::Values,
    ),
    Box<dyn Error>,
> {
    world.insert_resource(crate::dynamic_lod::load(assets, root)?);
    let mut settings = world_data::camera_settings(assets, root)?;
    let preferences = if world.contains_resource::<crate::options_preferences::Preferences>() {
        None
    } else {
        Some(crate::options_preferences::Preferences::load(
            assets,
            &root.join("UserData/Options.ini"),
        )?)
    };
    let values = preferences
        .as_ref()
        .unwrap_or_else(|| world.resource::<crate::options_preferences::Preferences>())
        .applied()
        .clone();
    settings.scroll_factor = values.scroll_factor;
    Ok((settings, preferences, values))
}

fn load_feedback(
    world: &mut World,
    assets: &VirtualAssets,
    simulation: &generals_sim::skirmish::Skirmish,
) -> Result<(), Box<dyn Error>> {
    let catalog = &simulation.catalog;
    // All remaining map loads have succeeded. Prepare these together so a failure
    // leaves the prior map's command presentation intact as well.
    let income = crate::income::load(assets)?;
    let money_crates = crate::income::map_crates::load(assets)?;
    let notifications = crate::notifications::load(assets)?;
    let weapon_fx = crate::weapon_fx::load(assets, catalog)?;
    let horde = crate::horde_decal::load(assets, catalog)?;
    let spectre = crate::spectre_decals::load(assets, simulation)?;
    let lockon = crate::lockon_cursor::load(assets, catalog)?;
    let attack = crate::attack_cursor::load(assets)?;
    let radius = crate::radius_cursor::load(assets)?;
    let feedback = selection_feedback::load(assets)?;
    let veterancy = crate::veterancy_feedback::load(assets)?;
    let waypoints = waypoint_display::load(assets)?;
    let bomb_icons = crate::sticky_bomb_icons::load(assets)?;
    let world_hover = crate::world_hover::load(assets)?;
    world.insert_resource(income);
    world.insert_resource(money_crates);
    world.insert_resource(notifications);
    world.insert_resource(weapon_fx);
    world.insert_resource(horde);
    world.insert_resource(spectre);
    world.insert_resource(lockon);
    world.insert_resource(attack);
    world.insert_resource(radius);
    world.insert_resource(feedback);
    world.insert_resource(veterancy);
    world.insert_resource(waypoints);
    world.insert_resource(bomb_icons);
    world.insert_resource(world_hover);
    world.insert_resource(crate::stealth_view::Cloaks::new());
    Ok(())
}

fn load_map_inputs(
    world: &World,
    root: &Path,
    map_name: &str,
) -> Result<
    (
        VirtualAssets,
        MapData,
        generals_assets::scripts::MapScripts,
        crate::weather_view::WeatherView,
    ),
    Box<dyn Error>,
> {
    let overlay = world.get_resource::<crate::content_overlay::ContentOverlay>();
    let assets = crate::content_overlay::assets(root, overlay)?;
    let bytes = assets.read(map_name)?.bytes;
    let map = MapData::parse(&bytes)?;
    let scripts = generals_assets::scripts::map_scripts(&bytes)?;
    let previous_weather = world.get_resource::<crate::weather_view::WeatherView>();
    let (assets, weather) = load_map_overrides(assets, map_name, previous_weather)?;
    Ok((assets, map, scripts, weather))
}

fn load_scripts(
    assets: &VirtualAssets,
    metadata: &generals_assets::scripts::MapScripts,
    records: &[generals_assets::map::MapObject],
    layout: &generals_sim::skirmish::MapLayout,
    catalog: &[generals_sim::skirmish::UnitType],
    graph: &generals_sim::skirmish::OpponentMap,
    allow_preview: bool,
) -> Result<Vec<generals_sim::skirmish::map_scripts::Script>, Box<dyn Error>> {
    match crate::map_scripts::load(assets, metadata, records, layout, catalog, graph) {
        Ok(scripts) => Ok(scripts),
        Err(error) if allow_preview => {
            // Setup may display an unavailable preferred map. Its admission gate
            // prevents Start; accepting a playable map replaces this entire world.
            eprintln!("Map preview only; gameplay unavailable: {error}");
            Ok(Vec::new())
        }
        Err(error) => Err(error),
    }
}

fn start_profile(app: &mut App, profile: &Profile) -> Result<(), Box<dyn Error>> {
    match profile {
        Profile::Skirmish => crate::render_performance::start_skirmish(app)?,
        Profile::Defenses => crate::defense_performance::start(app)?,
        Profile::None | Profile::Startup => {}
    }
    Ok(())
}

fn run_app(
    mut app: App,
    root: &Path,
    capture: &ScreenCapture,
    profile: &Profile,
    settings: CameraSettings,
    started: Instant,
) -> Result<(), Box<dyn Error>> {
    initialize_app(
        &mut app,
        !matches!(profile, Profile::None),
        !matches!(capture, ScreenCapture::None),
    );
    configure_app(&mut app);
    configure_frame_pacing(&mut app);
    prepare_scenario(&mut app, root, capture, settings)?;
    start_profile(&mut app, profile)?;
    let surface = app
        .world()
        .resource::<playtest_view::CaptureSurface>()
        .clone();
    eprintln!(
        "Startup event loop ready: elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    if app.run().is_error() {
        return Err("Game exited with an error; see the launch log.".into());
    }
    if !matches!(capture, ScreenCapture::None | ScreenCapture::Audio) && !surface.saved() {
        return Err("Game window closed before the diagnostic saved a verified screenshot".into());
    }
    Ok(())
}

fn configure_preview(
    ui: &mut control_bar::UiData,
    assets: &VirtualAssets,
    map_name: &str,
    properties: &std::collections::BTreeMap<String, generals_assets::map::MapValue>,
    terrain: &Terrain,
    layout: &generals_sim::skirmish::MapLayout,
) {
    let label = match properties.get("mapName") {
        Some(generals_assets::map::MapValue::Ascii(name)) if !name.is_empty() => ui.label(name),
        _ => map_name.rsplit('\\').next().unwrap_or(map_name),
    };
    ui.map_title = format!("{label} ({})", layout.starts.len());
    ui.map_preview = map_name.replace(".map", ".tga");
    ui.map_starts = layout
        .starts
        .iter()
        .map(|start| Vec2::from_array(start.position))
        .collect();
    ui.map_extent = Vec2::new(f32::from(terrain.width), f32::from(terrain.height)) * 10.0;
    // getMapPreviewImage leaves the original window blank if no preview loads.
    if let Ok(image) = crate::source_texture::load(assets, &ui.map_preview) {
        ui.textures.insert(ui.map_preview.clone(), image);
    }
}

fn configure_frame_pacing(app: &mut App) {
    // GameEngine::canUpdateGameLogic adds min(updateTime, targetFrameTime)
    // and executes at most one logic update per rendered engine iteration.
    let timestep = app.world().resource::<Time<Fixed>>().timestep();
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(timestep);
}

fn game_speed_pacing(
    battle: Res<Match>,
    mut fixed: ResMut<Time<Fixed>>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    set_game_speed(
        &mut fixed,
        &mut virtual_time,
        battle.into_inner().game_speed,
    );
}

fn set_game_speed(fixed: &mut Time<Fixed>, virtual_time: &mut Time<Virtual>, speed: u8) {
    // SkirmishGameOptionsMenu::startPressed maps the 61 endpoint to 1000 FPS.
    // The native logic frame remains 1/30 second; only its wall-clock pacing changes.
    let fps = if speed > 60 {
        1000.0
    } else {
        f64::from(speed.max(15))
    };
    fixed.set_timestep_hz(fps);
    virtual_time.set_max_delta(fixed.timestep());
}

fn prepare_scenario(
    app: &mut App,
    root: &Path,
    capture: &ScreenCapture,
    settings: CameraSettings,
) -> Result<(), Box<dyn Error>> {
    let mut battle = app
        .world_mut()
        .remove_resource::<Match>()
        .ok_or("Missing scenario match")?;
    let supplies = prepare_capture(&mut battle, capture)?;
    prepare_abilities(app, &mut battle, capture)?;
    let placement = matches!(capture, ScreenCapture::Placement)
        .then(|| crate::placement_verification::prepare(&mut battle))
        .transpose()?;
    let projectile_capture = prepare_projectiles(&mut battle, capture)?;
    let results = prepare_results(&mut battle, capture)?;
    if matches!(
        capture,
        ScreenCapture::LocalDefeat | ScreenCapture::ObserverQuit | ScreenCapture::ObserverInfo
    ) {
        crate::results_verification::local::prepare(
            app,
            &mut battle,
            match capture {
                ScreenCapture::ObserverQuit => crate::results_verification::local::Target::Scores,
                ScreenCapture::ObserverInfo => {
                    crate::results_verification::local::Target::PlayerInfo
                }
                _ => crate::results_verification::local::Target::Defeat,
            },
        )?;
    }
    let targeting = matches!(capture, ScreenCapture::Targeting)
        .then(|| crate::attack_cursor::verification::prepare(&mut battle))
        .transpose()?;
    let guard = matches!(capture, ScreenCapture::Guard)
        .then(|| crate::radius_cursor::verification::prepare(&mut battle))
        .transpose()?;
    let camera = capture_camera(
        &battle,
        capture,
        supplies.as_ref(),
        projectile_capture.as_ref(),
        placement.as_ref(),
        settings,
    );
    app.insert_resource(battle).insert_resource(camera);
    if matches!(capture, ScreenCapture::Overlord) {
        playtest_ui::frame_overlord_camera(app);
    }
    if matches!(capture, ScreenCapture::Technical) {
        crate::technical_verification::frame_camera(app);
    }
    crate::placement_verification::register(app, placement);
    if let Some(targeting) = targeting {
        crate::attack_cursor::verification::register(app, targeting);
    }
    register_diagnostics(app, root, capture, supplies, projectile_capture, results);
    if let Some(guard) = guard {
        crate::radius_cursor::verification::register(app, guard);
    }
    register_ability_captures(app, capture);
    Ok(())
}

fn register_ability_captures(app: &mut App, capture: &ScreenCapture) {
    match capture {
        ScreenCapture::HackerDisable => crate::hacker_verification::register(app),
        ScreenCapture::InfantryCapture => crate::capture_verification::register(app),
        ScreenCapture::Tunnel => crate::tunnel_verification::register(app),
        ScreenCapture::Crawler => crate::crawler_verification::register(app),
        ScreenCapture::PointDefense(_) => crate::point_defense_capture::register(app),
        ScreenCapture::Paladin => crate::paladin_capture::register(app),
        ScreenCapture::Drones => crate::drone_capture::register(app),
        ScreenCapture::Speaker => crate::speaker_capture::register(app),
        ScreenCapture::FireBase => crate::firebase_capture::register(app),
        ScreenCapture::Stinger => crate::stinger_capture::register(app),
        ScreenCapture::Infiltrators => crate::infiltrator_capture::register(app),
        ScreenCapture::Jarmen => crate::jarmen_capture::register(app),
        ScreenCapture::Outpost => crate::outpost_capture::register(app),
        ScreenCapture::Sentry => crate::sentry_capture::register(app),
        ScreenCapture::Disarm => crate::disarm_capture::register(app),
        ScreenCapture::DemoTrap(_) => crate::demo_trap_capture::register(app),
        ScreenCapture::Scout | ScreenCapture::Pathfinder => {
            crate::stealth_unit_capture::register(app);
        }
        ScreenCapture::Aircraft => crate::aircraft_capture::register(app),
        ScreenCapture::NeutralDamage => crate::neutral_expanded_capture::damage::register(app),
        ScreenCapture::AircraftCrash => crate::aircraft_crash_capture::register(app),
        ScreenCapture::PilotEjection => crate::aircraft_crash_capture::pilot::register(app),
        ScreenCapture::ParachuteTerrain => crate::aircraft_crash_capture::terrain::register(app),
        ScreenCapture::MigCrash => crate::china_aircraft_capture::crash::register(app),
        ScreenCapture::ComancheCountermeasures => {
            crate::aircraft_capture::countermeasures::register(app);
        }
        ScreenCapture::HijackerAirborne => crate::hijacker_capture::airborne::register(app),
        ScreenCapture::ChinaAircraft => crate::china_aircraft_capture::register(app),
        ScreenCapture::Terrorist => crate::terrorist_capture::register(app),
        ScreenCapture::Hijacker => crate::hijacker_capture::register(app),
        ScreenCapture::Helix => crate::helix_capture::register(app),
        ScreenCapture::Aurora => crate::aurora_capture::register(app),
        ScreenCapture::AngryMob => crate::angry_mob_capture::register(app),
        ScreenCapture::BattleBus => crate::battle_bus_capture::register(app),
        ScreenCapture::StealthFighter => crate::stealth_fighter_capture::register(app),
        ScreenCapture::Microwave => crate::microwave_capture::register(app),
        ScreenCapture::Ecm => crate::ecm_capture::register(app),
        ScreenCapture::LightPulse => crate::lightpulse_capture::register(app),
        ScreenCapture::Structure(_) => crate::structure_capture::register(app),
        ScreenCapture::World(_) => crate::world_capture::register(app),
        ScreenCapture::BridgeDestruction => crate::radar::bridge_destruction_capture::register(app),
        ScreenCapture::ReinforcementPad(_) => crate::reinforcement_pad_capture::register(app),
        ScreenCapture::SupplyDropZone => {
            crate::reinforcement_pad_capture::register_supply_drop_zone(app);
        }
        ScreenCapture::FakeBuildings => crate::fake_building_capture::register(app),
        ScreenCapture::Options => crate::playtest_ui::options::verification::register(app),
        ScreenCapture::Sabotage => crate::sabotage_verification::register(app),
        ScreenCapture::Tractor => crate::tractor_capture::register(app),
        ScreenCapture::Fire(_) => crate::fire_capture::register(app),
        ScreenCapture::Cleanup => crate::radius_cursor::cleanup_verification::register(app),
        ScreenCapture::Artillery(_) => crate::artillery_capture::register(app),
        ScreenCapture::UsaResearch => crate::horde_decal::chem_verification::register(app),
        ScreenCapture::Strategy => crate::strategy_capture::register(app),
        ScreenCapture::Horde => crate::horde_verification::register(app),
        ScreenCapture::Hacker => crate::income::verification::register(app),
        ScreenCapture::InternetCenter => crate::income::internet::register(app),
        _ => {}
    }
}

fn prepare_abilities(
    app: &mut App,
    battle: &mut Match,
    capture: &ScreenCapture,
) -> Result<(), Box<dyn Error>> {
    prepare_economy_captures(app, battle, capture)?;
    prepare_stealth_captures(app, battle, capture)?;
    prepare_aircraft_captures(app, battle, capture)?;
    match capture {
        ScreenCapture::HackerDisable => crate::hacker_verification::prepare(app, battle),
        ScreenCapture::InfantryCapture => crate::capture_verification::prepare(app, battle),
        ScreenCapture::Tunnel => crate::tunnel_verification::prepare(app, battle),
        ScreenCapture::Crawler => crate::crawler_verification::prepare(app, battle),
        ScreenCapture::PointDefense(air) => {
            crate::point_defense_capture::prepare(app, battle, *air)
        }
        ScreenCapture::Paladin => crate::paladin_capture::prepare(app, battle),
        ScreenCapture::Drones => crate::drone_capture::prepare(app, battle),
        ScreenCapture::Speaker => crate::speaker_capture::prepare(app, battle),
        ScreenCapture::FireBase => crate::firebase_capture::prepare(app, battle),
        ScreenCapture::Stinger => crate::stinger_capture::prepare(app, battle),
        ScreenCapture::Terrorist => crate::terrorist_capture::prepare(app, battle),
        ScreenCapture::Hijacker => crate::hijacker_capture::prepare(app, battle),
        ScreenCapture::AirPower(kind) => crate::p03_air_capture::prepare(app, battle, *kind),
        ScreenCapture::GlaPower(kind) => crate::p03_gla_capture::prepare(app, battle, *kind),
        ScreenCapture::ChinaPower(kind) => crate::p03_china_capture::prepare(app, battle, *kind),
        ScreenCapture::Recon => crate::p03_recon_capture::prepare(app, battle),
        ScreenCapture::Powers(faction) => crate::p03_capture::prepare(app, battle, *faction),
        ScreenCapture::Ground(kind) => crate::p02_ground_capture::prepare(app, battle, *kind),
        ScreenCapture::Sabotage => crate::sabotage_verification::prepare(app, battle),
        ScreenCapture::Tractor => crate::tractor_capture::prepare(app, battle),
        ScreenCapture::Fire(storm) => crate::fire_capture::prepare(app, battle, *storm),
        ScreenCapture::Cleanup => crate::radius_cursor::cleanup_verification::prepare(app, battle),
        ScreenCapture::Artillery(kind) => crate::artillery_capture::prepare(app, battle, *kind),
        ScreenCapture::Strategy => crate::strategy_capture::prepare(app, battle),
        ScreenCapture::Horde => crate::horde_verification::prepare(app, battle),
        ScreenCapture::Technical => crate::technical_verification::prepare(app, battle),
        ScreenCapture::Research => crate::research_capture::prepare(app, battle),
        ScreenCapture::Weapons => crate::laser_beam::verification::prepare(app, battle),
        ScreenCapture::Defenses => crate::laser_beam::verification::prepare_defenses(app, battle),
        ScreenCapture::Garrisons | ScreenCapture::NeutralGarrison => {
            crate::model_animation::garrison::prepare(
                app,
                battle,
                matches!(capture, ScreenCapture::NeutralGarrison),
            )
        }
        ScreenCapture::Overlord => playtest_ui::prepare_overlord(app, battle),
        ScreenCapture::Chinook => playtest_ui::prepare_chinook(app, battle),
        ScreenCapture::Transports => playtest_ui::prepare_transports(app, battle),
        ScreenCapture::BarracksHealing => playtest_ui::prepare_barracks_healing(app, battle),
        _ => Ok(()),
    }
}

fn prepare_aircraft_captures(
    app: &mut App,
    battle: &mut Match,
    capture: &ScreenCapture,
) -> Result<(), Box<dyn Error>> {
    if matches!(capture, ScreenCapture::Helix) {
        crate::helix_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::AngryMob) {
        crate::angry_mob_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::BattleBus) {
        crate::battle_bus_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::Microwave) {
        crate::microwave_capture::prepare(app, battle)?;
    }
    if let ScreenCapture::World(bridge) = capture {
        crate::world_capture::prepare(app, battle, *bridge)?;
    }
    if let ScreenCapture::Structure(topple) = capture {
        crate::structure_capture::prepare(app, battle, *topple)?;
    }
    if matches!(capture, ScreenCapture::LightPulse) {
        crate::lightpulse_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::Ecm) {
        crate::ecm_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::StealthFighter) {
        crate::stealth_fighter_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::Aurora) {
        crate::aurora_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::Aircraft) {
        crate::aircraft_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::ParachuteTerrain) {
        crate::aircraft_crash_capture::terrain::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::PilotEjection) {
        crate::aircraft_crash_capture::pilot::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::MigCrash) {
        crate::china_aircraft_capture::crash::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::ComancheCountermeasures) {
        crate::aircraft_capture::countermeasures::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::HijackerAirborne) {
        crate::hijacker_capture::airborne::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::AircraftCrash) {
        crate::aircraft_crash_capture::prepare(app, battle)?;
    }
    if matches!(capture, ScreenCapture::ChinaAircraft) {
        crate::china_aircraft_capture::prepare(app, battle)?;
    }
    Ok(())
}

fn prepare_economy_captures(
    app: &mut App,
    battle: &mut Match,
    capture: &ScreenCapture,
) -> Result<(), Box<dyn Error>> {
    match capture {
        ScreenCapture::BridgeDestruction => {
            crate::radar::bridge_destruction_capture::prepare(app, battle)
        }
        ScreenCapture::ReinforcementPad(faction) => {
            crate::reinforcement_pad_capture::prepare(app, battle, *faction)
        }
        ScreenCapture::FakeBuildings => crate::fake_building_capture::prepare(app, battle),
        ScreenCapture::NeutralExpanded => crate::neutral_expanded_capture::prepare(app, battle),
        ScreenCapture::NeutralDamage => {
            crate::neutral_expanded_capture::damage::prepare(app, battle)
        }
        ScreenCapture::RepairPad => {
            crate::neutral_expanded_capture::prepare_repair_pad(app, battle)
        }
        ScreenCapture::SupplyDropZone => {
            crate::reinforcement_pad_capture::prepare_supply_drop_zone(app, battle)
        }
        ScreenCapture::RepairBay => {
            crate::neutral_expanded_capture::prepare_repair_bay(app, battle)
        }
        ScreenCapture::UsaResearch => crate::horde_decal::chem_verification::prepare(app, battle),
        ScreenCapture::NeutralTech(refinery) => {
            crate::neutral_tech_capture::prepare(app, battle, *refinery)
        }
        ScreenCapture::Hacker => crate::income::verification::prepare(app, battle),
        ScreenCapture::InternetCenter => crate::income::internet::prepare(app, battle),
        _ => Ok(()),
    }
}

fn prepare_stealth_captures(
    app: &mut App,
    battle: &mut Match,
    capture: &ScreenCapture,
) -> Result<(), Box<dyn Error>> {
    match capture {
        ScreenCapture::Infiltrators => crate::infiltrator_capture::prepare(app, battle),
        ScreenCapture::Jarmen => crate::jarmen_capture::prepare(app, battle),
        ScreenCapture::Outpost => crate::outpost_capture::prepare(app, battle),
        ScreenCapture::Sentry => crate::sentry_capture::prepare(app, battle),
        ScreenCapture::Disarm => crate::disarm_capture::prepare(app, battle),
        ScreenCapture::DemoTrap(proximity) => {
            crate::demo_trap_capture::prepare(app, battle, *proximity)
        }
        ScreenCapture::Scout | ScreenCapture::Pathfinder => crate::stealth_unit_capture::prepare(
            app,
            battle,
            matches!(capture, ScreenCapture::Scout),
        ),
        _ => Ok(()),
    }
}

fn capture_camera(
    battle: &Match,
    capture: &ScreenCapture,
    supplies: Option<&crate::supply_verification::SupplyCapture>,
    projectile_capture: Option<&crate::projectile_verification::ProjectileCapture>,
    placement: Option<&crate::placement_verification::PlacementCapture>,
    mut settings: CameraSettings,
) -> CameraRig {
    if matches!(
        capture,
        ScreenCapture::Garrisons | ScreenCapture::NeutralGarrison
    ) {
        settings.yaw += std::f32::consts::PI;
    }
    let focus = match (supplies, projectile_capture) {
        _ if matches!(capture, ScreenCapture::Defenses) => {
            let event = &battle.simulation.presentation.assistance_events[0];
            (Vec2::from_array(battle.simulation.entities[event.assistant].position)
                + Vec2::from_array(battle.simulation.entities[event.target].position))
                * 0.05
        }
        _ if matches!(capture, ScreenCapture::Weapons) => {
            let actor = &battle.simulation.entities[battle.selected[0]];
            let target =
                &battle.simulation.entities[actor.laser_target().expect("prepared laser target")];
            (Vec2::from_array(actor.position) + Vec2::from_array(target.position)) * 0.05
        }
        (Some(scenario), _) => {
            Vec2::from_array(battle.simulation.supply_position(scenario.source)) * 0.1
        }
        (None, Some(scenario)) => {
            let shooter = Vec2::from_array(battle.simulation.entities[scenario.shooter].position);
            let target = Vec2::from_array(battle.simulation.entities[scenario.target].position);
            if matches!(capture, ScreenCapture::Flashbang) {
                shooter * 0.1
            } else {
                (shooter + target) * 0.05
            }
        }
        (None, None) if selection_focus(capture) => {
            let first = battle.simulation.entities[battle.selected[0]].position;
            let last = if matches!(capture, ScreenCapture::Targeting)
                && let generals_sim::skirmish::Order::Attack(target) =
                    battle.simulation.entities[battle.selected[0]].ai.order
            {
                battle.simulation.entities[target].position
            } else {
                battle.simulation.entities[*battle.selected.last().expect("capture selection")]
                    .position
            };
            (Vec2::from_array(first) + Vec2::from_array(last)) * 0.05
        }
        _ => Vec2::ZERO,
    };
    let generation = if placement.is_some()
        || supplies.is_some()
        || projectile_capture.is_some()
        || selection_focus(capture)
        || matches!(capture, ScreenCapture::Weapons | ScreenCapture::Defenses)
    {
        battle.generation
    } else {
        u32::MAX
    };
    CameraRig {
        focus: placement.map_or(focus, |scenario| scenario.focus),
        distance: if matches!(
            capture,
            ScreenCapture::Infantry(_)
                | ScreenCapture::InfantryMoving(_)
                | ScreenCapture::AnimationClock
                | ScreenCapture::UsaResearch
                | ScreenCapture::Flashbang
        ) {
            settings.min_height
        } else {
            settings.max_height
        },
        generation,
        settings,
    }
}

const fn selection_focus(capture: &ScreenCapture) -> bool {
    matches!(
        capture,
        ScreenCapture::Controls
            | ScreenCapture::Upgrades
            | ScreenCapture::Hacker
            | ScreenCapture::InternetCenter
            | ScreenCapture::Transports
            | ScreenCapture::BarracksHealing
            | ScreenCapture::Chinook
            | ScreenCapture::Overlord
            | ScreenCapture::Garrisons
            | ScreenCapture::NeutralGarrison
            | ScreenCapture::Targeting
            | ScreenCapture::Guard
            | ScreenCapture::HackerDisable
            | ScreenCapture::InfantryCapture
            | ScreenCapture::Tunnel
            | ScreenCapture::Crawler
            | ScreenCapture::Strategy
            | ScreenCapture::PointDefense(_)
            | ScreenCapture::Paladin
            | ScreenCapture::Drones
            | ScreenCapture::Speaker
            | ScreenCapture::FireBase
            | ScreenCapture::Stinger
            | ScreenCapture::Infiltrators
            | ScreenCapture::Jarmen
            | ScreenCapture::Outpost
            | ScreenCapture::Sentry
            | ScreenCapture::Disarm
            | ScreenCapture::DemoTrap(_)
            | ScreenCapture::Scout
            | ScreenCapture::Pathfinder
            | ScreenCapture::Aircraft
            | ScreenCapture::AircraftCrash
            | ScreenCapture::PilotEjection
            | ScreenCapture::ParachuteTerrain
            | ScreenCapture::MigCrash
            | ScreenCapture::ComancheCountermeasures
            | ScreenCapture::HijackerAirborne
            | ScreenCapture::ChinaAircraft
            | ScreenCapture::Terrorist
            | ScreenCapture::Sabotage
            | ScreenCapture::Hijacker
            | ScreenCapture::Helix
            | ScreenCapture::Ground(_)
            | ScreenCapture::Recon
            | ScreenCapture::Powers(_)
            | ScreenCapture::AirPower(_)
            | ScreenCapture::ChinaPower(_)
            | ScreenCapture::GlaPower(_)
            | ScreenCapture::Aurora
            | ScreenCapture::AngryMob
            | ScreenCapture::BattleBus
            | ScreenCapture::StealthFighter
            | ScreenCapture::Microwave
            | ScreenCapture::Ecm
            | ScreenCapture::LightPulse
            | ScreenCapture::Structure(_)
            | ScreenCapture::World(_)
            | ScreenCapture::BridgeDestruction
            | ScreenCapture::ReinforcementPad(_)
            | ScreenCapture::SupplyDropZone
            | ScreenCapture::NeutralExpanded
            | ScreenCapture::NeutralDamage
            | ScreenCapture::RepairBay
            | ScreenCapture::RepairPad
            | ScreenCapture::FakeBuildings
            | ScreenCapture::Tractor
            | ScreenCapture::Fire(_)
            | ScreenCapture::Cleanup
            | ScreenCapture::Artillery(_)
            | ScreenCapture::Horde
            | ScreenCapture::Technical
            | ScreenCapture::Research
            | ScreenCapture::Infantry(_)
            | ScreenCapture::InfantryMoving(_)
            | ScreenCapture::AnimationClock
            | ScreenCapture::UsaResearch
            | ScreenCapture::NeutralTech(_)
    )
}

fn prepare_results(
    battle: &mut Match,
    capture: &ScreenCapture,
) -> Result<Option<crate::results_verification::ResultsCapture>, Box<dyn Error>> {
    Ok(if matches!(capture, ScreenCapture::ResultsFour) {
        battle.start(Faction::Usa);
        Some(crate::results_verification::prepare_four(
            &mut battle.simulation,
        )?)
    } else if let ScreenCapture::Results(victorious) = capture {
        battle.start(Faction::Usa);
        Some(crate::results_verification::prepare(
            &mut battle.simulation,
            *victorious,
        )?)
    } else {
        None
    })
}

fn prepare_projectiles(
    battle: &mut Match,
    capture: &ScreenCapture,
) -> Result<Option<crate::projectile_verification::ProjectileCapture>, Box<dyn Error>> {
    if matches!(capture, ScreenCapture::Flashbang) {
        battle.start(Faction::Usa);
        let scenario = crate::projectile_verification::prepare_flashbang(&mut battle.simulation)?;
        battle.select_objects(&[scenario.shooter], true);
        return Ok(Some(scenario));
    }
    let (faction, kind) = match capture {
        ScreenCapture::Projectiles => (Faction::Usa, Kind::Tank),
        ScreenCapture::Missiles(faction) => (*faction, Kind::Infantry),
        _ => return Ok(None),
    };
    battle.start(faction);
    let scenario = crate::projectile_verification::prepare(&mut battle.simulation, kind)?;
    battle.select_objects(&[scenario.shooter], true);
    Ok(Some(scenario))
}

fn prepare_capture(
    battle: &mut Match,
    capture: &ScreenCapture,
) -> Result<Option<crate::supply_verification::SupplyCapture>, Box<dyn Error>> {
    battle.promotions_open = matches!(capture, ScreenCapture::Promotions);
    if let ScreenCapture::Infantry(faction) = capture {
        crate::infantry_verification::prepare(battle, *faction)?;
    }
    if matches!(capture, ScreenCapture::AnimationClock) {
        crate::model_animation::native_clock::prepare(battle)?;
    }
    if let ScreenCapture::InfantryMoving(faction) = capture {
        crate::infantry_verification::prepare_moving(battle, *faction)?;
    }
    if matches!(capture, ScreenCapture::Upgrades) {
        let selected = crate::maintenance_verification::prepare(&mut battle.simulation)?;
        battle.select_objects(&selected, true);
    }
    if matches!(capture, ScreenCapture::Controls) {
        let selected = crate::skirmish_verification::prepare_controls(&mut battle.simulation)?;
        battle.select_objects(&selected, true);
    }
    if matches!(
        capture,
        ScreenCapture::Supplies(_) | ScreenCapture::SupplyLift
    ) {
        let faction = if let ScreenCapture::Supplies(faction) = capture {
            *faction
        } else {
            Faction::Usa
        };
        battle.start(faction);
        let scenario = crate::supply_verification::prepare(&mut battle.simulation)?;
        battle.select_objects(&[scenario.collector], true);
        Ok(Some(scenario))
    } else {
        Ok(None)
    }
}

fn register_diagnostics(
    app: &mut App,
    root: &Path,
    capture: &ScreenCapture,
    supplies: Option<crate::supply_verification::SupplyCapture>,
    projectile_capture: Option<crate::projectile_verification::ProjectileCapture>,
    results: Option<crate::results_verification::ResultsCapture>,
) {
    if matches!(capture, ScreenCapture::Infantry(_)) {
        app.insert_resource(crate::infantry_verification::FiringShots::new())
            .add_systems(
                FixedUpdate,
                crate::infantry_verification::record
                    .after(crate::laser_beam::record)
                    .before(crate::particles::step),
            )
            .add_systems(
                PostUpdate,
                crate::infantry_verification::update
                    .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
            );
    }
    if matches!(capture, ScreenCapture::AnimationClock) {
        app.add_systems(
            PostUpdate,
            crate::model_animation::native_clock::update
                .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        );
    }
    if matches!(capture, ScreenCapture::InfantryMoving(_)) {
        app.add_systems(
            PostUpdate,
            crate::infantry_verification::update_moving
                .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
        );
    }
    if let Some(scenario) = projectile_capture {
        app.insert_resource(scenario).add_systems(
            PostUpdate,
            crate::projectile_verification::update
                .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        );
    }
    if let Some(results) = results {
        app.insert_resource(results).add_systems(
            Update,
            crate::results_verification::update
                .after(playtest_ui::update)
                .after(playtest_ui::result_update),
        );
    }

    if let Some(supplies) = supplies {
        if matches!(capture, ScreenCapture::SupplyLift) {
            app.insert_resource(crate::supply_animation_verification::SupplyAnimation::new(
                &supplies,
            ))
            .add_systems(
                Update,
                crate::supply_animation_verification::update.after(crate::model_animation::update),
            );
        } else {
            app.insert_resource(supplies).add_systems(
                Update,
                crate::supply_verification::update
                    .after(input::pointer)
                    .before(playtest_view::render)
                    .run_if(resource_exists::<Match>),
            );
        }
    }
    if matches!(capture, ScreenCapture::Audio) {
        crate::source_audio::enable_verification(app);
    }
    if matches!(capture, ScreenCapture::BarracksHealing) {
        playtest_ui::frame_barracks_healing(app);
    }
    if matches!(capture, ScreenCapture::Skirmish) {
        playtest_ui::verify_setup(app);
    }
    if matches!(capture, ScreenCapture::Movement | ScreenCapture::Waypoints) {
        app.insert_resource(crate::feedback_verification::FeedbackCapture {
            waypoint: matches!(capture, ScreenCapture::Waypoints),
            issued: false,
        })
        .add_systems(
            Update,
            crate::feedback_verification::update
                .after(input::pointer)
                .before(waypoint_display::update),
        );
    }
    if let ScreenCapture::MapSelection(capacity) = capture {
        crate::map_selection::verify_capacity(app, *capacity);
    }
    if matches!(capture, ScreenCapture::CustomMap) {
        crate::map_selection::verify_custom(app, 2);
    }
    register_capture(app, root, capture);
}

fn register_capture(app: &mut App, root: &Path, capture: &ScreenCapture) {
    playtest_view::register_capture(
        app,
        !matches!(capture, ScreenCapture::None | ScreenCapture::Audio),
        root.join(capture_path(capture)),
    );
    if capture_waits_for_scenario(capture) {
        app.world_mut()
            .resource_mut::<playtest_view::Capture>()
            .wait_for_scenario();
    }
}
const fn capture_waits_for_scenario(capture: &ScreenCapture) -> bool {
    matches!(
        capture,
        ScreenCapture::MapSelection(_)
            | ScreenCapture::CustomMap
            | ScreenCapture::Options
            | ScreenCapture::SupplyLift
            | ScreenCapture::Flashbang
            | ScreenCapture::Projectiles
            | ScreenCapture::Missiles(_)
            | ScreenCapture::Placement
            | ScreenCapture::ResultsFour
            | ScreenCapture::LocalDefeat
            | ScreenCapture::ObserverQuit
            | ScreenCapture::ObserverInfo
            | ScreenCapture::Results(_)
            | ScreenCapture::Skirmish
            | ScreenCapture::Infantry(_)
            | ScreenCapture::InfantryMoving(_)
            | ScreenCapture::AnimationClock
            | ScreenCapture::UsaResearch
            | ScreenCapture::NeutralTech(_)
            | ScreenCapture::Targeting
            | ScreenCapture::Guard
            | ScreenCapture::HackerDisable
            | ScreenCapture::InfantryCapture
            | ScreenCapture::Tunnel
            | ScreenCapture::Crawler
            | ScreenCapture::Strategy
            | ScreenCapture::PointDefense(_)
            | ScreenCapture::Paladin
            | ScreenCapture::Drones
            | ScreenCapture::Speaker
            | ScreenCapture::FireBase
            | ScreenCapture::Stinger
            | ScreenCapture::Infiltrators
            | ScreenCapture::Jarmen
            | ScreenCapture::Outpost
            | ScreenCapture::Sentry
            | ScreenCapture::Disarm
            | ScreenCapture::DemoTrap(_)
            | ScreenCapture::Scout
            | ScreenCapture::Pathfinder
            | ScreenCapture::Aircraft
            | ScreenCapture::AircraftCrash
            | ScreenCapture::PilotEjection
            | ScreenCapture::ParachuteTerrain
            | ScreenCapture::MigCrash
            | ScreenCapture::ComancheCountermeasures
            | ScreenCapture::HijackerAirborne
            | ScreenCapture::ChinaAircraft
            | ScreenCapture::Terrorist
            | ScreenCapture::Sabotage
            | ScreenCapture::Hijacker
            | ScreenCapture::Helix
            | ScreenCapture::Ground(_)
            | ScreenCapture::Recon
            | ScreenCapture::Powers(_)
            | ScreenCapture::AirPower(_)
            | ScreenCapture::ChinaPower(_)
            | ScreenCapture::GlaPower(_)
            | ScreenCapture::Aurora
            | ScreenCapture::AngryMob
            | ScreenCapture::BattleBus
            | ScreenCapture::StealthFighter
            | ScreenCapture::Microwave
            | ScreenCapture::Ecm
            | ScreenCapture::LightPulse
            | ScreenCapture::Structure(_)
            | ScreenCapture::World(_)
            | ScreenCapture::BridgeDestruction
            | ScreenCapture::ReinforcementPad(_)
            | ScreenCapture::SupplyDropZone
            | ScreenCapture::NeutralExpanded
            | ScreenCapture::NeutralDamage
            | ScreenCapture::RepairBay
            | ScreenCapture::RepairPad
            | ScreenCapture::FakeBuildings
            | ScreenCapture::Tractor
            | ScreenCapture::Fire(_)
            | ScreenCapture::Cleanup
            | ScreenCapture::Artillery(_)
            | ScreenCapture::Horde
            | ScreenCapture::Technical
            | ScreenCapture::Research
            | ScreenCapture::Weapons
            | ScreenCapture::Defenses
            | ScreenCapture::Transports
            | ScreenCapture::BarracksHealing
            | ScreenCapture::Chinook
            | ScreenCapture::Overlord
            | ScreenCapture::Garrisons
            | ScreenCapture::NeutralGarrison
    )
}

const fn capture_path(capture: &ScreenCapture) -> &'static str {
    if let Some(path) = flight_capture_path(capture) {
        return path;
    }
    match capture {
        ScreenCapture::Menu => "output/menu.png",
        ScreenCapture::ShellFlow => "output/shell-flow.png",
        ScreenCapture::Skirmish => "output/skirmish.png",
        ScreenCapture::Controls => "output/controls.png",
        ScreenCapture::Upgrades => "output/upgrades.png",
        ScreenCapture::Hacker => "output/hacker.png",
        ScreenCapture::HackerDisable => "output/hacker-disable.png",
        ScreenCapture::InfantryCapture => "output/infantry-capture.png",
        ScreenCapture::Tunnel => "output/tunnel.png",
        ScreenCapture::Crawler => "output/crawler.png",
        ScreenCapture::Strategy => "output/strategy.png",
        ScreenCapture::PointDefense(false) => "output/point-defense.png",
        ScreenCapture::PointDefense(true) => "output/avenger.png",
        ScreenCapture::Paladin => "output/paladin.png",
        ScreenCapture::Drones => "output/drones.png",
        ScreenCapture::Speaker => "output/speaker.png",
        ScreenCapture::FireBase => "output/firebase.png",
        ScreenCapture::Stinger => "output/stinger.png",
        ScreenCapture::Infiltrators => "output/infiltrators.png",
        ScreenCapture::Jarmen => "output/jarmen.png",
        ScreenCapture::Outpost => "output/outpost.png",
        ScreenCapture::Sentry => "output/sentry.png",
        ScreenCapture::Disarm => "output/disarm.png",
        ScreenCapture::DemoTrap(false) => "output/demo-trap.png",
        ScreenCapture::DemoTrap(true) => "output/demo-trap-proximity.png",
        ScreenCapture::Scout => "output/scout.png",
        ScreenCapture::Pathfinder => "output/pathfinder.png",
        ScreenCapture::CustomMap => "output/custom-map.png",
        ScreenCapture::MapSelection(2) => "output/map-selection-two.png",
        ScreenCapture::MapSelection(4) => "output/map-selection-four.png",
        ScreenCapture::MapSelection(6) => "output/map-selection-six.png",
        ScreenCapture::MapSelection(_) => "output/map-selection-eight.png",
        ScreenCapture::Terrorist => "output/terrorist.png",
        ScreenCapture::Hijacker => "output/hijacker.png",
        ScreenCapture::AngryMob => "output/angry-mob.png",
        ScreenCapture::BattleBus => "output/battle-bus.png",
        ScreenCapture::RepairBay => "output/repair-bay.png",
        ScreenCapture::RepairPad => "output/repair-pad.png",
        ScreenCapture::NeutralExpanded => "output/neutral-expanded.png",
        ScreenCapture::NeutralDamage => "output/neutral-tech-death.png",
        ScreenCapture::FakeBuildings => "output/fake-buildings.png",
        ScreenCapture::Options => "output/options.png",
        ScreenCapture::Sabotage => "output/sabotage.png",
        ScreenCapture::Tractor => "output/tractor.png",
        ScreenCapture::Fire(false) => "output/fire-wall.png",
        ScreenCapture::Fire(true) => "output/firestorm.png",
        ScreenCapture::Cleanup => "output/cleanup.png",
        ScreenCapture::Artillery(0) => "output/artillery.png",
        ScreenCapture::Artillery(1) => "output/buggy.png",
        ScreenCapture::Artillery(_) => "output/scud.png",
        ScreenCapture::Horde => "output/horde.png",
        ScreenCapture::Technical => "output/technical.png",
        ScreenCapture::Research => "output/research.png",
        ScreenCapture::InternetCenter => "output/internet-center.png",
        ScreenCapture::Weapons => "output/weapons.png",
        ScreenCapture::Defenses => "output/defenses.png",
        ScreenCapture::Flashbang => "output/flashbang.png",
        ScreenCapture::Transports => "output/transports.png",
        ScreenCapture::BarracksHealing => "output/barracks-healing.png",
        ScreenCapture::Overlord => "output/overlord.png",
        ScreenCapture::Garrisons => "output/garrisons.png",
        ScreenCapture::NeutralGarrison => "output/neutral-garrison.png",
        ScreenCapture::Placement => "output/placement.png",
        ScreenCapture::Targeting => "output/targeting.png",
        ScreenCapture::Guard => "output/guard-radius.png",
        ScreenCapture::Infantry(Faction::Usa) => "output/infantry-usa.png",
        ScreenCapture::Infantry(Faction::China) => "output/infantry-china.png",
        ScreenCapture::Infantry(Faction::Gla) => "output/infantry-gla.png",
        ScreenCapture::AnimationClock => "output/animation-clock.png",
        ScreenCapture::UsaResearch => "output/usa-research.png",
        ScreenCapture::NeutralTech(false) => "output/hospital.png",
        ScreenCapture::NeutralTech(true) => "output/refinery.png",
        ScreenCapture::InfantryMoving(Faction::Usa) => "output/infantry-moving-usa.png",
        ScreenCapture::InfantryMoving(Faction::China) => "output/infantry-moving-china.png",
        ScreenCapture::InfantryMoving(Faction::Gla) => "output/infantry-moving-gla.png",
        ScreenCapture::Promotions => "output/promotions.png",
        ScreenCapture::Movement => "output/movement.png",
        ScreenCapture::Waypoints => "output/waypoints.png",
        ScreenCapture::Supplies(Faction::Usa) => "output/supplies-usa.png",
        ScreenCapture::Supplies(Faction::China) => "output/supplies-china.png",
        ScreenCapture::Supplies(Faction::Gla) => "output/supplies-gla.png",
        ScreenCapture::Projectiles => "output/projectiles.png",
        ScreenCapture::Missiles(Faction::Usa) => "output/missiles-usa.png",
        ScreenCapture::Missiles(Faction::China) => "output/missiles-china.png",
        ScreenCapture::Missiles(Faction::Gla) => "output/missiles-gla.png",
        ScreenCapture::LocalDefeat => "output/local-defeat.png",
        ScreenCapture::ObserverQuit => "output/observer-results.png",
        ScreenCapture::ObserverInfo => "output/observer-info.png",
        ScreenCapture::ResultsFour => "output/results-four.png",
        ScreenCapture::Results(true) => "output/results-victory.png",
        ScreenCapture::Results(false) => "output/results-defeat.png",
        _ => "output/playtest.png",
    }
}
const fn flight_capture_path(capture: &ScreenCapture) -> Option<&'static str> {
    Some(match capture {
        ScreenCapture::AirPower(crate::p03_air_capture::Kind::A10) => "output/p03-a10-cleared.png",
        ScreenCapture::AirPower(crate::p03_air_capture::Kind::Paradrop) => {
            "output/p03-paradrop-cleared.png"
        }
        ScreenCapture::AirPower(crate::p03_air_capture::Kind::Daisy) => {
            "output/p03-daisy-cleared.png"
        }
        ScreenCapture::AirPower(crate::p03_air_capture::Kind::Moab) => {
            "output/p03-moab-cleared.png"
        }
        ScreenCapture::AirPower(crate::p03_air_capture::Kind::Leaflet) => {
            "output/p03-leaflet-cleared.png"
        }
        ScreenCapture::AirPower(crate::p03_air_capture::Kind::Spectre) => {
            "output/p03-spectre-cleared.png"
        }
        ScreenCapture::ChinaPower(crate::p03_china_capture::Kind::Artillery) => {
            "output/p03-china-artillery.png"
        }
        ScreenCapture::ChinaPower(crate::p03_china_capture::Kind::Cluster) => {
            "output/p03-china-cluster.png"
        }
        ScreenCapture::ChinaPower(crate::p03_china_capture::Kind::Carpet) => {
            "output/p03-china-carpet.png"
        }
        ScreenCapture::ChinaPower(crate::p03_china_capture::Kind::Emp) => {
            "output/p03-china-emp.png"
        }
        ScreenCapture::ChinaPower(crate::p03_china_capture::Kind::Frenzy) => {
            "output/p03-china-frenzy.png"
        }
        ScreenCapture::ChinaPower(crate::p03_china_capture::Kind::Cannon) => {
            "output/p03-china-cannon-neutron.png"
        }
        ScreenCapture::GlaPower(crate::p03_gla_capture::Kind::Ambush) => {
            "output/p03-gla-ambush.png"
        }
        ScreenCapture::GlaPower(crate::p03_gla_capture::Kind::Gps) => "output/p03-gla-gps.png",
        ScreenCapture::GlaPower(crate::p03_gla_capture::Kind::Sneak) => "output/p03-gla-sneak.png",
        ScreenCapture::GlaPower(crate::p03_gla_capture::Kind::Anthrax) => {
            "output/p03-gla-anthrax.png"
        }
        ScreenCapture::GlaPower(crate::p03_gla_capture::Kind::Radar) => "output/p03-gla-radar.png",
        ScreenCapture::GlaPower(crate::p03_gla_capture::Kind::Booby) => "output/p03-gla-booby.png",
        ScreenCapture::Recon => "output/p03-recon-cleared.png",
        ScreenCapture::Powers(Faction::Usa) => "output/p03-particle-scorch.png",
        ScreenCapture::Powers(Faction::China) => "output/p03-nuke-blast.png",
        ScreenCapture::Powers(Faction::Gla) => "output/p03-scud-toxin.png",
        ScreenCapture::Aircraft => "output/aircraft.png",
        ScreenCapture::AircraftCrash => "output/aircraft-crash-hulk.png",
        ScreenCapture::PilotEjection => "output/ejected-pilot-landed.png",
        ScreenCapture::ParachuteTerrain => "output/parachute-water-destruction.png",
        ScreenCapture::MigCrash => "output/mig-crash-hulk.png",
        ScreenCapture::ComancheCountermeasures => "output/comanche-countermeasures.png",
        ScreenCapture::HijackerAirborne => "output/hijacker-airborne-landed.png",
        ScreenCapture::ChinaAircraft => "output/china-aircraft.png",
        ScreenCapture::Helix => "output/helix.png",
        ScreenCapture::Ground(crate::p02_ground_capture::Kind::Burton) => {
            "output/burton-c4-detonation.png"
        }
        ScreenCapture::Ground(crate::p02_ground_capture::Kind::BombTruck) => {
            "output/bomb-truck-detonation.png"
        }
        ScreenCapture::Ground(crate::p02_ground_capture::Kind::CombatBike) => {
            "output/combat-bike-hulk.png"
        }
        ScreenCapture::Aurora => "output/aurora.png",
        ScreenCapture::StealthFighter => "output/stealth-fighter.png",
        ScreenCapture::Microwave => "output/microwave-emitter.png",
        ScreenCapture::Ecm => "output/ecm.png",
        ScreenCapture::LightPulse => "output/lightpulse.png",
        ScreenCapture::Structure(false) => "output/structure-collapse.png",
        ScreenCapture::Structure(true) => "output/structure-topple.png",
        ScreenCapture::World(false) => "output/world-tree-after.png",
        ScreenCapture::World(true) => "output/world-bridge.png",
        ScreenCapture::BridgeDestruction => "output/bridge-destroyed.png",
        ScreenCapture::ReinforcementPad(_) => "output/reinforcement-pad.png",
        ScreenCapture::SupplyDropZone => "output/supply-drop-zone.png",
        ScreenCapture::Chinook => "output/chinook.png",
        ScreenCapture::SupplyLift => "output/supply-lift.png",
        _ => return None,
    })
}

fn primary_window([width, height]: [u32; 2], diagnostic: bool) -> Window {
    Window {
        title: "Command & Conquer Generals: Zero Hour".into(),
        // An override makes bevy_winit request the initial window in physical pixels.
        resolution: bevy::window::WindowResolution::new(width, height)
            .with_scale_factor_override(1.0),
        // macOS withholds drawable frames from fully covered windows.
        window_level: if diagnostic {
            bevy::window::WindowLevel::AlwaysOnTop
        } else {
            bevy::window::WindowLevel::Normal
        },
        ..default()
    }
}

fn restore_monitor_scale(
    mut created: MessageReader<bevy::window::WindowCreated>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    preferences: Res<crate::options_preferences::Preferences>,
) {
    let [width, height] = preferences.into_inner().applied().resolution;
    for event in created.read() {
        if let Ok(mut window) = windows.get_mut(event.window) {
            // Creation multiplies Bevy's cached size by the backend DPI even for a
            // physical request. Restore those pixels and use native DPI for input/UI.
            window.resolution.set_scale_factor_override(None);
            window.resolution.set_physical_resolution(width, height);
        }
    }
}

fn initialize_app(app: &mut App, profile: bool, diagnostic: bool) {
    let size = app
        .world()
        .resource::<crate::options_preferences::Preferences>()
        .applied()
        .resolution;
    configure_fixed_update(app);
    app.add_systems(PreUpdate, maps::reload.run_if(resource_exists::<Match>));
    app.add_systems(
        PreUpdate,
        startup::update.run_if(resource_exists::<startup::Shell>),
    );
    app.add_systems(
        First,
        game_speed_pacing
            .before(bevy::time::TimeSystems)
            .run_if(resource_exists::<Match>),
    );
    app.add_systems(
        Last,
        crate::diagnostics::frame_time.run_if(resource_exists::<Match>),
    );
    app.insert_resource(Time::<Fixed>::from_hz(30.0))
        .insert_resource(crate::model_animation::Clock::new())
        .insert_resource(ClearColor(Color::srgb(0.08, 0.10, 0.12)));
    if profile {
        crate::render_performance::install(app);
    }
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(primary_window(size, profile || diagnostic)),
        ..default()
    }))
    .add_systems(First, restore_monitor_scale)
    .init_resource::<crate::light_environment::Frame>()
    .add_plugins(crate::source_material::SourceMaterialPlugin)
    .add_plugins(crate::tree_material::TreeMaterialPlugin)
    .add_plugins(crate::particles::SmudgePlugin)
    .add_plugins(crate::water_view::WaterPlugin)
    .add_plugins(crate::weather_view::WeatherPlugin)
    .add_plugins(crate::sorted_view::SortedPlugin)
    .add_plugins(crate::model_upload::Plugin)
    .add_systems(
        PostUpdate,
        playtest_view::prepared_meshes::upload_used.before(bevy::asset::AssetEventSystems),
    )
    .add_systems(
        PostUpdate,
        (
            crate::physical_debris::transform,
            crate::light_environment::update,
            selection_feedback::tint_models,
            crate::stealth_view::render,
        )
            .chain()
            .after(bevy::transform::TransformSystems::Propagate)
            .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .run_if(resource_exists::<Match>),
    )
    .add_plugins(playtest_ui::PromotionIconPlugin)
    .add_plugins(crate::source_audio::Plugin)
    .add_systems(
        Startup,
        (
            playtest_view::setup,
            playtest_ui::setup,
            input::setup,
            waypoint_display::setup,
            crate::move_hint::setup,
            crate::attack_cursor::setup,
            crate::radius_cursor::setup,
            crate::radar::setup,
            crate::notifications::setup,
        )
            .run_if(resource_exists::<Match>),
    );
    app.add_systems(
        Startup,
        playtest_ui::shell::setup.run_if(startup::shell_active),
    );
}

fn configure_fixed_update(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        (
            playtest_ui::result_tick,
            crate::source_audio::music_tick,
            (crate::weapon_fx::record, step).chain(),
            crate::terrain::changes::update,
            crate::source_audio::burning_tick,
            crate::stealth_view::record,
            crate::source_audio::container_tick,
            crate::source_audio::research_tick,
            crate::veterancy_feedback::record,
            crate::income::record,
            crate::income::map_crates::record,
            crate::particles::view_shake::prepare,
            crate::weapon_fx::record,
            crate::laser_beam::record,
            crate::particles::step,
            crate::notifications::record,
            crate::sabotage_feedback::record,
            crate::hijacker_feedback::record,
            crate::source_audio::strategy_tick,
        )
            .chain()
            .run_if(resource_exists::<Match>),
    );
}

fn configure_app(app: &mut App) {
    app.add_systems(
        Update,
        (
            (
                playtest_ui::name_input,
                (
                    crate::map_selection::pointer_input,
                    playtest_ui::options::pointer,
                    playtest_ui::buttons,
                    playtest_ui::options::update,
                    playtest_ui::observer::right_buttons,
                    playtest_ui::observer::overlays,
                )
                    .chain(),
                playtest_ui::setup_controls,
                playtest_ui::result_keys,
                keyboard,
                input::keyboard,
                playtest_ui::command_keys,
                camera_controls,
                update_camera,
                playtest_view::sync_pass_cameras,
                crate::radar::input,
                input::pointer,
                crate::diagnostics::update,
                crate::radius_cursor::update,
                crate::attack_cursor::update,
                input::draw_region,
                waypoint_display::update,
                crate::model_animation::update_clock,
                crate::move_hint::update,
            )
                .chain(),
            (
                (
                    crate::weapon_fx::record,
                    crate::tree_buffer::clear,
                    crate::laser_beam::update_native,
                )
                    .chain(),
                crate::sway::update,
                playtest_view::render,
                (
                    crate::physical_debris::update,
                    crate::physical_debris::animation::update,
                )
                    .chain()
                    .after(crate::model_animation::update_clock)
                    .after(playtest_view::render),
                (
                    crate::aircraft_pose::update,
                    crate::model_animation::update_truck_chassis,
                )
                    .chain()
                    .after(playtest_view::render),
                crate::projectile_render::update,
                crate::projectile_stream::update,
                crate::weapon_fx::update,
                crate::weapon_fx::update_ropes,
                crate::scorch::update,
                (
                    crate::model_animation::update,
                    crate::model_animation::treads::update
                        .after(crate::model_animation::update)
                        .before(selection_feedback::tint_models),
                    crate::model_animation::update_tracks,
                    crate::particles::advance,
                )
                    .chain(),
                (
                    crate::particles::update,
                    crate::w3d_emitter::update
                        .after(crate::weapon_fx::record)
                        .after(crate::model_animation::update_clock),
                )
                    .chain(),
                crate::model_animation::attachments::update,
                crate::horde_decal::update,
                crate::model_animation::update_world,
                crate::model_animation::garrison::update,
                crate::laser_beam::update,
                crate::supply_presentation::update,
            )
                .chain(),
        )
            .chain()
            .run_if(resource_exists::<Match>),
    );
    configure_presentation(app);
    playtest_ui::shell::configure(app);
    app.add_systems(Update, playtest_view::capture);
}

fn configure_presentation(app: &mut App) {
    app.add_systems(
        Update,
        (crate::world_hover::spawn, crate::world_hover::update)
            .chain()
            .after(input::pointer)
            .run_if(resource_exists::<Match>)
            .run_if(resource_exists::<crate::world_hover::Style>),
    );
    app.add_systems(
        PostUpdate,
        crate::dynamic_lod::update.run_if(resource_exists::<Match>),
    );
    app.add_systems(
        Update,
        (
            crate::spectre_decals::update,
            crate::lockon_cursor::update,
            crate::particle_uplink::update.after(crate::model_animation::update_world),
        )
            .run_if(resource_exists::<Match>),
    );
    app.add_systems(
        Update,
        crate::scenery_clear::update
            .after(crate::model_animation::update_world)
            .run_if(resource_exists::<Match>),
    );
    app.add_systems(
        Update,
        (
            selection_feedback::update_flashes,
            crate::stealth_view::fade,
        )
            .chain()
            .after(crate::sway::update)
            .before(playtest_view::render)
            .run_if(resource_exists::<Match>),
    );
    app.add_systems(
        Update,
        (
            (
                selection_feedback::health_bars,
                crate::sticky_bomb_icons::update,
                crate::veterancy_feedback::insignia,
                crate::veterancy_feedback::animations,
                crate::income::map_crates::animations,
                crate::income::update,
            )
                .chain(),
            playtest_view::placement,
            crate::particles::orientation::update,
            (playtest_ui::observer::update, playtest_ui::pictures).chain(),
            playtest_ui::cancellation_update,
            playtest_ui::ocl_timer_update,
            playtest_ui::result_update,
            crate::notifications::update,
            playtest_ui::promotion_icons,
            playtest_ui::promotion_panel,
            playtest_ui::update_command_icons,
            playtest_ui::update,
            (
                playtest_ui::update_options,
                playtest_ui::option_labels,
                playtest_ui::option_checkbox,
                playtest_ui::option_boxes,
                playtest_ui::option_thumb,
            )
                .chain(),
            playtest_ui::promotion_transition,
            playtest_ui::appearance,
            playtest_ui::name_presentation,
            crate::map_selection::update,
            (crate::visibility_view::update, crate::tree_buffer::draw).chain(),
            (
                crate::radar::update,
                crate::radar::view_box,
                crate::radar::event_update,
            )
                .chain(),
        )
            .chain()
            .after(crate::supply_presentation::update)
            .run_if(resource_exists::<Match>),
    );
}

fn step(
    settings: (
        Res<control_bar::UiData>,
        Res<crate::options_preferences::Preferences>,
    ),
    mut battle: ResMut<Match>,
    mut mouse: ResMut<input::Mouse>,
    mut audio: ResMut<crate::source_audio::SourceAudio>,
    mut enemy_buildings: Local<Vec<usize>>,
    timings: Option<Res<crate::render_performance::Timings>>,
    mut slow_tick: Local<Option<std::time::Instant>>,
) {
    let (data, preferences) = settings;
    let data = data.into_inner();
    let preferences = preferences.into_inner();
    let timings = timings.map(Res::into_inner);
    let _span = timings.map(|timings| timings.span("fixed_step"));
    if battle.started && !battle.paused {
        let started = std::time::Instant::now();
        battle
            .simulation
            .set_retaliation(0, preferences.applied().retaliation);
        input::commands::step(&mut battle, &mut mouse);
        battle.apply_bike_selections();
        input::apply_spectre_selections(&mut battle);
        audio.map_script_actions(&battle.simulation.take_map_script_audio());
        crate::diagnostics::simulation_time(&battle.simulation, started.elapsed(), &mut slow_tick);
        mouse.refresh_idle_workers(&battle.simulation, battle.generation);
        log_elimination(&battle.simulation, &mut enemy_buildings);
        deselect_sniped_vehicle(&mut battle);
    }
    let battle = battle.into_inner();
    battle.selected.retain(|&id| {
        battle.simulation.entities.get(id).is_some_and(|unit| {
            unit.selectable(&battle.simulation.catalog[unit.type_index])
                && (!data.units[unit.type_index].structure_inventory
                    || unit.owner == 0
                    || unit.owner == battle.simulation.neutral_owner())
        })
    });
    if battle.selected.is_empty() {
        battle.placing = None;
        mouse.pending_command = input::CommandMode::Normal;
    }
}

fn deselect_sniped_vehicle(battle: &mut Match) {
    battle.selected.retain(|id| {
        !battle
            .simulation
            .presentation
            .pilot_events
            .iter()
            .any(|event| event.object == *id)
    });
}

fn log_elimination(game: &Skirmish, previous: &mut Vec<usize>) {
    let remaining: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter_map(|(id, entity)| {
            (game.enemies(0, entity.owner)
                && entity.alive()
                && game.catalog[entity.type_index].scoring.victory)
                .then_some(id)
        })
        .collect();
    if game.tick > 1 && previous.iter().any(|id| !remaining.contains(id)) {
        eprintln!(
            "Enemy victory structures at frame {}: {} remaining; result {:?}",
            game.tick,
            remaining.len(),
            game.outcome
        );
        for &id in &remaining {
            let entity = &game.entities[id];
            eprintln!(
                "  {} at {:?}, construction {:.1}%, health {:.1}",
                game.catalog[entity.type_index].name,
                entity.position,
                entity.build_progress * 100.0,
                entity.health
            );
        }
    }
    *previous = remaining;
}

fn keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut battle: ResMut<Match>,
    mut menu: ResMut<playtest_ui::Menu>,
    mut mouse: ResMut<input::Mouse>,
    promotions: Res<playtest_ui::PromotionTransition>,
) {
    let keys = keys.into_inner();
    let promotions = promotions.into_inner();
    if menu.options_open {
        return;
    }
    if battle.simulation.end_game_at.is_some() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && battle.promotions_open && promotions.visible() {
        battle.promotions_open = false;
        return;
    }
    if keys.just_pressed(KeyCode::Escape)
        && !matches!(mouse.pending_command, input::CommandMode::Normal)
    {
        mouse.pending_command = input::CommandMode::Normal;
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && battle.placing.take().is_none() && battle.started {
        menu.clear_confirmation();
        battle.paused = !battle.paused;
    }
    if !battle.started || battle.paused {
        return;
    }
    // The installed STOP meta mapping is S/DOWN/NONE. Its separate DOWN event
    // remains native behavior even when the selected Stop button also binds UP.
    if keys.just_pressed(KeyCode::KeyS)
        && !keys.any_pressed([
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
        ])
    {
        let battle = battle.into_inner();
        for &id in &battle.selected {
            if battle
                .simulation
                .entities
                .get(id)
                .is_none_or(|unit| unit.owner != 0)
            {
                continue;
            }
            if let Err(error) = battle.simulation.stop(id) {
                eprintln!("Stop command rejected: {error}");
            }
        }
    }
}

pub fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    mouse: Res<crate::input::Mouse>,
    battle: Res<Match>,
    mut rig: ResMut<CameraRig>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    let keys = keys.into_inner();
    let scroll = scroll.into_inner();
    let time = time.into_inner();
    let mouse = mouse.into_inner();
    let battle = battle.into_inner();
    let window = window.into_inner();
    if rig.generation != battle.generation
        || (battle.started
            && !battle.paused
            && battle.simulation.end_game_at.is_none()
            && keys.just_pressed(KeyCode::KeyH)
            && !keys.any_pressed([
                KeyCode::ControlLeft,
                KeyCode::ControlRight,
                KeyCode::ShiftLeft,
                KeyCode::ShiftRight,
                KeyCode::AltLeft,
                KeyCode::AltRight,
            ]))
    {
        if let Some(base) = battle
            .simulation
            .entities
            .iter()
            .find(|unit| unit.owner == 0)
        {
            rig.focus = Vec2::from_array(base.position) * 0.1;
        }
        if rig.generation != battle.generation {
            rig.generation = battle.generation;
            rig.distance = rig.settings.max_height;
        }
    }
    if !battle.started || battle.paused || battle.simulation.end_game_at.is_some() || mouse.dragging
    {
        return;
    }
    let mut direction = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }
    if let Some(cursor) = window.cursor_position() {
        if cursor.x < 3.0 {
            direction.x -= 1.0;
        }
        if cursor.x >= window.width() - 3.0 {
            direction.x += 1.0;
        }
        if cursor.y < 3.0 {
            direction.y += 1.0;
        }
        if cursor.y >= window.height() - 3.0 {
            direction.y -= 1.0;
        }
    }
    // LookAtTranslator's 200-unit key/edge amount and 30 Hz frame ratio,
    // followed by W3DView::scrollBy's 250-unit device-to-view conversion.
    let frame_ratio = 30.0 * time.delta_secs();
    let device_scroll = 200.0 * rig.settings.scroll_factor * frame_ratio;
    let view_scale = 2.0 * (50.0_f32.to_radians() * 0.5).tan() / window.width().max(1.0);
    let speed = device_scroll * 250.0 * view_scale * 0.1;
    let screen_delta =
        direction * Vec2::new(rig.settings.horizontal_scroll, rig.settings.vertical_scroll) * speed;
    let rotation = Mat2::from_angle(rig.settings.yaw);
    rig.focus += rotation * screen_delta;
    rig.distance = scroll
        .delta
        .y
        .mul_add(-1.0, rig.distance)
        .clamp(rig.settings.min_height, rig.settings.max_height);
}

pub fn update_camera(
    terrain: Res<Terrain>,
    particles: Res<crate::particles::Particles>,
    rig: ResMut<CameraRig>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<
        (&mut Transform, &mut Projection, &mut Camera),
        With<playtest_view::WorldCamera>,
    >,
    mut audio: ResMut<crate::source_audio::SourceAudio>,
) {
    let particles = particles.into_inner();
    let terrain = terrain.into_inner();
    let rig = rig.into_inner();
    let window = window.into_inner();
    let (mut transform, mut projection, mut camera) = camera.into_inner();
    // W3DView::setWidth/setHeight use 50 degrees horizontally and the original
    // control-bar viewport scale. Bevy's perspective field is vertical.
    let viewport_size = (window.resolution.physical_size().as_vec2()
        * Vec2::new(1.0, rig.settings.viewport_height))
    .as_uvec2();
    camera.viewport = Some(bevy::camera::Viewport {
        physical_position: UVec2::ZERO,
        physical_size: viewport_size,
        depth: 0.0..1.0,
    });
    if viewport_size.x == 0 || viewport_size.y == 0 {
        return;
    }
    let aspect = window.width() / (window.height() * rig.settings.viewport_height);
    let half_fov = ((50.0_f32.to_radians() * 0.5).tan() / aspect).atan();
    if let Projection::Perspective(projection) = &mut *projection {
        projection.fov = 2.0 * half_fov;
        projection.near = 1.0;
        projection.far = 1000.0;
    }
    // W3DView::buildCameraPosition, normal user camera, then source [x,z,-y].
    let horizontal = rig.distance / rig.settings.pitch.tan();
    // W3DView::calcCameraAreaOffset: intersect the centre and bottom-centre
    // rays with the focus-height plane, then apply the SuperHackers 0.85 factor.
    let bottom_distance = rig.distance / (rig.settings.pitch + half_fov).tan();
    let extent = Vec2::new(f32::from(terrain.width), f32::from(terrain.height));
    let margin = ((horizontal - bottom_distance).abs() * 0.85)
        .min(extent.x * 0.5)
        .min(extent.y * 0.5);
    let lower = Vec2::splat(margin);
    let upper = extent - lower;
    if rig.focus.cmplt(lower).any() || rig.focus.cmpgt(upper).any() {
        // Source's 0.1 world-unit epsilon, converted to rendered units.
        let minimum = (lower + Vec2::splat(0.01)).min(extent * 0.5);
        let maximum = (upper - Vec2::splat(0.01)).max(extent * 0.5);
        rig.focus = rig.focus.clamp(minimum, maximum);
    }
    let (sine, cosine) = rig.settings.yaw.sin_cos();
    let shaken = rig.focus + particles.camera_shake();
    let focus = Vec3::new(shaken.x, terrain.sample(rig.focus), -shaken.y);
    let offset = Vec3::new(horizontal * sine, rig.distance, horizontal * cosine);
    transform.set_if_neq(Transform::from_translation(focus + offset).looking_at(focus, Vec3::Y));
    audio.set_listener(focus, transform.translation);
}

pub const MAP_NAME: &str = "Maps\\Tournament Desert\\Tournament Desert.map";

pub const FOUR_PLAYER_MAP: &str = "Maps\\Fallen Empire\\Fallen Empire.map";

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[ignore = "requires original game installation"]
    #[test]
    fn display_preference_uses_physical_pixels_and_native_cursor_scale() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let preferences = crate::options_preferences::Preferences::load(
            &assets,
            &root.join("UserData/Options.ini"),
        )
        .unwrap();
        let size = UVec2::from_array(preferences.applied().resolution);
        for factor in [1.0, 2.0] {
            let mut app = App::new();
            app.insert_resource(preferences.clone())
                .add_message::<bevy::window::WindowCreated>()
                .add_systems(First, restore_monitor_scale);
            let mut window = primary_window(size.to_array(), false);
            // bevy_winit passes an explicit physical inner size only with an override.
            assert_eq!(
                window.resolution.scale_factor_override().map(f32::to_bits),
                Some(1.0_f32.to_bits())
            );
            assert_eq!(window.physical_size(), size);
            window
                .resolution
                .set_scale_factor_and_apply_to_physical_size(factor);
            let id = app.world_mut().spawn((window, PrimaryWindow)).id();
            app.world_mut()
                .write_message(bevy::window::WindowCreated { window: id });
            app.update();
            let mut window = app.world_mut().get_mut::<Window>(id).unwrap();
            assert_eq!(window.physical_size(), size);
            assert!(window.resolution.scale_factor_override().is_none());
            assert_eq!(window.scale_factor().to_bits(), factor.to_bits());
            assert_eq!(window.size(), size.as_vec2() / factor);
            let cursor = size.as_dvec2() * 0.5;
            window.set_physical_cursor_position(Some(cursor));
            assert_eq!(window.cursor_position(), Some(cursor.as_vec2() / factor));
            // A later Options change or user resize survives subsequent updates.
            window.resolution.set_physical_resolution(1024, 768);
            app.update();
            assert_eq!(
                app.world().get::<Window>(id).unwrap().physical_size(),
                UVec2::new(1024, 768)
            );
        }
    }

    #[test]
    fn native_game_speed_changes_pacing_without_catchup_bursts() {
        for (speed, frames, expected) in
            [(15, 120, 15), (30, 120, 30), (60, 120, 60), (61, 120, 120)]
        {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(Time::<Fixed>::from_hz(30.0))
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
                    8_333_334,
                )));
            let mut fixed = app.world_mut().remove_resource::<Time<Fixed>>().unwrap();
            let mut virtual_time = app.world_mut().remove_resource::<Time<Virtual>>().unwrap();
            set_game_speed(&mut fixed, &mut virtual_time, speed);
            let timestep = fixed.timestep();
            app.insert_resource(fixed).insert_resource(virtual_time);
            app.update();
            for _ in 0..frames {
                app.update();
            }
            assert_eq!(
                app.world().resource::<Time<Fixed>>().elapsed(),
                timestep * expected
            );
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn aircraft_crash_capture_starts_the_match_before_preparing_paid_actors() {
        let (_, battle) = crate::cleanup_verification::tests::installed();
        let battle = Match::new(battle.simulation, &ScreenCapture::AircraftCrash);
        assert!(battle.started);
        assert_eq!(battle.generation, 1);
        assert!(!battle.simulation.entities.is_empty());
    }

    #[test]
    fn slow_rendering_cannot_age_a_native_one_frame_effect_before_drawing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Time::<Fixed>::from_hz(30.0))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)));
        configure_frame_pacing(&mut app);
        app.update();
        let timestep = app.world().resource::<Time<Fixed>>().timestep();
        for frame in 1..=12 {
            app.update();
            assert_eq!(
                app.world().resource::<Time<Fixed>>().elapsed(),
                timestep * frame
            );
        }
        app.insert_resource(TimeUpdateStrategy::ManualDuration(timestep / 3));
        for _ in 0..6 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<Time<Fixed>>().elapsed(),
            timestep * 14
        );
    }
}

#[cfg(test)]
mod crawler_tests;
