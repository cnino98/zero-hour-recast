use crate::playtest_view;
use bevy::{audio::Volume, ecs::system::SystemParam, prelude::*};

use super::{
    Match, Mouse, Request, Sequence, SourceAudio,
    data::{Event, INTERRUPT},
};

#[derive(Component)]
pub(super) struct Playing {
    pub(super) event: String,
    firing: Option<std::sync::Arc<()>>,
    pub(super) sample_names: Vec<String>,
    pub(super) object: Option<usize>,
    projectile: Option<u64>,
    position: Option<Vec3>,
    volume: f32,
    volume_shift: f32,
    serial: u64,
    pitch: f32,
    looping: Option<super::looping::Template>,
}

struct Active {
    entity: Entity,
    event: String,
    object: Option<usize>,
    positional: bool,
    voice: bool,
    priority: u8,
    serial: u64,
}

impl Playing {
    fn continuation(&mut self, event: &Event) -> Option<Request> {
        let looping = self.looping.as_mut()?;
        let delay_ms = looping.take_continuation()?;
        Some(Request {
            uninterruptible: false,
            firing: self.firing.clone(),
            event: self.event.clone(),
            object: self.object,
            projectile: self.projectile,
            position: self.position,
            samples: Vec::new(),
            sample_names: vec![event.sounds[looping.previous].clone()],
            looping: self.looping.take(),
            pitch: self.pitch,
            volume: self.volume,
            volume_shift: self.volume_shift,
            delay_ms,
        })
    }

    fn follow(&mut self, game: &generals_sim::skirmish::Skirmish) {
        if let Some(id) = self.projectile {
            if let Some((position, _)) = game
                .projectiles
                .iter()
                .find(|projectile| projectile.id == id)
                .and_then(|projectile| projectile.effect_transform(&game.entities))
            {
                self.position = Some(Vec3::new(position[0], position[2], -position[1]));
            }
        } else if self.position.is_some()
            && let Some(id) = self.object
            && game.entities.get(id).is_some_and(|unit| {
                unit.alive() || unit.physical_present() || unit.corpse_present()
            })
        {
            self.position = Some(playtest_view::unit_position(game, id) * 10.0);
        }
    }
}

type Sounds<'w, 's> = Query<'w, 's, (Entity, &'static mut Playing, Option<&'static mut AudioSink>)>;

#[derive(SystemParam)]
pub(super) struct Output<'w, 's> {
    commands: Commands<'w, 's>,
    assets: ResMut<'w, Assets<Sequence>>,
    playing: Sounds<'w, 's>,
    settings: Query<'w, 's, &'static mut PlaybackSettings>,
    timings: Option<Res<'w, crate::render_performance::Timings>>,
}

pub(super) fn reset_before_logic(
    mut commands: Commands,
    mut audio: ResMut<SourceAudio>,
    battle: Option<Res<Match>>,
    mut playing: Sounds,
) {
    let Some(battle) = battle else {
        return;
    };
    // Script startup and object updates enqueue this generation's sounds during
    // FixedUpdate. Retire the old match before accepting those events.
    reset_generation(&mut audio, battle.generation, &mut commands, &mut playing);
}

pub(super) fn update(
    output: Output,
    mut audio: ResMut<SourceAudio>,
    mouse: Option<ResMut<Mouse>>,
    battle: Option<Res<Match>>,
    time: Res<Time<Real>>,
) {
    let Output {
        mut commands,
        mut assets,
        mut playing,
        mut settings,
        timings,
    } = output;
    let _span = timings.as_deref().map(|timings| timings.span("audio"));
    let battle = battle.map(Res::into_inner);
    let mouse = battle.map(|_| mouse.expect("a loaded match requires mouse input for audio"));
    let time = time.into_inner();
    let audio = audio.as_mut();
    let reset = battle.is_some_and(|battle| {
        reset_generation(audio, battle.generation, &mut commands, &mut playing)
    });
    if let Some(battle) = battle {
        audio.update_capture(battle);
        audio.update_radar(battle);
        let eva_playing = playing.iter().any(|(_, playing, sink)| {
            use bevy::audio::AudioSinkPlayback;
            audio.sabotage.eva_name(&playing.event) && sink.is_none_or(|sink| !sink.empty())
        });
        audio.update_sabotage(battle, eva_playing);
        if battle.started {
            audio.update_aircraft(&battle.simulation, battle.observer_player.unwrap_or(0));
            audio.update_hazards(&battle.simulation, battle.observer_player.unwrap_or(0));
            audio.update_fires(&battle.simulation, battle.observer_player.unwrap_or(0));
        }
    }
    if let Some(mut mouse) = mouse {
        for request in mouse.voices.drain(..) {
            audio.voice(&request);
        }
    }
    let mut active = Vec::new();
    let mut continuations = Vec::new();
    let mut streams = Vec::new();
    let volume_changes = std::mem::take(&mut audio.volume_changes);
    for (entity, mut sound, sink) in &mut playing {
        if reset {
            continue;
        }
        if let Some(battle) = battle {
            sound.follow(&battle.simulation);
        }
        for (name, volume) in &volume_changes {
            if name == &sound.event {
                sound.volume = *volume * sound.volume_shift;
            }
        }
        let event = &audio.events[&sound.event];
        let volume = Volume::Linear(audio.volume(event, sound.volume, sound.position));
        if let Ok(mut settings) = settings.get_mut(entity) {
            settings.volume = volume;
        }
        if let Some(mut sink) = sink {
            if sink.empty() {
                if let Some(request) = sound.continuation(event) {
                    continuations.push(request);
                }
                if audio.events[&sound.event].speech {
                    // Native completion clears the flag for any speech stream,
                    // including an overlapping request admitted in the same update.
                    audio.speech_disallowed = false;
                }
                commands.entity(entity).despawn();
                continue;
            }
            sink.set_volume(volume);
        }
        let event = &audio.events[&sound.event];
        if event.speech {
            streams.push(entity);
            continue;
        }
        active.push(Active {
            entity,
            event: sound.event.clone(),
            object: sound.object,
            positional: sound.position.is_some(),
            voice: event.voice,
            priority: event.priority,
            serial: sound.serial,
        });
    }
    play_requests(
        &mut commands,
        &mut assets,
        audio,
        &mut playing,
        active,
        streams,
        time.delta_secs(),
    );
    // Miles processes requests before the playing list. A completion observed
    // this update cannot spend its first delay quantum until the next update.
    audio.requests.extend(continuations);
}

fn play_requests(
    commands: &mut Commands,
    assets: &mut Assets<Sequence>,
    audio: &mut SourceAudio,
    playing: &mut Sounds,
    mut active: Vec<Active>,
    mut streams: Vec<Entity>,
    delta: f32,
) {
    let mut requests = std::mem::take(&mut audio.requests);
    // GameSounds admits immediate streams before Miles starts queued effects.
    // A stream can use lower-priority sample capacity without evicting that sample.
    requests.retain(|request| {
        let event = &audio.events[&request.event];
        !event.speech || speech_can_play(event, &active, audio.settings.sample_counts)
    });
    let mut started: Vec<(String, bool)> = Vec::new();
    for mut request in requests {
        if !request_ready(&mut request, delta) {
            audio.requests.push(request);
            continue;
        }
        if request
            .looping
            .as_ref()
            .is_some_and(|source| !source.can_start())
        {
            // A deferred loop opens its selected body as a new play request.
            // If that file is absent, playAudioEvent retires it without decay.
            continue;
        }
        let event = &audio.events[&request.event];
        let position = if event.world { request.position } else { None };
        let volume = audio.volume(event, request.volume, position);
        if event.volume < audio.settings.minimum_volume
            || (event.priority != 4
                && position.is_some_and(|point| audio.listener.distance(point) >= event.max_range))
        {
            continue;
        }
        let admitted = if event.speech {
            Admission::Play
        } else {
            admission(
                event,
                &request,
                &active,
                &started,
                audio.settings.sample_counts,
            )
        };
        match admitted {
            Admission::Reject => continue,
            Admission::Play => {}
            Admission::Replace(kill) => {
                if let Ok((_, _, Some(sink))) = playing.get_mut(kill) {
                    sink.stop();
                }
                commands.entity(kill).despawn();
                active.retain(|sound| sound.entity != kill);
            }
        }
        let speech = event.speech;
        if speech && request.uninterruptible {
            for entity in std::mem::take(&mut streams) {
                if let Ok((_, _, Some(sink))) = playing.get_mut(entity) {
                    sink.stop();
                }
                commands.entity(entity).despawn();
            }
            audio.speech_disallowed = true;
        }
        if !speech {
            started.push((request.event.clone(), position.is_some()));
        }
        let entity = spawn(commands, assets, audio, request, position, volume);
        if speech {
            streams.push(entity);
        }
    }
}

fn request_ready(request: &mut Request, delta: f32) -> bool {
    if request.looping.is_some() {
        // SH GameEngine::update calls audio before its independently paced logic
        // gate. Miles still uses a fixed millisecond quantum per audio update.
        if request.delay_ms < super::looping::FRAME_MS {
            true
        } else {
            request.delay_ms -= super::looping::FRAME_MS;
            false
        }
    } else {
        let elapsed_ms = delta * 1000.0;
        request.delay_ms -= elapsed_ms;
        request.delay_ms <= 0.0
    }
}

fn speech_can_play(event: &Event, active: &[Active], capacity: [usize; 2]) -> bool {
    // Represented script speech is nonpositional, without voice or limit controls.
    active.iter().filter(|sound| !sound.positional).count() < capacity[0]
        || active
            .iter()
            .any(|sound| !sound.positional && sound.priority < event.priority)
}

pub(super) fn retire_failed_audio(
    mut commands: Commands,
    mut audio: ResMut<SourceAudio>,
    playing: Query<(Entity, &Playing), Without<AudioSink>>,
) {
    // Last runs after Bevy has attempted to attach sinks in PostUpdate. These
    // sources are already decoded/in Assets, so no sink means opening failed.
    // Native protection and firing handles survive only successful output.
    for (entity, sound) in &playing {
        if audio.events[&sound.event].speech || sound.firing.is_some() {
            commands.entity(entity).despawn();
            if audio.events[&sound.event].speech {
                audio.speech_disallowed = false;
            }
        }
    }
}

enum Admission {
    Reject,
    Play,
    Replace(Entity),
}

// GameSounds::canPlayNow and MilesAudioManager::doesViolateLimit.
fn admission(
    event: &Event,
    request: &Request,
    active: &[Active],
    pending: &[(String, bool)],
    capacity: [usize; 2],
) -> Admission {
    let positional = event.world && request.position.is_some();
    // isObjectPlayingVoice checks active sounds only. Immediate native requests
    // have requiresCheckForSample=false, so pending voices do not exclude each other.
    if event.voice
        && active.iter().any(|playing| {
            playing.voice && playing.object == request.object && request.object.is_some()
        })
        && event.control & INTERRUPT == 0
    {
        return Admission::Reject;
    }
    let same: Vec<_> = active
        .iter()
        .filter(|playing| playing.event == request.event)
        .collect();
    let queued = pending
        .iter()
        .filter(|(name, _)| *name == request.event)
        .count();
    if event.limit != 0 && same.len() + queued >= event.limit {
        if event.control & INTERRUPT == 0 || queued >= event.limit {
            return Admission::Reject;
        }
        return same
            .iter()
            .min_by_key(|playing| playing.serial)
            .map_or(Admission::Reject, |playing| {
                Admission::Replace(playing.entity)
            });
    }
    let count = active
        .iter()
        .filter(|playing| playing.positional == positional)
        .count()
        + pending
            .iter()
            .filter(|(_, world)| *world == positional)
            .count();
    if count < capacity[usize::from(positional)] {
        return Admission::Play;
    }
    active
        .iter()
        .filter(|playing| playing.positional == positional && playing.priority < event.priority)
        .min_by_key(|playing| (playing.priority, playing.serial))
        .map_or(Admission::Reject, |playing| {
            Admission::Replace(playing.entity)
        })
}

fn spawn(
    commands: &mut Commands,
    assets: &mut Assets<Sequence>,
    audio: &mut SourceAudio,
    request: Request,
    position: Option<Vec3>,
    volume: f32,
) -> Entity {
    let handle = assets.add(Sequence {
        samples: request.samples,
        looping: request.looping.clone(),
    });
    // A rerequest retains event identity, but Miles appends its new playback
    // to the end of the playing list used for oldest-playing interruption.
    audio.serial = audio.serial.wrapping_add(1);
    commands
        .spawn((
            AudioPlayer(handle),
            PlaybackSettings::ONCE
                .with_volume(Volume::Linear(volume))
                .with_speed(request.pitch),
            Playing {
                firing: request.firing,
                event: request.event,
                sample_names: request.sample_names,
                object: request.object,
                projectile: request.projectile,
                position,
                volume: request.volume,
                volume_shift: request.volume_shift,
                serial: audio.serial,
                pitch: request.pitch,
                looping: request.looping,
            },
        ))
        .id()
}

fn reset_generation(
    audio: &mut SourceAudio,
    generation: u32,
    commands: &mut Commands,
    playing: &mut Sounds,
) -> bool {
    let reset = audio.generation != generation;
    if reset {
        for (entity, _, sink) in playing.iter_mut() {
            if let Some(sink) = sink {
                sink.stop();
            }
            commands.entity(entity).despawn();
        }
        audio.requests.clear();
        audio.volume_changes.clear();
        for event in audio.events.values_mut() {
            event.volume_override = None;
        }
        audio.aircraft.reset();
        audio.object_sounds.reset();
        audio.firing.reset();
        audio.hazards.reset();
        audio.fires.reset();
        audio.capture.reset();
        audio.radar.reset();
        audio.sabotage.reset();
        audio.speech_disallowed = false;
        audio.generation = generation;
    }
    reset
}

#[cfg(test)]
mod tests {
    use super::{Active, Admission, admission};
    use crate::source_audio::{Request, data};
    use bevy::prelude::Entity;

    fn delayed_audio() -> super::SourceAudio {
        let mut audio = crate::source_audio::speech_tests::audio();
        let mut event = data::default_event();
        event.name = "weather-loop".into();
        event.volume = 1.0;
        event.control = data::LOOP | data::ALL | data::RANDOM;
        event.attack = vec!["click".into()];
        event.sounds = vec!["first".into(), "second".into(), "click".into()];
        event.decay = vec!["click".into()];
        event.pitch = [0.8, 1.2];
        event.volume_shift = -0.2;
        event.delay = [100.0; 2];
        audio.events.insert(event.name.clone(), event);
        audio.queue("weather-loop", Some(7), None, None, false);
        audio
    }

    #[test]
    fn loop_request_threshold_and_update_count_keep_native_f32_milliseconds() {
        let mut request = delayed_audio().requests.pop().unwrap();
        request.delay_ms = super::super::looping::FRAME_MS;
        assert!(!super::request_ready(&mut request, 0.0));
        assert_eq!(request.delay_ms.to_bits(), 0.0_f32.to_bits());
        assert!(super::request_ready(&mut request, 0.0));
        request.delay_ms = f32::from_bits(super::super::looping::FRAME_MS.to_bits() - 1);
        assert!(super::request_ready(&mut request, 0.0));
        // The pinned engine updates audio even when logic does not advance.
        // Declared milliseconds are not a wall-clock guarantee at high FPS.
        for delta in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
            for (delay, updates, remaining) in [
                (60_000.0, 1800, 0x3f4e_6380),
                (100_000.0, 2999, 0x41fd_55c6),
            ] {
                request.delay_ms = delay;
                for _ in 0..updates {
                    assert!(!super::request_ready(&mut request, delta));
                }
                assert!(super::request_ready(&mut request, delta));
                assert_eq!(request.delay_ms.to_bits(), remaining);
            }
        }
    }

    fn delayed_app(mut audio: super::SourceAudio) -> bevy::prelude::App {
        use bevy::prelude::*;
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        battle.started = false;
        let map = generals_assets::map::MapData::parse(
            &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
        )
        .unwrap();
        let terrain = crate::terrain::Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
        let mouse = crate::input::load(
            &assets,
            std::path::Path::new(""),
            &battle.simulation.catalog,
        )
        .unwrap();
        audio.requests[0].delay_ms = 0.0;
        let mut app = App::new();
        app.init_resource::<Assets<super::Sequence>>()
            .init_resource::<Time<Real>>()
            .insert_resource(audio)
            .insert_resource(battle)
            .insert_resource(terrain)
            .insert_resource(mouse)
            .add_systems(
                PostUpdate,
                super::super::before_audio_backend(super::update),
            );
        app
    }

    fn complete_portion(app: &mut bevy::prelude::App) -> Entity {
        use bevy::{audio::Decodable, prelude::*};
        let world = app.world_mut();
        let (entity, player) = world
            .query::<(Entity, &AudioPlayer<super::Sequence>)>()
            .single(world)
            .unwrap();
        let decoder = world
            .resource::<Assets<super::Sequence>>()
            .get(&player.0)
            .unwrap()
            .decoder();
        assert_eq!(decoder.count(), 32);
        // An exhausted platform sink lets the ordinary game-thread update
        // observe the callback. No sound device is needed for this ownership test.
        world
            .entity_mut(entity)
            .insert(AudioSink::new(rodio::Player::new().0));
        entity
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn delayed_loop_releases_sink_requeues_once_and_preserves_play_info_after_stop() {
        use bevy::{audio::Decodable, prelude::*};
        use std::sync::{Arc, atomic::Ordering};
        let audio = delayed_audio();
        let initial = &audio.requests[0];
        let pitch = initial.pitch;
        let volume = initial.volume;
        let shift = initial.volume_shift;
        let stop = initial.looping.as_ref().unwrap().stop.clone();
        let mut app = delayed_app(audio);
        app.update();
        let old = complete_portion(&mut app);
        app.update();
        assert!(app.world().get_entity(old).is_err());
        let request = &app.world().resource::<super::SourceAudio>().requests[0];
        assert_eq!(request.delay_ms.to_bits(), 100.0_f32.to_bits());
        assert_eq!(request.pitch.to_bits(), pitch.to_bits());
        assert_eq!(request.volume.to_bits(), volume.to_bits());
        assert_eq!(request.volume_shift.to_bits(), shift.to_bits());
        assert_eq!(request.object, Some(7));
        assert!(Arc::ptr_eq(&request.looping.as_ref().unwrap().stop, &stop));
        stop.store(true, Ordering::Release);
        let mut updates = 0;
        while !app
            .world()
            .resource::<super::SourceAudio>()
            .requests
            .is_empty()
        {
            app.update();
            updates += 1;
            assert!(updates <= 4);
        }
        assert_eq!(updates, 4);
        let world = app.world_mut();
        let (player, playing, settings) = world
            .query::<(
                &AudioPlayer<super::Sequence>,
                &super::Playing,
                &PlaybackSettings,
            )>()
            .single(world)
            .unwrap();
        assert_eq!(playing.pitch.to_bits(), pitch.to_bits());
        assert_eq!(playing.serial, 2);
        assert_eq!(world.resource::<super::SourceAudio>().serial, 2);
        assert_eq!(settings.speed.to_bits(), pitch.to_bits());
        let source = world
            .resource::<Assets<super::Sequence>>()
            .get(&player.0)
            .unwrap();
        assert!(source.looping.as_ref().unwrap().attack.is_none());
        assert_eq!(
            source.decoder().count(),
            64,
            "selected body then decay, even after queued stop"
        );
        assert!(
            source
                .looping
                .as_ref()
                .unwrap()
                .pending
                .lock()
                .unwrap()
                .is_none()
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn delayed_missing_body_fails_at_requeued_open_without_reselection_or_decay() {
        let mut audio = delayed_audio();
        let source = audio.requests[0].looping.as_mut().unwrap();
        source.bodies.fill(None);
        let mut app = delayed_app(audio);
        app.update();
        let old = complete_portion(&mut app);
        app.update();
        assert!(app.world().get_entity(old).is_err());
        for _ in 0..4 {
            app.update();
        }
        assert!(
            app.world()
                .resource::<super::SourceAudio>()
                .requests
                .is_empty()
        );
        let world = app.world_mut();
        assert_eq!(world.query::<&super::Playing>().iter(world).count(), 0);
    }

    #[test]
    fn map_retirement_removes_both_stream_types_and_preserves_non_audio_entities() {
        use bevy::prelude::*;
        let mut world = World::new();
        let ordinary = world
            .spawn(AudioPlayer::<AudioSource>(Handle::default()))
            .id();
        let sequence = world
            .spawn(AudioPlayer::<super::Sequence>(Handle::default()))
            .id();
        let engine = world.spawn_empty().id();
        crate::source_audio::retire_map_players(&mut world);
        assert!(world.get_entity(ordinary).is_err());
        assert!(world.get_entity(sequence).is_err());
        assert!(world.get_entity(engine).is_ok());
    }

    fn speech_output(output: super::Output, mut audio: bevy::prelude::ResMut<super::SourceAudio>) {
        let super::Output {
            mut commands,
            mut assets,
            mut playing,
            ..
        } = output;
        let streams = playing
            .iter()
            .filter_map(|(entity, event, _)| audio.events[&event.event].speech.then_some(entity))
            .collect();
        let active = playing
            .iter()
            .filter(|(_, event, _)| !audio.events[&event.event].speech)
            .map(|(entity, sound, _)| Active {
                entity,
                event: sound.event.clone(),
                object: sound.object,
                positional: sound.position.is_some(),
                voice: audio.events[&sound.event].voice,
                priority: audio.events[&sound.event].priority,
                serial: sound.serial,
            })
            .collect();
        super::play_requests(
            &mut commands,
            &mut assets,
            &mut audio,
            &mut playing,
            active,
            streams,
            0.0,
        );
    }

    #[test]
    fn speech_can_share_saturated_samples_without_retiring_an_effect() {
        use bevy::prelude::*;
        let mut app = App::new();
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.settings.sample_counts[0] = 1;
        audio.events.get_mut("first").unwrap().priority = 1;
        audio.ui("click");
        app.init_resource::<Assets<super::Sequence>>()
            .insert_resource(audio)
            .add_systems(Update, speech_output);
        app.update();
        app.world_mut()
            .resource_mut::<super::SourceAudio>()
            .speech("first", true);
        app.update();
        let world = app.world_mut();
        let mut names: Vec<_> = world
            .query::<&super::Playing>()
            .iter(world)
            .map(|sound| sound.event.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["click", "first"]);
    }

    #[test]
    fn queued_effects_do_not_consume_native_speech_admission_capacity() {
        use bevy::prelude::*;
        let mut app = App::new();
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.settings.sample_counts[0] = 1;
        audio.ui("click");
        audio.speech("first", true);
        app.init_resource::<Assets<super::Sequence>>()
            .insert_resource(audio)
            .add_systems(Update, speech_output);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&super::Playing>().iter(world).count(), 2);
    }

    #[test]
    fn speech_still_requires_open_capacity_or_a_lower_priority_active_sample() {
        let mut event = data::default_event();
        event.speech = true;
        event.priority = 1;
        assert!(!super::speech_can_play(
            &event,
            &[playing("busy", 8, false)],
            [1, 25]
        ));
        assert!(super::speech_can_play(
            &event,
            &[playing("busy", 8, false)],
            [2, 25]
        ));
    }

    #[test]
    fn failed_stream_open_releases_protection_and_does_not_leave_a_pending_player() {
        use bevy::prelude::*;
        let mut app = App::new();
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.speech("first", false);
        app.init_resource::<Assets<super::Sequence>>()
            .insert_resource(audio)
            .add_systems(PostUpdate, speech_output)
            .add_systems(Last, super::retire_failed_audio);
        app.update();
        let mut audio = app.world_mut().resource_mut::<super::SourceAudio>();
        assert!(!audio.speech_disallowed);
        audio.speech("second", false);
        assert_eq!(audio.requests.len(), 1);
        let world = app.world_mut();
        assert_eq!(world.query::<&super::Playing>().iter(world).count(), 0);
    }

    #[test]
    fn exclusive_speech_retires_pending_streams_before_backend_attachment() {
        use bevy::prelude::*;
        let mut app = App::new();
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.speech("first", true);
        audio.speech("second", false);
        app.init_resource::<Assets<super::Sequence>>()
            .insert_resource(audio)
            .add_systems(Update, speech_output);
        app.update();
        assert!(
            app.world()
                .resource::<super::SourceAudio>()
                .speech_disallowed
        );
        let world = app.world_mut();
        let names: Vec<_> = world
            .query::<&super::Playing>()
            .iter(world)
            .map(|event| event.event.as_str())
            .collect();
        assert_eq!(names, ["second"]);
        assert_eq!(
            world
                .query::<&AudioPlayer<super::Sequence>>()
                .iter(world)
                .count(),
            1
        );
    }

    #[test]
    fn already_queued_overlap_still_plays_after_exclusive_speech_starts() {
        use bevy::prelude::*;
        let mut app = App::new();
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.speech("first", false);
        audio.speech("second", true);
        app.init_resource::<Assets<super::Sequence>>()
            .insert_resource(audio)
            .add_systems(Update, speech_output);
        app.update();
        let mut audio = app.world_mut().resource_mut::<super::SourceAudio>();
        assert!(audio.speech_disallowed);
        audio.speech("first", true);
        assert!(audio.requests.is_empty());
        let world = app.world_mut();
        assert_eq!(world.query::<&super::Playing>().iter(world).count(), 2);
    }

    fn reset_speech(
        mut commands: bevy::prelude::Commands,
        mut audio: bevy::prelude::ResMut<super::SourceAudio>,
        mut playing: super::Sounds,
    ) {
        super::reset_generation(&mut audio, 1, &mut commands, &mut playing);
    }

    #[test]
    fn restart_clears_speech_protection_pending_audio_and_old_players() {
        use bevy::prelude::*;
        let mut app = App::new();
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.speech("first", false);
        app.init_resource::<Assets<super::Sequence>>()
            .insert_resource(audio)
            .add_systems(Update, speech_output);
        app.update();
        app.world_mut()
            .resource_mut::<super::SourceAudio>()
            .ui("click");
        app.add_systems(Update, reset_speech.before(speech_output));
        app.update();
        let audio = app.world().resource::<super::SourceAudio>();
        assert_eq!(audio.generation, 1);
        assert!(!audio.speech_disallowed);
        assert!(audio.requests.is_empty());
        let world = app.world_mut();
        assert_eq!(world.query::<&super::Playing>().iter(world).count(), 0);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn first_logic_frame_retains_script_volume_and_sound_after_restart() {
        use bevy::prelude::*;
        use generals_sim::skirmish::map_scripts::AudioAction;
        fn startup(mut audio: ResMut<super::SourceAudio>) {
            audio.map_script_actions(&[
                AudioAction::OverrideVolume("click".into(), 0.125),
                AudioAction::Play("click".into()),
            ]);
        }
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        battle.generation = 1;
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.ui("click"); // This request belongs to the old match.
        let mut app = App::new();
        app.insert_resource(audio)
            .insert_resource(battle)
            .add_systems(FixedFirst, super::reset_before_logic)
            .add_systems(FixedUpdate, startup);
        app.world_mut().run_schedule(FixedFirst);
        app.world_mut().run_schedule(FixedUpdate);
        // Playback's fallback runs later in PostUpdate and must not erase it.
        app.world_mut().run_system_cached(reset_speech).unwrap();
        let audio = app.world().resource::<super::SourceAudio>();
        assert_eq!(audio.generation, 1);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].volume.to_bits(), 0.125_f32.to_bits());
        assert_eq!(audio.events["click"].volume_override, Some(0.125));
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn active_volume_override_preserves_the_sampled_shift_and_reset_leaves_it_active() {
        use bevy::prelude::*;
        use generals_sim::skirmish::map_scripts::AudioAction;
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        battle.started = false;
        let map = generals_assets::map::MapData::parse(
            &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
        )
        .unwrap();
        let terrain = crate::terrain::Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
        let mouse = crate::input::load(
            &assets,
            std::path::Path::new(""),
            &battle.simulation.catalog,
        )
        .unwrap();
        let mut audio = crate::source_audio::speech_tests::audio();
        let event = audio.events.get_mut("click").unwrap();
        event.volume = 0.3;
        event.volume_shift = -0.2;
        let shift = crate::source_audio::random::Random::new(74).real(0.8, 1.0);
        audio.ui("click");
        let mut app = App::new();
        app.init_resource::<Assets<super::Sequence>>()
            .init_resource::<Time<Real>>()
            .insert_resource(audio)
            .insert_resource(battle)
            .insert_resource(terrain)
            .insert_resource(mouse)
            .add_systems(Update, super::update);
        app.update();
        app.world_mut()
            .resource_mut::<super::SourceAudio>()
            .map_script_actions(&[AudioAction::OverrideVolume("click".into(), 2.0)]);
        app.update();
        let world = app.world_mut();
        let (playing, settings) = world
            .query::<(&super::Playing, &PlaybackSettings)>()
            .single(world)
            .unwrap();
        assert_eq!(playing.volume.to_bits(), (2.0 * shift).to_bits());
        assert_eq!(
            settings.volume.to_linear().to_bits(),
            (2.0 * shift * 0.2).to_bits()
        );
        world
            .resource_mut::<super::SourceAudio>()
            .map_script_actions(&[
                AudioAction::OverrideVolume("click".into(), -1.0),
                AudioAction::OverrideVolume(String::new(), 0.0),
            ]);
        app.update();
        let world = app.world_mut();
        let playing = world.query::<&super::Playing>().single(world).unwrap();
        assert_eq!(playing.volume.to_bits(), (2.0 * shift).to_bits());
    }

    fn request() -> Request {
        Request {
            firing: None,
            uninterruptible: false,
            event: "voice".to_owned(),
            object: Some(7),
            projectile: None,
            position: None,
            samples: Vec::new(),
            looping: None,
            sample_names: Vec::new(),
            pitch: 1.0,
            volume: 1.0,
            volume_shift: 1.0,
            delay_ms: 0.0,
        }
    }

    fn playing(name: &str, object: usize, voice: bool) -> Active {
        Active {
            entity: Entity::PLACEHOLDER,
            event: name.to_owned(),
            object: Some(object),
            positional: false,
            voice,
            priority: 1,
            serial: 0,
        }
    }

    #[test]
    fn same_object_voice_is_suppressed_but_weapon_is_not_a_voice() {
        let mut event = data::default_event();
        event.voice = true;
        event.priority = 3;
        assert!(matches!(
            admission(
                &event,
                &request(),
                &[playing("previous", 7, true)],
                &[],
                [4, 25]
            ),
            Admission::Reject
        ));
        assert!(!matches!(
            admission(
                &event,
                &request(),
                &[playing("weapon", 7, false)],
                &[],
                [4, 25]
            ),
            Admission::Reject
        ));
        assert!(!matches!(
            admission(
                &event,
                &request(),
                &[playing("other", 8, true)],
                &[],
                [4, 25]
            ),
            Admission::Reject
        ));
    }

    #[test]
    fn immediate_pending_voices_do_not_trigger_active_voice_exclusion() {
        let mut event = data::default_event();
        event.voice = true;
        event.limit = 1;
        assert!(matches!(
            admission(
                &event,
                &request(),
                &[],
                &[("selection".to_owned(), false)],
                [4, 25]
            ),
            Admission::Play
        ));
        assert!(matches!(
            admission(
                &event,
                &request(),
                &[playing("selection", 7, true)],
                &[],
                [4, 25]
            ),
            Admission::Reject
        ));
    }

    #[test]
    fn interrupt_replaces_oldest_but_cannot_exceed_same_frame_limit() {
        let mut event = data::default_event();
        event.control = data::INTERRUPT;
        event.limit = 1;
        assert!(matches!(admission(
                &event,
                &request(),
                &[playing("voice", 9, false)],
                &[],
                [4, 25]
            ), Admission::Replace(entity) if entity == Entity::PLACEHOLDER));
        assert!(matches!(
            admission(
                &event,
                &request(),
                &[],
                &[("voice".to_owned(), false)],
                [4, 25]
            ),
            Admission::Reject
        ));
    }

    #[test]
    fn saturated_channel_requires_strictly_lower_priority_victim() {
        let mut event = data::default_event();
        event.priority = 1;
        assert!(matches!(
            admission(
                &event,
                &request(),
                &[playing("other", 8, false)],
                &[],
                [1, 25]
            ),
            Admission::Reject
        ));
        event.priority = 2;
        assert!(matches!(admission(
                &event,
                &request(),
                &[playing("other", 8, false)],
                &[],
                [1, 25]
            ), Admission::Replace(entity) if entity == Entity::PLACEHOLDER));
    }
}
