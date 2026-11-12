//! Native self-powered `SpyVisionUpdate` and per-object vision-spy reference counts.
use super::{Entity, Skirmish};

#[cfg(test)]
mod tests;

pub struct Rules {
    pub needs_upgrade: bool,
    pub self_powered: bool,
    pub trigger: String,
    pub duration: u32,
    pub interval: u32,
    pub command_centers_only: bool,
}

struct Module {
    upgraded: bool,
    active: bool,
    deactivate: u32,
    reset: bool,
}

impl Module {
    const fn new() -> Self {
        Self {
            upgraded: false,
            active: false,
            deactivate: 0,
            reset: false,
        }
    }
}

pub struct State {
    modules: Vec<Module>,
    by: [i32; 16],
}

impl State {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            modules: Vec::new(),
            by: [0; 16],
        }
    }
}

impl Entity {
    pub(super) fn vision_spy_mask(&self) -> u16 {
        self.spy_vision
            .by
            .iter()
            .enumerate()
            .fold(
                0,
                |mask, (owner, &count)| {
                    if count > 0 { mask | (1 << owner) } else { mask }
                },
            )
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_spy_vision_module(&mut self, id: usize) {
        self.entities[id].spy_vision.modules.push(Module::new());
    }

    pub(in crate::skirmish) fn reset_spy_vision_upgrade(&mut self, id: usize, ordinal: usize) {
        self.entities[id].spy_vision.modules[ordinal].upgraded = false;
    }

    fn initialize_spy_vision(&mut self, id: usize) {
        let count = self.catalog[self.entities[id].type_index]
            .behavior
            .spy_vision
            .len();
        self.entities[id]
            .spy_vision
            .modules
            .resize_with(count, Module::new);
    }

    pub(in crate::skirmish) fn apply_spy_vision_upgrade(&mut self, id: usize, ordinal: usize) {
        self.initialize_spy_vision(id);
        let duration = self.catalog[self.entities[id].type_index]
            .behavior
            .spy_vision[ordinal]
            .duration;
        self.spy_activation(id, ordinal, self.entities[id].owner, true);
        let now = self.update_frame();
        let state = &mut self.entities[id].spy_vision.modules[ordinal];
        state.upgraded = true;
        state.deactivate = if duration == 0 {
            u32::MAX
        } else {
            now.wrapping_add(duration)
        };
        self.wake_family_update(
            id,
            "SpyVisionUpdate",
            ordinal,
            if duration == 0 {
                super::module_updates::FOREVER
            } else {
                duration
            },
        );
    }

    fn spy_activation(&mut self, id: usize, index: usize, owner: usize, setting: bool) {
        let centers = self.catalog[self.entities[id].type_index]
            .behavior
            .spy_vision[index]
            .command_centers_only;
        for player in 0..self.players.len() {
            if !self.enemies(owner, player) {
                continue;
            }
            for target in self.team_membership.player_objects(player) {
                let actor = &self.entities[target];
                if centers
                    && !self.catalog[actor.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "COMMANDCENTER")
                {
                    continue;
                }
                let count = &mut self.entities[target].spy_vision.by[owner];
                *count += if setting { 1 } else { -1 };
                if *count == i32::from(setting) {
                    self.observe_visibility(target);
                }
            }
        }
        self.entities[id].spy_vision.modules[index].active = setting;
    }

    pub(in crate::skirmish) fn capture_spy_vision(
        &mut self,
        id: usize,
        old_owner: usize,
        new_owner: usize,
    ) {
        for index in 0..self.entities[id].spy_vision.modules.len() {
            if self.entities[id].spy_vision.modules[index].active {
                self.spy_activation(id, index, old_owner, false);
                self.spy_activation(id, index, new_owner, true);
            }
        }
    }

    pub(in crate::skirmish) fn delete_spy_vision(&mut self, id: usize) {
        for index in 0..self.entities[id].spy_vision.modules.len() {
            if self.entities[id].spy_vision.modules[index].active {
                self.spy_activation(id, index, self.entities[id].owner, false);
            }
        }
    }

    fn disable_spy_until(&mut self, id: usize, deadline: u64) {
        self.initialize_spy_vision(id);
        let now = self.update_frame();
        let when = u32::try_from(deadline & u64::from(u32::MAX)).expect("native spy deadline");
        for index in 0..self.entities[id].spy_vision.modules.len() {
            if when > now && self.entities[id].spy_vision.modules[index].active {
                self.spy_activation(id, index, self.entities[id].owner, false);
            }
            self.entities[id].spy_vision.modules[index].reset = true;
            self.wake_family_update(
                id,
                "SpyVisionUpdate",
                index,
                if when > now { when - now } else { 1 },
            );
        }
    }

    pub(in crate::skirmish) fn sabotage_spy_vision(&mut self, owner: usize, deadline: u64) {
        for id in self.team_membership.player_objects(owner) {
            if !self.catalog[self.entities[id].type_index]
                .behavior
                .spy_vision
                .is_empty()
            {
                self.disable_spy_until(id, deadline);
            }
        }
    }

    pub(in crate::skirmish) fn observe_spy_vision_disabled(&mut self, id: usize) {
        self.observe_special_power_disabled(id);
        let mask = self.object_disabled_mask(id);
        let old = self.entities[id].disabled_observed;
        self.observe_native_disabled_tint(id, old, mask);
        if (old == 0) != (mask == 0) {
            self.native_disabled_edge(id, mask != 0, old != 0);
        }
        self.entities[id].disabled_observed = self.object_disabled_mask(id);
    }

    pub(in crate::skirmish) fn native_spy_disabled_edge(
        &mut self,
        id: usize,
        index: usize,
        disabled: bool,
    ) {
        let now = self.update_frame();
        let frame = if disabled { u32::MAX } else { 0 };
        if frame > now && self.entities[id].spy_vision.modules[index].active {
            self.spy_activation(id, index, self.entities[id].owner, false);
        }
        self.entities[id].spy_vision.modules[index].reset = true;
        self.wake_family_update(
            id,
            "SpyVisionUpdate",
            index,
            if frame > now { frame - now } else { 1 },
        );
    }

    pub(in crate::skirmish) fn update_spy_module(&mut self, id: usize, index: usize) -> i32 {
        let rule = &self.catalog[self.entities[id].type_index]
            .behavior
            .spy_vision[index];
        let (duration, interval, self_powered) = (rule.duration, rule.interval, rule.self_powered);
        let now = self.update_frame();
        let state = &mut self.entities[id].spy_vision.modules[index];
        if state.reset {
            state.reset = false;
            if self_powered {
                if interval == 0 {
                    self.spy_activation(id, index, self.entities[id].owner, true);
                    return 0x3fff_ffff;
                }
                return i32::from_ne_bytes(interval.to_ne_bytes());
            }
        }
        let setting = if state.active && state.deactivate <= now {
            Some(false)
        } else if !state.active && self_powered {
            Some(true)
        } else {
            None
        };
        if let Some(setting) = setting {
            self.spy_activation(id, index, self.entities[id].owner, setting);
            self.entities[id].spy_vision.modules[index].deactivate = if !setting {
                0
            } else if duration == 0 {
                u32::MAX
            } else {
                now.wrapping_add(duration)
            };
        }
        if !self_powered {
            return 0x3fff_ffff;
        }
        let sleep = if self.entities[id].spy_vision.modules[index].active {
            duration
        } else {
            interval
        };
        i32::from_ne_bytes(sleep.to_ne_bytes())
    }
}

pub(super) fn validate(rules: &[Rules]) -> Result<(), &'static str> {
    if rules
        .iter()
        .any(|rule| rule.needs_upgrade && rule.trigger.is_empty())
    {
        return Err("Invalid represented self-powered spy vision");
    }
    Ok(())
}
