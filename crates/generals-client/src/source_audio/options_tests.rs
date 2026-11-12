//! Pending Bevy players retain current source options before a sink exists.
use super::*;
use crate::terrain::Terrain;
use bevy::audio::AudioSink;
use std::path::Path;

#[ignore = "requires original game installation"]
#[test]
fn installed_options_volume_changes_reach_pending_sfx_and_speech_without_sink() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let map = generals_assets::map::MapData::parse(
        &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
    )
    .unwrap();
    let terrain = Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
    let mouse = crate::input::load(&assets, Path::new(""), &battle.simulation.catalog).unwrap();
    let mut audio = load(&assets, &battle.simulation.catalog, Path::new("")).unwrap();
    audio.generation = battle.generation;
    audio.apply_volumes(1.0, 1.0, 1.0, 1.0);
    audio.ui("MoneyWithdraw");
    audio.speech("EvaUSA_Victory", true);
    assert_eq!(audio.requests.len(), 2);
    assert!(
        audio
            .requests
            .iter()
            .all(|request| !request.samples.is_empty())
    );
    let bases: BTreeMap<_, _> = audio
        .requests
        .iter()
        .map(|request| (request.event.clone(), request.volume))
        .collect();
    let mut app = App::new();
    // No AudioPlugin: production playback creates real pending AudioPlayer and
    // PlaybackSettings components; platform sink attachment has not happened.
    app.insert_resource(audio)
        .insert_resource(mouse)
        .insert_resource(battle)
        .insert_resource(terrain)
        .insert_resource(Time::<Real>::default())
        .insert_resource(Assets::<Sequence>::default())
        .add_systems(Update, playback::update);
    app.update();
    for (sound, settings, sink) in app
        .world_mut()
        .query::<(&playback::Playing, &PlaybackSettings, Option<&AudioSink>)>()
        .iter(app.world())
    {
        assert!(sink.is_none());
        assert_eq!(
            settings.volume.to_linear().to_bits(),
            bases[&sound.event].to_bits()
        );
    }
    for (sound_volume, speech_volume) in [(0.25, 0.5), (0.0, 0.0), (0.75, 1.0)] {
        app.world_mut().resource_mut::<SourceAudio>().apply_volumes(
            1.0,
            sound_volume,
            1.0,
            speech_volume,
        );
        app.update();
        let mut count = 0;
        for (sound, settings, sink) in app
            .world_mut()
            .query::<(&playback::Playing, &PlaybackSettings, Option<&AudioSink>)>()
            .iter(app.world())
        {
            assert!(sink.is_none());
            let volume = if sound.event == "EvaUSA_Victory" {
                speech_volume
            } else {
                sound_volume
            };
            assert_eq!(
                settings.volume.to_linear().to_bits(),
                (bases[&sound.event] * volume).to_bits()
            );
            count += 1;
        }
        assert_eq!(count, 2);
    }
}
