//! Original civilian music script requests and faction score tracks.
use std::{collections::BTreeMap, error::Error, path::Path, time::Duration};

use bevy::{audio::Volume, prelude::*};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::Faction;

use super::{
    SourceAudio,
    music_scripts::{self, Change, Program},
};
use crate::{
    input::{CommandMode, Mouse},
    playtest::Match,
    playtest_ui::{Menu, ResultsScreen},
};

mod data;

struct TrackChange {
    name: String,
    fade: bool,
}

struct Track {
    source: Option<AudioSource>,
    handle: Option<Handle<AudioSource>>,
    volume: f32,
    source_volume: f32,
    duration: Duration,
}

pub(super) struct Music {
    tracks: BTreeMap<String, Track>,
    score_tracks: [String; 3],
    scripts: Option<Program>,
    generation: Option<u32>,
    frame: Option<u64>,
    reset_requested: bool,
    score_started: bool,
    playing: Option<Playing>,
    fading: Vec<Playing>,
    pending: Vec<TrackChange>,
    fade_frames: u16,
    volume: f32,
    pending_volume: Option<f32>,
}

impl Music {
    pub(super) const fn set_volume(&mut self, volume: f32) {
        self.pending_volume = Some(volume);
    }
    pub(super) fn speech_names(&self) -> impl Iterator<Item = &str> {
        self.scripts.iter().flat_map(Program::speeches)
    }

    pub(super) fn diagnostic(&self) -> String {
        let playing = self
            .playing
            .as_ref()
            .map(|playing| (playing.entity, playing.name.as_str()));
        let pending: Vec<_> = self
            .pending
            .iter()
            .map(|request| request.name.as_str())
            .collect();
        format!(
            "music_generation={:?} script_frame={:?} reset_requested={} score_started={} raw_playing={playing:?} pending={pending:?}",
            self.generation, self.frame, self.reset_requested, self.score_started
        )
    }

    pub(super) fn playing_track(&self, generation: u32) -> Option<(Entity, &str)> {
        if self.generation != Some(generation) || self.reset_requested || self.score_started {
            return None;
        }
        self.playing
            .as_ref()
            .map(|playing| (playing.entity, playing.name.as_str()))
    }
}

/// Existing loading streams survive scene replacement until their source fade ends.
#[derive(Component)]
pub struct RetiringMusic {
    source_volume: f32,
    faded: u16,
    frames: u16,
}

/// Preserve the load screen's actual players while its `SourceAudio` is replaced.
///
/// # Panics
/// Requires the installed shell audio resource and its still-live music players.
pub fn finish_load_screen(world: &mut World) {
    let retiring: Vec<_> = {
        let mut audio = world.resource_mut::<SourceAudio>();
        let music = &mut audio.music;
        assert!(
            music.scripts.is_none(),
            "load-screen completion requires shell audio"
        );
        music.pending.clear();
        if let Some(playing) = music.playing.take() {
            music.fading.push(playing);
        }
        let Music {
            tracks,
            fading,
            fade_frames,
            ..
        } = music;
        fading
            .drain(..)
            .map(|playing| {
                (
                    playing.entity,
                    RetiringMusic {
                        source_volume: tracks[&playing.name].source_volume,
                        faded: playing.faded,
                        frames: *fade_frames,
                    },
                )
            })
            .collect()
    };
    for (entity, state) in retiring {
        world.entity_mut(entity).insert(state);
    }
}

pub(super) fn fade_retiring(
    mut commands: Commands,
    audio: Res<SourceAudio>,
    mut players: Query<(
        Entity,
        &mut RetiringMusic,
        &mut PlaybackSettings,
        Option<&mut AudioSink>,
    )>,
) {
    let audio = audio.into_inner();
    for (entity, mut fade, mut settings, sink) in &mut players {
        let frames = fade.frames;
        let volume = fade.source_volume * audio.music.volume;
        if let Some(volume) = fade_volume(&mut fade.faded, frames, volume) {
            let volume = Volume::Linear(volume);
            settings.volume = volume;
            if let Some(mut sink) = sink {
                sink.set_volume(volume);
            }
        } else {
            if let Some(sink) = sink {
                sink.stop();
            }
            commands.entity(entity).despawn();
        }
    }
}

#[derive(Component)]
pub struct ScoreMusic;

struct Playing {
    entity: Entity,
    name: String,
    volume: f32,
    duration: Duration,
    faded: u16,
}

pub(super) fn load(assets: &VirtualAssets, options: &Path) -> Result<Music, Box<dyn Error>> {
    let scripts = music_scripts::load(assets)?;
    let (tracks, score_tracks, fade_frames, volume) = data::load(assets, options, &scripts)?;
    Ok(Music {
        tracks,
        score_tracks,
        scripts: Some(scripts),
        generation: None,
        frame: None,
        reset_requested: false,
        score_started: false,
        playing: None,
        fading: Vec::new(),
        pending: Vec::new(),
        fade_frames,
        volume,
        pending_volume: None,
    })
}

pub(super) fn load_shell(assets: &VirtualAssets, options: &Path) -> Result<Music, Box<dyn Error>> {
    let (tracks, fade_frames, volume) = data::load_shell(assets, options)?;
    Ok(Music {
        tracks,
        score_tracks: std::array::from_fn(|_| String::new()),
        scripts: None,
        generation: None,
        frame: None,
        reset_requested: false,
        score_started: false,
        playing: None,
        fading: Vec::new(),
        pending: Vec::new(),
        fade_frames,
        volume,
        pending_volume: None,
    })
}

impl SourceAudio {
    /// Start the original local `PlayerTemplate`'s loading music without a roster.
    ///
    /// # Errors
    /// Rejects a gameplay audio resource or missing/invalid original recording.
    pub fn begin_load_screen(
        &mut self,
        assets: &VirtualAssets,
        music_name: &str,
    ) -> Result<(), Box<dyn Error>> {
        let music = &mut self.music;
        if music.scripts.is_some() {
            return Err("load-screen music requires shell audio".into());
        }
        // MultiPlayerLoadScreen::init submits no event for an empty template field.
        if music_name.is_empty() {
            return Ok(());
        }
        if !music.tracks.contains_key(music_name) {
            let mut track = data::load_track(assets, music_name)?;
            track.volume = track.source_volume * music.pending_volume.unwrap_or(music.volume);
            music.tracks.insert(music_name.to_owned(), track);
        }
        music.pending.clear();
        music.pending.push(TrackChange {
            name: music_name.to_owned(),
            fade: true,
        });
        Ok(())
    }

    /// Retire the load screen and resume the original shell music after failure.
    pub fn end_load_screen(&mut self) {
        let music = &mut self.music;
        if music.scripts.is_some() {
            return;
        }
        music.pending.clear();
        if music
            .playing
            .as_ref()
            .is_none_or(|playing| playing.name == "Shell")
        {
            return;
        }
        music.pending.push(TrackChange {
            name: "Shell".to_owned(),
            fade: true,
        });
    }
}

/// `GameLogic::update` evaluates global scripts before updating objects.
pub fn tick(
    mut audio: ResMut<SourceAudio>,
    mut battle: ResMut<Match>,
    sinks: Query<&AudioSink>,
    mut screen: ResMut<ResultsScreen>,
    mut menu: ResMut<Menu>,
    mut mouse: ResMut<Mouse>,
) {
    let music = &mut audio.music;
    let Some(scripts) = &mut music.scripts else {
        return;
    };
    if music.generation != Some(battle.generation) {
        scripts.reset();
        music.pending.clear();
        music.frame = None;
        music.generation = Some(battle.generation);
        music.reset_requested = true;
        music.score_started = false;
    }
    let game = &battle.simulation;
    if !battle.started
        || battle.paused
        || game.end_game_at.is_some()
        || music.frame == Some(game.tick)
    {
        return;
    }
    music.frame = Some(game.tick);
    let completed = music
        .playing
        .as_ref()
        .filter(|_| !music.reset_requested)
        .and_then(|playing| {
            let sink = sinks.get(playing.entity).ok()?;
            (!sink.empty()).then(|| {
                let loops = sink.position().as_nanos() / playing.duration.as_nanos();
                (
                    playing.name.as_str(),
                    u32::try_from(loops).unwrap_or(u32::MAX),
                )
            })
        });
    let changes = scripts.advance(game, completed);
    for change in changes {
        match change {
            Change::Track { name, fade } => audio.music.pending.push(TrackChange { name, fade }),
            Change::Speech {
                name,
                allow_overlap,
            } => audio.speech(&name, allow_overlap),
            Change::Result(action) => {
                screen.apply(action, &mut battle, &mut menu);
                if battle.simulation.end_game_at.is_some() {
                    mouse.pending_command = CommandMode::Normal;
                    mouse.waypoint_mode = false;
                }
            }
        }
    }
}

pub(super) fn update(
    mut commands: Commands,
    mut audio: ResMut<SourceAudio>,
    mut sources: ResMut<Assets<AudioSource>>,
    battle: Option<Res<Match>>,
    screen: Option<Res<ResultsScreen>>,
    mut sinks: Query<&mut AudioSink>,
    mut settings: Query<&mut PlaybackSettings>,
) {
    let music = &mut audio.music;
    if let Some(volume) = music.pending_volume.take() {
        apply_volume(music, volume, &mut sinks, &mut settings);
    }
    if battle.is_none() && music.scripts.is_none() {
        for request in std::mem::take(&mut music.pending) {
            stop_active(music, &mut commands, &sinks, request.fade);
            play(music, &request.name, &mut commands, &mut sources);
        }
        if music.playing.is_none() {
            play(music, "Shell", &mut commands, &mut sources);
        }
        fade_out(music, &mut commands, &mut sinks);
        return;
    }
    let Some(battle) = battle else {
        return;
    };
    let screen = screen.expect("a loaded match requires its results screen for music");
    if music.reset_requested {
        stop_active(music, &mut commands, &sinks, false);
        for playing in music.fading.drain(..) {
            stop(&playing, &mut commands, &sinks);
        }
        music.reset_requested = false;
    }
    if !battle.started {
        stop_active(music, &mut commands, &sinks, true);
        music.pending.clear();
    } else if screen.scores() && !music.score_started {
        let index = match battle.simulation.players[0].faction {
            Faction::Usa => 0,
            Faction::China => 1,
            Faction::Gla => 2,
            Faction::Civilian => unreachable!("Neutral cannot own the local music player"),
        };
        music.pending.clear();
        music.pending.push(TrackChange {
            name: music.score_tracks[index].clone(),
            fade: false,
        });
        music.score_started = true;
    }
    for request in std::mem::take(&mut music.pending) {
        stop_active(music, &mut commands, &sinks, request.fade);
        play(music, &request.name, &mut commands, &mut sources);
    }
    fade_out(music, &mut commands, &mut sinks);
}

fn apply_volume(
    music: &mut Music,
    volume: f32,
    sinks: &mut Query<&mut AudioSink>,
    settings: &mut Query<&mut PlaybackSettings>,
) {
    music.volume = volume;
    for track in music.tracks.values_mut() {
        track.volume = track.source_volume * volume;
    }
    for playing in music.playing.iter_mut().chain(&mut music.fading) {
        playing.volume = music.tracks[&playing.name].volume;
        let fade = if playing.faded == 0 {
            1.0
        } else {
            1.0 - f32::from(playing.faded) / f32::from(music.fade_frames)
        };
        let volume = Volume::Linear(playing.volume * fade);
        if let Ok(mut sink) = sinks.get_mut(playing.entity) {
            sink.set_volume(volume);
        }
        // Queued streams can receive their sink after this update.
        if let Ok(mut playback) = settings.get_mut(playing.entity) {
            playback.volume = volume;
        }
    }
}

fn play(music: &mut Music, name: &str, commands: &mut Commands, sources: &mut Assets<AudioSource>) {
    let Some(track) = music.tracks.get_mut(name) else {
        return;
    };
    if let Some(source) = track.source.take() {
        track.handle = Some(sources.add(source));
    }
    let Some(handle) = &track.handle else {
        return;
    };
    // MilesAudioManager::playStream sets music to INFINITE_LOOP_COUNT.
    let mut entity = commands.spawn((
        AudioPlayer::new(handle.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(track.volume)),
    ));
    if music.score_started {
        entity.insert(ScoreMusic);
    }
    music.playing = Some(Playing {
        entity: entity.id(),
        name: name.to_owned(),
        volume: track.volume,
        duration: track.duration,
        faded: 0,
    });
    eprintln!(
        "Original {} music: {name}",
        if music.score_started {
            "score"
        } else if music.scripts.is_none() {
            "shell"
        } else {
            "skirmish"
        }
    );
}

fn stop_active(
    music: &mut Music,
    commands: &mut Commands,
    sinks: &Query<&mut AudioSink>,
    fade: bool,
) {
    if let Some(playing) = music.playing.take() {
        if fade {
            music.fading.push(playing);
        } else {
            stop(&playing, commands, sinks);
        }
    }
}

fn stop(playing: &Playing, commands: &mut Commands, sinks: &Query<&mut AudioSink>) {
    if let Ok(sink) = sinks.get(playing.entity) {
        sink.stop();
    }
    commands.entity(playing.entity).despawn();
}

fn fade_out(music: &mut Music, commands: &mut Commands, sinks: &mut Query<&mut AudioSink>) {
    music.fading.retain_mut(|playing| {
        if let Some(volume) = fade_volume(&mut playing.faded, music.fade_frames, playing.volume) {
            if let Ok(mut sink) = sinks.get_mut(playing.entity) {
                sink.set_volume(Volume::Linear(volume));
            }
            true
        } else {
            stop(playing, commands, sinks);
            false
        }
    });
}

// MilesAudioManager::processFadingList stops on the update *after* reaching zero.
fn fade_volume(frame: &mut u16, length: u16, volume: f32) -> Option<f32> {
    if *frame >= length {
        return None;
    }
    *frame += 1;
    Some(volume * (1.0 - f32::from(*frame) / f32::from(length)))
}

#[cfg(test)]
impl Music {
    pub(super) fn test_fixture() -> Self {
        Self {
            tracks: BTreeMap::new(),
            score_tracks: std::array::from_fn(|_| String::new()),
            scripts: Some(Program::test_fixture()),
            generation: None,
            frame: None,
            reset_requested: false,
            score_started: false,
            playing: None,
            fading: Vec::new(),
            pending: Vec::new(),
            fade_frames: 0,
            volume: 1.0,
            pending_volume: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fade_volume;

    #[test]
    fn music_mute_and_restore_preserve_track_gain_fade_and_queued_stream_volume() {
        use super::{Music, Playing, Track, apply_volume};
        use bevy::{ecs::system::SystemState, prelude::*};
        use std::time::Duration;

        let mut world = World::new();
        let current = world.spawn(PlaybackSettings::LOOP).id();
        let fading = world.spawn(PlaybackSettings::LOOP).id();
        let mut music = Music::test_fixture();
        music.fade_frames = 60;
        for (name, gain) in [("current", 0.8), ("fading", 0.4)] {
            music.tracks.insert(
                name.to_owned(),
                Track {
                    source: None,
                    handle: None,
                    volume: gain,
                    source_volume: gain,
                    duration: Duration::from_mins(1),
                },
            );
        }
        music.playing = Some(Playing {
            entity: current,
            name: "current".to_owned(),
            volume: 0.8,
            duration: Duration::from_mins(1),
            faded: 0,
        });
        music.fading.push(Playing {
            entity: fading,
            name: "fading".to_owned(),
            volume: 0.4,
            duration: Duration::from_mins(1),
            faded: 30,
        });
        let mut state: SystemState<(Query<&mut AudioSink>, Query<&mut PlaybackSettings>)> =
            SystemState::new(&mut world);
        for (master, expected) in [(0.0, [0.0_f32, 0.0]), (0.5, [0.4, 0.1])] {
            let (mut sinks, mut settings) = state.get_mut(&mut world).expect("audio queries");
            apply_volume(&mut music, master, &mut sinks, &mut settings);
            for (entity, expected) in [current, fading].into_iter().zip(expected) {
                assert_eq!(
                    settings
                        .get(entity)
                        .expect("queued stream")
                        .volume
                        .to_linear()
                        .to_bits(),
                    expected.to_bits()
                );
            }
            assert_eq!(music.fading[0].faded, 30);
        }
        assert_eq!(
            music.playing.expect("current track").volume.to_bits(),
            0.4_f32.to_bits()
        );
        assert_eq!(music.fading[0].volume.to_bits(), 0.2_f32.to_bits());
    }

    #[test]
    fn source_fade_reaches_silence_before_the_stop_update() {
        let mut frame = 0;
        let mut prior = 0.55;
        for _ in 0..60 {
            let volume = fade_volume(&mut frame, 60, 0.55).expect("native fading frame");
            assert!(volume < prior && volume >= 0.0);
            prior = volume;
        }
        assert_eq!(prior.to_bits(), 0.0_f32.to_bits());
        assert!(fade_volume(&mut frame, 60, 0.55).is_none());
        assert_eq!(frame, 60);
        assert!(fade_volume(&mut 0, 0, 0.55).is_none());
    }
}
