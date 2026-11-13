//! Invisible original firestorm drawables still own ordinary ambient sound loops.
use super::{SourceAudio, data};
use bevy::prelude::Vec3;
use generals_sim::skirmish::{Skirmish, UnitType, visibility::Status};
use std::{
    collections::BTreeMap,
    error::Error,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
mod burning;
pub use burning::tick;
mod projectiles;

pub(super) struct State {
    active: BTreeMap<usize, Entry>,
    burning: BTreeMap<(usize, usize), burning::Entry>,
    projectiles: BTreeMap<(usize, usize), String>,
}

struct Entry {
    handle: Weak<()>,
    stop: Arc<AtomicBool>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            active: BTreeMap::new(),
            burning: BTreeMap::new(),
            projectiles: BTreeMap::new(),
        }
    }
    pub(super) fn reset(&mut self) {
        self.retain(|_| false);
        for entry in self.burning.values() {
            entry.stop();
        }
        self.burning.clear();
    }
    fn retain(&mut self, present: impl Fn(usize) -> bool) {
        self.active.retain(|&id, entry| {
            let keep = present(id);
            if !keep {
                entry.stop.store(true, Ordering::Release);
            }
            keep
        });
    }
}

pub(super) fn validate(
    catalog: &[UnitType],
    events: &BTreeMap<String, data::Event>,
) -> Result<(), Box<dyn Error>> {
    for name in catalog
        .iter()
        .filter_map(|unit| unit.fire.as_ref())
        .map(|rules| rules.ambient.as_str())
        .filter(|name| !name.is_empty())
    {
        let event = events.get(name).ok_or("missing fire ambient sound")?;
        if !event.world
            || event.speech
            || event.control & data::LOOP == 0
            || event.loop_count != 0
            || event.restrictions & data::ST_PLAYER != 0
        {
            return Err(format!("untranslated fire ambient sound {name}").into());
        }
    }
    Ok(())
}

impl SourceAudio {
    pub(super) fn update_fires(&mut self, game: &Skirmish, observer: usize) {
        self.fires
            .retain(|id| game.entities.get(id).is_some_and(|actor| !actor.masked()));
        for (id, actor) in game.entities.iter().enumerate() {
            let Some(rules) = &game.catalog[actor.type_index].fire else {
                continue;
            };
            if actor.masked() || rules.ambient.is_empty() {
                continue;
            }
            let [x, y, z] = game.object_position(id);
            let clear = matches!(
                game.visibility_status(observer, id),
                Status::Clear | Status::Partial
            );
            self.fire_ambient_sound(id, &rules.ambient, Vec3::new(x, z, -y), clear);
        }
    }

    fn fire_ambient_sound(&mut self, id: usize, name: &str, position: Vec3, clear: bool) {
        let Some(event) = self.events.get(name) else {
            return;
        };
        let in_range = event.priority == 4
            || position.distance_squared(self.listener) < event.max_range * event.max_range;
        if !in_range {
            self.fires.retain(|other| other != id);
            return;
        }
        if self
            .fires
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
            self.fires.active.insert(
                id,
                Entry {
                    handle: Arc::downgrade(&handle),
                    stop: request
                        .looping
                        .as_ref()
                        .expect("validated original fire loop")
                        .stop
                        .clone(),
                },
            );
            request.handle = Some(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_health_fire_sound_uses_drawable_lifetime_and_failed_output_can_retry() {
        let mut audio = super::super::speech_tests::audio();
        let event = audio.events.get_mut("click").unwrap();
        event.control = data::LOOP | data::ALL;
        event.world = true;
        event.max_range = 100.0;
        audio.fire_ambient_sound(9, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        let stop = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        audio.fire_ambient_sound(9, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        audio.requests.clear();
        audio.fire_ambient_sound(9, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        audio.fires.reset();
        assert!(
            audio.requests[0]
                .looping
                .as_ref()
                .unwrap()
                .stop
                .load(Ordering::Acquire)
        );
        assert!(!stop.load(Ordering::Acquire));
    }
}
