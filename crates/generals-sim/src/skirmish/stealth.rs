//! Native `StealthUpdate` and `StealthDetectorUpdate`, SH 2bfaea60.
use super::{Entity, Skirmish, UnitType};

mod detection;
mod eligibility;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(in crate::skirmish) use tests::rules as test_rules;

pub const ATTACKING: u32 = 1;
pub const MOVING: u32 = 2;
pub const USING_ABILITY: u32 = 4;
pub const FIRING_PRIMARY: u32 = 8;
pub const FIRING_SECONDARY: u32 = 16;
pub const FIRING_TERTIARY: u32 = 32;
pub const NO_BLACK_MARKET: u32 = 64;
pub const TAKING_DAMAGE: u32 = 128;
pub const RIDERS_ATTACKING: u32 = 256;

pub enum Wake {
    Automatic,
    SpecialPower,
}

mod disguise;
pub use disguise::{Appearance, Rules as DisguiseRules};

const CAN_STEALTH: u8 = 1;
const STEALTHED: u8 = 2;
const DETECTED: u8 = 4;
const UPGRADED: u8 = 8;

pub struct Rules {
    pub module_order: usize,
    pub delay: u32,
    pub forbidden: u32,
    pub threshold_speed: f32,
    pub required_status: Vec<String>,
    pub forbidden_status: Vec<String>,
    pub hint_status: Vec<String>,
    pub innate: bool,
    pub use_rider: bool,
    pub idle_enemies_on_reveal: bool,
    pub wake: Wake,
    pub upgrades: Vec<String>,
    pub reveal_distance: f32,
    pub black_market_frames: u32,
    pub friendly_opacity: [f32; 2],
    pub pulse_frames: u32,
    pub disguise: Option<DisguiseRules>,
}

pub struct Detector {
    pub module_order: usize,
    pub rate: u32,
    pub range: f32,
    pub initially_disabled: bool,
    pub required_kind: Vec<String>,
    pub forbidden_kind: Vec<String>,
    pub while_garrisoned: bool,
    pub while_contained: bool,
    pub feedback: Option<DetectorFeedback>,
}

pub struct DetectorFeedback {
    pub bone: String,
    /// Normal ping, detected ping, and the beacon, in native creation order.
    pub particles: [String; 3],
    pub sounds: [String; 2],
}

pub struct ScanFeedback {
    pub type_index: usize,
    pub position: [f32; 3],
    pub facing: f32,
    pub health_ratio: f32,
    pub visible_players: u16,
}

pub enum Event {
    Pulse {
        object: usize,
    },
    DisguiseOpacity {
        object: usize,
        opacity: f32,
    },
    Look {
        object: usize,
        stealthed: bool,
        detected: bool,
        hint: bool,
    },
    Heat {
        object: usize,
    },
    Sound {
        object: usize,
        on: bool,
    },
    Detection {
        detector: usize,
        target: usize,
    },
    Scan {
        detector: usize,
        module: usize,
        found: bool,
        feedback: Option<ScanFeedback>,
    },
}

pub(super) struct State {
    allowed_frame: u64,
    detected_until: u64,
    next_black_market: u64,
    status: u8,
    enabled: bool,
    frames_granted: u32,
    disguise: disguise::State,
}

impl State {
    const fn has(&self, flag: u8) -> bool {
        self.status & flag != 0
    }
    const fn set(&mut self, flag: u8, value: bool) {
        if value {
            self.status |= flag;
        } else {
            self.status &= !flag;
        }
    }

    pub(super) fn for_unit(unit: &UnitType, frame: u64) -> Option<Box<Self>> {
        unit.behavior.stealth.as_ref().map(|rules| {
            Box::new(Self {
                allowed_frame: deadline(frame, rules.delay),
                detected_until: 0,
                next_black_market: 0,
                status: if rules.innate { CAN_STEALTH } else { 0 },
                enabled: matches!(rules.wake, Wake::Automatic) && rules.disguise.is_none(),
                frames_granted: 0,
                disguise: disguise::State::default(),
            })
        })
    }
}

impl Entity {
    #[must_use]
    pub fn stealthed(&self) -> bool {
        self.stealth
            .as_ref()
            .is_some_and(|state| state.has(STEALTHED))
    }

    #[must_use]
    pub fn detected(&self) -> bool {
        self.stealth
            .as_ref()
            .is_some_and(|state| state.has(DETECTED))
    }
}

impl Skirmish {
    /// Player group move/append and stop give a revealed unit time to cloak again.
    pub(super) fn delay_group_stealth_acquisition(&mut self, id: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let Some(rules) = &unit.behavior.stealth else {
            return;
        };
        if actor.ai.registration.is_none()
            || unit.ai.acquisition == 0
            || unit.ai.acquire_stealthed()
            || matches!(rules.wake, Wake::SpecialPower)
            || actor
                .stealth
                .as_ref()
                .is_none_or(|state| !state.has(CAN_STEALTH))
            || actor.stealthed()
            || actor.detected()
        {
            return;
        }
        let delay = rules.delay.wrapping_add(self.random_between(30));
        let mood = &mut self.entities[id].ai.mood;
        mood.next_check = deadline(self.tick, delay);
        mood.offset_check = false;
    }

    pub(in crate::skirmish) fn apply_stealth_upgrade(&mut self, id: usize) {
        if let Some(state) = &mut self.entities[id].stealth {
            state.set(CAN_STEALTH | UPGRADED, true);
        }
        if self.uses_spawn_weapons(id) {
            for child in self.spawned_children(id).to_vec() {
                if let Some(state) = &mut self.entities[child].stealth {
                    state.set(CAN_STEALTH, true);
                }
            }
        }
    }

    /// Native logical visibility does not replace map shroud or container hiding.
    #[must_use]
    pub fn logically_visible(&self, owner: usize, id: usize) -> bool {
        let Some(actor) = self.entities.get(id) else {
            return false;
        };
        let outer = actor.contained_by().unwrap_or(id);
        let actor = &self.entities[outer];
        let unit = &self.catalog[actor.type_index];
        unit.kind_of.iter().any(|kind| kind == "DISGUISER")
            || !actor.stealthed()
            || actor.detected()
            || self.allied(owner, actor.owner)
            || self.defeated.get(owner).is_some_and(|defeated| *defeated)
    }

    #[must_use]
    pub fn stealth_targetable(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(victim)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        // WeaponSet's ordinary object attack gate is independent of shroud.
        actor.ai.attack_context.ignoring_stealth
            || !victim.stealthed()
            || victim.detected()
            || self.attack_kind(target, "DISGUISER")
                && victim
                    .disguised_player()
                    .is_none_or(|owner| self.enemies(actor.owner, owner))
    }

    pub(in crate::skirmish) fn grant_stealth(&mut self, id: usize) {
        let now = u64::from(self.update_frame());
        if let Some(state) = &mut self.entities[id].stealth {
            state.enabled = true;
            state.frames_granted = 0;
            state.set(CAN_STEALTH, true);
            state.set(STEALTHED, true);
            state.allowed_frame = now;
            self.wake_family_update(id, "StealthUpdate", 0, 1);
        }
    }

    /// `SupplyCenterDockUpdate` preserves an existing permanent stealth grant.
    pub(in crate::skirmish) fn receive_temporary_stealth(&mut self, id: usize, frames: u32) {
        if self.entities[id]
            .stealth
            .as_ref()
            .is_some_and(|state| state.frames_granted > 0 || !state.has(CAN_STEALTH))
        {
            self.receive_stealth_grant(id, true, frames);
        }
    }

    fn receive_stealth_grant(&mut self, id: usize, active: bool, frames: u32) {
        let now = self.tick;
        if self.catalog[self.entities[id].type_index]
            .behavior
            .stealth
            .as_ref()
            .is_some_and(|rules| rules.disguise.is_some())
        {
            return;
        }
        let Some(state) = self.entities[id].stealth.as_mut() else {
            return;
        };
        state.enabled = active;
        state.set(CAN_STEALTH | STEALTHED, active);
        state.allowed_frame = if active { now } else { u64::from(u32::MAX) };
        state.frames_granted = if active { frames } else { 0 };
        if active {
            self.wake_family_update(id, "StealthUpdate", 0, 1);
        } else {
            self.presentation.stealth_events.push(Event::Look {
                object: id,
                stealthed: false,
                detected: self.entities[id].detected(),
                hint: false,
            });
        }
        if self.catalog[self.entities[id].type_index]
            .combat_bike
            .is_some()
            && let Some(&rider) = self.passengers(id).first()
        {
            self.receive_stealth_grant(rider, active, frames);
        }
    }

    fn update_temporary_stealth(&mut self, id: usize) {
        let state = self.entities[id]
            .stealth
            .as_mut()
            .expect("Stealth constructor");
        if state.frames_granted == 0 {
            return;
        }
        state.frames_granted -= 1;
        let player_command = self.entities[id].ai.registration.is_some()
            && self.entities[id].ai.command.last_command_source
                == super::ai_goals::COMMAND_FROM_PLAYER;
        if player_command {
            self.receive_stealth_grant(id, false, 0);
        }
        if self.entities[id]
            .stealth
            .as_ref()
            .expect("Stealth constructor")
            .frames_granted
            == 0
        {
            self.receive_stealth_grant(id, false, 0);
        }
    }

    pub(super) fn construct_stealth_update(&mut self, id: usize, registration: usize) {
        let enabled = self.entities[id]
            .stealth
            .as_ref()
            .expect("bound StealthUpdate")
            .enabled;
        self.wake_update(
            id,
            registration,
            if enabled {
                1
            } else {
                super::module_updates::FOREVER
            },
        );
    }

    pub(super) fn construct_stealth_detector(&mut self, id: usize, registration: usize) {
        let index = self.entities[id].behaviors.detection_enabled.len();
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .detectors[index];
        let enabled = !rules.initially_disabled;
        let rate = rules.rate;
        self.entities[id].behaviors.detection_enabled.push(enabled);
        let delay = if enabled {
            self.random_update_phase(rate)
        } else {
            super::module_updates::FOREVER
        };
        self.wake_update(id, registration, delay);
    }

    /// Native setSDEnabled wakes each concrete module, even when its value is unchanged.
    pub fn set_detection_enabled(&mut self, id: usize, enabled: bool) {
        let Some(actor) = self.entities.get_mut(id) else {
            return;
        };
        actor.behaviors.detection_enabled.fill(enabled);
        let count = actor.behaviors.detection_enabled.len();
        for index in 0..count {
            self.wake_family_update(
                id,
                "StealthDetectorUpdate",
                index,
                if enabled {
                    1
                } else {
                    super::module_updates::FOREVER
                },
            );
        }
    }

    pub(super) fn update_stealth_module(&mut self, id: usize) -> i32 {
        if !self.entities[id]
            .stealth
            .as_ref()
            .expect("bound StealthUpdate")
            .enabled
        {
            return i32::from_ne_bytes(super::module_updates::FOREVER.to_ne_bytes());
        }
        self.update_concealment(id);
        1
    }

    fn update_concealment(&mut self, id: usize) {
        let actor = &self.entities[id];
        let Some(state) = &actor.stealth else { return };
        if !state.enabled {
            return;
        }
        let disguise = self.catalog[actor.type_index]
            .behavior
            .stealth
            .as_ref()
            .is_some_and(|rules| rules.disguise.is_some());
        if disguise {
            if !self.update_disguise(id) {
                return;
            }
        } else {
            self.presentation
                .stealth_events
                .push(Event::Pulse { object: id });
        }
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .behavior
            .stealth
            .as_ref()
            .expect("stealth module");
        let delay = self
            .rider_stealth_rules(id, rules)
            .map_or(rules.delay, |rider| rider.delay);
        if rules.reveal_distance > 0.0
            && actor.ai.current_victim.is_some_and(|target| {
                super::movement::distance_squared(actor.position, self.entities[target].position)
                    <= rules.reveal_distance * rules.reveal_distance
            })
        {
            self.mark_detected(id, 0);
            return;
        }
        self.update_temporary_stealth(id);
        let allowed = self.allowed_to_stealth(id);
        let state = self.entities[id].stealth.as_mut().expect("stealth module");
        if allowed {
            if state.allowed_frame > self.tick {
                return;
            }
            if !state.has(STEALTHED) {
                self.presentation.stealth_events.push(Event::Sound {
                    object: id,
                    on: true,
                });
            }
            state.set(STEALTHED, true);
        } else {
            state.allowed_frame = deadline(self.tick, delay);
            // The pinned implementation plays SoundStealthOn on this edge too.
            if state.has(STEALTHED) {
                self.presentation.stealth_events.push(Event::Sound {
                    object: id,
                    on: true,
                });
            }
            state.set(STEALTHED, false);
        }
        let detected = state.detected_until > self.tick;
        if detected != state.has(DETECTED) {
            self.presentation.stealth_events.push(Event::Sound {
                object: id,
                on: !detected,
            });
        }
        state.set(DETECTED, detected);
        let stealthed = state.has(STEALTHED);
        let hint = !allowed
            && self.catalog[self.entities[id].type_index]
                .behavior
                .stealth
                .as_ref()
                .is_some_and(|rules| {
                    rules
                        .hint_status
                        .iter()
                        .any(|status| self.stealth_status(id, status))
                });
        self.presentation.stealth_events.push(Event::Look {
            object: id,
            stealthed,
            detected,
            hint,
        });
    }

    pub(super) fn mark_detected(&mut self, id: usize, frames: u32) {
        let actor = &self.entities[id];
        let Some(rules) = &self.catalog[actor.type_index].behavior.stealth else {
            return;
        };
        let owner_rules = self.rider_stealth_rules(id, rules).unwrap_or(rules);
        let delay = owner_rules.delay;
        let wake_enemies = owner_rules.idle_enemies_on_reveal;
        if let Some(disguise) = &rules.disguise {
            self.reveal_vehicle_disguise(id, disguise.reveal_transition);
        }
        let state = self.entities[id].stealth.as_mut().expect("stealth module");
        state.detected_until = if frames == 0 {
            deadline(self.tick, delay)
        } else {
            state.detected_until.max(self.tick + u64::from(frames))
        };
        if wake_enemies {
            let position = self.object_position(id);
            let owner = self.entities[id].owner;
            for other in 0..self.entities.len() {
                if !self.enemies(owner, self.entities[other].owner) {
                    continue;
                }
                let range = self.entities[other]
                    .vision_range(&self.catalog[self.entities[other].type_index]);
                let point = self.object_position(other);
                let distance = (0..3)
                    .map(|axis| (point[axis] - position[axis]).powi(2))
                    .sum::<f32>();
                if distance <= range * range {
                    self.entities[other].ai.mood.next_check = self.tick;
                }
            }
        }
    }

    fn rider_stealth_rules(&self, id: usize, rules: &Rules) -> Option<&Rules> {
        if !rules.use_rider {
            return None;
        }
        let rider = *self.passengers(id).first()?;
        self.catalog[self.entities[rider].type_index]
            .behavior
            .stealth
            .as_ref()
    }
}

// Native frame counters and unsigned stealth delays add with 32-bit wrapping.
fn deadline(frame: u64, delay: u32) -> u64 {
    frame.wrapping_add(u64::from(delay)) & u64::from(u32::MAX)
}
