//! `BattlePlanUpdate::setStatus` announcement callbacks over the installed audio data.
use super::{SourceAudio, data::Event};
use crate::{
    control_bar::UiData,
    notifications::Notifications,
    playtest::Match,
    playtest_view,
    skirmish_data::{blocks, field, scalar},
};
use bevy::prelude::*;
use generals_assets::content::ContentLine;
use generals_sim::skirmish::battle_plan::{Plan, Status};
use std::{collections::BTreeMap, error::Error};

pub(super) struct Bindings {
    pub names: [String; 10],
    labels: [String; 3],
}

pub(super) fn bindings(lines: &[ContentLine]) -> Result<Option<Bindings>, Box<dyn Error>> {
    let Some(module) = blocks(lines, 0, "Behavior")?.into_iter().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "BattlePlanUpdate")
    }) else {
        return Ok(None);
    };
    let mut names = std::array::from_fn(|_| String::new());
    let mut labels = std::array::from_fn(|_| String::new());
    for (index, prefix) in ["Bombardment", "HoldTheLine", "SearchAndDestroy"]
        .iter()
        .enumerate()
    {
        for (slot, suffix) in [
            "PlanUnpackSoundName",
            "PlanPackSoundName",
            "AnnouncementName",
        ]
        .iter()
        .enumerate()
        {
            names[index * 3 + slot] = value(module, &format!("{prefix}{suffix}"))?;
        }
        labels[index] = value(module, &format!("{prefix}MessageLabel"))?;
    }
    names[9] = value(module, "SearchAndDestroyPlanIdleLoopSoundName")?;
    Ok(Some(Bindings { names, labels }))
}

fn value(lines: &[ContentLine], key: &str) -> Result<String, Box<dyn Error>> {
    Ok(field(lines, 1, key)
        .map(scalar)
        .transpose()?
        .unwrap_or_default()
        .into())
}

pub(super) fn validate(
    bindings: &Bindings,
    events: &BTreeMap<String, Event>,
) -> Result<(), Box<dyn Error>> {
    // Stock definitions leave all seven mechanical hooks empty. Do not silently
    // accept authored playback until native handle cancellation is represented.
    for (index, name) in bindings.names.iter().enumerate() {
        if index < 9 && index % 3 == 2 {
            continue;
        }
        if let Some(event) = events.get(name)
            && (!event.sounds.is_empty() || !event.attack.is_empty() || !event.decay.is_empty())
        {
            return Err(format!(
                "{name}: authored battle-plan mechanical sound cancellation is not represented"
            )
            .into());
        }
    }
    Ok(())
}

pub fn record(
    mut battle: ResMut<Match>,
    mut audio: ResMut<SourceAudio>,
    mut messages: ResMut<Notifications>,
    data: Res<UiData>,
) {
    let data = data.into_inner();
    if !battle.started || battle.paused {
        return;
    }
    for event in std::mem::take(&mut battle.simulation.presentation.battle_plan_events) {
        if event.status != Status::Unpacking {
            continue;
        }
        let actor = &battle.simulation.entities[event.object];
        let Some(bindings) = audio.units[actor.type_index].strategy.as_ref() else {
            continue;
        };
        let index = match event.plan {
            Plan::Bombardment => 0,
            Plan::HoldTheLine => 1,
            Plan::SearchAndDestroy => 2,
        };
        messages.add(data.label(&bindings.labels[index]).to_owned(), event.frame);
        let name = bindings.names[index * 3 + 2].clone();
        let position = playtest_view::unit_position(&battle.simulation, event.object) * 10.0;
        announce(&mut audio, &name, position);
    }
}

fn announce(audio: &mut SourceAudio, name: &str, position: Vec3) {
    // Native announcements set only the position, never object ID or player restriction.
    let _ = audio.queue(name, None, Some(position), None, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcements_keep_world_position_without_object_follow_or_speech_gate() {
        let mut audio = super::super::speech_tests::audio();
        audio.speech_disallowed = true;
        announce(&mut audio, "click", Vec3::new(100.0, 15.0, -80.0));
        let request = &audio.requests[0];
        assert_eq!(request.object, None);
        assert_eq!(request.position, Some(Vec3::new(100.0, 15.0, -80.0)));
    }

    #[test]
    fn stock_empty_mechanical_hooks_are_silent_and_authored_unsupported_hooks_fail() {
        let mut bindings = Bindings {
            names: std::array::from_fn(|_| String::new()),
            labels: std::array::from_fn(|_| String::new()),
        };
        let audio = super::super::speech_tests::audio();
        bindings.names[2] = "click".into();
        assert!(validate(&bindings, &audio.events).is_ok());
        bindings.names[9] = "click".into();
        assert!(validate(&bindings, &audio.events).is_err());
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct StrategyAudioInspection<'w, 's> {
    audio: Res<'w, SourceAudio>,
    sinks: Query<'w, 's, (&'static super::playback::Playing, &'static AudioSink)>,
}

impl StrategyAudioInspection<'_, '_> {
    pub fn played(self, type_index: usize, index: usize) -> bool {
        use bevy::audio::AudioSinkPlayback;
        let Some(bindings) = self.audio.units[type_index].strategy.as_ref() else {
            return false;
        };
        self.sinks.iter().any(|(playing, sink)| {
            // A rendering stall can outlast the original recording. Its advanced
            // playhead still proves output before PostUpdate retires the sink.
            playing.event == bindings.names[index * 3 + 2] && sink.position().as_secs_f64() > 0.01
        })
    }
}
