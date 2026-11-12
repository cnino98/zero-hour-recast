//! `ProductionUpdate`'s two authored upgrade-completion sound callbacks.
use super::SourceAudio;
use crate::{
    playtest::Match,
    skirmish_data::{definition, field, scalar},
};
use bevy::prelude::*;
use generals_assets::{
    content::{self, ContentLine},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{ResearchEvent, UnitType};
use std::error::Error;

pub(super) struct Bindings {
    display: bool,
    pub names: [String; 2],
}

pub(super) fn load(
    assets: &VirtualAssets,
    catalog: &[UnitType],
) -> Result<Vec<Vec<Bindings>>, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/upgrade.ini")?)?];
    catalog
        .iter()
        .map(|unit| {
            unit.maintenance
                .research
                .iter()
                .map(|upgrade| bindings(&definition(&files, "Upgrade", &upgrade.name)?.lines))
                .collect()
        })
        .collect()
}

fn bindings(lines: &[ContentLine]) -> Result<Bindings, Box<dyn Error>> {
    let value = |key| -> Result<String, Box<dyn Error>> {
        Ok(field(lines, 0, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into())
    };
    Ok(Bindings {
        display: !value("DisplayName")?.is_empty(),
        names: [value("ResearchSound")?, value("UnitSpecificSound")?],
    })
}

pub fn record(mut battle: ResMut<Match>, mut audio: ResMut<SourceAudio>) {
    if !battle.started || battle.paused {
        return;
    }
    let viewed = battle.resource_owner();
    for event in std::mem::take(&mut battle.simulation.presentation.research_events) {
        play(&mut audio, &event, viewed);
    }
}

fn play(audio: &mut SourceAudio, event: &ResearchEvent, viewed: Option<usize>) {
    let bindings = &audio.research[event.type_index][event.upgrade];
    // ProductionUpdate gates both calls before AudioManager chooses play-info.
    if viewed != Some(event.owner) || !bindings.display {
        return;
    }
    let names = bindings.names.clone();
    let [x, y, z] = event.position;
    for name in names {
        let _ = audio.queue(
            &name,
            Some(event.producer),
            Some(Vec3::new(x, z, -y)),
            None,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_keeps_source_order_object_and_pre_capture_owner_and_position() {
        let mut audio = super::super::speech_tests::audio();
        audio.research = vec![vec![Bindings {
            display: true,
            names: ["click".into(), "first".into()],
        }]];
        let event = ResearchEvent {
            frame: 20,
            producer: 13,
            type_index: 0,
            upgrade: 0,
            owner: 1,
            position: [10.0, 20.0, 30.0],
        };
        play(&mut audio, &event, Some(1));
        assert_eq!(audio.requests.len(), 2);
        assert_eq!(audio.requests[0].event, "click");
        assert_eq!(audio.requests[1].event, "first");
        for request in &audio.requests {
            assert_eq!(request.object, Some(13));
            assert_eq!(request.position, Some(Vec3::new(10.0, 30.0, -20.0)));
        }
    }

    #[test]
    fn hidden_upgrade_and_foreign_viewer_do_not_queue_or_draw_audio_random() {
        let mut audio = super::super::speech_tests::audio();
        audio.events.get_mut("click").unwrap().pitch = [0.9, 1.1];
        audio.research = vec![vec![Bindings {
            display: true,
            names: ["click".into(), "click".into()],
        }]];
        let event = ResearchEvent {
            frame: 20,
            producer: 13,
            type_index: 0,
            upgrade: 0,
            owner: 1,
            position: [0.0; 3],
        };
        play(&mut audio, &event, Some(0));
        play(&mut audio, &event, None);
        audio.research[0][0].display = false;
        play(&mut audio, &event, Some(1));
        assert!(audio.requests.is_empty());
        assert_eq!(
            audio.random.lock().unwrap().index(10_000),
            super::super::random::Random::new(74).index(10_000)
        );
    }
}
