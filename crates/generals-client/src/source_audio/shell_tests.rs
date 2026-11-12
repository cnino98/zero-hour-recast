use super::{Sequence, SourceAudio, load_shell, music, playback, random};
use bevy::prelude::*;
use generals_assets::virtual_assets::VirtualAssets;
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Arc, Mutex},
};

fn assets() -> VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

fn shell_app(audio: SourceAudio) -> App {
    let mut app = App::new();
    app.init_resource::<Assets<Sequence>>()
        .init_resource::<Assets<AudioSource>>()
        .init_resource::<Time<Real>>()
        .insert_resource(audio)
        .add_systems(FixedFirst, playback::reset_before_logic)
        .add_systems(PostUpdate, (playback::update, music::update))
        .add_systems(PostUpdate, music::fade_retiring.after(music::update));
    app
}

#[ignore = "requires original game installation"]
#[test]
fn shell_load_decodes_only_original_click_samples_and_has_no_gameplay_bindings() {
    let audio = load_shell(&assets(), Path::new("")).unwrap();
    assert_eq!(
        audio.events.keys().map(String::as_str).collect::<Vec<_>>(),
        ["GUIClick", "GUIClickDisabled"]
    );
    assert_eq!(
        audio.samples.keys().map(String::as_str).collect::<Vec<_>>(),
        ["umenucla", "umenucle"]
    );
    assert!(audio.samples.values().all(Option::is_some));
    assert!(audio.units.is_empty());
    assert!(audio.research.is_empty());
    assert!(audio.object_fx.is_empty());
    assert_eq!(audio.music.speech_names().count(), 0);
    let mut app = shell_app(audio);
    app.update();
    let sources = app.world().resource::<Assets<AudioSource>>();
    assert_eq!(sources.len(), 1);
    let (_, original) = sources.iter().next().unwrap();
    assert_eq!(
        original.bytes.as_ref(),
        assets().read("data/audio/tracks/usa_11.mp3").unwrap().bytes
    );
}

#[ignore = "requires original game installation"]
#[test]
fn shell_click_and_music_play_without_match_mouse_or_duplicate_players() {
    let mut audio = load_shell(&assets(), Path::new("")).unwrap();
    audio.random = Arc::new(Mutex::new(random::Random::new(74)));
    let mut expected = random::Random::new(74);
    super::play_info(&audio.events["GUIClick"], &mut expected, None);
    audio.ui("GUIClick");
    assert_eq!(audio.requests.len(), 1);
    assert_eq!(
        audio.random.lock().unwrap().real(0.0, 1.0).to_bits(),
        expected.real(0.0, 1.0).to_bits()
    );
    let mut app = shell_app(audio);
    assert!(!app.world().contains_resource::<crate::playtest::Match>());
    assert!(!app.world().contains_resource::<crate::input::Mouse>());
    assert!(!app.world().contains_resource::<crate::terrain::Terrain>());
    app.world_mut().run_schedule(FixedFirst);
    for _ in 0..3 {
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.query::<&AudioPlayer<Sequence>>().iter(world).count(),
            1
        );
        assert_eq!(world.query::<&AudioPlayer>().iter(world).count(), 1);
    }
    let world = app.world_mut();
    let settings = world
        .query_filtered::<&PlaybackSettings, With<AudioPlayer>>()
        .single(world)
        .unwrap();
    assert!(matches!(settings.mode, bevy::audio::PlaybackMode::Loop));
    let sound = world.query::<&playback::Playing>().single(world).unwrap();
    assert_eq!(sound.event, "GUIClick");
    assert_eq!(sound.sample_names, ["umenucla"]);
}

#[ignore = "requires original game installation"]
#[test]
fn leaving_shell_retires_pending_players_before_replacing_audio() {
    let mut audio = load_shell(&assets(), Path::new("")).unwrap();
    audio.ui("GUIClickDisabled");
    let mut app = shell_app(audio);
    app.update();
    let world = app.world_mut();
    let before: BTreeSet<_> = world
        .query_filtered::<Entity, Or<(With<AudioPlayer<Sequence>>, With<AudioPlayer>)>>()
        .iter(world)
        .collect();
    assert_eq!(before.len(), 2);
    super::retire_map_players(world);
    world.insert_resource(super::speech_tests::audio());
    assert!(
        before
            .into_iter()
            .all(|entity| world.get_entity(entity).is_err())
    );
    app.update();
    let world = app.world_mut();
    assert_eq!(
        world.query::<&AudioPlayer<Sequence>>().iter(world).count(),
        0
    );
    assert_eq!(world.query::<&AudioPlayer>().iter(world).count(), 0);
    assert!(world.resource::<SourceAudio>().requests.is_empty());
}

#[ignore = "requires original game installation"]
#[test]
#[should_panic(expected = "a loaded match requires mouse input for audio")]
fn loaded_match_keeps_required_mouse_audio_contract() {
    let (_, battle) = crate::cleanup_verification::tests::installed();
    let mut app = shell_app(super::speech_tests::audio());
    app.insert_resource(battle);
    app.world_mut().run_system_cached(playback::update).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
#[should_panic(expected = "a loaded match requires its results screen for music")]
fn loaded_match_keeps_required_results_music_contract() {
    let (_, battle) = crate::cleanup_verification::tests::installed();
    let mut app = shell_app(super::speech_tests::audio());
    app.insert_resource(battle);
    app.world_mut().run_system_cached(music::update).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
fn original_player_template_loading_tracks_keep_gui_audio_and_current_volume() {
    use crate::skirmish_data::{definition, field, scalar};
    use generals_assets::content;
    let assets = assets();
    let templates = [content::scan(assets.read("data/ini/playertemplate.ini").unwrap()).unwrap()];
    for (template, expected, filename) in [
        ("FactionAmerica", "Load_USA", "usa_08.mp3"),
        ("FactionChina", "Load_China", "chi_03.mp3"),
        ("FactionGLA", "Load_GLA", "gla_02.mp3"),
    ] {
        let lines = &definition(&templates, "PlayerTemplate", template)
            .unwrap()
            .lines;
        let name = scalar(field(lines, 0, "LoadScreenMusic").unwrap()).unwrap();
        assert_eq!(name, expected);
        let mut app = shell_app(load_shell(&assets, Path::new("")).unwrap());
        app.update();
        {
            let mut audio = app.world_mut().resource_mut::<SourceAudio>();
            audio.apply_volumes(0.0, 0.2, 0.3, 0.7);
            audio.begin_load_screen(&assets, name).unwrap();
            audio.ui("GUIClick");
        }
        app.update();
        let world = app.world_mut();
        let original = assets
            .read(&format!("data/audio/tracks/{filename}"))
            .unwrap()
            .bytes;
        let sources = world.resource::<Assets<AudioSource>>();
        assert_eq!(sources.len(), 2, "only shell and selected faction track");
        let handle = sources
            .iter()
            .find(|(_, source)| source.bytes.as_ref() == original)
            .unwrap()
            .0;
        let settings = world
            .query::<(&AudioPlayer, &PlaybackSettings)>()
            .iter(world)
            .find(|(player, _)| player.0.id() == handle)
            .unwrap()
            .1;
        assert_eq!(settings.volume.to_linear().to_bits(), 0.0_f32.to_bits());
        assert!(matches!(settings.mode, bevy::audio::PlaybackMode::Loop));
        assert_eq!(
            world.query::<&AudioPlayer<Sequence>>().iter(world).count(),
            1
        );
        let audio = world.resource::<SourceAudio>();
        assert_eq!(audio.samples.len(), 2);
        assert!(audio.units.is_empty());
        assert_eq!(audio.music.speech_names().count(), 0);
        assert!(audio.music.diagnostic().contains(name));
    }
}

#[ignore = "requires original game installation"]
#[test]
fn loading_failure_returns_to_shell_after_original_fade_without_duplicate_restarts() {
    use crate::skirmish_data::{definition, duration_frames, field};
    use generals_assets::content;
    let assets = assets();
    let settings = [content::scan(assets.read("data/ini/audiosettings.ini").unwrap()).unwrap()];
    let lines = &definition(&settings, "AudioSettings", "").unwrap().lines;
    let frames = duration_frames(field(lines, 0, "TimeToFadeAudio").unwrap()).unwrap();
    let mut app = shell_app(load_shell(&assets, Path::new("")).unwrap());
    app.update();
    let world = app.world_mut();
    let original_shell = world
        .query_filtered::<Entity, With<AudioPlayer>>()
        .single(world)
        .unwrap();
    app.world_mut()
        .resource_mut::<SourceAudio>()
        .begin_load_screen(&assets, "Load_China")
        .unwrap();
    app.update();
    app.world_mut()
        .resource_mut::<SourceAudio>()
        .end_load_screen();
    app.update();
    let world = app.world_mut();
    let shell = world
        .resource::<Assets<AudioSource>>()
        .iter()
        .find(|(_, source)| {
            source.bytes.as_ref() == assets.read("data/audio/tracks/usa_11.mp3").unwrap().bytes
        })
        .unwrap()
        .0;
    // Required source fade may temporarily retain old streams; it is not a new
    // smoothing interval and must retire after the original frame count.
    for _ in 0..=frames {
        app.update();
    }
    let world = app.world_mut();
    assert_eq!(world.query::<&AudioPlayer>().iter(world).count(), 1);
    let only = world
        .query::<(Entity, &AudioPlayer)>()
        .single(world)
        .unwrap();
    assert_eq!(only.1.0.id(), shell);
    let stable = only.0;
    assert!(world.get_entity(original_shell).is_err());
    world.resource_mut::<SourceAudio>().end_load_screen();
    app.update();
    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<AudioPlayer>>()
            .single(world)
            .unwrap(),
        stable
    );
    assert!(
        world
            .resource_mut::<SourceAudio>()
            .begin_load_screen(&assets, "MissingOriginalLoadingMusic")
            .is_err()
    );
    app.update();
    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<AudioPlayer>>()
            .single(world)
            .unwrap(),
        stable
    );
    assert_eq!(world.resource::<SourceAudio>().samples.len(), 2);
}

#[ignore = "requires original game installation"]
#[test]
fn successful_load_preserves_actual_music_player_until_source_fade_finishes() {
    use crate::skirmish_data::{definition, duration_frames, field};
    use generals_assets::content;
    let assets = assets();
    let settings = [content::scan(assets.read("data/ini/audiosettings.ini").unwrap()).unwrap()];
    let lines = &definition(&settings, "AudioSettings", "").unwrap().lines;
    let frames =
        u16::try_from(duration_frames(field(lines, 0, "TimeToFadeAudio").unwrap()).unwrap())
            .unwrap();
    let mut app = shell_app(load_shell(&assets, Path::new("")).unwrap());
    app.update();
    {
        let mut audio = app.world_mut().resource_mut::<SourceAudio>();
        audio.begin_load_screen(&assets, "Load_USA").unwrap();
        audio.ui("GUIClick");
    }
    app.update();
    let world = app.world_mut();
    let original = assets.read("data/audio/tracks/usa_08.mp3").unwrap().bytes;
    let source = world
        .resource::<Assets<AudioSource>>()
        .iter()
        .find(|(_, source)| source.bytes.as_ref() == original)
        .unwrap()
        .0;
    let (entity, player, settings) = world
        .query::<(Entity, &AudioPlayer, &PlaybackSettings)>()
        .iter(world)
        .find(|(_, player, _)| player.0.id() == source)
        .unwrap();
    let handle = player.0.clone();
    let volume = settings.volume.to_linear();
    super::finish_load_screen(world);
    super::retire_map_players(world);
    assert_eq!(world.get::<AudioPlayer>(entity).unwrap().0, handle);
    assert!(world.get::<music::RetiringMusic>(entity).is_some());
    assert_eq!(
        world.query::<&AudioPlayer<Sequence>>().iter(world).count(),
        0
    );
    let mut next = super::speech_tests::audio();
    // The replacement receives the same Options volume as the loaded world.
    next.apply_volumes(0.55, 0.2, 0.3, 0.7);
    world.insert_resource(next);
    app.update();
    let world = app.world_mut();
    assert_eq!(world.get::<AudioPlayer>(entity).unwrap().0, handle);
    let faded = world
        .get::<PlaybackSettings>(entity)
        .unwrap()
        .volume
        .to_linear();
    assert_eq!(
        faded.to_bits(),
        (volume * (1.0 - 1.0 / f32::from(frames))).to_bits()
    );
    assert!(faded > 0.0);
    for _ in 1..frames {
        app.update();
    }
    let world = app.world_mut();
    assert!(
        world.get_entity(entity).is_ok(),
        "source stops after the zero-volume update"
    );
    assert_eq!(
        world
            .get::<PlaybackSettings>(entity)
            .unwrap()
            .volume
            .to_linear()
            .to_bits(),
        0.0_f32.to_bits()
    );
    app.update();
    let world = app.world_mut();
    assert!(world.get_entity(entity).is_err());
    assert_eq!(
        world.query::<&music::RetiringMusic>().iter(world).count(),
        0
    );
    assert_eq!(world.query::<&AudioPlayer>().iter(world).count(), 0);
}

pub fn retiring_load_screen() -> (World, Entity) {
    let assets = assets();
    let mut app = shell_app(load_shell(&assets, Path::new("")).unwrap());
    app.update();
    app.world_mut()
        .resource_mut::<SourceAudio>()
        .begin_load_screen(&assets, "Load_USA")
        .unwrap();
    app.update();
    let world = app.world_mut();
    let original = assets.read("data/audio/tracks/usa_08.mp3").unwrap().bytes;
    let source = world
        .resource::<Assets<AudioSource>>()
        .iter()
        .find(|(_, source)| source.bytes.as_ref() == original)
        .unwrap()
        .0;
    let entity = world
        .query::<(Entity, &AudioPlayer)>()
        .iter(world)
        .find(|(_, player)| player.0.id() == source)
        .unwrap()
        .0;
    super::finish_load_screen(world);
    (std::mem::take(world), entity)
}
