use super::{SourceAudio, data, looping, random::Random};
use crate::skirmish_data::{definition, field, scalar};
use generals_assets::{content, virtual_assets::VirtualAssets};
use rodio::buffer::SamplesBuffer;
use std::{
    collections::BTreeMap,
    error::Error,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

pub(super) struct State {
    pub(super) sounds: [String; 4],
}

impl State {
    pub(super) const fn new(sounds: [String; 4]) -> Self {
        Self { sounds }
    }

    pub(super) fn load(assets: &VirtualAssets) -> Result<Self, Box<dyn Error>> {
        let files = [content::scan(assets.read("data/ini/miscaudio.ini")?)?];
        let lines = &definition(&files, "MiscAudio", "")?.lines;
        let sound = |key| -> Result<String, Box<dyn Error>> {
            Ok(field(lines, 0, key)
                .map(scalar)
                .transpose()?
                .unwrap_or_default()
                .into())
        };
        Ok(Self::new([
            sound("BuildingDisabled")?,
            sound("BuildingReenabled")?,
            sound("VehicleDisabled")?,
            sound("VehicleReenabled")?,
        ]))
    }
}

pub(super) fn validate_event(event: &data::Event) -> Result<(), Box<dyn Error>> {
    // AC_ALL does not start loops: Miles only reselects bodies for AC_LOOP.
    // Finite attack/body/decay portions use the ordinary sequence decoder.
    if event.control & data::LOOP == 0 {
        return Ok(());
    }
    if event.control & !data::ALL != data::LOOP | data::RANDOM
        || event.loop_count != 0
        || event.attack.len() > 1
        || event.decay.len() > 1
        || event.sounds.is_empty()
        || event.speech
        || event.voice
    {
        return Err(format!(
            "{}: unrepresented native audio loop controls or portions",
            event.name
        )
        .into());
    }
    Ok(())
}

pub(super) fn validate_samples(
    event: &data::Event,
    samples: &BTreeMap<String, Option<SamplesBuffer>>,
) -> Result<(), Box<dyn Error>> {
    if event.control & data::LOOP == 0 {
        return Ok(());
    }
    for name in event.attack.iter().chain(&event.sounds).chain(&event.decay) {
        let Some(sample) = samples.get(name).and_then(Option::as_ref) else {
            // AudioFileCache::openFile fails the selected portion at playback.
            // Retain missing randomized slots; removing them changes selection.
            continue;
        };
        // The decoder converts each original recording's rate/channels lazily.
        // Missing slots remain intact; present portions must still contain PCM.
        if sample.clone().next().is_none() {
            return Err(format!("{}: loop portion has no native samples", event.name).into());
        }
    }
    Ok(())
}

pub(super) fn loop_template(
    event: &data::Event,
    samples: &BTreeMap<String, Option<SamplesBuffer>>,
    previous: usize,
    random: Arc<Mutex<Random>>,
) -> Option<looping::Template> {
    (event.control & data::LOOP != 0).then(|| looping::Template {
        attack: event.attack.first().and_then(|name| samples[name].clone()),
        decay: event.decay.first().and_then(|name| samples[name].clone()),
        bodies: event
            .sounds
            .iter()
            .map(|name| samples[name].clone())
            .collect(),
        previous,
        delay: event.delay,
        pending: Arc::new(Mutex::new(None)),
        random,
        stop: Arc::new(AtomicBool::new(false)),
    })
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct HackerAudioInspection<'w, 's> {
    audio: bevy::prelude::Res<'w, SourceAudio>,
    sinks: bevy::prelude::Query<
        'w,
        's,
        (
            &'static super::playback::Playing,
            &'static bevy::prelude::AudioSink,
        ),
    >,
}

impl HackerAudioInspection<'_, '_> {
    pub fn running(&self, actor: usize, expected: &str) -> bool {
        use bevy::audio::AudioSinkPlayback;
        self.audio.object_sounds.running(actor, expected)
            && self.sinks.iter().any(|(playing, sink)| {
                playing.object == Some(actor)
                    && playing.event == expected
                    && !sink.empty()
                    && sink.position().as_secs_f64() > 0.025
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_thunder_keeps_seven_original_samples_and_initial_and_repeated_delays() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let mut definitions = data::definitions(&assets).unwrap();
        let event = definitions.remove("ThunderAmbienceStereo").unwrap();
        // INIZH.big SoundEffects.ini:7346 and AudioZH.big use letter suffixes.
        assert_eq!(
            event.sounds,
            [
                "gthuamba", "gthuambb", "gthuambc", "gthuambd", "gthuambe", "gthuambf", "gthuambg"
            ]
        );
        assert_eq!(
            event.delay.map(f32::to_bits),
            [60_000.0_f32, 100_000.0].map(f32::to_bits)
        );
        assert_eq!(event.control, data::LOOP | data::ALL | data::RANDOM);
        assert_eq!(event.loop_count, 0);
        assert!(event.attack.is_empty() && event.decay.is_empty());
        let mut audio = super::super::speech_tests::audio();
        audio.settings = data::settings(&assets, std::path::Path::new("")).unwrap();
        data::load_event(&assets, &audio.settings, &event, &mut audio.samples).unwrap();
        assert!(
            event
                .sounds
                .iter()
                .all(|name| audio.samples[name].is_some())
        );
        audio.events.insert(event.name.clone(), event);
        let mut expected = Random::new(74);
        let (mut previous, initial_delay, pitch, shift) =
            super::super::play_info(&audio.events["ThunderAmbienceStereo"], &mut expected, None);
        audio.ui("ThunderAmbienceStereo");
        let request = audio.requests.pop().unwrap();
        assert_eq!(request.delay_ms.to_bits(), initial_delay.to_bits());
        assert!((60_000.0..=100_000.0).contains(&request.delay_ms));
        assert_eq!(request.pitch.to_bits(), pitch.to_bits());
        assert_eq!(request.volume_shift.to_bits(), shift.to_bits());
        let mut template = request.looping.unwrap();
        for _ in 0..8 {
            let frames = template.bodies[previous].as_ref().unwrap().clone().count();
            // Drain the actual installed recording, ending at the callback's
            // delay handoff. No silent samples stand in for a pending request.
            assert_eq!(template.decoder().count(), frames);
            let selected = super::super::random::choice(&mut expected, 7, Some(previous), true);
            let delay = expected.real(60_000.0, 100_000.0);
            assert_eq!(
                template.take_continuation().unwrap().to_bits(),
                delay.to_bits()
            );
            assert!((60_000.0..=100_000.0).contains(&delay));
            assert_eq!(template.previous, selected);
            assert_ne!(selected, previous);
            previous = selected;
        }
        assert_eq!(
            audio.random.lock().unwrap().index(10_000),
            expected.index(10_000)
        );
    }

    #[test]
    fn missing_loop_recordings_fail_only_when_their_native_portion_is_opened() {
        let mut audio = super::super::speech_tests::audio();
        let mut event = data::default_event();
        event.name = "original-loop".into();
        event.control = data::LOOP | data::RANDOM | data::ALL;
        event.attack = vec!["click".into()];
        event.sounds = vec!["missing-body".into()];
        event.decay = vec!["click".into()];
        audio.samples.insert("missing-body".into(), None);
        validate_event(&event).unwrap();
        validate_samples(&event, &audio.samples).unwrap();
        audio.events.insert(event.name.clone(), event);
        audio.queue("original-loop", Some(1), None, None, false);
        let request = audio.requests.pop().expect("existing attack must start");
        let template = request.looping.unwrap();
        assert_eq!(template.bodies.len(), 1);
        assert!(template.bodies[0].is_none());
        assert_eq!(template.decoder().count(), 64);
        audio
            .events
            .get_mut("original-loop")
            .unwrap()
            .attack
            .clear();
        audio.queue("original-loop", Some(1), None, None, false);
        assert!(
            audio.requests.is_empty(),
            "missing initial body cannot start"
        );
    }
}
