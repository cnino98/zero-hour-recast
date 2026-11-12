//! Original `Drawable` ambient-loop admission, retry and removal for poison fields.
use super::{SourceAudio, data};
use bevy::prelude::Vec3;
use generals_sim::{
    collision_geometry::Body,
    skirmish::{Skirmish, UnitType, visibility::Status},
};
use std::{
    collections::BTreeMap,
    error::Error,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct State {
    active: BTreeMap<u64, Entry>,
}

struct Entry {
    handle: Weak<()>,
    stop: Arc<AtomicBool>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            active: BTreeMap::new(),
        }
    }

    pub(super) fn reset(&mut self) {
        self.retain(|_| false);
    }

    fn retain(&mut self, present: impl Fn(u64) -> bool) {
        self.active.retain(|&id, entry| {
            let keep = present(id);
            if !keep {
                entry.stop.store(true, Ordering::Release);
            }
            keep
        });
    }
}

pub(super) fn names(catalog: &[UnitType]) -> impl Iterator<Item = &str> {
    catalog
        .iter()
        .flat_map(|unit| {
            unit.hazard_origins()
                .filter_map(|origin| Some(unit.hazard_rules(origin)?.ambient.as_str()))
        })
        .filter(|name| !name.is_empty())
}

pub(super) fn validate(
    catalog: &[UnitType],
    events: &BTreeMap<String, data::Event>,
) -> Result<(), Box<dyn Error>> {
    for name in names(catalog) {
        let event = events.get(name).ok_or("missing poison ambient sound")?;
        if !event.world
            || event.speech
            || event.control & data::LOOP == 0
            || event.loop_count != 0
            || event.restrictions & data::ST_PLAYER != 0
        {
            return Err(format!("unrepresented poison ambient sound {name}").into());
        }
    }
    Ok(())
}

impl SourceAudio {
    pub(super) fn update_hazards(&mut self, game: &Skirmish, observer: usize) {
        self.hazards
            .retain(|id| game.hazards.iter().any(|field| field.id == id));
        for field in &game.hazards {
            let rules = game.catalog[field.template_type]
                .hazard_rules(field.origin)
                .expect("hazard rules");
            let clear = self.events.get(&rules.ambient).is_none_or(|event| {
                event.restrictions & data::ST_SHROUDED == 0
                    || matches!(
                        game.visibility_footprint(
                            observer,
                            &Body {
                                position: field.position,
                                angle: 0.0,
                                geometry: &rules.geometry,
                            }
                        ),
                        Status::Clear | Status::Partial
                    )
            });
            let [x, y, z] = field.position;
            self.hazard_sound(field.id, &rules.ambient, Vec3::new(x, z, -y), clear);
        }
    }

    fn hazard_sound(&mut self, id: u64, name: &str, position: Vec3, clear: bool) {
        let Some(event) = self.events.get(name) else {
            return;
        };
        // Drawable::startAmbientSound checks maximum distance before audio RNG.
        let in_range = event.priority == 4
            || position.distance_squared(self.listener) < event.max_range * event.max_range;
        if !in_range {
            self.hazards.retain(|other| other != id);
            return;
        }
        if self
            .hazards
            .active
            .get(&id)
            .is_some_and(|entry| entry.handle.strong_count() > 0)
        {
            return;
        }
        let hidden = event.priority != 4 && event.restrictions & data::ST_SHROUDED != 0 && !clear;
        let count = self.requests.len();
        let _ = self.queue(name, None, Some(position), None, false);
        if hidden {
            self.requests.truncate(count);
        }
        if let Some(request) = self.requests.get_mut(count) {
            let handle = Arc::new(());
            let entry = Entry {
                handle: Arc::downgrade(&handle),
                stop: request
                    .looping
                    .as_ref()
                    .expect("validated permanent ambient loop")
                    .stop
                    .clone(),
            };
            // Share the backend's retained request/playing token, without
            // attributing this separate drawable to its launching vehicle.
            request.firing = Some(handle);
            self.hazards.active.insert(id, entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SourceAudio {
        let mut audio = super::super::speech_tests::audio();
        let event = audio.events.get_mut("click").unwrap();
        event.control = data::LOOP | data::RANDOM | data::ALL;
        event.world = true;
        event.max_range = 100.0;
        event.pitch = [0.9, 1.1];
        audio
    }

    #[test]
    fn ambient_retains_pending_handle_retries_failed_output_and_stops_on_removal() {
        let mut audio = fixture();
        audio.hazard_sound(1, "click", Vec3::ZERO, true);
        audio.hazard_sound(1, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        audio.requests.clear();
        audio.hazard_sound(1, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        let stop = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        audio.hazards.retain(|_| false);
        assert!(stop.load(Ordering::Acquire));
    }

    #[test]
    fn ambient_range_precedes_audio_rng_and_permanent_loop_restarts_on_return() {
        let mut audio = fixture();
        let mut expected = super::super::random::Random::new(74);
        audio.hazard_sound(1, "click", Vec3::new(100.0, 0.0, 0.0), true);
        assert!(audio.requests.is_empty());
        assert_eq!(
            audio.random.lock().unwrap().real(0.0, 1.0).to_bits(),
            expected.real(0.0, 1.0).to_bits()
        );
        audio.hazard_sound(1, "click", Vec3::ZERO, true);
        let stop = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        audio.hazard_sound(1, "click", Vec3::new(101.0, 0.0, 0.0), true);
        assert!(stop.load(Ordering::Acquire));
        audio.hazard_sound(1, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 2);
        let stop = audio.requests[1].looping.as_ref().unwrap().stop.clone();
        audio.hazards.reset();
        assert!(stop.load(Ordering::Acquire));
    }
}
