//! SH 2bfaea60 `SpecialPowerModule` / Player shared timers, CRC=0.
use super::{Skirmish, ai_dispatch::native_frame};

#[cfg(test)]
mod tests;

pub struct Rules {
    pub tag: String,
    pub name: String,
    pub power_type: String,
    pub required_science: String,
    pub timer: Timer,
    pub view_range: f32,
    pub view_frames: u32,
    pub view_template: Option<String>,
    pub initiate_sound: String,
    pub target_sound: String,
    pub module_initiate_sound: String,
    pub academy_classification: String,
    pub update_starts_attack: bool,
    pub startup: Startup,
    pub script_only: bool,
}

pub struct Timer {
    pub reload_frames: u32,
    pub shared: bool,
    pub public: bool,
}

pub struct Startup {
    pub paused: bool,
    pub ready: bool,
}

pub(in crate::skirmish) struct State {
    available: u32,
    paused_count: i32,
    paused_frame: u32,
    paused_percent: f32,
    disabled_mask: u32,
}

/// `ScriptEngine` retains notification identity; it precedes sound, view and recharge.
pub struct EvaEvent {
    pub owner: usize,
    pub power: String,
}

pub struct Trigger {
    pub owner: usize,
    pub power: String,
    pub object: usize,
}

impl Skirmish {
    /// `UnpauseSpecialPowerUpgrade::upgradeImplementation`, after the shared mux.
    pub(in crate::skirmish) fn apply_unpause_special_power_upgrade(
        &mut self,
        id: usize,
        ordinal: usize,
    ) {
        let kind = self.entities[id].type_index;
        let Some(name) = self.catalog[kind].unpause_special_powers[ordinal].clone() else {
            return;
        };
        for power in 0..self.catalog[kind].special_powers.len() {
            let rules = &self.catalog[kind].special_powers[power];
            if rules.name != name {
                continue;
            }
            let ready = rules.startup.ready;
            self.pause_special_power(id, power, false);
            if ready {
                self.set_special_power_ready_at_ordinal(id, power, self.update_frame());
            }
        }
    }

    pub(in crate::skirmish) fn native_power_available(&self, id: usize, name: &str) -> bool {
        let Some(object) = self.entities.get(id).filter(|object| object.complete()) else {
            return false;
        };
        self.catalog[object.type_index]
            .special_powers
            .iter()
            .any(|rules| {
                rules.name == name
                    && !rules.script_only
                    && (rules.required_science.is_empty()
                        || rules.required_science.eq_ignore_ascii_case("NONE")
                        || self.players[object.owner].has_science(&rules.required_science))
            })
    }

    /// `Player::addScience` visits behavior modules in authored order, including
    /// unfinished objects, and sets the local ready frame after the callback.
    pub(in crate::skirmish) fn special_power_science_acquired(
        &mut self,
        owner: usize,
        science: &str,
    ) {
        for id in self.team_membership.player_objects(owner) {
            let object = &self.entities[id];
            if !object.exists() || object.owner != owner {
                continue;
            }
            let modules: Vec<_> = self.catalog[object.type_index]
                .special_powers
                .iter()
                .enumerate()
                .filter(|(_, rules)| rules.required_science == science)
                .map(|(ordinal, _)| ordinal)
                .collect();
            for ordinal in modules {
                self.special_power_on_creation(id, ordinal);
                self.set_special_power_ready_at_ordinal(id, ordinal, self.update_frame());
            }
        }
    }

    pub(in crate::skirmish) fn special_power_remaining(&mut self, id: usize, name: &str) -> u32 {
        let Some(ordinal) = self.special_power_ordinal(id, name) else {
            return 0;
        };
        let mut ready = self.special_power_timer(id, ordinal);
        let rules = &self.catalog[self.entities[id].type_index].special_powers[ordinal];
        let state = &self.entities[id].behaviors.special_powers[ordinal];
        if !rules.timer.shared && (state.paused_count > 0 || self.object_disabled(id)) {
            ready = ready.wrapping_add(self.update_frame().wrapping_sub(state.paused_frame));
        }
        ready.saturating_sub(self.update_frame())
    }

    pub(in crate::skirmish) fn construct_special_power(&mut self, id: usize, ordinal: usize) {
        let rules = &self.catalog[self.entities[id].type_index].special_powers[ordinal];
        let (shared, ready, paused) = (
            rules.timer.shared,
            rules.startup.ready,
            rules.startup.paused,
        );
        self.entities[id].behaviors.special_powers.push(State {
            // Stock buildings without SpecialPowerCreate, including USA's
            // Strategy Center, rely on the retail constructor's zero frame.
            available: 0,
            paused_count: 0,
            paused_frame: 0,
            paused_percent: 0.0,
            disabled_mask: 0,
        });
        if !self.entities[id].under_construction() && !shared {
            self.recharge_special_power(id, ordinal);
            if ready {
                self.entities[id].behaviors.special_powers[ordinal].available = self.update_frame();
            }
        }
        if paused {
            self.pause_special_power(id, ordinal, true);
        }
    }

    pub(in crate::skirmish) fn special_power_ordinal(
        &self,
        id: usize,
        name: &str,
    ) -> Option<usize> {
        self.catalog[self.entities[id].type_index]
            .special_powers
            .iter()
            .position(|rules| rules.name == name)
    }

    pub(in crate::skirmish) fn recharge_special_power(&mut self, id: usize, ordinal: usize) {
        let now = self.update_frame();
        let rules = &self.catalog[self.entities[id].type_index].special_powers[ordinal];
        if rules.timer.shared {
            // Player::resetOrStart inserts an absent timer at NOW; only an
            // existing timer receives the reload interval.
            let timers = &mut self.players[self.entities[id].owner].special_power_ready;
            timers
                .entry(rules.name.clone())
                .and_modify(|ready| *ready = now.wrapping_add(rules.timer.reload_frames))
                .or_insert(now);
        } else {
            self.entities[id].behaviors.special_powers[ordinal].available =
                now.wrapping_add(rules.timer.reload_frames);
        }
    }

    pub(in crate::skirmish) fn special_power_on_creation(&mut self, id: usize, ordinal: usize) {
        self.recharge_special_power(id, ordinal);
        let rules = &self.catalog[self.entities[id].type_index].special_powers[ordinal];
        let paused = rules.startup.paused;
        if rules.timer.shared {
            let now = self.update_frame();
            self.players[self.entities[id].owner]
                .special_power_ready
                .insert(rules.name.clone(), now);
            self.entities[id].behaviors.special_powers[ordinal].available = now;
        }
        if paused {
            self.pause_special_power(id, ordinal, true);
        }
    }

    pub(in crate::skirmish) fn set_special_power_ready_frame(
        &mut self,
        id: usize,
        name: &str,
        frame: u32,
    ) {
        let Some(ordinal) = self.special_power_ordinal(id, name) else {
            return;
        };
        self.set_special_power_ready_at_ordinal(id, ordinal, frame);
    }

    fn set_special_power_ready_at_ordinal(&mut self, id: usize, ordinal: usize, frame: u32) {
        let now = self.update_frame();
        let state = &mut self.entities[id].behaviors.special_powers[ordinal];
        state.available = frame;
        state.paused_frame = now;
    }

    /// Native `SpecialPowerModule::getReadyFrame` includes paused elapsed time.
    pub(in crate::skirmish) fn special_power_ready_frame(
        &mut self,
        id: usize,
        name: &str,
    ) -> Option<u32> {
        let ordinal = self.special_power_ordinal(id, name)?;
        if self.catalog[self.entities[id].type_index].special_powers[ordinal]
            .timer
            .shared
        {
            return Some(self.special_power_timer(id, ordinal));
        }
        let state = &self.entities[id].behaviors.special_powers[ordinal];
        let elapsed = if state.paused_count > 0 || self.object_disabled(id) {
            self.update_frame().wrapping_sub(state.paused_frame)
        } else {
            0
        };
        Some(state.available.wrapping_add(elapsed))
    }

    fn special_power_timer(&mut self, id: usize, ordinal: usize) -> u32 {
        let now = self.update_frame();
        let rules = &self.catalog[self.entities[id].type_index].special_powers[ordinal];
        if rules.timer.shared {
            *self.players[self.entities[id].owner]
                .special_power_ready
                .entry(rules.name.clone())
                .or_insert(now)
        } else {
            self.entities[id].behaviors.special_powers[ordinal].available
        }
    }

    pub fn special_power_ready(&mut self, id: usize, name: &str) -> bool {
        let Some(ordinal) = self.special_power_ordinal(id, name) else {
            return false;
        };
        self.special_power_ready_at_ordinal(id, ordinal)
    }

    fn special_power_ready_at_ordinal(&mut self, id: usize, ordinal: usize) -> bool {
        let rules = &self.catalog[self.entities[id].type_index].special_powers[ordinal];
        let unpaused = rules.timer.shared
            || self.entities[id].behaviors.special_powers[ordinal].paused_count == 0;
        unpaused && self.update_frame() >= self.special_power_timer(id, ordinal)
    }

    pub fn special_power_percent(&mut self, id: usize, name: &str) -> f32 {
        self.special_power_ordinal(id, name).map_or(0.0, |ordinal| {
            self.special_power_percent_at_ordinal(id, ordinal)
        })
    }

    fn special_power_percent_at_ordinal(&mut self, id: usize, ordinal: usize) -> f32 {
        let state = &self.entities[id].behaviors.special_powers[ordinal];
        if state.paused_count > 0 && state.paused_percent.to_bits() == 1.0_f32.to_bits() {
            return 0.99999;
        }
        if self.special_power_ready_at_ordinal(id, ordinal) {
            return 1.0;
        }
        let state = &self.entities[id].behaviors.special_powers[ordinal];
        if state.paused_count > 0 {
            return state.paused_percent;
        }
        let ready = self.special_power_timer(id, ordinal);
        let reload = self.catalog[self.entities[id].type_index].special_powers[ordinal]
            .timer
            .reload_frames;
        1.0 - native_real(ready.wrapping_sub(self.update_frame())) / native_real(reload)
    }

    /// `Object::setDisabled` and `clearDisabled` pause once per changed type.
    /// HELD is deliberately excluded; overlapping disable causes remain nested.
    pub(in crate::skirmish) fn observe_special_power_disabled(&mut self, id: usize) {
        // HACKED owns its exact source setter/clear pauses; the remaining
        // translated producers are observed here. HELD never pauses.
        let observed = !((1 << 0) | (1 << 1) | (1 << 2) | (1 << 3));
        let mask = self.object_disabled_mask(id) & observed;
        let mut changed = self.entities[id]
            .behaviors
            .special_powers
            .iter()
            .fold(0, |bits, state| {
                bits | ((state.disabled_mask ^ mask) & observed)
            });
        while changed != 0 {
            let flag = 1 << changed.trailing_zeros();
            changed &= !flag;
            for ordinal in 0..self.entities[id].behaviors.special_powers.len() {
                let old = self.entities[id].behaviors.special_powers[ordinal].disabled_mask;
                if (old ^ mask) & flag != 0 {
                    self.pause_special_power(id, ordinal, mask & flag != 0);
                    self.entities[id].behaviors.special_powers[ordinal].disabled_mask =
                        (old & !flag) | (mask & flag);
                }
            }
        }
    }

    /// A native disabled producer pauses before writing its status bit.
    pub(in crate::skirmish) fn native_unmanned_power_pause(&mut self, id: usize) {
        for ordinal in 0..self.entities[id].behaviors.special_powers.len() {
            self.pause_special_power(id, ordinal, true);
            self.entities[id].behaviors.special_powers[ordinal].disabled_mask |= 1 << 5;
        }
    }

    /// HACKED pauses before a deadline/mask write, including past deadlines.
    pub(in crate::skirmish) fn native_hacked_power_change(&mut self, id: usize, pause: bool) {
        for ordinal in 0..self.entities[id].behaviors.special_powers.len() {
            self.pause_special_power(id, ordinal, pause);
        }
    }

    pub(in crate::skirmish) fn native_timed_power_change(
        &mut self,
        id: usize,
        flag: u32,
        pause: bool,
    ) {
        for ordinal in 0..self.entities[id].behaviors.special_powers.len() {
            self.pause_special_power(id, ordinal, pause);
            let flags = &mut self.entities[id].behaviors.special_powers[ordinal].disabled_mask;
            *flags = (*flags & !flag) | if pause { flag } else { 0 };
        }
    }

    pub(in crate::skirmish) fn pause_special_power(
        &mut self,
        id: usize,
        ordinal: usize,
        pause: bool,
    ) {
        if pause {
            if self.entities[id].behaviors.special_powers[ordinal].paused_count == 0 {
                let now = self.update_frame();
                let percent = self.special_power_percent_at_ordinal(id, ordinal);
                let state = &mut self.entities[id].behaviors.special_powers[ordinal];
                state.paused_frame = now;
                state.paused_percent = percent;
            }
            self.entities[id].behaviors.special_powers[ordinal].paused_count += 1;
        } else {
            let now = self.update_frame();
            let state = &mut self.entities[id].behaviors.special_powers[ordinal];
            if state.paused_count > 0 {
                state.paused_count -= 1;
                if state.paused_count == 0 {
                    state.available = state
                        .available
                        .wrapping_add(now.wrapping_sub(state.paused_frame));
                }
            }
        }
    }

    /// Native commands perform readiness checks in `ActionManager` before dispatch.
    pub(in crate::skirmish) fn special_power_at_location(
        &mut self,
        id: usize,
        name: &str,
        target: [f32; 3],
        script: bool,
    ) -> bool {
        self.native_special_power(id, name, None, Some(target), script)
    }

    pub(in crate::skirmish) fn special_power_at_object(
        &mut self,
        id: usize,
        name: &str,
        target: usize,
    ) -> bool {
        self.native_special_power(id, name, Some(target), None, false)
    }

    pub(in crate::skirmish) fn native_special_power(
        &mut self,
        id: usize,
        name: &str,
        target: Option<usize>,
        position: Option<[f32; 3]>,
        script: bool,
    ) -> bool {
        let Some(ordinal) = self.special_power_ordinal(id, name) else {
            return false;
        };
        if self.entities[id].behaviors.special_powers[ordinal].paused_count > 0
            || self.object_disabled(id)
        {
            return false;
        }
        let kind = self.entities[id].type_index;
        if let Some(ocl) = self.catalog[kind]
            .behavior
            .ocl_powers
            .iter()
            .position(|rules| rules.power == name)
        {
            if let Some(position) = target
                .map(|target| self.object_position(target))
                .or(position)
            {
                self.execute_power_ocl(id, ocl, position, None);
            } else {
                self.execute_power_ocl_without_target(id, ocl);
            }
            return true;
        }
        let tag = &self.catalog[kind].special_powers[ordinal].tag;
        if let Some(cash) = self.catalog[kind]
            .behavior_modules
            .iter()
            .filter(|module| module.family == "CashHackSpecialPower")
            .position(|module| &module.tag == tag)
        {
            let Some(target) = target else {
                return false;
            };
            self.record_special_power_used(id, ordinal);
            self.mark_special_power_triggered(id, name, Some(self.object_position(target)));
            self.execute_cash_hack(id, target, cash);
            return true;
        }
        if let Some(spy) = self.catalog[kind]
            .behavior_modules
            .iter()
            .filter(|module| module.family == "SpyVisionSpecialPower")
            .position(|module| &module.tag == tag)
        {
            self.record_special_power_used(id, ordinal);
            self.mark_special_power_triggered(id, name, None);
            self.use_spy_vision_power(id, spy);
            return true;
        }
        self.initiate_special_power_update(id, ordinal, name, target, position, script)
    }

    fn initiate_special_power_update(
        &mut self,
        id: usize,
        ordinal: usize,
        name: &str,
        target: Option<usize>,
        position: Option<[f32; 3]>,
        script: bool,
    ) -> bool {
        let update_starts =
            self.catalog[self.entities[id].type_index].special_powers[ordinal].update_starts_attack;
        let modules: Vec<_> = self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .filter(|module| {
                matches!(
                    module.family.as_str(),
                    "SpecialAbilityUpdate"
                        | "SpectreGunshipUpdate"
                        | "SpectreGunshipDeploymentUpdate"
                        | "ParticleUplinkCannonUpdate"
                )
            })
            .map(|module| module.family.clone())
            .collect();
        let mut ability = 0;
        let mut deployment = 0;
        let mut particle = 0;
        let mut handled = false;
        for family in modules {
            match family.as_str() {
                "SpecialAbilityUpdate" => {
                    handled = self.initiate_native_ability(id, ability, name, target, position);
                    ability += 1;
                }
                "SpectreGunshipDeploymentUpdate" => {
                    let rules =
                        &self.catalog[self.entities[id].type_index].spectre_deployments[deployment];
                    let science = &rules.required_science;
                    let valid = science.is_empty()
                        || science.eq_ignore_ascii_case("NONE")
                        || self.players[self.entities[id].owner].has_science(science);
                    if valid && let Some(position) = position {
                        handled = self.deploy_spectre(id, deployment, name, position, script);
                    }
                    deployment += 1;
                }
                "ParticleUplinkCannonUpdate" => {
                    if let Some(position) = position {
                        handled =
                            self.initiate_particle_uplink(id, particle, name, position, script);
                    }
                    particle += 1;
                }
                "SpectreGunshipUpdate" => {
                    if let Some(position) = position {
                        handled = self.initiate_spectre(id, name, position, script);
                    }
                }
                _ => unreachable!("selected SpecialPowerUpdate interface"),
            }
            if handled {
                break;
            }
        }
        self.record_special_power_used(id, ordinal);
        if !update_starts {
            let position = target
                .map(|target| self.object_position(target))
                .or(position);
            self.mark_special_power_triggered(id, name, position);
        }
        handled
    }

    pub(in crate::skirmish) fn mark_special_power_triggered(
        &mut self,
        id: usize,
        name: &str,
        target: Option<[f32; 3]>,
    ) {
        let Some(ordinal) = self.special_power_ordinal(id, name) else {
            return;
        };
        let owner = self.entities[id].owner;
        self.special_power_triggers.push(Trigger {
            owner,
            power: name.into(),
            object: id,
        });
        let rules = &self.catalog[self.entities[id].type_index].special_powers[ordinal];
        self.presentation.superweapon_eva.push(EvaEvent {
            owner,
            power: rules.power_type.clone(),
        });
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::SuperweaponEva(
                self.presentation.superweapon_eva.len() - 1,
            ));
        let (sound, target_sound) = (rules.initiate_sound.clone(), rules.target_sound.clone());
        self.object_sound(id, sound, super::object_sounds::Action::Play);
        if let Some(target) = target {
            self.position_sound(id, target_sound, target);
            self.special_power_view(id, ordinal, target);
        }
        self.recharge_special_power(id, ordinal);
    }

    fn special_power_view(&mut self, id: usize, ordinal: usize, target: [f32; 3]) {
        let rules = &self.catalog[self.entities[id].type_index].special_powers[ordinal];
        if rules.view_range == 0.0 || rules.view_frames == 0 {
            return;
        }
        let Some(template) = &rules.view_template else {
            return;
        };
        let kind = self
            .catalog
            .iter()
            .position(|unit| unit.name == *template)
            .expect("validated special power view template");
        let (range, frames) = (rules.view_range, rules.view_frames);
        let object = self.new_object(kind, self.entities[id].owner, true);
        self.set_object_position(object, target);
        self.set_shroud_clearing_range(object, range);
        if let Some(registration) = self.catalog[kind].update_modules.iter().position(|module| matches!(module, super::module_updates::ModuleRegistration::Behavior { family, .. } if family == "DeletionUpdate")) {
            let delay = frames.max(1);
            self.entities[object].behaviors.deletion_frames[0] = native_frame(self.tick).wrapping_add(delay);
            self.wake_update(object, registration, delay);
        }
    }
}

fn native_real(value: u32) -> f32 {
    let low = u16::try_from(value & u32::from(u16::MAX)).expect("low word");
    let high = u16::try_from(value >> 16).expect("high word");
    f32::from(high) * 65536.0 + f32::from(low)
}
