//! Cleanup command audio inspection and source object-identity regression.
use super::{SourceAudio, Voice, playback::Playing};
use bevy::{audio::AudioSinkPlayback, ecs::system::SystemParam, prelude::*};

#[derive(SystemParam)]
pub struct Inspection<'w, 's> {
    audio: Res<'w, SourceAudio>,
    sinks: Query<'w, 's, (&'static Playing, &'static AudioSink)>,
}

impl Inspection<'_, '_> {
    pub fn verify_silent_mode(&self, unit: usize) -> Result<(), &'static str> {
        let name = &self.audio.units[unit].voices[Voice::CleanupMode.index()].name;
        if name != "AmbulanceVoiceModeDetox" {
            return Err("cleanup button lost its original mode voice binding");
        }
        let event = self
            .audio
            .events
            .get(name)
            .ok_or("cleanup mode event missing")?;
        silent_mode(event)
    }

    pub fn played(self, actor: usize, unit: usize) -> [bool; 2] {
        [
            (Voice::CleanupMode, None),
            (Voice::CleanupArea, Some(actor)),
        ]
        .map(|(voice, object)| {
            let event = &self.audio.units[unit].voices[voice.index()].name;
            self.sinks.iter().any(|(playing, sink)| {
                playing.event == *event
                    && playing.object == object
                    && sink.position().as_secs_f64() > 0.01
            })
        })
    }
}

const fn silent_mode(event: &super::data::Event) -> Result<(), &'static str> {
    if event.sounds.is_empty() && event.attack.is_empty() && event.decay.is_empty() {
        Ok(())
    } else {
        Err("original cleanup mode event unexpectedly contains audible samples")
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::{Binding, UnitAudio, Voice, VoiceRequest, speech_tests::audio};

    pub fn assert_candidates(request: &VoiceRequest, expected: &[usize]) {
        let selected: Vec<_> = request.candidates.iter().map(|&(id, _)| id).collect();
        assert_eq!(selected, expected);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_cleanup_mode_is_silent_while_the_area_order_has_original_samples() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let mut events = super::super::data::definitions(&assets).unwrap();
        let mode = events.remove("AmbulanceVoiceModeDetox").unwrap();
        super::silent_mode(&mode).unwrap();
        assert!(super::silent_mode(&events["AmbulanceVoiceDetox"]).is_err());
        let mut audio = audio();
        audio.events.insert(mode.name.clone(), mode);
        assert!(
            audio
                .queue("AmbulanceVoiceModeDetox", None, None, None, false)
                .is_none()
        );
        assert!(audio.requests.is_empty());
    }

    #[test]
    fn cleanup_and_lotus_modes_are_global_while_area_voice_retains_first_selected_object() {
        let mut audio = audio();
        let mut voices: Vec<_> = (0..27)
            .map(|_| Binding {
                name: String::new(),
                previous: None,
            })
            .collect();
        voices[Voice::CleanupMode.index()].name = "first".into();
        voices[Voice::CleanupArea.index()].name = "second".into();
        for voice in [
            Voice::LotusCaptureMode,
            Voice::LotusVehicleMode,
            Voice::LotusCashMode,
        ] {
            voices[voice.index()].name = "first".into();
        }
        audio.units.push(UnitAudio {
            name: "Ambulance".into(),
            voices,
            weapon: Vec::new(),
            weapon_loops: Vec::new(),
            rapid_fire: String::new(),
            hacker: std::array::from_fn(|_| String::new()),
            lotus: Vec::new(),
            stealth: std::array::from_fn(|_| String::new()),
            detectors: Vec::new(),
            capture: std::array::from_fn(|_| String::new()),
            contain: std::array::from_fn(|_| String::new()),
            strategy: None,
        });
        audio.voice(&VoiceRequest {
            voice: Voice::CleanupMode,
            car_bomb: false,
            candidates: vec![(7, 0), (8, 0)],
        });
        audio.voice(&VoiceRequest {
            voice: Voice::CleanupArea,
            car_bomb: false,
            candidates: vec![(7, 0), (8, 0)],
        });
        assert_eq!(audio.requests.len(), 2);
        assert_eq!(audio.requests[0].event, "first");
        assert_eq!(audio.requests[0].object, None);
        assert_eq!(audio.requests[1].event, "second");
        assert_eq!(audio.requests[1].object, Some(7));
        audio.requests.clear();
        for voice in [
            Voice::LotusCaptureMode,
            Voice::LotusVehicleMode,
            Voice::LotusCashMode,
        ] {
            audio.voice(&VoiceRequest {
                voice,
                car_bomb: false,
                candidates: vec![(7, 0)],
            });
        }
        assert_eq!(audio.requests.len(), 3);
        assert!(
            audio
                .requests
                .iter()
                .all(|request| request.object.is_none() && request.event == "first")
        );
    }
}
