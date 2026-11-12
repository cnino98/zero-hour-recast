//! Native `RadarUpdate` extension animation and `RadarUpgrade` availability.
use super::{Skirmish, progression};

#[cfg(test)]
mod grant_tests;
#[cfg(test)]
mod tests;

pub struct Upgrade {
    pub triggers: Vec<String>,
    pub conflicts: Vec<String>,
    pub requires_all: bool,
    pub disable_proof: bool,
}

pub struct Grant {
    pub name: Option<String>,
    pub academy_radar: bool,
    pub player: bool,
    pub on_created: bool,
}

pub struct Event {
    pub serial: u64,
    pub frame: u64,
    pub owner: usize,
    pub online: bool,
}

pub(super) struct PlayerRadar {
    count: i32,
    disable_proof: i32,
    pub(super) disabled: bool,
}

impl PlayerRadar {
    pub(super) const fn new() -> Self {
        Self {
            count: 0,
            disable_proof: 0,
            disabled: false,
        }
    }
    pub(super) const fn available(&self) -> bool {
        self.count > 0 && (!self.disabled || self.disable_proof != 0)
    }
}

pub struct Rules {
    /// INI parseDurationReal preserves fractional logic frames.
    pub extend_frames: f32,
}

struct Module {
    extend_done: u32,
    complete: bool,
}

pub(super) struct State {
    modules: Vec<Module>,
    upgrades: Vec<bool>,
    disabled: bool,
    model_flags: u64,
}

// Native ModelState.h indices. These are shared Drawable flags, not per-module.
const EXTENDING: u64 = 1 << 58;
const UPGRADED: u64 = 1 << 59;

impl State {
    pub(super) const fn new() -> Self {
        Self {
            modules: Vec::new(),
            upgrades: Vec::new(),
            disabled: false,
            model_flags: 0,
        }
    }

    pub(super) fn conditions(&self) -> Vec<&'static str> {
        let mut result = Vec::new();
        if self.model_flags & EXTENDING != 0 {
            result.push("RADAR_EXTENDING");
        }
        if self.model_flags & UPGRADED != 0 {
            result.push("RADAR_UPGRADED");
        }
        result
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_radar(&mut self, id: usize) {
        self.entities[id].radar.modules.push(Module {
            extend_done: 0,
            complete: false,
        });
    }

    pub(in crate::skirmish) fn update_radar(&mut self, id: usize, ordinal: usize) -> i32 {
        let now = self.update_frame();
        let drawable = self.entities[id].has_drawable();
        let state = &mut self.entities[id].radar;
        let module = &mut state.modules[ordinal];
        if module.extend_done != 0 && !module.complete && now > module.extend_done {
            module.complete = true;
            module.extend_done = 0;
            if drawable {
                state.model_flags = (state.model_flags & !EXTENDING) | UPGRADED;
            }
        }
        1
    }

    fn extend_radar(&mut self, id: usize) {
        let actor = &self.entities[id];
        let Some(rules) = self.catalog[actor.type_index]
            .behavior
            .radar_updates
            .first()
        else {
            return;
        };
        // UInt + Real is evaluated in Real, then truncated on assignment to UInt.
        let deadline = progression::integer_real(self.update_frame()) + rules.extend_frames;
        let deadline = num_traits::ToPrimitive::to_u32(&deadline.trunc())
            .expect("RadarUpdate: native UInt deadline conversion is undefined outside the admitted runtime domain");
        let drawable = actor.has_drawable();
        let state = &mut self.entities[id].radar;
        let module = state.modules.first_mut().expect("constructed RadarUpdate");
        module.extend_done = deadline;
        // Native extendRadar does not reset m_extendComplete after a previous completion.
        if drawable {
            state.model_flags |= EXTENDING;
        }
    }

    pub(in crate::skirmish) fn construct_radar_upgrade(&mut self, id: usize) {
        self.entities[id].radar.upgrades.push(false);
    }

    pub(in crate::skirmish) fn apply_radar_upgrade(&mut self, id: usize, ordinal: usize) {
        let owner = self.entities[id].owner;
        let proof = self.catalog[self.entities[id].type_index]
            .behavior
            .radar_upgrades[ordinal]
            .disable_proof;
        self.change_radar_count(owner, proof, true);
        self.extend_radar(id);
        self.entities[id].radar.upgrades[ordinal] = true;
    }

    pub(in crate::skirmish) fn reset_radar_upgrade(&mut self, id: usize, ordinal: usize) {
        // UpgradeMux::resetUpgrade only clears this latch; it does not undo implementation.
        self.entities[id].radar.upgrades[ordinal] = false;
    }

    fn change_radar_count(&mut self, owner: usize, proof: bool, add: bool) {
        let old = self.players[owner].radar.available();
        let state = &mut self.players[owner].radar;
        if add {
            state.count = state
                .count
                .checked_add(1)
                .expect("RadarUpgrade: native signed radar count overflow is undefined");
        } else {
            debug_assert!(
                state.count > 0,
                "native removeRadar requires positive count"
            );
            state.count = state
                .count
                .checked_sub(1)
                .expect("RadarUpgrade: native signed radar count overflow is undefined");
        }
        if proof {
            state.disable_proof =
                if add {
                    state.disable_proof.checked_add(1).expect(
                        "RadarUpgrade: native signed proof radar count overflow is undefined",
                    )
                } else {
                    state.disable_proof.checked_sub(1).expect(
                        "RadarUpgrade: native signed proof radar count overflow is undefined",
                    )
                };
        }
        self.radar_edge(owner, old);
    }

    fn radar_edge(&mut self, owner: usize, old: bool) {
        let online = self.players[owner].radar.available();
        if old != online && self.logic_update && self.tick > 0 && !self.defeated[owner] {
            self.presentation.radar_events.push(Event {
                serial: self.radar_event_serial,
                frame: self.tick,
                owner,
                online,
            });
            self.radar_event_serial = self.radar_event_serial.wrapping_add(1);
        }
    }

    pub(in crate::skirmish) fn set_player_radar_disabled(&mut self, owner: usize, disabled: bool) {
        let old = self.players[owner].radar.available();
        self.players[owner].radar.disabled = disabled;
        self.radar_edge(owner, old);
    }

    pub(in crate::skirmish) fn remember_radar_disabled_mask(&mut self, id: usize, disabled: bool) {
        self.entities[id].radar.disabled = disabled;
    }

    pub(in crate::skirmish) fn native_radar_disabled_edge(&mut self, id: usize, disabled: bool) {
        self.entities[id].radar.disabled = self.object_disabled(id);
        if self.entities[id].radar.upgrades.first() == Some(&true) {
            let actor = &self.entities[id];
            let proof = self.catalog[actor.type_index].behavior.radar_upgrades[0].disable_proof;
            self.change_radar_count(actor.owner, proof, !disabled);
        }
    }

    pub(in crate::skirmish) fn radar_on_capture(&mut self, id: usize, old: usize, new: usize) {
        if self.object_disabled(id) {
            return;
        }
        let index = self.entities[id].type_index;
        for ordinal in 0..self.entities[id].radar.upgrades.len() {
            if self.entities[id].radar.upgrades[ordinal] {
                let proof = self.catalog[index].behavior.radar_upgrades[ordinal].disable_proof;
                self.change_radar_count(old, proof, false);
                self.entities[id].radar.upgrades[ordinal] = false;
                self.change_radar_count(new, proof, true);
                self.entities[id].radar.upgrades[ordinal] = true;
            }
        }
    }

    pub(in crate::skirmish) fn radar_on_delete(&mut self, id: usize) {
        if self.object_disabled(id) {
            return;
        }
        let index = self.entities[id].type_index;
        let owner = self.entities[id].owner;
        for ordinal in 0..self.entities[id].radar.upgrades.len() {
            if self.entities[id].radar.upgrades[ordinal] {
                let proof = self.catalog[index].behavior.radar_upgrades[ordinal].disable_proof;
                self.change_radar_count(owner, proof, false);
                self.entities[id].radar.upgrades[ordinal] = false;
            }
        }
    }

    pub(in crate::skirmish) fn grant_native_upgrade_create(
        &mut self,
        id: usize,
        ordinal: usize,
        created: bool,
    ) {
        let rule = &self.catalog[self.entities[id].type_index]
            .behavior
            .radar_grants[ordinal];
        if created && (!rule.on_created || self.entities[id].under_construction()) {
            return;
        }
        let Some(name) = rule.name.clone() else {
            return;
        };
        let player = rule.player;
        let academy_radar = rule.academy_radar;
        let owner = self.entities[id].owner;
        if player {
            self.complete_player_upgrade(owner, &name);
        } else {
            self.grant_native_object_upgrade(id, &name);
        }
        // GrantUpgradeCreate::onCreate alone records this automatically granted upgrade.
        if created && academy_radar {
            self.players[owner].academy.researched_radar = true;
        }
    }
}
