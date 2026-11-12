//! Native Physics bounce and `JetSlowDeath` sound callbacks, including explicit stop.
use super::{SourceAudio, data};
use bevy::prelude::Vec3;
use generals_sim::skirmish::{
    UnitType,
    object_sounds::{Action, Event},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct State(BTreeMap<(usize, String, Option<usize>), Arc<AtomicBool>>);

impl State {
    pub(super) const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub(super) fn running(&self, actor: usize, name: &str) -> bool {
        self.0.iter().any(|((object, event, _), stop)| {
            *object == actor && event == name && !stop.load(Ordering::Acquire)
        })
    }

    pub(super) fn reset(&mut self) {
        for (_, stop) in std::mem::take(&mut self.0) {
            stop.store(true, Ordering::Release);
        }
    }
}

pub(super) fn names(catalog: &[UnitType]) -> impl Iterator<Item = &str> {
    catalog.iter().flat_map(|unit| {
        let loops = unit.jet_death_rules().map(|jet| jet.loop_sound.as_str());
        let helicopter_loops = unit.helicopter_death_rules().flat_map(|rules| {
            [
                rules.loop_sound.as_str(),
                rules.eject_sounds[0].as_str(),
                rules.eject_sounds[1].as_str(),
            ]
        });
        let bounces = generals_sim::skirmish::physical_ocl::lists(unit)
            .flat_map(|list| &list.nuggets)
            .map(|nugget| nugget.bounce_sound.as_str());
        let deployment = unit
            .deployment
            .iter()
            .flat_map(|rules| rules.sounds.iter().map(String::as_str));
        let hijacker = unit.hijacker.iter().map(|_| "HijackDriver");
        let parachute = unit.parachute.iter().map(|rules| rules.open_sound.as_str());
        loops
            .chain([
                unit.damaged_sound.as_str(),
                unit.really_damaged_sound.as_str(),
                unit.fear_sound.as_str(),
            ])
            .chain(unit.aircraft.iter().flat_map(|rules| {
                [
                    rules.lockon.tick_sound.as_str(),
                    rules.sounds.afterburner.as_str(),
                    rules.sounds.low_fuel.as_str(),
                    rules.sounds.wheel_screech.as_str(),
                ]
            }))
            .chain(
                unit.behavior
                    .sticky_bombs
                    .iter()
                    .flat_map(|rules| [rules.created_sound.as_str(), rules.ping_sound.as_str()]),
            )
            .chain(unit.behavior.special_abilities.iter().flat_map(|rules| {
                rules.feedback.sounds.iter().map(String::as_str).chain([
                    rules.feedback.completion_voice.as_str(),
                    rules.feedback.capture_tick.as_str(),
                ])
            }))
            .chain(helicopter_loops)
            .chain(bounces)
            .chain(deployment)
            .chain(hijacker)
            .chain(parachute)
            .chain(unit.spectre.iter().flat_map(|rules| {
                [
                    rules.afterburner_sound.as_str(),
                    rules.howitzer_sound.as_str(),
                    rules.attack_voice.as_str(),
                ]
            }))
            .chain(unit.special_powers.iter().flat_map(|rules| {
                [
                    rules.initiate_sound.as_str(),
                    rules.target_sound.as_str(),
                    rules.module_initiate_sound.as_str(),
                ]
            }))
    })
}

impl SourceAudio {
    pub fn object_sound(&mut self, event: &Event, viewer: usize) {
        if matches!(event.action, Action::StopAmbient) {
            self.stop_aircraft_ambient(event.object);
            return;
        }
        let instance = match event.action {
            Action::PlayLoop(index) | Action::StopLoop(index) => Some(index),
            _ => None,
        };
        let key = (event.object, event.name.clone(), instance);
        if matches!(event.action, Action::Stop | Action::StopLoop(_)) {
            if let Some(stop) = self.object_sounds.0.remove(&key) {
                stop.store(true, Ordering::Release);
            }
            return;
        }
        let Some(sound) = self.events.get(&event.name) else {
            return;
        };
        let unowned = matches!(event.action, Action::PlayAtPositionUnowned);
        if unowned && sound.restrictions & data::ST_REQUIRES_OWNER != 0 {
            return;
        }
        if sound.restrictions & data::ST_PLAYER != 0 && !unowned && event.owner != viewer {
            return;
        }
        let point = Vec3::new(event.position[0], event.position[2], -event.position[1]);
        // Direct addAudioEvent constructs play info before SoundManager culls it.
        let hidden = sound.world
            && sound.priority != 4
            && ((sound.restrictions & data::ST_SHROUDED != 0
                && event.clear_cells & (1 << viewer) == 0)
                || point.distance_squared(self.listener) >= sound.max_range * sound.max_range);
        let count = self.requests.len();
        let bound = (!matches!(
            event.action,
            Action::PlayAtPosition | Action::PlayAtPositionUnowned
        ))
        .then_some(event.object);
        let _ = self.queue(&event.name, bound, Some(point), None, false);
        if hidden {
            self.requests.truncate(count);
        }
        if bound.is_some()
            && let Some(looping) = self
                .requests
                .get(count)
                .and_then(|request| request.looping.as_ref())
        {
            self.object_sounds.0.insert(key, looping.stop.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_uses_native_object_binding_and_landing_cell_shroud_after_rng() {
        let mut hidden = crate::source_audio::speech_tests::audio();
        let mut clear = crate::source_audio::speech_tests::audio();
        for audio in [&mut hidden, &mut clear] {
            let event = audio.events.get_mut("click").unwrap();
            event.world = true;
            event.max_range = 100.0;
            event.restrictions = data::ST_SHROUDED;
            event.pitch = [0.8, 1.2];
        }
        let mut event = Event {
            object: 13,
            position: [10.0, 20.0, 30.0],
            owner: 1,
            clear_cells: 0,
            name: "click".into(),
            action: Action::Play,
        };
        hidden.object_sound(&event, 0);
        event.clear_cells = 1;
        clear.object_sound(&event, 0);
        assert!(hidden.requests.is_empty());
        assert_eq!(clear.requests.len(), 1);
        assert_eq!(clear.requests[0].object, Some(13));
        assert_eq!(
            clear.requests[0].position,
            Some(Vec3::new(10.0, 30.0, -20.0))
        );
        assert_eq!(
            hidden.random.lock().unwrap().index(10_000),
            clear.random.lock().unwrap().index(10_000)
        );
    }

    #[test]
    fn special_power_location_sound_preserves_owner_and_has_no_object_binding() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.events.get_mut("click").unwrap().restrictions = data::ST_PLAYER;
        let event = Event {
            object: 9,
            position: [10.0, 20.0, 30.0],
            owner: 2,
            clear_cells: 4,
            name: "click".into(),
            action: Action::PlayAtPosition,
        };
        audio.object_sound(&event, 0);
        assert!(audio.requests.is_empty());
        audio.object_sound(&event, 2);
        assert_eq!(audio.requests[0].object, None);
        assert_eq!(
            audio.requests[0].position,
            Some(Vec3::new(10.0, 30.0, -20.0))
        );
    }

    #[test]
    fn crash_loop_stop_and_generation_reset_release_only_the_native_bound_loop() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.events.get_mut("click").unwrap().control = data::LOOP | data::ALL | data::RANDOM;
        let mut event = Event {
            object: 4,
            position: [0.0; 3],
            owner: 0,
            clear_cells: 1,
            name: "click".into(),
            action: Action::Play,
        };
        audio.object_sound(&event, 0);
        let first = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        event.object = 5;
        audio.object_sound(&event, 0);
        let second = audio.requests[1].looping.as_ref().unwrap().stop.clone();
        event.object = 4;
        event.action = Action::Stop;
        audio.object_sound(&event, 0);
        assert!(first.load(Ordering::Acquire));
        assert!(!second.load(Ordering::Acquire));
        assert_eq!(audio.requests.len(), 2);
        audio.object_sounds.reset();
        assert!(second.load(Ordering::Acquire));
    }
    #[test]
    fn authored_loop_instances_stop_their_own_handle_even_when_the_names_match() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.events.get_mut("click").unwrap().control = data::LOOP | data::ALL | data::RANDOM;
        let mut event = Event {
            object: 4,
            position: [0.0; 3],
            owner: 0,
            clear_cells: 1,
            name: "click".into(),
            action: Action::PlayLoop(0),
        };
        audio.object_sound(&event, 0);
        let first = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        event.action = Action::PlayLoop(1);
        audio.object_sound(&event, 0);
        let second = audio.requests[1].looping.as_ref().unwrap().stop.clone();
        event.action = Action::StopLoop(0);
        audio.object_sound(&event, 0);
        assert!(first.load(Ordering::Acquire));
        assert!(!second.load(Ordering::Acquire));
    }

    #[test]
    fn unowned_ejection_sound_obeys_owner_requirement_and_never_binds_to_copter() {
        let mut audio = crate::source_audio::speech_tests::audio();
        let event = Event {
            object: 4,
            position: [1.0, 2.0, 3.0],
            owner: 0,
            clear_cells: 1,
            name: "click".into(),
            action: Action::PlayAtPositionUnowned,
        };
        audio.events.get_mut("click").unwrap().restrictions =
            data::ST_PLAYER | data::ST_REQUIRES_OWNER;
        audio.object_sound(&event, 0);
        assert!(audio.requests.is_empty());
        audio.events.get_mut("click").unwrap().restrictions = 0;
        audio.object_sound(&event, 0);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].object, None);
    }
}
