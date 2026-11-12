//! `HackInternetAIUpdate` and its three original states, outdoors or contained.
use super::{Entity, Order, Skirmish, startup};

pub struct HackerRules {
    pub unpack: u32,
    pub pack: u32,
    pub variation: f32,
    pub delay: u32,
    pub contained_delay: u32,
    pub amounts: [u32; 4],
    pub experience: u32,
}

use super::ai_dispatch::stored_command::{Command, Kind as CommandKind};
use super::ai_goals::{COMMAND_FROM_AI, COMMAND_FROM_PLAYER, MainState};
use num_traits::ToPrimitive;

enum Phase {
    Idle,
    Unpack,
    Hack,
    Pack,
}

pub(super) struct State {
    phase: Phase,
    remaining: u32,
    pending: Option<Command>,
    pub(super) animation: Option<(u64, u32, bool)>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            remaining: 0,
            pending: None,
            animation: None,
        }
    }
}

pub enum Effect {
    Unpack,
    Pack,
    Cash {
        amount: u32,
        position: [f32; 3],
        container_radii: Option<[f32; 2]>,
        visible_players: u32,
    },
}

pub struct Event {
    pub object: usize,
    pub frame: u64,
    pub effect: Effect,
}

impl Entity {
    // HackInternetAIUpdate::isIdle hides the brief idle state before reissuing
    // the pending command, so team scripts cannot replace it in that interval.
    pub(super) fn ai_idle(&self) -> bool {
        self.chinook_idle()
            && self.jet_idle()
            && matches!(
                self.ai.command.main_state,
                super::ai_goals::MainState::Active
            )
            && matches!(self.ai.order, Order::Idle)
            && self.hacking.pending.is_none()
    }

    #[must_use]
    pub const fn hacking_condition(&self) -> Option<&'static str> {
        match self.hacking.phase {
            Phase::Idle => None,
            Phase::Unpack => Some("UNPACKING"),
            Phase::Hack => Some("FIRING_A"),
            Phase::Pack => Some("PACKING"),
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_hacker_module(&mut self, id: usize, registration: usize) {
        self.construct_ai_module(id, registration);
        self.entities[id].hacking = State::new();
    }

    pub(super) fn start_contained_hacking(&mut self, id: usize) {
        self.native_hack_internet_from(id, COMMAND_FROM_AI);
    }

    /// Starts the original outdoor internet-hacking command.
    /// # Errors
    /// Rejects unavailable objects and objects without the native Hacker AI.
    pub fn hack_internet(&mut self, id: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        self.hack_internet_actor(id)
    }

    pub(super) fn hack_internet_actor(&mut self, id: usize) -> Result<(), &'static str> {
        if self.catalog[self.entities[id].type_index]
            .behavior
            .hacking
            .is_none()
        {
            return Err("This object cannot hack the internet");
        }
        self.native_hack_internet_from(id, COMMAND_FROM_PLAYER);
        Ok(())
    }

    pub(in crate::skirmish) fn native_hack_internet_from(&mut self, id: usize, source: i32) {
        if self.native_subclass_command(
            id,
            Command {
                source,
                kind: CommandKind::HackInternet,
            },
        ) || !self.native_ai_command_allowed(id)
            || self.catalog[self.entities[id].type_index]
                .behavior
                .hacking
                .is_none()
        {
            return;
        }
        self.clear_ai_goal(id);
        self.set_last_command_source(id, source);
        self.enter_hacking_state(id, false);
    }

    pub(in crate::skirmish) fn hacker_command(&mut self, id: usize, command: Command) -> bool {
        if !self.native_ai_command_allowed_for_sleep(
            id,
            matches!(command.kind, CommandKind::MoveEvenIfSleeping(_)),
        ) {
            return true;
        }
        let phase = &self.entities[id].hacking.phase;
        if matches!(command.kind, CommandKind::HackInternet)
            && matches!(phase, Phase::Unpack | Phase::Hack)
        {
            return true;
        }
        if matches!(phase, Phase::Hack | Phase::Pack) {
            let packing = matches!(phase, Phase::Pack);
            self.entities[id].hacking.pending = Some(command);
            if !packing {
                self.clear_ai_goal(id);
                self.set_last_command_source(id, COMMAND_FROM_AI);
                self.enter_hacking_state(id, true);
            }
            return true;
        }
        self.entities[id].hacking.pending = None;
        false
    }

    pub(super) fn cancel_hacking(&mut self, id: usize) {
        let flag = match self.entities[id].hacking.phase {
            Phase::Idle => 0,
            Phase::Unpack => 1 << 74,
            Phase::Hack => 1 << 36,
            Phase::Pack => 1 << 73,
        };
        self.ability_model_flags(id, 0, flag);
        self.entities[id].hacking.phase = Phase::Idle;
        self.entities[id].hacking.animation = None;
    }

    fn enter_hacking_state(&mut self, id: usize, packing: bool) {
        let position = self.entities[id].ai.command.goal_position;
        let object = self.entities[id].ai.command.goal_object;
        if self
            .prepare_order_state(id, Order::HackInternet, false, Some(position), None)
            .is_none()
        {
            return;
        }
        self.entities[id].ai.command.goal_object = object;
        self.entities[id].ai.command.main_state = MainState::HackInternet;
        self.enter_hacker_phase(id, packing);
        self.ai_state_changed(id);
    }

    fn enter_hacker_phase(&mut self, id: usize, packing: bool) {
        let rules = self.catalog[self.entities[id].type_index]
            .behavior
            .hacking
            .as_ref()
            .expect("Hacker module");
        let factor = startup::random_real(
            &mut self.random,
            1.0 - rules.variation,
            1.0 + rules.variation,
        );
        let frames = super::progression::integer_real(if !packing {
            rules.unpack
        } else if self.entities[id].contained_by().is_some() {
            0
        } else {
            rules.pack
        });
        // C++ unsigned conversion truncates the nonnegative randomized frame count.
        let frames = (frames * factor)
            .to_u32()
            .expect("bounded native hacker duration");
        self.ability_model_flags(
            id,
            if packing { 1 << 73 } else { 1 << 74 },
            if packing {
                1 << 36
            } else {
                (1 << 73) | (1 << 74) | (1 << 36)
            },
        );
        let state = &mut self.entities[id].hacking;
        state.phase = if packing { Phase::Pack } else { Phase::Unpack };
        state.remaining = frames;
        state.animation = Some((self.tick, frames, false));
        self.presentation.hacking_events.push(Event {
            object: id,
            frame: self.tick,
            effect: if packing {
                Effect::Pack
            } else {
                Effect::Unpack
            },
        });
    }

    // The subclass replays before the shared primary update, one callback after packing.
    pub(super) fn dispatch_hacker_pending(&mut self, id: usize) {
        if !matches!(self.entities[id].ai.command.main_state, MainState::Active)
            || !matches!(self.entities[id].ai.order, Order::Idle)
        {
            return;
        }
        if let Some(command) = self.entities[id].hacking.pending.take() {
            self.replay_stored_command(id, command);
        }
    }

    pub(super) fn update_hacking(&mut self, id: usize) {
        if matches!(self.entities[id].hacking.phase, Phase::Unpack) {
            self.ability_model_flags(id, 1 << 74, 0);
        }
        let type_index = self.entities[id].type_index;
        let contained = self.entities[id].contained_by().is_some();
        if matches!(self.entities[id].hacking.phase, Phase::Hack) && self.entities[id].hacked() {
            return;
        }
        let state = &mut self.entities[id].hacking;
        if state.remaining > 0 {
            state.remaining -= 1;
            return;
        }
        match state.phase {
            Phase::Idle => {}
            Phase::Unpack => {
                state.phase = Phase::Hack;
                state.animation = None;
                let rules = self.catalog[type_index]
                    .behavior
                    .hacking
                    .as_ref()
                    .expect("Hacker module");
                state.remaining = if contained {
                    rules.contained_delay
                } else {
                    rules.delay
                };
                self.ability_model_flags(id, 1 << 36, 1 << 74);
            }
            Phase::Pack => {
                self.set_ai_order(id, Order::Idle);
            }
            Phase::Hack => self.hacker_deposit(id),
        }
    }

    fn hacker_deposit(&mut self, id: usize) {
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .behavior
            .hacking
            .as_ref()
            .expect("Hacker module");
        let amount = rules.amounts[..=usize::from(actor.veterancy)]
            .iter()
            .rev()
            .copied()
            .find(|value| *value != 0)
            .unwrap_or(1);
        let experience = rules.experience;
        let player = &mut self.players[actor.owner];
        player.cash = player.cash.wrapping_add(amount);
        player.statistics.money_earned = player.statistics.money_earned.wrapping_add(amount);
        self.add_unit_experience(id, i32::from_ne_bytes(experience.to_ne_bytes()));
        // Floating text captures the original payment position immediately;
        // a later same-frame death/exit must not replace it with the new location.
        let position = self.object_position(id);
        let container_radii = self.entities[id].contained_by().map(|carrier| {
            let geometry = self.body_geometry(carrier);
            [geometry.major, geometry.minor]
        });
        let visible_players = self
            .players
            .iter()
            .enumerate()
            .fold(0, |mask, (viewer, _)| {
                if self.logically_visible(viewer, id) {
                    mask | (1 << viewer)
                } else {
                    mask
                }
            });
        self.presentation.hacking_events.push(Event {
            object: id,
            frame: self.tick,
            effect: Effect::Cash {
                amount,
                position,
                container_radii,
                visible_players,
            },
        });
        let rules = self.catalog[self.entities[id].type_index]
            .behavior
            .hacking
            .as_ref()
            .expect("Hacker module");
        self.entities[id].hacking.remaining = if self.entities[id].contained_by().is_some() {
            rules.contained_delay
        } else {
            rules.delay
        };
    }
}

pub(super) fn validate(rules: &HackerRules) -> Result<(), &'static str> {
    if !rules.variation.is_finite()
        || !(0.0..=1.0).contains(&rules.variation)
        || rules.unpack > u32::from(u16::MAX)
        || rules.pack > u32::from(u16::MAX)
        || rules.delay > u32::from(u16::MAX)
        || rules.contained_delay > u32::from(u16::MAX)
    {
        return Err("Invalid original hacker timers");
    }
    Ok(())
}

#[cfg(test)]
pub(in crate::skirmish) mod tests;
