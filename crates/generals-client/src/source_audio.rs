//! Native audio event selection, limits and volumes over original decoded samples.
use std::{
    collections::BTreeMap,
    error::Error,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::{
    audio::{AddAudioSource, Decodable},
    ecs::{schedule::ScheduleConfigs, system::ScheduleSystem},
    prelude::*,
    transform::TransformSystems,
};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::{Skirmish, UnitType};
use rodio::buffer::SamplesBuffer;

use crate::{input::Mouse, playtest::Match};

mod aircraft;
mod capture;
mod car_bomb;
pub mod cleanup_verification;
mod contain;
mod data;
mod drones;
mod fire;
mod firing;
mod hacker;
mod hazards;
mod looping;
mod lotus;
pub use lotus::Inspection as LotusAudioInspection;
mod map_scripts;
mod music;
mod music_scripts;
mod object_fx;
mod object_sounds;
mod pilot;
mod playback;
mod radar;
mod random;
mod research;
mod sabotage;
mod salvage;
mod strategy;
mod verification;
pub use capture::CaptureAudioInspection;
pub use contain::record as container_tick;
pub use drones::repair_sound;
pub use fire::tick as burning_tick;
pub use firing::FiringAudioInspection;
pub use hacker::HackerAudioInspection;
pub use music::{RetiringMusic, ScoreMusic, finish_load_screen, tick as music_tick};
pub use music_scripts::verify as verify_result_scripts;
pub use research::record as research_tick;
pub use strategy::{StrategyAudioInspection, record as strategy_tick};
pub use verification::enable_verification;

pub struct Plugin;

pub fn retire_map_players(world: &mut World) {
    let players: Vec<_> = world
        .query_filtered::<Entity, (
            Without<RetiringMusic>,
            Or<(With<AudioPlayer<Sequence>>, With<AudioPlayer>)>,
        )>()
        .iter(world)
        .collect();
    for entity in players {
        world.despawn(entity);
    }
}

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Sequence>()
            .add_systems(FixedFirst, playback::reset_before_logic)
            .add_systems(
                PostUpdate,
                before_audio_backend((playback::update, music::update)),
            )
            .add_systems(
                PostUpdate,
                before_audio_backend(music::fade_retiring.after(music::update)),
            )
            .add_systems(Last, playback::retire_failed_audio);
    }
}

fn before_audio_backend<M>(
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> ScheduleConfigs<ScheduleSystem> {
    // Bevy attaches deferred AudioSinks after transform propagation. Retiring or
    // replacing a pending player must finish before that backend reads it.
    systems.before(TransformSystems::Propagate)
}

#[derive(Clone, Copy)]
pub enum Voice {
    Select,
    Move,
    Attack,
    Guard,
    Supply,
    Build,
    Repair,
    HackInternet,
    Enter,
    Unload,
    PrimaryWeaponMode,
    SecondaryWeaponMode,
    LaserMode,
    LaserAttack,
    Salvage,
    AttackAir,
    CleanupMode,
    CleanupArea,
    EnterHostile,
    ContaminateMode,
    PoisonLocation,
    FlameMode,
    FlameLocation,
    RocketPodMode,
    LotusCaptureMode,
    LotusVehicleMode,
    LotusCashMode,
    SnipeMode,
    SnipePilot,
    Disarm,
    SpecialPower(usize),
    CombatDrop,
}

impl Voice {
    const fn index(self) -> usize {
        match self {
            Self::Select => 0,
            Self::Move => 1,
            Self::Attack => 2,
            Self::Guard => 3,
            Self::Supply => 4,
            Self::Build => 5,
            Self::Repair => 6,
            Self::HackInternet => 7,
            Self::Enter => 8,
            Self::Unload => 9,
            Self::PrimaryWeaponMode => 10,
            Self::SecondaryWeaponMode => 11,
            Self::LaserMode => 12,
            Self::LaserAttack => 13,
            Self::Salvage => 14,
            Self::AttackAir => 15,
            Self::CleanupMode => 16,
            Self::CleanupArea => 17,
            Self::EnterHostile => 18,
            Self::ContaminateMode => 19,
            Self::PoisonLocation => 20,
            Self::FlameMode => 21,
            Self::FlameLocation => 22,
            Self::RocketPodMode => 23,
            Self::LotusCaptureMode => 24,
            Self::LotusVehicleMode => 25,
            Self::LotusCashMode => 26,
            Self::SnipeMode => 27,
            Self::SnipePilot => 28,
            Self::Disarm => 29,
            Self::SpecialPower(module) => data::VOICE_KEYS.len() + module,
            Self::CombatDrop => 30,
        }
    }
}

pub struct VoiceRequest {
    voice: Voice,
    candidates: Vec<(usize, usize)>,
    car_bomb: bool,
}

impl VoiceRequest {
    pub const fn voice(&self) -> &Voice {
        &self.voice
    }

    /// `CommandXlat` picks the first selected object with the requested special power.
    pub fn special_power(game: &Skirmish, ids: &[usize], power_types: &[&str]) -> Option<Self> {
        ids.iter().find_map(|&id| {
            let actor = game.entities.get(id)?;
            let unit = &game.catalog[actor.type_index];
            if !actor.alive() || actor.owner != 0 || unit.progression.ignored_in_gui {
                return None;
            }
            let module = unit
                .special_powers
                .iter()
                .position(|power| power_types.contains(&power.power_type.as_str()))?;
            Some(Self {
                voice: Voice::SpecialPower(module),
                candidates: vec![(id, actor.type_index)],
                car_bomb: actor.car_bomb(),
            })
        })
    }

    pub fn new(game: &Skirmish, ids: &[usize], voice: Voice, waypoint_mode: bool) -> Self {
        let disarmer = if matches!(voice, Voice::Attack | Voice::AttackAir) {
            ids.iter().copied().find(|&id| {
                game.entities.get(id).is_some_and(|actor| {
                    actor.alive()
                        && actor.owner == 0
                        && matches!(
                            actor
                                .active_weapon(&game.catalog[actor.type_index])
                                .damage_type,
                            generals_sim::skirmish::DamageType::Disarm
                        )
                })
            })
        } else {
            None
        };
        // CommandXlat's specialty response overrides the normal attack voice
        // with the first selected DISARM weapon's UnitSpecificSounds binding.
        let voice = if disarmer.is_some() {
            Voice::Disarm
        } else {
            voice
        };
        let candidates: Vec<_> = ids
            .iter()
            .filter_map(|&id| {
                let object = game.entities.get(id)?;
                let unit = &game.catalog[object.type_index];
                if !object.alive()
                    || object.owner != 0
                    || unit.progression.ignored_in_gui
                    || disarmer.is_some_and(|selected| selected != id)
                    || (matches!(voice, Voice::CleanupArea) && unit.behavior.cleanup.is_empty())
                    || (matches!(voice, Voice::ContaminateMode | Voice::PoisonLocation)
                        && unit.behavior.contamination.is_empty())
                    || (matches!(voice, Voice::FlameMode | Voice::FlameLocation)
                        && unit.name != "ChinaTankDragon")
                    || (matches!(voice, Voice::RocketPodMode)
                        && unit.name != "AmericaVehicleComanche")
                    || (matches!(voice, Voice::Move | Voice::Salvage)
                        && (unit.kind.is_building() || (waypoint_mode && object.moving())))
                {
                    return None;
                }
                Some((id, object.type_index))
            })
            .collect();
        let car_bomb = candidates
            .first()
            .is_some_and(|&(id, _)| game.entities[id].car_bomb());
        Self {
            voice,
            candidates,
            car_bomb,
        }
    }
}

struct Binding {
    name: String,
    previous: Option<usize>,
}
struct UnitAudio {
    name: String,
    voices: Vec<Binding>,
    weapon: Vec<String>,
    weapon_loops: Vec<u32>,
    rapid_fire: String,
    hacker: [String; 3],
    capture: [String; 2],
    lotus: Vec<[String; 6]>,
    stealth: [String; 2],
    detectors: Vec<[String; 2]>,
    contain: [String; 5],
    strategy: Option<strategy::Bindings>,
}

struct Request {
    uninterruptible: bool,
    firing: Option<Arc<()>>,
    event: String,
    object: Option<usize>,
    projectile: Option<u64>,
    position: Option<Vec3>,
    samples: Vec<SamplesBuffer>,
    looping: Option<looping::Template>,
    sample_names: Vec<String>,
    pitch: f32,
    volume: f32,
    volume_shift: f32,
    delay_ms: f32,
}

#[derive(Resource)]
pub struct SourceAudio {
    speech_disallowed: bool,
    events: BTreeMap<String, data::Event>,
    samples: BTreeMap<String, Option<SamplesBuffer>>,
    units: Vec<UnitAudio>,
    settings: data::Settings,
    random: Arc<Mutex<random::Random>>,
    aircraft: aircraft::State,
    firing: firing::State,
    hazards: hazards::State,
    fires: fire::State,
    capture: capture::State,
    radar: radar::State,
    sabotage: sabotage::State,
    salvage: [String; 3],
    car_bomb: [String; 3],
    pilot: String,
    object_fx: BTreeMap<String, Vec<String>>,
    object_sounds: object_sounds::State,
    research: Vec<Vec<research::Bindings>>,
    requests: Vec<Request>,
    volume_changes: Vec<(String, f32)>,
    listener: Vec3,
    zoom_volume: f32,
    generation: u32,
    serial: u64,
    music: music::Music,
}

pub fn load(
    assets: &VirtualAssets,
    catalog: &[UnitType],
    options: &Path,
) -> Result<SourceAudio, Box<dyn Error>> {
    let settings = data::settings(assets, options)?;
    let all_events = data::definitions(assets)?;
    let units = data::units(assets, catalog)?;
    let aircraft = aircraft::State::load(assets, catalog)?;
    let hacker = hacker::State::load(assets)?;
    let capture = capture::State::load(assets)?;
    let radar = radar::State::load(assets)?;
    let fires = fire::State::load(assets, catalog)?;
    let sabotage = sabotage::State::load(assets, options)?;
    let salvage = salvage::sounds(assets)?;
    let car_bomb = car_bomb::sounds(assets)?;
    let pilot = pilot::sound(assets)?;
    let repair = repair_sound(assets)?;
    let object_fx = crate::particles::object_fx_sounds(assets, catalog)?;
    let weapon_fx = crate::particles::weapon_fx_sounds(assets, catalog)?;
    let research = research::load(assets, catalog)?;
    let music = music::load(assets, options)?;
    data::validate_speech_subtitles(assets, music.speech_names())?;
    let names = units
        .iter()
        .flat_map(UnitAudio::names)
        .chain([
            "GUIClick",
            "GUIClickDisabled",
            "GUIMessageReceived",
            "ExplosionRocketBuggyMissile",
            "ExplosionFlashBang",
        ])
        .chain(aircraft.names())
        .chain(music.speech_names())
        .chain(hacker.sounds.iter().map(String::as_str))
        .chain(capture.sounds.iter().map(String::as_str))
        .chain(radar.sounds.iter().map(String::as_str))
        .chain(sabotage.names())
        .chain(salvage.iter().map(String::as_str))
        .chain(car_bomb.iter().map(String::as_str))
        .chain(std::iter::once(pilot.as_str()))
        .chain(object_fx.values().flatten().map(String::as_str))
        .chain(weapon_fx.iter().map(String::as_str))
        .chain(object_sounds::names(catalog))
        .chain(std::iter::once(repair.as_str()))
        .chain(
            research
                .iter()
                .flatten()
                .flat_map(|binding| binding.names.iter().map(String::as_str)),
        )
        .chain(hazards::names(catalog))
        .chain(fires.names(catalog));
    let mut events = BTreeMap::new();
    let mut samples = BTreeMap::new();
    for name in names {
        if name.is_empty() || name.eq_ignore_ascii_case("NoSound") || events.contains_key(name) {
            continue;
        }
        let event = all_events
            .get(name)
            .ok_or_else(|| format!("Missing original AudioEvent {name}"))?;
        data::load_event(assets, &settings, event, &mut samples)?;
        events.insert(name.to_owned(), event.clone());
    }
    validate_events(&units, &events, &aircraft, catalog, &fires)?;
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let seed = u32::try_from(seconds & u64::from(u32::MAX))?;
    eprintln!(
        "Original audio: {} events, {} decoded samples",
        events.len(),
        samples.values().flatten().count()
    );
    Ok(SourceAudio {
        speech_disallowed: false,
        events,
        samples,
        units,
        settings,
        random: Arc::new(Mutex::new(random::Random::new(seed))),
        aircraft,
        firing: firing::State::new(),
        hazards: hazards::State::new(),
        fires,
        capture,
        radar,
        sabotage,
        salvage,
        car_bomb,
        pilot,
        object_fx,
        object_sounds: object_sounds::State::new(),
        research,
        requests: Vec::new(),
        volume_changes: Vec::new(),
        listener: Vec3::ZERO,
        zoom_volume: 1.0,
        generation: 0,
        serial: 0,
        music,
    })
}

/// Original shell clicks and its single music track, without a gameplay roster.
pub fn load_shell(assets: &VirtualAssets, options: &Path) -> Result<SourceAudio, Box<dyn Error>> {
    let settings = data::settings(assets, options)?;
    let definitions = data::shell_definitions(assets)?;
    let mut events = BTreeMap::new();
    let mut samples = BTreeMap::new();
    for name in ["GUIClick", "GUIClickDisabled"] {
        let event = definitions
            .get(name)
            .ok_or_else(|| format!("Missing original AudioEvent {name}"))?;
        data::load_event(assets, &settings, event, &mut samples)?;
        events.insert(name.to_owned(), event.clone());
    }
    eprintln!(
        "Original shell audio: {} events, {} decoded samples",
        events.len(),
        samples.values().flatten().count()
    );
    // Shell::showShellMap calls InitRandom(0), including the separate audio stream.
    Ok(SourceAudio {
        speech_disallowed: false,
        events,
        samples,
        units: Vec::new(),
        settings,
        random: Arc::new(Mutex::new(random::Random::new(0))),
        aircraft: aircraft::State::new(),
        firing: firing::State::new(),
        hazards: hazards::State::new(),
        fires: fire::State::new(),
        capture: capture::State::new(std::array::from_fn(|_| String::new())),
        radar: radar::State::new(std::array::from_fn(|_| String::new())),
        sabotage: sabotage::State::load(assets, options)?,
        salvage: std::array::from_fn(|_| String::new()),
        car_bomb: std::array::from_fn(|_| String::new()),
        pilot: String::new(),
        object_fx: BTreeMap::new(),
        object_sounds: object_sounds::State::new(),
        research: Vec::new(),
        requests: Vec::new(),
        volume_changes: Vec::new(),
        listener: Vec3::ZERO,
        zoom_volume: 1.0,
        generation: 0,
        serial: 0,
        music: music::load_shell(assets, options)?,
    })
}

#[cfg(test)]
pub mod shell_tests;

fn validate_events(
    units: &[UnitAudio],
    events: &BTreeMap<String, data::Event>,
    aircraft: &aircraft::State,
    catalog: &[UnitType],
    fires: &fire::State,
) -> Result<(), Box<dyn Error>> {
    for bindings in units.iter().filter_map(|unit| unit.strategy.as_ref()) {
        strategy::validate(bindings, events)?;
    }
    aircraft.validate(events)?;
    hazards::validate(catalog, events)?;
    fire::validate(catalog, events)?;
    fires.validate_projectiles(events)?;
    Ok(())
}

impl UnitAudio {
    fn names(&self) -> impl Iterator<Item = &str> {
        self.voices
            .iter()
            .map(|binding| binding.name.as_str())
            .chain(self.weapon.iter().map(String::as_str))
            .chain(std::iter::once(self.rapid_fire.as_str()))
            .chain(self.hacker.iter().map(String::as_str))
            .chain(self.capture.iter().map(String::as_str))
            .chain(self.lotus.iter().flatten().map(String::as_str))
            .chain(self.stealth.iter().map(String::as_str))
            .chain(self.detectors.iter().flatten().map(String::as_str))
            .chain(self.contain.iter().map(String::as_str))
            .chain(
                self.strategy
                    .iter()
                    .flat_map(|bindings| bindings.names.iter().map(String::as_str)),
            )
    }
}

impl SourceAudio {
    pub const fn apply_volumes(&mut self, music: f32, sound: f32, world: f32, voice: f32) {
        self.settings.sound_volume = sound;
        self.settings.world_volume = world;
        self.settings.speech_volume = voice;
        self.music.set_volume(music);
    }
    pub fn speech(&mut self, name: &str, allow_overlap: bool) {
        let _ = self.queue(name, None, None, None, !allow_overlap);
    }

    pub fn ui(&mut self, name: &str) {
        let _ = self.queue(name, None, None, None, false);
    }

    pub fn fx(&mut self, name: &str, position: [f32; 3]) {
        let point = Vec3::new(position[0], position[2], -position[1]);
        let _ = self.queue(name, None, Some(point), None, false);
    }

    pub fn hacker(
        &mut self,
        type_index: usize,
        object_id: usize,
        position: Vec3,
        effect: &generals_sim::skirmish::income::Effect,
    ) {
        let index = match effect {
            generals_sim::skirmish::income::Effect::Unpack => 0,
            generals_sim::skirmish::income::Effect::Pack => 1,
            generals_sim::skirmish::income::Effect::Cash { .. } => 2,
        };
        let name = self.units[type_index].hacker[index].clone();
        let _ = self.queue(&name, Some(object_id), Some(position * 10.0), None, false);
    }

    fn voice(&mut self, request: &VoiceRequest) {
        // CommandXlat::pickAndPlayUnitVoiceResponse retains first selected voice.
        let Some(&(object, unit)) = request.candidates.first() else {
            return;
        };
        let index = if matches!(request.voice, Voice::Salvage)
            && self
                .events
                .get(&self.units[unit].voices[request.voice.index()].name)
                .is_none_or(|event| event.sounds.is_empty())
        {
            Voice::Move.index()
        } else {
            request.voice.index()
        };
        let binding = &self.units[unit].voices[index];
        let name = binding.name.clone();
        let previous = binding.previous;
        // ControlBar button sounds carry the local player but no object ID.
        let object = (!matches!(
            request.voice,
            Voice::CleanupMode
                | Voice::ContaminateMode
                | Voice::FlameMode
                | Voice::RocketPodMode
                | Voice::LotusCaptureMode
                | Voice::LotusVehicleMode
                | Voice::LotusCashMode
        ))
        .then_some(object);
        self.units[unit].voices[index].previous = self.queue(&name, object, None, previous, false);
        if request.car_bomb
            && let Some(object) = object
        {
            self.car_bomb_voice(request.voice, object);
        }
    }

    fn queue(
        &mut self,
        name: &str,
        object: Option<usize>,
        position: Option<Vec3>,
        previous: Option<usize>,
        uninterruptible: bool,
    ) -> Option<usize> {
        let event = self.events.get(name)?;
        if event.speech && self.speech_disallowed {
            return previous;
        }
        if event.sounds.is_empty() {
            return previous;
        }
        let mut random = self.random.lock().expect("audio RNG lock");
        let (index, delay, pitch, volume_shift) = play_info(event, &mut random, previous);
        let volume = event.volume_override.unwrap_or(event.volume) * volume_shift;
        let mut samples = Vec::new();
        let mut sample_names = Vec::new();
        for (section, list) in [&event.attack, &event.sounds, &event.decay]
            .into_iter()
            .enumerate()
        {
            if list.is_empty() {
                continue;
            }
            let picked = if section == 1 {
                index
            } else {
                random.index(list.len())
            };
            let sample = self.samples.get(&list[picked]).and_then(Option::as_ref);
            // Miles opens only the current portion. Later loop portions can be
            // absent: their completion callback fails that file and tries decay.
            let first = usize::from(event.attack.is_empty());
            if sample.is_none() && (event.control & data::LOOP == 0 || section == first) {
                return Some(index);
            }
            if let Some(sample) = sample {
                samples.push(sample.clone());
            }
            sample_names.push(list[picked].clone());
        }
        drop(random);
        self.requests.push(Request {
            uninterruptible,
            firing: None,
            event: name.to_owned(),
            object,
            projectile: None,
            position,
            looping: hacker::loop_template(event, &self.samples, index, self.random.clone()),
            samples,
            sample_names,
            pitch,
            volume,
            volume_shift,
            delay_ms: delay,
        });
        Some(index)
    }

    // GameAudio::update microphone placement, including native zoom attenuation.
    pub fn set_listener(&mut self, pivot: Vec3, camera: Vec3) {
        let pivot = pivot * 10.0;
        let camera = camera * 10.0;
        let delta = camera - pivot;
        let fraction = if delta.y <= self.settings.microphone_height || delta.y <= 0.0 {
            self.settings.microphone_fraction
        } else {
            self.settings
                .microphone_fraction
                .min(self.settings.microphone_height / delta.y)
        };
        self.listener = pivot + delta * fraction;
        let distance = camera.distance(self.listener);
        let [minimum, maximum, amount] = self.settings.zoom;
        self.zoom_volume = if amount > 0.0 && distance < minimum {
            1.0
        } else if amount > 0.0 && distance < maximum {
            let reduction = (distance - minimum) / (maximum - minimum) * amount;
            1.0 - reduction
        } else {
            1.0 - amount
        };
    }

    fn volume(&self, event: &data::Event, base: f32, position: Option<Vec3>) -> f32 {
        if event.speech {
            return base * self.settings.speech_volume;
        }
        if event.world
            && let Some(position) = position
        {
            let distance = self.listener.distance(position);
            if distance >= event.max_range {
                return 0.0;
            }
            let mut volume = base * self.settings.world_volume * self.zoom_volume;
            if self.settings.fade && distance > event.min_range {
                let attenuation = ((distance - event.min_range)
                    / (event.max_range - event.min_range))
                    .powf(self.settings.exponent);
                volume *= 1.0 - attenuation;
            }
            volume
        } else {
            base * self.settings.sound_volume
        }
    }
}

// AudioEventRTS::generateFilename followed by generatePlayInfo. Streaming files
// bypass filename choice/delay, consume pitch/volume draws, and play at source rate.
fn play_info(
    event: &data::Event,
    random: &mut random::Random,
    previous: Option<usize>,
) -> (usize, f32, f32, f32) {
    let index = if event.speech {
        0
    } else {
        random::choice(
            random,
            event.sounds.len(),
            previous,
            event.control & data::RANDOM != 0,
        )
    };
    let delay = if event.speech {
        0.0
    } else {
        random.real(event.delay[0], event.delay[1])
    };
    let pitch = random.real(event.pitch[0], event.pitch[1]);
    let volume_shift = random.real(1.0 + event.volume_shift, 1.0);
    (
        index,
        delay,
        if event.speech { 1.0 } else { pitch },
        volume_shift,
    )
}

#[cfg(test)]
mod speech_tests;

#[cfg(test)]
mod disarm_tests;
#[cfg(test)]
mod fx_dependency_tests;
#[cfg(test)]
mod helix_tests;

#[derive(Asset, TypePath)]
struct Sequence {
    samples: Vec<SamplesBuffer>,
    looping: Option<looping::Template>,
}

impl Decodable for Sequence {
    type Decoder = looping::Decoder;
    fn decoder(&self) -> Self::Decoder {
        self.looping.as_ref().map_or_else(
            || looping::Decoder::Once(rodio::source::from_iter(self.samples.clone())),
            looping::Template::decoder,
        )
    }
}

#[cfg(test)]
mod lifecycle_tests;

#[cfg(test)]
mod options_tests;
