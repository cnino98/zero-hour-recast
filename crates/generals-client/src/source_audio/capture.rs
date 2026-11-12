//! `SpecialAbilityUpdate` capture callbacks and `Object::defect`'s original tick/ding.
use super::{Match, SourceAudio};
use crate::{
    playtest_view,
    skirmish_data::{definition, field, scalar},
};
use generals_assets::{
    content::{self, ContentLine},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::CaptureEffect;
use std::{collections::BTreeSet, error::Error};

pub(super) struct State {
    pub(super) sounds: [String; 2],
    seen: BTreeSet<(u64, usize, u8)>,
    pub(super) stealth_frame: Option<u64>,
}

impl State {
    pub(super) const fn new(sounds: [String; 2]) -> Self {
        Self {
            sounds,
            seen: BTreeSet::new(),
            stealth_frame: None,
        }
    }
    pub(super) fn load(assets: &VirtualAssets) -> Result<Self, Box<dyn Error>> {
        let files = [content::scan(assets.read("data/ini/miscaudio.ini")?)?];
        let lines = &definition(&files, "MiscAudio", "")?.lines;
        Ok(Self::new(defector_sounds(lines)?))
    }
    pub(super) fn reset(&mut self) {
        self.seen.clear();
        self.stealth_frame = None;
    }
}

fn defector_sounds(lines: &[ContentLine]) -> Result<[String; 2], Box<dyn Error>> {
    // AudioEventRTS's constructor leaves omitted MiscAudio hooks unnamed.
    // Installed Zero Hour authors the tick but deliberately omits the ding.
    let sound = |key| -> Result<String, Box<dyn Error>> {
        Ok(field(lines, 0, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into())
    };
    Ok([
        sound("DefectorTimerTickSound")?,
        sound("DefectorTimerDingSound")?,
    ])
}

pub(super) fn bindings(lines: &[ContentLine]) -> Result<[String; 2], Box<dyn Error>> {
    let mut sounds = std::array::from_fn(|_| String::new());
    sounds[1] = field(lines, 0, "VoiceDefect")
        .map(scalar)
        .transpose()?
        .unwrap_or_default()
        .into();
    sounds[0] = field(lines, 0, "VoiceTaskComplete")
        .map(scalar)
        .transpose()?
        .unwrap_or_default()
        .into();

    Ok(sounds)
}

impl SourceAudio {
    pub fn task_complete(&mut self, battle: &Match, object: usize, type_index: usize) {
        let name = self.units[type_index].capture[0].clone();
        let point = playtest_view::unit_position(&battle.simulation, object) * 10.0;
        let _ = self.queue(&name, Some(object), Some(point), None, false);
    }

    pub(super) fn update_capture(&mut self, battle: &Match) {
        if !battle.started {
            return;
        }
        self.update_stealth_audio(battle);
        let game = &battle.simulation;
        self.capture
            .seen
            .retain(|&(frame, _, _)| frame + 1 >= game.tick);
        for event in &game.presentation.capture_events {
            let (kind, target) = match event.effect {
                CaptureEffect::Captured => (4, event.target),
                CaptureEffect::DefectedWithoutDrawable => (8, event.target),
                CaptureEffect::DefectorTick => (7, event.target),
                CaptureEffect::Detected => (6, event.target),
                CaptureEffect::Infiltrated(_) => {
                    continue;
                }
            };
            if !self.capture.seen.insert((event.frame, event.object, kind)) {
                continue;
            }
            let actor = &game.entities[target];
            let point = playtest_view::unit_position(game, target) * 10.0;
            let names = match kind {
                4 => vec![
                    self.units[actor.type_index].capture[1].clone(),
                    self.capture.sounds[0].clone(),
                ],
                8 => vec![self.units[actor.type_index].capture[1].clone()],
                7 => vec![self.capture.sounds[0].clone()],
                _ => vec![self.capture.sounds[1].clone()],
            };
            for name in names {
                let _ = self.queue(&name, Some(target), Some(point), None, false);
            }
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct CaptureAudioInspection<'w, 's> {
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

impl CaptureAudioInspection<'_, '_> {
    pub fn tick_playing(&self, target: usize) -> bool {
        use bevy::audio::AudioSinkPlayback;
        self.sinks.iter().any(|(playing, sink)| {
            playing.object == Some(target)
                && playing.event == self.audio.capture.sounds[0]
                && !sink.empty()
                && sink.position().as_secs_f64() > 0.01
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;

    #[test]
    fn original_omitted_ding_remains_silent_but_malformed_binding_is_rejected() {
        let parse = |source: &str| {
            let file = content::scan(Asset {
                path: "data/ini/miscaudio.ini".into(),
                origin: "fixture".into(),
                bytes: source.as_bytes().to_vec(),
            })
            .unwrap();
            defector_sounds(&file.declarations[0].lines)
        };
        assert_eq!(
            parse("MiscAudio\nDefectorTimerTickSound = DefectorTimerTick\nEnd").unwrap(),
            ["DefectorTimerTick", ""]
        );
        assert_eq!(parse("MiscAudio\nEnd").unwrap(), ["", ""]);
        assert_eq!(
            parse("MiscAudio\nDefectorTimerDingSound = AuthoredDing\nEnd").unwrap(),
            ["", "AuthoredDing"]
        );
        assert!(parse("MiscAudio\nDefectorTimerDingSound =\nEnd").is_err());
    }
}
