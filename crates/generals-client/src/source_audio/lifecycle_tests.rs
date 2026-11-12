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
