//! Deferred audio ownership without requiring a platform sound device.
use super::{Sequence, before_audio_backend};
use bevy::{audio::Decodable, prelude::*, transform::TransformSystems};

#[derive(Component)]
struct Retiring;

#[derive(Component)]
struct BackendSink;

fn retire<T: Asset + Decodable>(
    mut commands: Commands,
    sounds: Query<Entity, (With<AudioPlayer<T>>, With<Retiring>)>,
) {
    for entity in &sounds {
        commands.entity(entity).despawn();
    }
    commands.spawn(AudioPlayer::<T>(Handle::default()));
}

fn attach_sink<T: Asset + Decodable>(
    mut commands: Commands,
    sounds: Query<Entity, (With<AudioPlayer<T>>, Without<BackendSink>)>,
) {
    // Bevy's play_queued_audio_system queues this insertion once a pending
    // player's recording is ready. The mixer itself is irrelevant to the race.
    for entity in &sounds {
        commands.entity(entity).insert(BackendSink);
    }
}

fn propagation_boundary() {}

fn fixture(ordered: bool, reset: bool) -> (App, Vec<Entity>, Vec<Entity>) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_systems(
        PostUpdate,
        (
            propagation_boundary.in_set(TransformSystems::Propagate),
            attach_sink::<Sequence>
                .after(TransformSystems::Propagate)
                .after_ignore_deferred(retire::<Sequence>),
            attach_sink::<AudioSource>
                .after(TransformSystems::Propagate)
                .after_ignore_deferred(retire::<AudioSource>),
        ),
    );
    let retirement = (retire::<Sequence>, retire::<AudioSource>);
    if ordered {
        app.add_systems(PostUpdate, before_audio_backend(retirement));
    } else {
        app.add_systems(PostUpdate, retirement);
    }
    let mut retired = vec![
        app.world_mut()
            .spawn((AudioPlayer::<Sequence>(Handle::default()), Retiring))
            .id(),
        app.world_mut()
            .spawn((AudioPlayer::<AudioSource>(Handle::default()), Retiring))
            .id(),
    ];
    let mut survivors = vec![
        app.world_mut()
            .spawn(AudioPlayer::<Sequence>(Handle::default()))
            .id(),
        app.world_mut()
            .spawn(AudioPlayer::<AudioSource>(Handle::default()))
            .id(),
    ];
    if reset {
        for entity in std::mem::take(&mut survivors) {
            app.world_mut().entity_mut(entity).insert(Retiring);
            retired.push(entity);
        }
    }
    (app, retired, survivors)
}

fn verify_retirement(reset: bool) {
    let (mut app, retired, survivors) = fixture(true, reset);
    app.update();
    for entity in retired {
        assert!(app.world().get_entity(entity).is_err());
    }
    for entity in survivors {
        assert!(app.world().get::<BackendSink>(entity).is_some());
    }
    let world = app.world_mut();
    // Both sound and music replacements must reach the backend this iteration.
    assert_eq!(
        world.query::<&BackendSink>().iter(world).count(),
        if reset { 2 } else { 4 }
    );
    assert_eq!(
        world
            .query_filtered::<Entity, (With<AudioPlayer<Sequence>>, Without<BackendSink>)>()
            .iter(world)
            .count(),
        0
    );
}

#[test]
fn interrupted_pending_audio_is_retired_before_sink_attachment() {
    verify_retirement(false);
}

#[test]
fn match_reset_retires_pending_sound_and_music_before_sink_attachment() {
    verify_retirement(true);
}

#[test]
#[should_panic(expected = "Entity despawned")]
fn unordered_pending_audio_reproduces_stale_entity_insertion() {
    let (mut app, _, _) = fixture(false, false);
    app.update();
}

fn attach_decoded_sinks(
    mut commands: Commands,
    assets: Res<Assets<Sequence>>,
    sounds: Query<(Entity, &AudioPlayer<Sequence>), Without<AudioSink>>,
) {
    let assets = assets.into_inner();
    // Match Bevy 0.19.1's nonspatial ONCE attachment command, without opening a
    // hardware device. The production request path supplies the decoded source.
    for (entity, player) in &sounds {
        let source = assets.get(&player.0).expect("decoded original sample");
        let (sink, _) = rodio::Player::new();
        sink.append(source.decoder());
        commands.entity(entity).insert(AudioSink::new(sink));
    }
}

fn pending_crusader_shots(ordered: bool) -> (App, Vec<Entity>) {
    use super::{SourceAudio, data, playback};
    use generals_assets::virtual_assets::VirtualAssets;
    use std::path::Path;

    let root = generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut audio = super::speech_tests::audio();
    audio.events = data::definitions(&assets).unwrap();
    audio.settings = data::settings(&assets, Path::new("")).unwrap();
    audio.samples.clear();
    for name in ["CrusaderTankWeapon", "CrusaderTankVoiceSelect"] {
        data::load_event(
            &assets,
            &audio.settings,
            &audio.events[name],
            &mut audio.samples,
        )
        .unwrap();
    }
    assert_eq!(audio.events["CrusaderTankWeapon"].limit, 2);
    assert_ne!(
        audio.events["CrusaderTankWeapon"].control & data::INTERRUPT,
        0
    );
    assert_eq!(
        audio.events["CrusaderTankVoiceSelect"].control & data::INTERRUPT,
        0
    );
    audio.fx("CrusaderTankWeapon", [0.0; 3]);
    audio.fx("CrusaderTankWeapon", [0.0; 3]);
    let mut app = App::new();
    app.init_resource::<Assets<Sequence>>()
        .init_resource::<Time<Real>>()
        .insert_resource(audio);
    // The old unordered schedule could run playback after the backend, leaving
    // these admitted players pending until the next display iteration.
    app.world_mut().run_system_cached(playback::update).unwrap();
    let world = app.world_mut();
    let pending: Vec<_> = world
        .query_filtered::<Entity, (With<AudioPlayer<Sequence>>, Without<AudioSink>)>()
        .iter(world)
        .collect();
    assert_eq!(pending.len(), 2);
    let mut audio = world.resource_mut::<SourceAudio>();
    audio.fx("CrusaderTankWeapon", [0.0; 3]);
    let _ = audio.queue("CrusaderTankVoiceSelect", Some(7), None, None, false);
    assert_eq!(audio.requests.len(), 2);
    assert!(
        audio
            .requests
            .iter()
            .all(|request| !request.samples.is_empty())
    );
    app.add_systems(
        PostUpdate,
        (
            propagation_boundary.in_set(TransformSystems::Propagate),
            attach_decoded_sinks
                .after(TransformSystems::Propagate)
                .after_ignore_deferred(playback::update),
        ),
    );
    if ordered {
        app.add_systems(PostUpdate, before_audio_backend(playback::update));
    } else {
        app.add_systems(PostUpdate, playback::update);
    }
    (app, pending)
}

#[ignore = "requires original game installation"]
#[test]
fn installed_crusader_shot_replacement_and_selection_attach_after_retirement() {
    let (mut app, pending) = pending_crusader_shots(true);
    app.update();
    assert_eq!(
        pending
            .iter()
            .filter(|&&entity| app.world().get_entity(entity).is_err())
            .count(),
        1
    );
    assert_eq!(
        pending
            .iter()
            .filter(|&&entity| app.world().get::<AudioSink>(entity).is_some())
            .count(),
        1
    );
    let world = app.world_mut();
    let mut playing: Vec<_> = world
        .query::<(&super::playback::Playing, &AudioSink)>()
        .iter(world)
        .map(|(playing, _)| playing.event.as_str())
        .collect();
    playing.sort_unstable();
    assert_eq!(
        playing,
        [
            "CrusaderTankVoiceSelect",
            "CrusaderTankWeapon",
            "CrusaderTankWeapon"
        ]
    );
}

#[ignore = "requires original game installation"]
#[test]
#[should_panic(expected = "Entity despawned")]
fn installed_unordered_crusader_shot_replacement_reproduces_stale_sink_insertion() {
    let (mut app, _) = pending_crusader_shots(false);
    app.update();
}
