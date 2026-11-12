//! Player-indexed radar edge sounds from native `MiscAudio` bindings.
use super::SourceAudio;
use crate::{
    playtest::Match,
    skirmish_data::{definition, field, scalar},
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{collections::BTreeSet, error::Error};

pub(super) struct State {
    pub(super) sounds: [String; 2],
    seen: BTreeSet<(u64, u64)>,
}

impl State {
    pub(super) const fn new(sounds: [String; 2]) -> Self {
        Self {
            sounds,
            seen: BTreeSet::new(),
        }
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
            sound("RadarOnlineSound")?,
            sound("RadarOfflineSound")?,
        ]))
    }
    pub(super) fn reset(&mut self) {
        self.seen.clear();
    }
}

impl SourceAudio {
    pub(super) fn update_radar(&mut self, battle: &Match) {
        if !battle.started {
            return;
        }
        self.queue_radar_events(
            battle.simulation.tick,
            battle.resource_owner(),
            &battle.simulation.presentation.radar_events,
        );
    }

    fn queue_radar_events(
        &mut self,
        frame: u64,
        viewed: Option<usize>,
        events: &[generals_sim::skirmish::radar_update::Event],
    ) {
        self.radar
            .seen
            .retain(|&(previous, _)| previous + 1 >= frame);
        for event in events {
            if !self.radar.seen.insert((event.frame, event.serial)) || viewed != Some(event.owner) {
                continue;
            }
            let name = self.radar.sounds[usize::from(!event.online)].clone();
            let _ = self.queue(&name, None, None, None, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use generals_sim::skirmish::radar_update::Event;

    #[test]
    fn actual_queue_filters_owner_preserves_edges_and_resets_generation_deduplication() {
        let mut audio = super::super::speech_tests::audio();
        audio.radar = super::State::new(["first".into(), "second".into()]);
        let events = [
            Event {
                serial: 0,
                frame: 10,
                owner: 1,
                online: true,
            },
            Event {
                serial: 1,
                frame: 10,
                owner: 0,
                online: true,
            },
            Event {
                serial: 2,
                frame: 10,
                owner: 0,
                online: false,
            },
        ];
        audio.queue_radar_events(10, Some(0), &events);
        let names: Vec<_> = audio
            .requests
            .iter()
            .map(|request| request.event.as_str())
            .collect();
        assert_eq!(names, ["first", "second"]);
        assert!(
            audio
                .requests
                .iter()
                .all(|request| request.object.is_none() && request.position.is_none())
        );
        audio.queue_radar_events(10, Some(0), &events);
        assert_eq!(audio.requests.len(), 2);
        audio.radar.reset();
        audio.queue_radar_events(10, Some(0), &events);
        assert_eq!(audio.requests.len(), 4);
        // A remote edge already observed does not replay when the viewer changes.
        audio.queue_radar_events(10, Some(1), &events);
        assert_eq!(audio.requests.len(), 4);
    }
}
