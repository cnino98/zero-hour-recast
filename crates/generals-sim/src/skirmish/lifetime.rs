//! LifetimeUpdate.cpp: constructor and setLifetimeRange own the same module wake.
use super::{Skirmish, module_updates::FOREVER};

impl Skirmish {
    pub(in crate::skirmish) fn native_lifetime_deadline(&self, id: usize) -> u64 {
        self.entities[id]
            .behaviors
            .lifetime_die_frames
            .first()
            .map_or(u64::MAX, |&frame| u64::from(frame))
    }

    pub(in crate::skirmish) fn set_native_lifetime(&mut self, id: usize, frames: u32) {
        if frames != 0 && !self.entities[id].behaviors.lifetime_die_frames.is_empty() {
            self.set_lifetime_range(id, 0, [frames; 2]);
        }
    }

    pub(super) fn construct_lifetime(&mut self, id: usize, registration: usize) {
        let ordinal = self.entities[id].behaviors.lifetime_die_frames.len();
        let unit = &self.catalog[self.entities[id].type_index];
        let range = if unit.kind_of.iter().any(|kind| kind == "HULK") {
            self.hulk_lifetime_override
                .map_or(unit.behavior.lifetimes[ordinal], |frames| [frames; 2])
        } else {
            unit.behavior.lifetimes[ordinal]
        };
        self.entities[id].behaviors.lifetime_die_frames.push(0);
        let delay = self.lifetime_delay(id, ordinal, range);
        self.wake_update(id, registration, delay);
    }

    pub(super) fn set_lifetime_range(&mut self, id: usize, ordinal: usize, range: [u32; 2]) {
        let delay = self.lifetime_delay(id, ordinal, range);
        self.wake_family_update(id, "LifetimeUpdate", ordinal, delay);
    }

    fn lifetime_delay(&mut self, id: usize, ordinal: usize, range: [u32; 2]) -> u32 {
        // GameLogicRandomValue's signed inclusive endpoint check runs before RNG.
        let [low, high] = range.map(|value| i32::from_ne_bytes(value.to_ne_bytes()));
        let delay = if low >= high {
            range[1]
        } else {
            range[0].wrapping_add(self.random_between(range[1].wrapping_sub(range[0])))
        }
        .max(1);
        self.entities[id].behaviors.lifetime_die_frames[ordinal] =
            self.update_frame().wrapping_add(delay);
        delay
    }

    pub(super) fn update_lifetime(&mut self, id: usize) -> i32 {
        self.kill_object(id, super::DeathType::Normal);
        i32::from_ne_bytes(FOREVER.to_ne_bytes())
    }
}

#[cfg(test)]
mod tests;
