//! SH `W3DTruckDraw::doDrawModule` per-draw landing and powerslide audio.
use super::{SourceAudio, data};
use bevy::prelude::Vec3;
use generals_sim::skirmish::{Skirmish, visibility::Status};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

struct Playing {
    handle: Weak<()>,
    stop: Option<Arc<AtomicBool>>,
}

pub(super) struct State {
    slides: BTreeMap<usize, Option<Playing>>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            slides: BTreeMap::new(),
        }
    }

    pub(super) fn reset(&mut self) {
        for playing in self.slides.values().flatten() {
            playing.stop();
        }
        self.slides.clear();
    }
}

impl Playing {
    fn stop(&self) {
        // Miles marks this exact pending/playing handle. It lets the current
        // portion finish, then skips further loops and proceeds to decay.
        if self.handle.strong_count() != 0
            && let Some(stop) = &self.stop
        {
            stop.store(true, Ordering::Release);
        }
    }
}

impl SourceAudio {
    pub fn wheel_sounds(
        &mut self,
        game: &Skirmish,
        object: usize,
        landing: bool,
        sliding: bool,
        observer: usize,
    ) {
        let names = self.units[game.entities[object].type_index].wheels.clone();
        if landing {
            let _ = self.queue_wheel_sound(game, object, &names[0], observer);
        }
        // Keep the draw's latch even when admission fails or the one-shot ends;
        // W3DTruckDraw only submits a new request on the next false-to-true edge.
        if sliding && !self.wheels.slides.contains_key(&object) {
            let playing = self.queue_wheel_sound(game, object, &names[1], observer);
            self.wheels.slides.insert(object, playing);
        } else if !sliding && let Some(Some(playing)) = self.wheels.slides.remove(&object) {
            playing.stop();
        }
    }

    pub fn retire_wheel_sounds(&mut self, object: usize) {
        // TruckDraw's destructor tosses emitters, not the audio event. Losing
        // the Object later requests stop through the ordinary playback path.
        let _ = self.wheels.slides.remove(&object);
    }

    fn queue_wheel_sound(
        &mut self,
        game: &Skirmish,
        object: usize,
        name: &str,
        observer: usize,
    ) -> Option<Playing> {
        let actor = &game.entities[object];
        let event = self.events.get(name)?;
        if event.restrictions & data::ST_PLAYER != 0 && actor.owner != observer {
            return None;
        }
        let point = game.object_position(object);
        let point = Vec3::new(point[0], point[2], -point[1]);
        let clear = matches!(
            game.visibility_cell(observer, actor.position),
            Status::Clear
        );
        let hidden = event.world
            && event.priority != 4
            && ((event.restrictions & data::ST_SHROUDED != 0 && !clear)
                || point.distance_squared(self.listener) >= event.max_range * event.max_range);
        let count = self.requests.len();
        let _ = self.queue(name, Some(object), Some(point), None, false);
        if hidden {
            self.requests.truncate(count);
        }
        let request = self.requests.get_mut(count)?;
        let handle = Arc::new(());
        let playing = Playing {
            handle: Arc::downgrade(&handle),
            stop: request.looping.as_ref().map(|looping| looping.stop.clone()),
        };
        request.handle = Some(handle);
        Some(playing)
    }
}

#[cfg(test)]
mod tests;
