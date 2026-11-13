use super::{SourceAudio, data, music::Music, play_info, random::Random};
use bevy::prelude::Vec3;
use std::collections::BTreeMap;

pub(super) fn audio() -> SourceAudio {
    let mut events = BTreeMap::new();
    let mut samples = BTreeMap::new();
    for (name, speech) in [("first", true), ("second", true), ("click", false)] {
        let mut event = data::default_event();
        event.name = name.into();
        event.speech = speech;
        event.volume = 1.0;
        event.sounds = vec![name.into()];
        events.insert(name.into(), event);
        samples.insert(
            name.into(),
            Some(rodio::buffer::SamplesBuffer::new(
                std::num::NonZeroU16::new(1).expect("one channel"),
                std::num::NonZeroU32::new(22_050).expect("source sample rate"),
                vec![0.1; 32],
            )),
        );
    }
    SourceAudio {
        speech_disallowed: false,
        events,
        samples,
        units: Vec::new(),
        settings: data::Settings {
            speech_volume: 0.7,
            streaming_folder: "Speech".into(),
            sound_volume: 0.2,
            world_volume: 0.3,
            sample_counts: [4, 25],
            minimum_volume: 0.02,
            fade: true,
            exponent: 4.0,
            microphone_height: 0.0,
            microphone_fraction: 0.0,
            zoom: [0.0; 3],
            root: "Data/Audio".into(),
            folder: "Sounds".into(),
            extension: "wav".into(),
        },
        random: std::sync::Arc::new(std::sync::Mutex::new(Random::new(74))),
        firing: super::firing::State::new(),
        wheels: super::wheels::State::new(),
        hazards: super::hazards::State::new(),
        fires: super::fire::State::new(),
        aircraft: super::aircraft::State::new(),
        capture: super::capture::State::new([String::new(), String::new()]),
        radar: super::radar::State::new([String::new(), String::new()]),
        sabotage: super::sabotage::State::fixture(),
        salvage: [String::new(), String::new(), String::new()],
        car_bomb: [String::new(), String::new(), String::new()],
        pilot: String::new(),
        object_fx: BTreeMap::new(),
        object_sounds: super::object_sounds::State::new(),
        research: Vec::new(),
        requests: Vec::new(),
        volume_changes: Vec::new(),
        listener: Vec3::ZERO,
        zoom_volume: 1.0,
        generation: 0,
        serial: 0,
        music: Music::test_fixture(),
    }
}

#[test]
fn protected_speech_rejects_new_requests_before_random_and_keeps_effects_enabled() {
    let mut audio = audio();
    audio.speech_disallowed = true;
    audio.events.get_mut("first").expect("speech").pitch = [0.8, 1.2];
    audio.speech("first", false);
    audio.speech("first", true);
    assert!(audio.requests.is_empty());
    assert_eq!(
        audio.random.lock().unwrap().index(10_000),
        Random::new(74).index(10_000)
    );
    audio.ui("click");
    assert_eq!(audio.requests.len(), 1);
    assert_eq!(audio.requests[0].event, "click");
    assert_eq!(
        audio.volume(&audio.events["first"], 1.0, None).to_bits(),
        0.7_f32.to_bits()
    );
    assert_eq!(
        audio.volume(&audio.events["click"], 1.0, None).to_bits(),
        0.2_f32.to_bits()
    );
}

#[test]
fn speech_uses_literal_file_and_source_rate_but_preserves_audio_random_draws() {
    let mut event = data::default_event();
    event.speech = true;
    event.sounds = vec!["original.wav".into()];
    event.delay = [10.0, 250.0];
    event.pitch = [0.8, 1.2];
    event.volume = 0.7;
    event.volume_shift = -0.2;
    let mut actual = Random::new(74);
    let mut expected = Random::new(74);
    expected.real(0.8, 1.2);
    let expected_volume = 0.7 * expected.real(0.8, 1.0);
    let (file, delay, pitch, volume_shift) = play_info(&event, &mut actual, Some(7));
    assert_eq!(file, 0);
    assert_eq!(delay.to_bits(), 0.0_f32.to_bits());
    assert_eq!(pitch.to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        (event.volume * volume_shift).to_bits(),
        expected_volume.to_bits()
    );
    assert_eq!(actual.index(10_000), expected.index(10_000));
}
