//! `CommandXlat` adds the driver's voice after the converted vehicle's response.
use super::{SourceAudio, Voice};
use crate::skirmish_data::{definition, field, scalar};
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::error::Error;

pub(super) fn sounds(assets: &VirtualAssets) -> Result<[String; 3], Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/miscaudio.ini")?)?];
    let lines = &definition(&files, "MiscAudio", "")?.lines;
    let sound = |key| -> Result<String, Box<dyn Error>> {
        Ok(field(lines, 0, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into())
    };
    Ok([
        sound("TerroristInCarSelectVoice")?,
        sound("TerroristInCarMoveVoice")?,
        sound("TerroristInCarAttackVoice")?,
    ])
}

impl SourceAudio {
    pub(super) fn car_bomb_voice(&mut self, voice: Voice, object: usize) {
        let index = match voice {
            Voice::Select => 0,
            Voice::Move | Voice::Salvage => 1,
            Voice::Attack | Voice::AttackAir => 2,
            _ => return,
        };
        let name = self.car_bomb[index].clone();
        // Native MiscAudio is copied and receives the car ID. It does not retain
        // the per-unit binding's previous variation index.
        let _ = self.queue(&name, Some(object), None, None, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_audio::{Binding, UnitAudio, VoiceRequest};

    #[test]
    fn converted_car_keeps_vehicle_then_driver_order_and_object_identity() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.car_bomb = std::array::from_fn(|_| "second".into());
        audio.units.push(UnitAudio {
            name: "CarTaxiCab02".into(),
            voices: (0..19)
                .map(|_| Binding {
                    name: "first".into(),
                    previous: None,
                })
                .collect(),
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
        for voice in [Voice::Select, Voice::Move, Voice::Attack, Voice::Salvage] {
            audio.requests.clear();
            audio.voice(&VoiceRequest {
                voice,
                candidates: vec![(9, 0)],
                car_bomb: true,
            });
            assert_eq!(audio.requests.len(), 2);
            assert_eq!(audio.requests[0].event, "first");
            assert_eq!(audio.requests[1].event, "second");
            assert!(
                audio
                    .requests
                    .iter()
                    .all(|request| request.object == Some(9))
            );
        }
        for (voice, car_bomb) in [
            (Voice::Move, false),
            (Voice::Guard, true),
            (Voice::EnterHostile, true),
        ] {
            audio.requests.clear();
            audio.voice(&VoiceRequest {
                voice,
                candidates: vec![(9, 0)],
                car_bomb,
            });
            assert_eq!(audio.requests.len(), 1);
        }
        // NoSound vehicle responses still leave the original additional voice.
        audio.units[0].voices[Voice::Move.index()].name = "NoSound".into();
        audio.requests.clear();
        audio.voice(&VoiceRequest {
            voice: Voice::Move,
            candidates: vec![(9, 0)],
            car_bomb: true,
        });
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].event, "second");
    }
}
