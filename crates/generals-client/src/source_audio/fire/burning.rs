//! `FlammableUpdate` owns a sound handle; ignition does not retry admission.
use super::SourceAudio;
use bevy::prelude::*;
use generals_sim::skirmish::fire::flammable::SoundEvent;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct Entry {
    stop: Option<Arc<AtomicBool>>,
}

impl Entry {
    pub(super) fn stop(&self) {
        if let Some(stop) = &self.stop {
            stop.store(true, Ordering::Release);
        }
    }
}

impl SourceAudio {
    pub(in crate::source_audio) fn burning_event(&mut self, event: SoundEvent, clear: bool) {
        match event {
            SoundEvent::Start {
                object,
                ordinal,
                sound,
                position: [x, y, z],
            } => {
                let hidden = self.events.get(&sound).is_some_and(|event| {
                    event.world
                        && event.restrictions & super::super::data::ST_SHROUDED != 0
                        && event.priority != 4
                        && !clear
                });
                let count = self.requests.len();
                let _ = self.queue(&sound, Some(object), Some(Vec3::new(x, z, -y)), None, false);
                // SoundManager tests the position cell after play-info selection.
                if hidden {
                    self.requests.truncate(count);
                }
                let stop = self
                    .requests
                    .get(count)
                    .and_then(|request| request.looping.as_ref())
                    .map(|looping| looping.stop.clone());
                self.fires.burning.insert((object, ordinal), Entry { stop });
            }
            SoundEvent::Stop { object, ordinal } => {
                if let Some(entry) = self.fires.burning.remove(&(object, ordinal)) {
                    entry.stop();
                }
            }
        }
    }
}

pub fn tick(mut battle: ResMut<crate::playtest::Match>, mut audio: ResMut<SourceAudio>) {
    if !battle.started || battle.paused || audio.generation != battle.generation {
        return;
    }
    for event in battle.simulation.take_burning_sound_events() {
        let clear = match &event {
            SoundEvent::Start { position, .. } => matches!(
                battle.simulation.visibility_cell(
                    battle.observer_player.unwrap_or(0),
                    [position[0], position[1]],
                ),
                generals_sim::skirmish::visibility::Status::Clear
            ),
            SoundEvent::Stop { .. } => true,
        };
        audio.burning_event(event, clear);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_ignition_is_not_retried_and_stop_is_scoped_to_authored_instance() {
        let mut audio = super::super::super::speech_tests::audio();
        audio.events.get_mut("click").unwrap().control = super::super::super::data::LOOP;
        for ordinal in [0, 1] {
            audio.burning_event(
                SoundEvent::Start {
                    object: 7,
                    ordinal,
                    sound: "click".into(),
                    position: [0.0; 3],
                },
                true,
            );
        }
        let first = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        let second = audio.requests[1].looping.as_ref().unwrap().stop.clone();
        audio.burning_event(
            SoundEvent::Stop {
                object: 7,
                ordinal: 0,
            },
            true,
        );
        assert!(first.load(Ordering::Acquire));
        assert!(!second.load(Ordering::Acquire));
        audio.requests.clear();
        audio.fires.reset();
        assert!(second.load(Ordering::Acquire));
        assert!(audio.requests.is_empty());
    }

    #[test]
    fn shrouded_world_start_selects_once_but_keeps_no_loop_and_critical_bypasses() {
        use super::super::super::{data, speech_tests};
        let mut hidden = speech_tests::audio();
        let mut clear = speech_tests::audio();
        for audio in [&mut hidden, &mut clear] {
            let event = audio.events.get_mut("click").unwrap();
            event.world = true;
            event.control = data::LOOP;
            event.restrictions = data::ST_SHROUDED;
        }
        let start = || SoundEvent::Start {
            object: 7,
            ordinal: 0,
            sound: "click".into(),
            position: [0.0; 3],
        };
        hidden.burning_event(start(), false);
        clear.burning_event(start(), true);
        assert!(hidden.requests.is_empty());
        assert_eq!(clear.requests.len(), 1);
        assert_eq!(
            hidden.random.lock().unwrap().index(10_000),
            clear.random.lock().unwrap().index(10_000)
        );
        assert!(hidden.fires.burning[&(7, 0)].stop.is_none());
        hidden.burning_event(
            SoundEvent::Stop {
                object: 7,
                ordinal: 0,
            },
            false,
        );
        assert!(hidden.fires.burning.is_empty());
        hidden.events.get_mut("click").unwrap().priority = 4;
        hidden.burning_event(start(), false);
        assert_eq!(hidden.requests.len(), 1);
        hidden.requests.clear();
        let event = hidden.events.get_mut("click").unwrap();
        event.priority = 2;
        event.world = false;
        hidden.burning_event(start(), false);
        assert_eq!(hidden.requests.len(), 1);
    }
}
