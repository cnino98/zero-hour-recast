use std::{collections::BTreeMap, error::Error, io::Cursor, path::Path};

use generals_assets::{
    AssetError,
    content::{self, ContentFile, ContentLine},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::UnitType;
use rodio::{Source, buffer::SamplesBuffer};

use crate::skirmish_data::{blocks, definition, field, finite_number, scalar};

use super::{Binding, UnitAudio};

pub const LOOP: u8 = 1;
pub const ALL: u8 = 4;
pub const RANDOM: u8 = 2;
pub const INTERRUPT: u8 = 16;
pub const ST_SHROUDED: u8 = 4;
pub const ST_PLAYER: u8 = 32;
// GameAudio::shouldPlayLocally rejects positional sounds with no player when
// an audience restriction requires one; PLAYER+UI explicitly permits no owner.
pub const ST_REQUIRES_OWNER: u8 = 64;

#[derive(Clone)]
pub struct Event {
    pub speech: bool,
    pub name: String,
    pub sounds: Vec<String>,
    pub attack: Vec<String>,
    pub decay: Vec<String>,
    pub volume: f32,
    pub volume_override: Option<f32>,
    pub volume_shift: f32,
    pub pitch: [f32; 2],
    pub delay: [f32; 2],
    pub limit: usize,
    pub priority: u8,
    pub world: bool,
    pub voice: bool,
    pub restrictions: u8,
    pub control: u8,
    pub loop_count: usize,
    pub unsupported_controls: Vec<String>,
    pub min_range: f32,
    pub max_range: f32,
}

pub struct Settings {
    pub speech_volume: f32,
    pub streaming_folder: String,
    pub sound_volume: f32,
    pub world_volume: f32,
    pub sample_counts: [usize; 2],
    pub minimum_volume: f32,
    pub fade: bool,
    pub exponent: f32,
    pub microphone_height: f32,
    pub microphone_fraction: f32,
    pub zoom: [f32; 3],
    pub root: String,
    pub folder: String,
    pub extension: String,
}

pub(super) fn load_event(
    assets: &VirtualAssets,
    settings: &Settings,
    event: &Event,
    samples: &mut BTreeMap<String, Option<SamplesBuffer>>,
) -> Result<(), Box<dyn Error>> {
    let name = &event.name;
    if !event.unsupported_controls.is_empty() {
        return Err(format!(
            "{name}: unsupported audio Control flags {:?}",
            event.unsupported_controls
        )
        .into());
    }
    super::hacker::validate_event(event)?;
    if event.speech
        && (!event.attack.is_empty()
            || !event.decay.is_empty()
            || event.limit != 0
            || event.world
            || event.voice
            || event.control != 0)
    {
        return Err(
            format!("{name}: additional streaming audio controls require translation").into(),
        );
    }
    for name in event.sounds.iter().chain(&event.attack).chain(&event.decay) {
        if !samples.contains_key(name) {
            samples.insert(name.clone(), sample(assets, settings, name, event.speech)?);
        }
    }
    super::hacker::validate_samples(event, samples)?;
    Ok(())
}

pub fn settings(assets: &VirtualAssets, options: &Path) -> Result<Settings, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/audiosettings.ini")?)?;
    let files = [file];
    let lines = &definition(&files, "AudioSettings", "")?.lines;
    let relative = percent(lines, "Relative2DVolume", 0.0)?;
    let mut result = Settings {
        speech_volume: percent(lines, "DefaultSpeechVolume", 0.0)?,
        streaming_folder: value(lines, "StreamingFolder")?
            .ok_or("StreamingFolder missing")?
            .to_owned(),
        sound_volume: percent(lines, "DefaultSoundVolume", 0.0)? * (1.0 + relative.min(0.0)),
        world_volume: percent(lines, "Default3DSoundVolume", 0.0)? * (1.0 - relative.max(0.0)),
        sample_counts: [
            integer(lines, "SampleCount2D", 4)?,
            integer(lines, "SampleCount3D", 25)?,
        ],
        minimum_volume: percent(lines, "MinSampleVolume", 0.0)?,
        fade: value(lines, "Use3DSoundRangeVolumeFade")?
            .is_none_or(|v| v.eq_ignore_ascii_case("yes")),
        exponent: numeric(lines, "3DSoundRangeVolumeFadeExponent", 4.0)?,
        microphone_height: numeric(lines, "MicrophoneDesiredHeightAboveTerrain", 0.0)?,
        microphone_fraction: percent(lines, "MicrophoneMaxPercentageBetweenGroundAndCamera", 0.0)?,
        zoom: [
            numeric(lines, "ZoomMinDistance", 0.0)?,
            numeric(lines, "ZoomMaxDistance", 0.0)?,
            percent(lines, "ZoomSoundVolumePercentageAmount", 0.0)?,
        ],
        root: value(lines, "AudioRoot")?
            .ok_or("AudioRoot missing")?
            .to_owned(),
        folder: value(lines, "SoundsFolder")?
            .ok_or("SoundsFolder missing")?
            .to_owned(),
        extension: value(lines, "SoundsExtension")?
            .ok_or("SoundsExtension missing")?
            .to_owned(),
    };
    if options.is_file() {
        for line in std::fs::read_to_string(options)?.lines() {
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let destination = match name.trim() {
                "VoiceVolume" => &mut result.speech_volume,
                "SFXVolume" => &mut result.sound_volume,
                "SFX3DVolume" => &mut result.world_volume,
                _ => continue,
            };
            *destination = signed_number(value.trim())?.max(0.0) / 100.0;
        }
    }
    Ok(result)
}

pub fn definitions(assets: &VirtualAssets) -> Result<BTreeMap<String, Event>, Box<dyn Error>> {
    definitions_from(
        assets,
        &[
            "default/soundeffects",
            "soundeffects",
            "default/speech",
            "speech",
            "default/voice",
            "voice",
        ],
    )
}

pub(super) fn shell_definitions(
    assets: &VirtualAssets,
) -> Result<BTreeMap<String, Event>, Box<dyn Error>> {
    definitions_from(assets, &["default/soundeffects", "soundeffects"])
}

fn definitions_from(
    assets: &VirtualAssets,
    stems: &[&str],
) -> Result<BTreeMap<String, Event>, Box<dyn Error>> {
    let mut events = BTreeMap::new();
    for stem in stems {
        for file in files(assets, stem)? {
            for declaration in file
                .declarations
                .iter()
                .filter(|entry| matches!(entry.kind.as_str(), "AudioEvent" | "DialogEvent"))
            {
                let event = read_event(declaration, &events)?;
                events.insert(declaration.name.clone(), event);
            }
        }
    }
    Ok(events)
}

pub fn validate_speech_subtitles<'a>(
    assets: &VirtualAssets,
    names: impl Iterator<Item = &'a str>,
) -> Result<(), Box<dyn Error>> {
    let labels = crate::control_bar::strings(&assets.read("data/english/generals.csf")?.bytes)?;
    for name in names {
        let label = format!("dialogevent:{name}subtitle").to_ascii_lowercase();
        if labels
            .get(&label)
            .is_some_and(|text| !text.is_empty() && !text.starts_with('*'))
        {
            return Err(format!(
                "{name}: authored military subtitle requires original subtitle presentation"
            )
            .into());
        }
    }
    Ok(())
}

fn read_event(
    declaration: &generals_assets::content::Declaration,
    events: &BTreeMap<String, Event>,
) -> Result<Event, Box<dyn Error>> {
    let lines = &declaration.lines;
    let speech = declaration.kind == "DialogEvent";
    let mut event = events
        .get(if speech {
            "DefaultDialog"
        } else {
            "DefaultSoundEffect"
        })
        .cloned()
        .unwrap_or_else(default_event);
    event.speech = speech;
    event.name.clone_from(&declaration.name);
    if speech && let Some(filename) = value(lines, "Filename")? {
        event.sounds = vec![filename.to_owned()];
    }
    if field(lines, 0, "Sounds").is_some() {
        event.sounds = words(lines, "Sounds");
    }
    if field(lines, 0, "Attack").is_some() {
        event.attack = words(lines, "Attack");
    }
    if field(lines, 0, "Decay").is_some() {
        event.decay = words(lines, "Decay");
    }
    event.volume = percent(lines, "Volume", event.volume)?;
    event.volume_shift = percent(lines, "VolumeShift", event.volume_shift)?;
    event.pitch = pair(lines, "PitchShift", event.pitch, true)?;
    event.delay = pair(lines, "Delay", event.delay, false)?;
    event.limit = integer(lines, "Limit", event.limit)?;
    event.loop_count = integer(lines, "LoopCount", event.loop_count)?;
    event.min_range = numeric(lines, "MinRange", event.min_range)?;
    event.max_range = numeric(lines, "MaxRange", event.max_range)?;
    if let Some(priority) = value(lines, "Priority")? {
        event.priority = match priority.to_ascii_lowercase().as_str() {
            "lowest" => 0,
            "low" => 1,
            "normal" => 2,
            "high" => 3,
            "critical" => 4,
            _ => return Err(format!("invalid audio priority {priority}").into()),
        };
    }
    if field(lines, 0, "Type").is_some() {
        let flags = words(lines, "Type");
        event.world = contains(&flags, "world");
        event.voice = contains(&flags, "voice");
        event.restrictions =
            (u8::from(contains(&flags, "player") && !contains(&flags, "everyone")) * ST_PLAYER)
                | (u8::from(contains(&flags, "shrouded") && !contains(&flags, "global"))
                    * ST_SHROUDED)
                | (u8::from(
                    !contains(&flags, "everyone")
                        && (contains(&flags, "player")
                            || contains(&flags, "allies")
                            || contains(&flags, "enemies"))
                        && !(contains(&flags, "player") && contains(&flags, "ui")),
                ) * ST_REQUIRES_OWNER);
    }
    if field(lines, 0, "Control").is_some() {
        let flags = words(lines, "Control");
        event.control = (u8::from(contains(&flags, "random")) * RANDOM)
            | (u8::from(contains(&flags, "interrupt")) * INTERRUPT)
            | (u8::from(contains(&flags, "loop")) * LOOP)
            | (u8::from(contains(&flags, "all")) * ALL);
        event.unsupported_controls = flags
            .into_iter()
            .filter(|flag| {
                !["random", "interrupt", "loop", "all"]
                    .contains(&flag.to_ascii_lowercase().as_str())
            })
            .collect();
    }
    Ok(event)
}

pub(super) const fn default_event() -> Event {
    Event {
        speech: false,
        name: String::new(),
        sounds: Vec::new(),
        attack: Vec::new(),
        decay: Vec::new(),
        volume: 0.0,
        volume_override: None,
        volume_shift: 0.0,
        pitch: [1.0; 2],
        delay: [0.0; 2],
        limit: 0,
        priority: 0,
        world: false,
        voice: false,
        restrictions: 0,
        control: 0,
        loop_count: 0,
        unsupported_controls: Vec::new(),
        min_range: 0.0,
        max_range: 0.0,
    }
}

pub(super) const VOICE_KEYS: [&str; 40] = [
    "VoiceSelect",
    "VoiceMove",
    "VoiceAttack",
    "VoiceGuard",
    "VoiceSupply",
    "VoiceBuildResponse",
    "VoiceRepair",
    "VoiceHackInternet",
    "VoiceEnter",
    "VoiceUnload",
    "VoicePrimaryWeaponMode",
    "VoiceSecondaryWeaponMode",
    "VoiceModeLaser",
    "VoiceAttackLaser",
    "VoiceSalvage",
    "VoiceAttackAir",
    "VoiceModeDetox",
    "VoiceDetox",
    "VoiceEnterHostile",
    "VoiceModeContam",
    "VoicePoisonLocation",
    "VoiceModeFireStorm",
    "VoiceFlameLocation",
    "VoiceModeRocketPods",
    "VoiceModeLotusCapture",
    "VoiceModeLotusVehicle",
    "VoiceModeLotusCash",
    "VoiceModeSnipe",
    "VoiceSnipePilot",
    "VoiceDisarm",
    "VoiceCombatDrop",
    "VoiceModeBurtonKnife",
    "VoiceModeBurtonTimed",
    "VoiceModeBurtonRemote",
    "VoiceMelee",
    "VoiceModeBombTruckDisguise",
    "VoiceModeBombTruckBioBomb",
    "VoiceModeBombTruckHighExplosive",
    "VoiceModeBoobyTrap",
    "VoiceNoBuild",
];

pub fn units(
    assets: &VirtualAssets,
    catalog: &[UnitType],
) -> Result<Vec<UnitAudio>, Box<dyn Error>> {
    let selected: Vec<_> = catalog.iter().map(|unit| unit.name.as_str()).collect();
    let (objects, _) = crate::skirmish_data::source_files::effective(assets, &selected)?;
    let mut result = Vec::new();
    for unit in catalog {
        let object = definition(&objects, "Object", &unit.name)?;
        let per_unit = blocks(&object.lines, 0, "UnitSpecificSounds")?;
        let wheel = |key| {
            per_unit
                .iter()
                .find_map(|lines| field(lines, 1, key))
                .map(scalar)
                .transpose()
                .map(|name| name.unwrap_or("NoSound").to_owned())
        };
        result.push(UnitAudio {
            wheels: [wheel("TruckLandingSound")?, wheel("TruckPowerslideSound")?],
            hacker: hacker_sounds(unit, &per_unit)?,
            capture: super::capture::bindings(&object.lines)?,
            lotus: super::lotus::bindings(unit, &object.lines)?,
            stealth: super::lotus::stealth_bindings(&object.lines)?,
            detectors: super::lotus::detector_bindings(unit),
            contain: super::contain::bindings(unit.contain.is_some(), &object.lines)?,
            strategy: super::strategy::bindings(&object.lines)?,
            name: unit.name.clone(),
            voices: voice_bindings(&objects, unit, &object.lines, &per_unit)?,
            rapid_fire: per_unit
                .iter()
                .find_map(|lines| field(lines, 1, "VoiceRapidFire"))
                .map(scalar)
                .transpose()?
                .unwrap_or("NoSound")
                .to_owned(),
            // Only represented firing weapons emit the simulation shot hook.
            weapon_loops: (0..unit.weapon_template_count())
                .map(|index| weapon_loop_frames(&objects, unit.weapon_template(index)))
                .collect::<Result<Vec<_>, Box<dyn Error>>>()?,
            weapon: (0..unit.weapon_template_count())
                .map(|index| {
                    if index == 0 && !unit.weapon.name.is_empty() {
                        fire_sound(&objects, &object.lines)
                    } else {
                        alternate_fire_sound(
                            &objects,
                            unit.weapon_template(index)
                                .filter(|weapon| !weapon.name.is_empty()),
                        )
                    }
                })
                .collect::<Result<Vec<_>, Box<dyn Error>>>()?,
        });
    }
    Ok(result)
}

fn weapon_loop_frames(
    objects: &[ContentFile],
    weapon: Option<&generals_sim::skirmish::WeaponRules>,
) -> Result<u32, Box<dyn Error>> {
    let Some(weapon) = weapon.filter(|weapon| !weapon.name.is_empty()) else {
        return Ok(0);
    };
    let lines = &definition(objects, "Weapon", &weapon.name)?.lines;
    Ok(field(lines, 0, "FireSoundLoopTime")
        .map(crate::skirmish_data::duration_frames)
        .transpose()?
        .unwrap_or(0))
}

fn alternate_fire_sound(
    objects: &[ContentFile],
    weapon: Option<&generals_sim::skirmish::WeaponRules>,
) -> Result<String, Box<dyn Error>> {
    Ok(if let Some(weapon) = weapon {
        let lines = &definition(objects, "Weapon", &weapon.name)?.lines;
        value(lines, "FireSound")?.unwrap_or_default().to_owned()
    } else {
        String::new()
    })
}

fn laser_voice<'a>(
    files: &'a [ContentFile],
    object: &'a [ContentLine],
    key: &str,
) -> Result<Option<&'a str>, Box<dyn Error>> {
    if key == "VoiceModeLaser" {
        value(
            &definition(
                files,
                "CommandButton",
                "Command_AmericaMissileDefenderLaserGuidedMissiles",
            )?
            .lines,
            "UnitSpecificSound",
        )
    } else {
        let module = blocks(object, 0, "Behavior")?
            .into_iter()
            .find(|block| {
                block[0]
                    .tokens
                    .get(1)
                    .is_some_and(|name| name == "SpecialAbility")
            })
            .ok_or("Laser Lock SpecialAbility sound missing")?;
        field(module, 1, "InitiateSound").map(scalar).transpose()
    }
}

fn voice_bindings(
    objects: &[ContentFile],
    unit: &UnitType,
    object: &[ContentLine],
    per_unit: &[&[ContentLine]],
) -> Result<Vec<Binding>, Box<dyn Error>> {
    let mut voices = Vec::new();
    for key in VOICE_KEYS {
        let key = if key == "VoiceEnter" && unit.aircraft.is_some() {
            "VoiceGarrison"
        } else {
            key
        };
        let name = if matches!(
            key,
            "VoiceModeContam"
                | "VoiceModeFireStorm"
                | "VoiceModeRocketPods"
                | "VoiceModeLotusCapture"
                | "VoiceModeLotusVehicle"
                | "VoiceModeLotusCash"
                | "VoiceModeSnipe"
                | "VoiceModeBurtonKnife"
                | "VoiceModeBurtonTimed"
                | "VoiceModeBurtonRemote"
                | "VoiceModeBombTruckDisguise"
                | "VoiceModeBombTruckBioBomb"
                | "VoiceModeBombTruckHighExplosive"
                | "VoiceModeBoobyTrap"
        ) {
            let button = mode_button(unit, key);
            if let Some(button) = button {
                value(
                    &definition(objects, "CommandButton", button)?.lines,
                    "UnitSpecificSound",
                )?
            } else {
                None
            }
        } else if matches!(key, "VoiceModeDetox" | "VoiceDetox") {
            if unit.behavior.cleanup.is_empty() {
                None
            } else {
                cleanup_voice(objects, object, key)?
            }
        } else if matches!(key, "VoiceModeLaser" | "VoiceAttackLaser") {
            if unit.behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialMissileDefenderLaserGuidedMissiles)) {
                laser_voice(objects, object, key)?
            } else {
                None
            }
        } else if matches!(
            key,
            "VoiceSupply"
                | "VoiceBuildResponse"
                | "VoiceNoBuild"
                | "VoiceRepair"
                | "VoiceHackInternet"
                | "VoiceEnter"
                | "VoiceGarrison"
                | "VoiceEnterHostile"
                | "VoicePoisonLocation"
                | "VoiceFlameLocation"
                | "VoiceUnload"
                | "VoiceCombatDrop"
                | "VoicePrimaryWeaponMode"
                | "VoiceSecondaryWeaponMode"
                | "VoiceModeLaser"
                | "VoiceAttackLaser"
                | "VoiceSalvage"
                | "VoiceSnipePilot"
                | "VoiceMelee"
                | "VoiceDisarm"
        ) {
            per_unit
                .iter()
                .find_map(|block| field(block, 1, key))
                .map(scalar)
                .transpose()?
        } else {
            value(object, key)?
        };
        voices.push(Binding {
            name: name.unwrap_or_default().to_owned(),
            previous: None,
        });
    }
    voices.extend(unit.special_powers.iter().map(|module| Binding {
        name: module.module_initiate_sound.clone(),
        previous: None,
    }));
    Ok(voices)
}

fn mode_button(unit: &UnitType, key: &str) -> Option<&'static str> {
    if unit.name == "AmericaInfantryColonelBurton" {
        return match key {
            "VoiceModeBurtonKnife" => Some("Command_ColonelBurtonKnifeAttack"),
            "VoiceModeBurtonTimed" => Some("Command_ColonelBurtonTimedDemoCharge"),
            "VoiceModeBurtonRemote" => Some("Command_ColonelBurtonRemoteDemoCharge"),
            _ => None,
        };
    }
    if unit
        .behavior
        .stealth
        .as_ref()
        .is_some_and(|rules| rules.disguise.is_some())
    {
        return match key {
            "VoiceModeBombTruckDisguise" => Some("Command_DisguiseAsVehicle"),
            "VoiceModeBombTruckBioBomb" => Some("Command_UpgradeGLABombTruckBioBomb"),
            "VoiceModeBombTruckHighExplosive" => {
                Some("Command_UpgradeGLABombTruckHighExplosiveBomb")
            }
            _ => None,
        };
    }
    match key {
        "VoiceModeBoobyTrap" if unit.name == "GLAInfantryRebel" => {
            Some("Command_GLAInfantryRebelBoobyTrapAttack")
        }
        "VoiceModeSnipe" if unit.name == "GLAInfantryJarmenKell" => {
            Some("Command_GLAInfantryJarmenKellSnipeVehicleAttack")
        }
        "VoiceModeContam" if !unit.behavior.contamination.is_empty() => {
            Some("Command_GLAToxinTractorContaminateGround")
        }
        "VoiceModeFireStorm" if unit.name == "ChinaTankDragon" => {
            Some("Command_ChinaDragonTankFireWall")
        }
        "VoiceModeRocketPods" if unit.name == "AmericaVehicleComanche" => {
            Some("Command_AmericaVehicleComancheFireRocketPods")
        }
        "VoiceModeLotusCapture" if unit.behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding)) => {
            Some("Command_ChinaInfantryBlackLotusCaptureHack")
        }
        "VoiceModeLotusVehicle" if unit.behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding)) => {
            Some("Command_ChinaInfantryBlackLotusVehicleHack")
        }
        "VoiceModeLotusCash" if unit.behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding)) => {
            Some("Command_ChinaInfantryBlackLotusCashHack")
        }
        _ => None,
    }
}

fn cleanup_voice<'a>(
    files: &'a [ContentFile],
    object: &'a [ContentLine],
    key: &str,
) -> Result<Option<&'a str>, Box<dyn Error>> {
    if key == "VoiceModeDetox" {
        value(
            &definition(files, "CommandButton", "Command_AmbulanceCleanupArea")?.lines,
            "UnitSpecificSound",
        )
    } else {
        let module = blocks(object, 0, "Behavior")?
            .into_iter()
            .find(|block| {
                block[0]
                    .tokens
                    .get(1)
                    .is_some_and(|name| name == "CleanupAreaPower")
            })
            .ok_or("Ambulance CleanupAreaPower sound missing")?;
        field(module, 1, "InitiateSound").map(scalar).transpose()
    }
}

fn hacker_sounds(
    unit: &UnitType,
    per_unit: &[&[ContentLine]],
) -> Result<[String; 3], Box<dyn Error>> {
    let mut sounds = [String::new(), String::new(), String::new()];
    if unit.behavior.hacking.is_some() {
        for (sound, key) in sounds
            .iter_mut()
            .zip(["UnitUnpack", "UnitPack", "UnitCashPing"])
        {
            per_unit
                .iter()
                .find_map(|block| field(block, 1, key))
                .map(scalar)
                .transpose()?
                .unwrap_or_default()
                .clone_into(sound);
        }
    }
    Ok(sounds)
}

fn fire_sound(objects: &[ContentFile], object: &[ContentLine]) -> Result<String, Box<dyn Error>> {
    let mut selected = None;
    for set in blocks(object, 0, "WeaponSet")? {
        let conditions = field(set, 1, "Conditions").ok_or("WeaponSet lacks Conditions")?;
        if !conditions.tokens[1..]
            .iter()
            .all(|flag| flag.eq_ignore_ascii_case("NONE"))
        {
            continue;
        }
        if selected.replace(set).is_some() {
            return Err("multiple normal WeaponSet blocks".into());
        }
    }
    let Some(line) = selected.and_then(|set| {
        set.iter().find(|line| {
            line.depth == Some(1)
                && line.tokens.first().is_some_and(|v| v == "Weapon")
                && line.tokens.get(1).is_some_and(|v| v == "PRIMARY")
        })
    }) else {
        return Ok(String::new());
    };
    let name = line.tokens.get(2).ok_or("PRIMARY weapon has no name")?;
    if name.eq_ignore_ascii_case("none") {
        return Ok(String::new());
    }
    let lines = &definition(objects, "Weapon", name)?.lines;
    Ok(value(lines, "FireSound")?.unwrap_or_default().to_owned())
}

pub fn sample(
    assets: &VirtualAssets,
    settings: &Settings,
    name: &str,
    speech: bool,
) -> Result<Option<SamplesBuffer>, Box<dyn Error>> {
    let folder = if speech {
        &settings.streaming_folder
    } else {
        &settings.folder
    };
    let base = format!("{}/{folder}", settings.root).replace('\\', "/");
    // Streaming Filename already includes its extension; sound variants do not.
    let filename = if speech {
        name.to_owned()
    } else {
        format!("{name}.{}", settings.extension)
    };
    let localized = format!("{base}/English/{filename}");
    let common = format!("{base}/{filename}");
    let asset = match assets.read(&localized) {
        Ok(asset) => asset,
        Err(AssetError::Missing(_)) => match assets.read(&common) {
            Ok(asset) => asset,
            Err(AssetError::Missing(_)) => {
                eprintln!("Original audio sample absent: {common}");
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        },
        Err(error) => return Err(error.into()),
    };
    decode(asset.bytes)
        .map(Some)
        .map_err(|error| format!("{} ({}): {error}", asset.path, asset.origin).into())
}

fn decode(bytes: Vec<u8>) -> Result<SamplesBuffer, Box<dyn Error>> {
    let decoder = rodio::Decoder::try_from(Cursor::new(bytes))?;
    let channels = decoder.channels();
    let rate = decoder.sample_rate();
    let samples: Vec<_> = decoder.take(8_000_001).collect();
    if samples.is_empty() || samples.len() > 8_000_000 || samples.iter().any(|v| !v.is_finite()) {
        return Err("invalid or oversized decoded audio".into());
    }
    Ok(SamplesBuffer::new(channels, rate, samples))
}

pub(super) fn files(
    assets: &VirtualAssets,
    stem: &str,
) -> Result<Vec<ContentFile>, Box<dyn Error>> {
    let mut names = vec![format!("data/ini/{stem}.ini")];
    names.extend(
        assets
            .list(&format!("data/ini/{stem}/"))?
            .into_iter()
            .filter(|name| {
                Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
            }),
    );
    let mut result = Vec::new();
    for name in names {
        match assets.read(&name) {
            Ok(asset) => result.push(content::scan(asset)?),
            Err(AssetError::Missing(_)) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(result)
}

pub(super) fn value<'a>(
    lines: &'a [ContentLine],
    key: &str,
) -> Result<Option<&'a str>, Box<dyn Error>> {
    field(lines, 0, key).map(scalar).transpose()
}
fn numeric(lines: &[ContentLine], key: &str, fallback: f32) -> Result<f32, Box<dyn Error>> {
    value(lines, key)?.map_or(Ok(fallback), finite_number)
}
fn integer(lines: &[ContentLine], key: &str, fallback: usize) -> Result<usize, Box<dyn Error>> {
    value(lines, key)?.map_or(Ok(fallback), |v| Ok(v.parse()?))
}
pub(super) fn percent(
    lines: &[ContentLine],
    key: &str,
    fallback: f32,
) -> Result<f32, Box<dyn Error>> {
    value(lines, key)?.map_or(Ok(fallback), |v| {
        Ok(signed_number(v.trim_end_matches('%'))? / 100.0)
    })
}
fn words(lines: &[ContentLine], key: &str) -> Vec<String> {
    field(lines, 0, key).map_or_else(Vec::new, |line| line.tokens[1..].to_vec())
}
fn contains(words: &[String], flag: &str) -> bool {
    words.iter().any(|word| word.eq_ignore_ascii_case(flag))
}
fn pair(
    lines: &[ContentLine],
    key: &str,
    fallback: [f32; 2],
    pitch: bool,
) -> Result<[f32; 2], Box<dyn Error>> {
    let Some(line) = field(lines, 0, key) else {
        return Ok(fallback);
    };
    let [_, low, high] = line.tokens.as_slice() else {
        return Err(format!("{key} needs two values").into());
    };
    let mut result = [signed_number(low)?, signed_number(high)?];
    if result[0] > result[1] || (pitch && result[0] <= -100.0) || (!pitch && result[0] < 0.0) {
        return Err(format!("invalid {key} range").into());
    }
    if pitch {
        result = result.map(|v| 1.0 + v / 100.0);
    }
    Ok(result)
}

pub(super) fn signed_number(value: &str) -> Result<f32, Box<dyn Error>> {
    let number: f32 = value.parse()?;
    if !number.is_finite() {
        return Err(format!("invalid finite audio number {value}").into());
    }
    Ok(number)
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_rebel_mode_voice_is_loaded_from_the_original_command_button() {
        let root = generals_assets::game_root().unwrap();
        let assets = super::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let data = crate::skirmish_data::load(&assets, &[]).unwrap();
        let rebel = data
            .types
            .iter()
            .find(|unit| unit.name == "GLAInfantryRebel")
            .unwrap();
        let audio = super::units(&assets, std::slice::from_ref(rebel)).unwrap();
        assert_eq!(
            audio[0].voices[super::super::Voice::BoobyTrapMode.index()].name,
            "RebelVoiceModeBoobyTrap"
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_zero_primary_sprayers_keep_original_fire_loop_and_both_command_voices() {
        let root = &generals_assets::game_root().unwrap();
        let assets = super::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let data = crate::skirmish_data::load(&assets, &[]).unwrap();
        let unit = data
            .types
            .into_iter()
            .find(|unit| unit.name == "GLAVehicleToxinTruck")
            .unwrap();
        let audio = super::units(&assets, std::slice::from_ref(&unit)).unwrap();
        let sound = &audio[0];
        let mut sprayers = 0;
        for index in 0..unit.weapon_template_count() {
            if unit
                .weapon_template(index)
                .is_some_and(|weapon| weapon.name.starts_with("ToxinTruckSprayer"))
            {
                sprayers += 1;
                assert_eq!(sound.weapon[index], "ToxinTractorContaminate");
                assert_eq!(sound.weapon_loops[index], 3);
                assert_eq!(
                    unit.weapon_template(index).unwrap().fire_sound_loop_frames,
                    3
                );
            }
        }
        assert_eq!(sprayers, 6);
        assert_eq!(
            sound.voices[super::super::Voice::ContaminateMode.index()].name,
            "ToxinTractorVoiceModeContam"
        );
        assert_eq!(
            sound.voices[super::super::Voice::PoisonLocation.index()].name,
            "ToxinTractorVoiceAttackContam"
        );
    }

    use super::{decode, fire_sound, pair, percent, read_event};
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn dialog_inherits_its_own_defaults_and_keeps_filename_extension() {
        let file = content::scan(Asset {
            path: "speech.ini".into(),
            origin: "fixture".into(),
            bytes: b"AudioEvent DefaultSoundEffect\nVolume = 20\nType = world voice\nEnd\nDialogEvent DefaultDialog\nVolume = 100\nType = ui everyone\nEnd\nDialogEvent EvaUSA_Victory\nFilename = euplvic.wav\nEnd\nDialogEvent UI_AllCheerSound\nFilename = ucheer.wav\nVolume = 70\nEnd\n".to_vec(),
        }).expect("native speech definitions");
        let mut events = std::collections::BTreeMap::new();
        for definition in &file.declarations {
            let event = read_event(definition, &events).expect("native defaults");
            events.insert(definition.name.clone(), event);
        }
        let speech = &events["EvaUSA_Victory"];
        assert!(speech.speech && !speech.world && !speech.voice);
        assert_eq!(speech.volume.to_bits(), 1.0_f32.to_bits());
        assert_eq!(speech.sounds, ["euplvic.wav"]);
        assert_eq!(
            events["UI_AllCheerSound"].volume.to_bits(),
            0.7_f32.to_bits()
        );
        assert_eq!(
            events["DefaultSoundEffect"].volume.to_bits(),
            0.2_f32.to_bits()
        );
    }

    #[test]
    fn normal_primary_fire_sound_accepts_blank_and_none_conditions() {
        for conditions in ["", "None", "NONE"] {
            let file = content::scan(Asset {
                path: "test.ini".to_owned(),
                origin: "fixture".to_owned(),
                bytes: format!(
                    "Object Tank\nWeaponSet\nConditions = {conditions}\nWeapon = PRIMARY TankGun\nEnd\nWeaponSet\nConditions = PLAYER_UPGRADE\nWeapon = PRIMARY UpgradedGun\nEnd\nEnd\nWeapon TankGun\nFireSound = OriginalTankShot\nEnd\n"
                )
                .into_bytes(),
            })
            .expect("valid weapon fixture");
            let files = [file];
            assert_eq!(
                fire_sound(&files, &files[0].declarations[0].lines)
                    .expect("normal weapon resolves independently of upgraded set"),
                "OriginalTankShot"
            );
        }
    }

    #[test]
    fn rejects_truncated_wave_header() {
        assert!(decode(b"RIFF\xff\xff\xff\xffWAVEfmt ".to_vec()).is_err());
    }

    #[test]
    fn original_ima_adpcm_wave_container_decodes_complete_block() {
        // Same RIFF format as installed bcmasela.wav: tag17, mono22050,
        // 512-byte IMA blocks containing 1017 samples.
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(564_u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(20_u32.to_le_bytes());
        for value in [17_u16, 1] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [22_050_u32, 11_100] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [512_u16, 4, 2, 1017] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(b"fact");
        bytes.extend(4_u32.to_le_bytes());
        bytes.extend(1017_u32.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(512_u32.to_le_bytes());
        let mut block = [0_u8; 512];
        block[..2].copy_from_slice(&100_i16.to_le_bytes());
        bytes.extend(block);
        let mut decoded = decode(bytes).expect("original IMA container and codec enabled");
        assert_eq!(
            decoded.next().expect("predictor sample").to_bits(),
            (100.0_f32 / 32_768.0).to_bits()
        );
        assert_eq!(decoded.count(), 1016);
    }

    #[test]
    fn signed_volume_and_pitch_shifts_preserve_original_ranges() {
        let file = content::scan(Asset {
            path: "test.ini".to_owned(),
            origin: "fixture".to_owned(),
            bytes: b"AudioEvent Test\nVolumeShift = -10\nPitchShift = -5 5\nEnd\n".to_vec(),
        })
        .expect("valid source audio fixture");
        let lines = &file.declarations[0].lines;
        assert_eq!(
            percent(lines, "VolumeShift", 0.0)
                .expect("signed volume shift")
                .to_bits(),
            (-0.1_f32).to_bits()
        );
        assert_eq!(
            pair(lines, "PitchShift", [1.0; 2], true)
                .expect("signed pitch shift")
                .map(f32::to_bits),
            [0.95_f32.to_bits(), 1.05_f32.to_bits()]
        );
    }

    #[test]
    fn rejects_pitch_reversal_and_zero_playback_rate() {
        for range in ["5 -5", "-100 5"] {
            let file = content::scan(Asset {
                path: "test.ini".to_owned(),
                origin: "fixture".to_owned(),
                bytes: format!("AudioEvent Test\n PitchShift = {range}\nEnd\n").into_bytes(),
            })
            .expect("valid INI fixture");
            assert!(pair(&file.declarations[0].lines, "PitchShift", [1.0; 2], true).is_err());
        }
    }
}
