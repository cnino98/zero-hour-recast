//! `FiringTracker`'s per-object looping fire sound, including pending audio handles.
use super::{SourceAudio, data::ST_SHROUDED};
use bevy::prelude::*;
use generals_sim::skirmish::{Skirmish, visibility::Status};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

struct Entry {
    deadline: u32,
    handle: Weak<()>,
    stop: Option<Arc<AtomicBool>>,
}

pub(super) struct State {
    active: BTreeMap<usize, Entry>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            active: BTreeMap::new(),
        }
    }

    pub(super) fn reset(&mut self) {
        for entry in self.active.values() {
            entry.stop();
        }
        self.active.clear();
    }
}

impl Entry {
    fn stop(&self) {
        if let Some(stop) = &self.stop {
            stop.store(true, Ordering::Release);
        }
    }
}

impl SourceAudio {
    pub fn firing_sound(
        &mut self,
        game: &Skirmish,
        object: usize,
        weapon: usize,
        position: [f32; 3],
        clear: bool,
    ) {
        let unit = &self.units[game.entities[object].type_index];
        let name = unit.weapon[weapon].clone();
        let loop_frames = unit.weapon_loops[weapon];
        let [x, y, z] = position;
        self.fire_sound(
            &name,
            loop_frames,
            object,
            game.tick,
            Vec3::new(x, z, -y),
            clear,
        );
    }

    pub fn rapid_fire(
        &mut self,
        game: &Skirmish,
        object: usize,
        position: [f32; 3],
        observer: usize,
    ) {
        let name = self.units[game.entities[object].type_index]
            .rapid_fire
            .clone();
        let [x, y, z] = position;
        let clear = matches!(game.visibility_cell(observer, [x, y]), Status::Clear);
        let hidden = self.events.get(&name).is_some_and(|event| {
            event.world && event.restrictions & ST_SHROUDED != 0 && event.priority != 4 && !clear
        });
        let count = self.requests.len();
        let _ = self.queue(&name, Some(object), Some(Vec3::new(x, z, -y)), None, false);
        if hidden {
            self.requests.truncate(count);
        }
    }

    fn fire_sound(
        &mut self,
        name: &str,
        loop_frames: u32,
        object: usize,
        frame: u64,
        position: Vec3,
        clear: bool,
    ) {
        let entry = self.firing.active.get(&object);
        let start = loop_frames == 0
            || entry.is_none_or(|entry| entry.deadline == 0 || entry.handle.strong_count() == 0);
        if start {
            let hidden = self.events.get(name).is_some_and(|event| {
                event.world
                    && event.restrictions & ST_SHROUDED != 0
                    && event.priority != 4
                    && !clear
            });
            let count = self.requests.len();
            let _ = self.queue(name, Some(object), Some(position), None, false);
            // Native admission checks shroud after generating filename/play info.
            if hidden {
                self.requests.truncate(count);
            }
            if loop_frames != 0 {
                let mut entry = Entry {
                    deadline: 0,
                    handle: Weak::new(),
                    stop: None,
                };
                if let Some(request) = self.requests.get_mut(count) {
                    let handle = Arc::new(());
                    entry.handle = Arc::downgrade(&handle);
                    entry.stop = request.looping.as_ref().map(|looping| looping.stop.clone());
                    request.handle = Some(handle);
                }
                self.firing.active.insert(object, entry);
            }
        }
        if let Some(entry) = self.firing.active.get_mut(&object) {
            // A fire-and-forget weapon clears the timer without stopping the
            // retained handle. This is the original FiringTracker branch.
            entry.deadline = if loop_frames == 0 {
                0
            } else {
                u32::try_from(frame & u64::from(u32::MAX))
                    .expect("native audio frame")
                    .wrapping_add(loop_frames)
            };
        }
    }

    pub fn stop_firing(&mut self, object: usize) {
        if let Some(entry) = self.firing.active.remove(&object) {
            // Miles marks a pending request as stopped but still plays its
            // initial portion; the decoder consumes this same stop flag.
            entry.stop();
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct FiringAudioInspection<'w, 's> {
    sinks: Query<'w, 's, (&'static super::playback::Playing, &'static AudioSink)>,
}

impl FiringAudioInspection<'_, '_> {
    pub fn present(&self, object: usize, event: &str) -> bool {
        self.sinks
            .iter()
            .any(|(playing, _)| playing.object == Some(object) && playing.event == event)
    }

    pub fn ambient<const N: usize>(self, names: [&str; N]) -> [bool; N] {
        self.played_events(names.map(|name| (None, name)))
    }

    pub fn finished(&self, object: usize, event: &str) -> bool {
        use bevy::audio::AudioSinkPlayback;
        !self.sinks.iter().any(|(playing, sink)| {
            playing.object == Some(object) && playing.event == event && !sink.empty()
        })
    }

    pub fn object_fx(self, event: &str) -> bool {
        self.ambient([event])[0]
    }

    pub fn played<const N: usize>(self, events: [(usize, &str); N]) -> [bool; N] {
        self.played_events(events.map(|(object, event)| (Some(object), event)))
    }

    pub fn played_events<const N: usize>(self, events: [(Option<usize>, &str); N]) -> [bool; N] {
        use bevy::audio::AudioSinkPlayback;
        events.map(|(object, event)| {
            self.sinks.iter().any(|(playing, sink)| {
                playing.object == object
                    && playing.event == event
                    && (object.is_some() || !sink.empty())
                    && sink.position().as_secs_f64() > 0.01
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SourceAudio {
        let mut audio = super::super::speech_tests::audio();
        let event = audio.events.get_mut("click").unwrap();
        event.control = super::super::data::LOOP | super::super::data::RANDOM;
        audio
    }

    #[test]
    fn pending_loop_handle_deduplicates_until_the_simulation_stop_callback() {
        let mut audio = fixture();
        audio.fire_sound("click", 3, 7, 10, Vec3::ZERO, true);
        let stop = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        audio.fire_sound("click", 3, 7, 12, Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        assert!(!stop.load(Ordering::Acquire));
        audio.stop_firing(7);
        assert!(stop.load(Ordering::Acquire));
        assert_eq!(audio.requests.len(), 1);
    }

    #[test]
    fn rejected_loop_request_can_retry_and_destruction_stops_only_that_object() {
        let mut audio = fixture();
        audio.fire_sound("click", 3, 7, 10, Vec3::ZERO, true);
        audio.requests.clear();
        audio.fire_sound("click", 3, 7, 11, Vec3::ZERO, true);
        audio.fire_sound("click", 3, 8, 11, Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 2);
        let stopped = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        audio.stop_firing(7);
        assert!(stopped.load(Ordering::Acquire));
        assert_eq!(audio.requests.len(), 2);
        assert_eq!(audio.requests[1].object, Some(8));
        assert!(
            !audio.requests[1]
                .looping
                .as_ref()
                .unwrap()
                .stop
                .load(Ordering::Acquire)
        );
    }

    #[test]
    fn wrapped_zero_timer_restarts_loop_while_the_old_handle_remains_playing() {
        let mut audio = fixture();
        audio.fire_sound("click", 3, 7, u64::from(u32::MAX - 2), Vec3::ZERO, true);
        let stop = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        audio.fire_sound("click", 3, 7, u64::from(u32::MAX - 1), Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 2);
        assert!(!stop.load(Ordering::Acquire));
        audio.stop_firing(7);
        assert!(!stop.load(Ordering::Acquire));
        assert!(
            audio.requests[1]
                .looping
                .as_ref()
                .unwrap()
                .stop
                .load(Ordering::Acquire)
        );
    }

    #[test]
    fn nonloop_shot_clears_deadline_without_stopping_retained_handle_until_object_destruction() {
        let mut audio = fixture();
        audio.fire_sound("click", 3, 7, 10, Vec3::ZERO, true);
        let stop = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        audio.fire_sound("first", 0, 7, 11, Vec3::ZERO, true);
        assert!(!stop.load(Ordering::Acquire));
        audio.stop_firing(7);
        assert!(stop.load(Ordering::Acquire));
    }
}
